use crate::cards::card::Card;
use crate::cards::deck::Deck;
use crate::eval::evaluator;

/// Trait for mapping (hole_cards, board) to an integer bucket.
///
/// Card abstraction reduces the state space by grouping strategically
/// similar hands together.
pub trait CardAbstraction: Send + Sync {
    /// Given a player's hole cards and the public board, return the bucket index.
    fn bucket(&self, hole: [Card; 2], board: &[Card]) -> u16;

    /// Total number of buckets.
    fn num_buckets(&self) -> u16;
}

/// No abstraction: each distinct hand is its own bucket.
/// Suitable for small games or when full precision is needed.
pub struct NoAbstraction {
    n_buckets: u16,
}

impl NoAbstraction {
    pub fn new(n_buckets: u16) -> Self {
        Self { n_buckets }
    }
}

impl CardAbstraction for NoAbstraction {
    fn bucket(&self, hole: [Card; 2], _board: &[Card]) -> u16 {
        // Use the combo index as the bucket
        crate::solver::range::HandRange::combo_index(hole[0], hole[1])
    }

    fn num_buckets(&self) -> u16 {
        self.n_buckets
    }
}

/// Equity-based abstraction: cluster hands into buckets based on
/// Expected Hand Strength (EHS).
///
/// On the river, EHS is computed exactly by enumerating all possible
/// opponent holdings. On earlier streets, Monte Carlo rollouts estimate EHS.
pub struct EquityBuckets {
    /// Number of equity buckets.
    pub n_buckets: u16,
}

impl EquityBuckets {
    pub fn new(n_buckets: u16) -> Self {
        Self { n_buckets }
    }

    /// Compute exact Expected Hand Strength on the river.
    ///
    /// Enumerates all possible opponent 2-card holdings (given the board
    /// and our hole cards as dead cards) and computes win/tie/loss.
    pub fn river_ehs(hole: [Card; 2], board: &[Card; 5]) -> f64 {
        let mut deck = Deck::new();
        deck.remove(hole[0]);
        deck.remove(hole[1]);
        for &c in board.iter() {
            deck.remove(c);
        }

        // Our best hand
        let our_cards: [Card; 7] = [
            hole[0], hole[1], board[0], board[1], board[2], board[3], board[4],
        ];
        let our_value = evaluator::best_five_of_seven(&our_cards);

        let remaining: Vec<Card> = (0..52u8)
            .filter(|&i| deck.contains(Card::from_index(i)))
            .map(Card::from_index)
            .collect();

        let mut wins = 0u32;
        let mut ties = 0u32;
        let mut total = 0u32;

        for i in 0..remaining.len() {
            for j in (i + 1)..remaining.len() {
                let opp_cards: [Card; 7] = [
                    remaining[i],
                    remaining[j],
                    board[0],
                    board[1],
                    board[2],
                    board[3],
                    board[4],
                ];
                let opp_value = evaluator::best_five_of_seven(&opp_cards);
                total += 1;
                if our_value > opp_value {
                    wins += 1;
                } else if our_value == opp_value {
                    ties += 1;
                }
            }
        }

        (wins as f64 + 0.5 * ties as f64) / total as f64
    }

    /// Compute approximate EHS via Monte Carlo rollouts for flop/turn.
    ///
    /// Randomly completes the board and opponent's hand `n_rollouts` times.
    pub fn monte_carlo_ehs(
        hole: [Card; 2],
        board: &[Card],
        n_rollouts: u32,
    ) -> f64 {
        use rand::SeedableRng;
        use rand_xoshiro::Xoshiro256PlusPlus;

        let mut rng = Xoshiro256PlusPlus::seed_from_u64(
            hole[0].index() as u64 * 52 + hole[1].index() as u64,
        );

        let board_remaining = 5 - board.len();
        let mut wins = 0u32;
        let mut ties = 0u32;

        for _ in 0..n_rollouts {
            let mut deck = Deck::new();
            deck.remove(hole[0]);
            deck.remove(hole[1]);
            for &c in board {
                deck.remove(c);
            }

            // Complete the board
            let mut full_board = [Card::from_index(0); 5];
            for (i, &c) in board.iter().enumerate() {
                full_board[i] = c;
            }
            for i in 0..board_remaining {
                full_board[board.len() + i] = deck.deal_random(&mut rng).unwrap();
            }

            // Deal opponent's cards
            let opp1 = deck.deal_random(&mut rng).unwrap();
            let opp2 = deck.deal_random(&mut rng).unwrap();

            let our_cards: [Card; 7] = [
                hole[0],
                hole[1],
                full_board[0],
                full_board[1],
                full_board[2],
                full_board[3],
                full_board[4],
            ];
            let opp_cards: [Card; 7] = [
                opp1,
                opp2,
                full_board[0],
                full_board[1],
                full_board[2],
                full_board[3],
                full_board[4],
            ];

            let our_value = evaluator::best_five_of_seven(&our_cards);
            let opp_value = evaluator::best_five_of_seven(&opp_cards);

            if our_value > opp_value {
                wins += 1;
            } else if our_value == opp_value {
                ties += 1;
            }
        }

        (wins as f64 + 0.5 * ties as f64) / n_rollouts as f64
    }
}

impl CardAbstraction for EquityBuckets {
    fn bucket(&self, hole: [Card; 2], board: &[Card]) -> u16 {
        let ehs = if board.len() == 5 {
            let board_arr: [Card; 5] = [board[0], board[1], board[2], board[3], board[4]];
            Self::river_ehs(hole, &board_arr)
        } else {
            Self::monte_carlo_ehs(hole, board, 200)
        };

        // Map EHS [0, 1] to bucket [0, n_buckets - 1]
        (ehs * self.n_buckets as f64).min(self.n_buckets as f64 - 1.0) as u16
    }

    fn num_buckets(&self) -> u16 {
        self.n_buckets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::card::{Rank, Suit};

    #[test]
    fn river_ehs_aces_vs_board() {
        // AA on a low board should have very high EHS
        let hole = [
            Card::new(Rank::Ace, Suit::Hearts),
            Card::new(Rank::Ace, Suit::Spades),
        ];
        let board = [
            Card::new(Rank::Two, Suit::Clubs),
            Card::new(Rank::Five, Suit::Diamonds),
            Card::new(Rank::Seven, Suit::Hearts),
            Card::new(Rank::Nine, Suit::Spades),
            Card::new(Rank::Three, Suit::Clubs),
        ];
        let ehs = EquityBuckets::river_ehs(hole, &board);
        assert!(ehs > 0.85, "AA on low board should have EHS > 0.85, got {ehs:.3}");
    }

    #[test]
    fn river_ehs_low_hand() {
        // 72o on a high board should have low EHS
        let hole = [
            Card::new(Rank::Seven, Suit::Hearts),
            Card::new(Rank::Two, Suit::Spades),
        ];
        let board = [
            Card::new(Rank::Ace, Suit::Clubs),
            Card::new(Rank::King, Suit::Diamonds),
            Card::new(Rank::Queen, Suit::Hearts),
            Card::new(Rank::Jack, Suit::Spades),
            Card::new(Rank::Nine, Suit::Clubs),
        ];
        let ehs = EquityBuckets::river_ehs(hole, &board);
        assert!(ehs < 0.15, "72o on high board should have EHS < 0.15, got {ehs:.3}");
    }

    #[test]
    fn monte_carlo_ehs_flop() {
        // AA on a flop should have high EHS
        let hole = [
            Card::new(Rank::Ace, Suit::Hearts),
            Card::new(Rank::Ace, Suit::Spades),
        ];
        let board = [
            Card::new(Rank::Two, Suit::Clubs),
            Card::new(Rank::Five, Suit::Diamonds),
            Card::new(Rank::Seven, Suit::Hearts),
        ];
        let ehs = EquityBuckets::monte_carlo_ehs(hole, &board, 1000);
        assert!(ehs > 0.75, "AA on low flop should have EHS > 0.75, got {ehs:.3}");
    }

    #[test]
    fn equity_bucket_assignment() {
        let abstraction = EquityBuckets::new(10);

        let strong_hole = [
            Card::new(Rank::Ace, Suit::Hearts),
            Card::new(Rank::Ace, Suit::Spades),
        ];
        let weak_hole = [
            Card::new(Rank::Seven, Suit::Hearts),
            Card::new(Rank::Two, Suit::Spades),
        ];
        let board = [
            Card::new(Rank::Two, Suit::Clubs),
            Card::new(Rank::Five, Suit::Diamonds),
            Card::new(Rank::Nine, Suit::Hearts),
            Card::new(Rank::Jack, Suit::Spades),
            Card::new(Rank::Three, Suit::Clubs),
        ];

        let strong_bucket = abstraction.bucket(strong_hole, &board);
        let weak_bucket = abstraction.bucket(weak_hole, &board);

        assert!(
            strong_bucket > weak_bucket,
            "AA (bucket {strong_bucket}) should be in a higher bucket than 72o (bucket {weak_bucket})"
        );
    }
}
