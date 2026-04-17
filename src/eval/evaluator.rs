//! Core hand evaluation functions.

use crate::cards::card::Card;
use crate::eval::lookup::{lookup_pairs, FLUSH_TABLE, UNIQUE5_TABLE};
use crate::eval::rank::HandValue;
use itertools::Itertools;

/// Evaluate exactly 5 cards, returning a HandValue (higher = better).
///
/// Hot path: called 21× per Texas Hold'em hand, hundreds of millions of times
/// across a simulation run.  Every allocation and branch counts.
#[inline]
pub fn evaluate_five(cards: &[Card; 5]) -> HandValue {
    // Compute rank bitmask and flush flag in one unrolled pass — no loops, no
    // HashSet allocation.  The compiler can pipeline / partially vectorise this.
    let r0 = cards[0].rank.index();
    let r1 = cards[1].rank.index();
    let r2 = cards[2].rank.index();
    let r3 = cards[3].rank.index();
    let r4 = cards[4].rank.index();
    let mask = (1u16 << r0) | (1u16 << r1) | (1u16 << r2) | (1u16 << r3) | (1u16 << r4);

    // Flush check: all five suits equal the first (4 comparisons, short-circuit).
    let s = cards[0].suit;
    if cards[1].suit == s && cards[2].suit == s && cards[3].suit == s && cards[4].suit == s {
        // Straight-flush or regular flush — FLUSH_TABLE handles both.
        return HandValue::from_cactus_kev(FLUSH_TABLE[mask as usize]);
    }

    // If the rank bitmask has exactly 5 bits set, all ranks are distinct →
    // must be a straight or high-card hand.  UNIQUE5_TABLE covers every
    // C(13,5) = 1287 combination, so the entry is always non-zero here.
    if mask.count_ones() == 5 {
        return HandValue::from_cactus_kev(UNIQUE5_TABLE[mask as usize]);
    }

    // Paired hand: compute prime product (unrolled) and binary-search the table.
    let product = cards[0].rank.prime()
        * cards[1].rank.prime()
        * cards[2].rank.prime()
        * cards[3].rank.prime()
        * cards[4].rank.prime();
    HandValue::from_cactus_kev(lookup_pairs(product))
}

/// Evaluate the best 5-card hand from any number of cards (n >= 5).
pub fn best_five_of_n(cards: &[Card]) -> HandValue {
    assert!(
        cards.len() >= 5,
        "need at least 5 cards, got {}",
        cards.len()
    );
    cards
        .iter()
        .combinations(5)
        .map(|combo| {
            let arr: [Card; 5] = [*combo[0], *combo[1], *combo[2], *combo[3], *combo[4]];
            evaluate_five(&arr)
        })
        .max()
        .unwrap()
}

/// Specialised fast path for exactly 7 cards (Texas Hold'em, Stud).
/// Generates all C(7,5)=21 combinations via a static index table — no itertools.
#[inline]
pub fn best_five_of_seven(cards: &[Card; 7]) -> HandValue {
    const COMBOS: [[usize; 5]; 21] = [
        [0, 1, 2, 3, 4],
        [0, 1, 2, 3, 5],
        [0, 1, 2, 3, 6],
        [0, 1, 2, 4, 5],
        [0, 1, 2, 4, 6],
        [0, 1, 2, 5, 6],
        [0, 1, 3, 4, 5],
        [0, 1, 3, 4, 6],
        [0, 1, 3, 5, 6],
        [0, 1, 4, 5, 6],
        [0, 2, 3, 4, 5],
        [0, 2, 3, 4, 6],
        [0, 2, 3, 5, 6],
        [0, 2, 4, 5, 6],
        [0, 3, 4, 5, 6],
        [1, 2, 3, 4, 5],
        [1, 2, 3, 4, 6],
        [1, 2, 3, 5, 6],
        [1, 2, 4, 5, 6],
        [1, 3, 4, 5, 6],
        [2, 3, 4, 5, 6],
    ];
    let mut best = HandValue(0);
    for idxs in &COMBOS {
        let arr = [
            cards[idxs[0]],
            cards[idxs[1]],
            cards[idxs[2]],
            cards[idxs[3]],
            cards[idxs[4]],
        ];
        let val = evaluate_five(&arr);
        if val > best {
            best = val;
        }
    }
    best
}

/// Evaluate Omaha hand: must use exactly 2 of 4 hole cards and exactly 3 of 5 board cards.
pub fn evaluate_omaha(hole: &[Card; 4], board: &[Card; 5]) -> HandValue {
    let mut best = HandValue(0);
    // C(4,2) = 6 hole combinations
    for hi in 0..4 {
        for hj in (hi + 1)..4 {
            // C(5,3) = 10 board combinations
            for bi in 0..5 {
                for bj in (bi + 1)..5 {
                    for bk in (bj + 1)..5 {
                        let arr = [hole[hi], hole[hj], board[bi], board[bj], board[bk]];
                        let val = evaluate_five(&arr);
                        if val > best {
                            best = val;
                        }
                    }
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::card::{Card, Rank, Suit};
    use crate::eval::rank::HandCategory;

    fn card(r: Rank, s: Suit) -> Card {
        Card::new(r, s)
    }

    #[test]
    fn royal_flush() {
        let cards = [
            card(Rank::Ace, Suit::Spades),
            card(Rank::King, Suit::Spades),
            card(Rank::Queen, Suit::Spades),
            card(Rank::Jack, Suit::Spades),
            card(Rank::Ten, Suit::Spades),
        ];
        let val = evaluate_five(&cards);
        assert_eq!(val.category(), HandCategory::RoyalFlush);
    }

    #[test]
    fn straight_flush() {
        let cards = [
            card(Rank::Nine, Suit::Hearts),
            card(Rank::Eight, Suit::Hearts),
            card(Rank::Seven, Suit::Hearts),
            card(Rank::Six, Suit::Hearts),
            card(Rank::Five, Suit::Hearts),
        ];
        let val = evaluate_five(&cards);
        assert_eq!(val.category(), HandCategory::StraightFlush);
    }

    #[test]
    fn four_of_a_kind() {
        let cards = [
            card(Rank::Ace, Suit::Spades),
            card(Rank::Ace, Suit::Hearts),
            card(Rank::Ace, Suit::Diamonds),
            card(Rank::Ace, Suit::Clubs),
            card(Rank::King, Suit::Spades),
        ];
        let val = evaluate_five(&cards);
        assert_eq!(val.category(), HandCategory::FourOfAKind);
    }

    #[test]
    fn full_house() {
        let cards = [
            card(Rank::King, Suit::Spades),
            card(Rank::King, Suit::Hearts),
            card(Rank::King, Suit::Diamonds),
            card(Rank::Ace, Suit::Clubs),
            card(Rank::Ace, Suit::Spades),
        ];
        let val = evaluate_five(&cards);
        assert_eq!(val.category(), HandCategory::FullHouse);
    }

    #[test]
    fn flush() {
        let cards = [
            card(Rank::Ace, Suit::Hearts),
            card(Rank::Jack, Suit::Hearts),
            card(Rank::Nine, Suit::Hearts),
            card(Rank::Six, Suit::Hearts),
            card(Rank::Two, Suit::Hearts),
        ];
        let val = evaluate_five(&cards);
        assert_eq!(val.category(), HandCategory::Flush);
    }

    #[test]
    fn straight() {
        let cards = [
            card(Rank::Nine, Suit::Spades),
            card(Rank::Eight, Suit::Hearts),
            card(Rank::Seven, Suit::Diamonds),
            card(Rank::Six, Suit::Clubs),
            card(Rank::Five, Suit::Spades),
        ];
        let val = evaluate_five(&cards);
        assert_eq!(val.category(), HandCategory::Straight);
    }

    #[test]
    fn wheel_straight() {
        let cards = [
            card(Rank::Ace, Suit::Spades),
            card(Rank::Two, Suit::Hearts),
            card(Rank::Three, Suit::Diamonds),
            card(Rank::Four, Suit::Clubs),
            card(Rank::Five, Suit::Spades),
        ];
        let val = evaluate_five(&cards);
        assert_eq!(val.category(), HandCategory::Straight);
    }

    #[test]
    fn two_pair() {
        let cards = [
            card(Rank::Ace, Suit::Spades),
            card(Rank::Ace, Suit::Hearts),
            card(Rank::King, Suit::Diamonds),
            card(Rank::King, Suit::Clubs),
            card(Rank::Queen, Suit::Spades),
        ];
        let val = evaluate_five(&cards);
        assert_eq!(val.category(), HandCategory::TwoPair);
    }

    #[test]
    fn one_pair() {
        let cards = [
            card(Rank::Ace, Suit::Spades),
            card(Rank::Ace, Suit::Hearts),
            card(Rank::King, Suit::Diamonds),
            card(Rank::Queen, Suit::Clubs),
            card(Rank::Jack, Suit::Spades),
        ];
        let val = evaluate_five(&cards);
        assert_eq!(val.category(), HandCategory::OnePair);
    }

    #[test]
    fn high_card() {
        let cards = [
            card(Rank::Ace, Suit::Spades),
            card(Rank::King, Suit::Hearts),
            card(Rank::Queen, Suit::Diamonds),
            card(Rank::Jack, Suit::Clubs),
            card(Rank::Nine, Suit::Spades),
        ];
        let val = evaluate_five(&cards);
        assert_eq!(val.category(), HandCategory::HighCard);
    }

    #[test]
    fn best_of_seven() {
        // Player has two pair but board gives a full house
        let cards = [
            card(Rank::King, Suit::Spades),
            card(Rank::King, Suit::Hearts), // hole
            card(Rank::King, Suit::Diamonds),
            card(Rank::Ace, Suit::Clubs), // board
            card(Rank::Ace, Suit::Spades),
            card(Rank::Two, Suit::Clubs),
            card(Rank::Three, Suit::Diamonds),
        ];
        let val = best_five_of_seven(&cards);
        assert_eq!(val.category(), HandCategory::FullHouse);
    }

    #[test]
    fn hand_ordering() {
        let rf = evaluate_five(&[
            card(Rank::Ace, Suit::Spades),
            card(Rank::King, Suit::Spades),
            card(Rank::Queen, Suit::Spades),
            card(Rank::Jack, Suit::Spades),
            card(Rank::Ten, Suit::Spades),
        ]);
        let sf = evaluate_five(&[
            card(Rank::Nine, Suit::Hearts),
            card(Rank::Eight, Suit::Hearts),
            card(Rank::Seven, Suit::Hearts),
            card(Rank::Six, Suit::Hearts),
            card(Rank::Five, Suit::Hearts),
        ]);
        let foak = evaluate_five(&[
            card(Rank::Ace, Suit::Spades),
            card(Rank::Ace, Suit::Hearts),
            card(Rank::Ace, Suit::Diamonds),
            card(Rank::Ace, Suit::Clubs),
            card(Rank::King, Suit::Spades),
        ]);
        assert!(rf > sf);
        assert!(sf > foak);
    }
}
