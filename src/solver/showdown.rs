//! River showdown evaluation for vector CFR.
//!
//! Vector CFR carries a per-combo reach probability for each player and must
//! produce a per-combo counterfactual value at every showdown terminal. A naive
//! formulation compares each of the 1326 possible player-0 combos against each
//! of the 1326 possible player-1 combos — that is ~1.76M per-combo
//! comparisons per terminal, per iteration, per direction.
//!
//! `ShowdownRanker` precomputes the per-combo hand value on a fixed 5-card
//! board once, so at solve time terminal evaluation reduces to comparing two
//! i16 values plus a card-conflict mask. This module exposes two evaluators:
//!
//! - [`ShowdownRanker::terminal_ev_naive`] — O(N²) in the number of combos,
//!   kept as a correctness reference. Used by tests and by [`terminal_ev`] for
//!   small problems.
//! - [`ShowdownRanker::terminal_ev`] currently dispatches to the naive version;
//!   a follow-up commit will add an O(N log N) sorted-prefix implementation
//!   once its correctness is validated against this baseline.
//!
//! Output convention: the returned per-combo values are in units of
//! "fraction of pot that player X wins over player Y", i.e. each entry lies in
//! `[-1, 1]`. Callers multiply by the pot size at the terminal to get chips.
//! Combos that conflict with the board (share a card) produce 0 — they can't
//! be dealt in the first place, so reach probability is always 0 there anyway.

use crate::cards::card::Card;
use crate::eval::evaluator::best_five_of_seven;
use crate::eval::rank::HandValue;
use crate::solver::range::HandRange;

/// Total number of two-card combos (52 choose 2).
pub const N_COMBOS: usize = 1326;

/// Precomputed per-combo hand value on a fixed 5-card board.
///
/// Built once per river terminal (or once per runout), then consulted many
/// times across CFR iterations. Stores a compact `i16` rank per combo and a
/// `u64` card mask for O(1) conflict checks.
#[derive(Clone)]
pub struct ShowdownRanker {
    /// Hand value for each combo, or `i16::MIN` if the combo conflicts with
    /// the board (and thus can never be dealt).
    ///
    /// `i16::MIN` is used rather than `Option<HandValue>` to keep the hot
    /// loop branch-free — conflicts are detected via the mask instead.
    rank: [i16; N_COMBOS],
    /// Bitmask over the 52 cards for each combo (`1 << card_index | 1 << ...`).
    /// Two combos are compatible iff `mask[i] & mask[j] == 0`.
    mask: [u64; N_COMBOS],
    /// `true` if combo `i` conflicts with the board (uses a board card).
    /// Stored as a separate array so the hot loop can skip board-conflicting
    /// combos with a single branch rather than a mask-and.
    blocked: [bool; N_COMBOS],
}

impl ShowdownRanker {
    /// Build a ranker for the given 5-card board.
    ///
    /// Evaluates all 1326 possible two-card combos once; combos containing a
    /// board card are marked blocked and assigned `i16::MIN` rank.
    pub fn new(board: &[Card; 5]) -> Self {
        let board_mask: u64 = board.iter().fold(0u64, |m, c| m | (1u64 << c.index()));

        let mut rank = [0i16; N_COMBOS];
        let mut mask = [0u64; N_COMBOS];
        let mut blocked = [false; N_COMBOS];

        for combo_idx in 0..N_COMBOS as u16 {
            let (c1, c2) = HandRange::cards_from_index(combo_idx);
            let combo_mask = (1u64 << c1.index()) | (1u64 << c2.index());
            mask[combo_idx as usize] = combo_mask;

            if combo_mask & board_mask != 0 {
                blocked[combo_idx as usize] = true;
                rank[combo_idx as usize] = i16::MIN;
                continue;
            }

            let seven = [c1, c2, board[0], board[1], board[2], board[3], board[4]];
            let hv: HandValue = best_five_of_seven(&seven);
            // HandValue.0 ranges 1..=7462, fits in i16 comfortably.
            rank[combo_idx as usize] = hv.0 as i16;
        }

        Self {
            rank,
            mask,
            blocked,
        }
    }

    /// Hand value of a combo, or `None` if the combo conflicts with the board.
    #[inline]
    pub fn rank(&self, combo_idx: u16) -> Option<HandValue> {
        if self.blocked[combo_idx as usize] {
            None
        } else {
            Some(HandValue(self.rank[combo_idx as usize] as u16))
        }
    }

    /// `true` if combo `i` conflicts with the board.
    #[inline]
    pub fn is_blocked(&self, combo_idx: u16) -> bool {
        self.blocked[combo_idx as usize]
    }

    /// Compute per-combo terminal EV against an opponent's reach vector.
    ///
    /// Returns `(ev_p0, ev_p1)` where each is a `[f32; 1326]`-shaped slice of
    /// "fraction-of-pot-won" values: `+1` = always win, `-1` = always lose,
    /// `0` = either a tie or an incompatible matchup. Call sites multiply by
    /// the terminal's pot size to get chip values.
    ///
    /// Naive implementation — O(N²) pairwise comparisons. Preserved here as
    /// the correctness reference against which faster implementations can be
    /// checked.
    pub fn terminal_ev_naive(
        &self,
        reach_p0: &[f32; N_COMBOS],
        reach_p1: &[f32; N_COMBOS],
    ) -> (Box<[f32; N_COMBOS]>, Box<[f32; N_COMBOS]>) {
        let mut ev_p0 = Box::new([0.0f32; N_COMBOS]);
        let mut ev_p1 = Box::new([0.0f32; N_COMBOS]);

        for i in 0..N_COMBOS {
            if self.blocked[i] {
                continue;
            }
            let r_i = self.rank[i];
            let m_i = self.mask[i];
            let reach_i = reach_p0[i];
            let mut acc = 0.0f32;

            for j in 0..N_COMBOS {
                if self.blocked[j] || (m_i & self.mask[j]) != 0 {
                    continue;
                }
                let r_j = self.rank[j];
                // sign(r_i - r_j): +1 win, -1 loss, 0 tie.
                let outcome = (r_i > r_j) as i32 - (r_i < r_j) as i32;
                let reach_j = reach_p1[j];
                acc += outcome as f32 * reach_j;
                ev_p1[j] -= outcome as f32 * reach_i;
            }
            ev_p0[i] = acc;
        }

        (ev_p0, ev_p1)
    }

    /// Compute per-combo terminal EV — currently dispatches to the naive
    /// implementation. See module docs.
    #[inline]
    pub fn terminal_ev(
        &self,
        reach_p0: &[f32; N_COMBOS],
        reach_p1: &[f32; N_COMBOS],
    ) -> (Box<[f32; N_COMBOS]>, Box<[f32; N_COMBOS]>) {
        self.terminal_ev_naive(reach_p0, reach_p1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::card::{Rank, Suit};

    fn card(r: Rank, s: Suit) -> Card {
        Card::new(r, s)
    }

    fn test_board() -> [Card; 5] {
        [
            card(Rank::Ace, Suit::Spades),
            card(Rank::King, Suit::Hearts),
            card(Rank::Queen, Suit::Diamonds),
            card(Rank::Seven, Suit::Clubs),
            card(Rank::Two, Suit::Spades),
        ]
    }

    #[test]
    fn blocked_combos_match_board() {
        let board = test_board();
        let ranker = ShowdownRanker::new(&board);
        // Every combo that contains any board card must be blocked.
        for combo_idx in 0..N_COMBOS as u16 {
            let (c1, c2) = HandRange::cards_from_index(combo_idx);
            let touches_board = board.iter().any(|b| *b == c1 || *b == c2);
            assert_eq!(
                ranker.is_blocked(combo_idx),
                touches_board,
                "combo {combo_idx} blocked mismatch"
            );
        }
    }

    #[test]
    fn strong_hand_beats_weak_hand() {
        let board = test_board();
        let ranker = ShowdownRanker::new(&board);

        // Pocket aces (AhAd) vs pocket twos (2h2d) — pocket 2s already has 2s
        // on the board, so pair vs pair of aces (given A on board) vs trips(2).
        // Use AhAd vs 3h3d instead to make the matchup clean.
        let ah_ad = HandRange::combo_index(
            card(Rank::Ace, Suit::Hearts),
            card(Rank::Ace, Suit::Diamonds),
        );
        let three_pair = HandRange::combo_index(
            card(Rank::Three, Suit::Hearts),
            card(Rank::Three, Suit::Diamonds),
        );

        let r_aa = ranker.rank(ah_ad).expect("AhAd not blocked");
        let r_33 = ranker.rank(three_pair).expect("3h3d not blocked");
        assert!(r_aa > r_33, "trip aces should beat pocket threes");
    }

    #[test]
    fn terminal_ev_naive_zero_sum() {
        // With a pot contribution of 1, every incompatible pair contributes 0
        // and every compatible pair contributes ±1 symmetrically: the sum of
        // ev_p0 · reach_p0 should equal -(sum of ev_p1 · reach_p1).
        let board = test_board();
        let ranker = ShowdownRanker::new(&board);

        let mut reach_p0 = [0.0f32; N_COMBOS];
        let mut reach_p1 = [0.0f32; N_COMBOS];
        // Small fixed range so the test is fast and deterministic.
        for idx in [10u16, 42, 100, 200, 400, 800, 1200] {
            if !ranker.is_blocked(idx) {
                reach_p0[idx as usize] = 1.0;
                reach_p1[idx as usize] = 1.0;
            }
        }

        let (ev_p0, ev_p1) = ranker.terminal_ev_naive(&reach_p0, &reach_p1);
        let ev0: f32 = ev_p0.iter().zip(reach_p0.iter()).map(|(e, r)| e * r).sum();
        let ev1: f32 = ev_p1.iter().zip(reach_p1.iter()).map(|(e, r)| e * r).sum();
        // Symmetric ranges against a symmetric opponent should net 0.
        assert!(
            (ev0 + ev1).abs() < 1e-4,
            "ev0={ev0}, ev1={ev1} — should sum to 0"
        );
    }

    #[test]
    fn terminal_ev_heads_up_single_combo() {
        // With only one combo in each range, the outcome is deterministic:
        // sign of rank(p0) - rank(p1).
        let board = test_board();
        let ranker = ShowdownRanker::new(&board);

        let aa = HandRange::combo_index(
            card(Rank::Ace, Suit::Hearts),
            card(Rank::Ace, Suit::Diamonds),
        );
        let three_three = HandRange::combo_index(
            card(Rank::Three, Suit::Hearts),
            card(Rank::Three, Suit::Diamonds),
        );

        let mut reach_p0 = [0.0f32; N_COMBOS];
        let mut reach_p1 = [0.0f32; N_COMBOS];
        reach_p0[aa as usize] = 1.0;
        reach_p1[three_three as usize] = 1.0;

        let (ev_p0, ev_p1) = ranker.terminal_ev_naive(&reach_p0, &reach_p1);
        assert!((ev_p0[aa as usize] - 1.0).abs() < 1e-6, "AA should win");
        assert!(
            (ev_p1[three_three as usize] + 1.0).abs() < 1e-6,
            "33 should lose"
        );
    }
}
