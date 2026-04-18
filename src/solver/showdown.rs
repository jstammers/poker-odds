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
//! - [`ShowdownRanker::terminal_ev_naive`] — O(N²) in the number of combos.
//!   The correctness reference. Left `pub` for tests and for callers that want
//!   a straightforward implementation.
//! - [`ShowdownRanker::terminal_ev`] — O(N·52) via a sorted-rank walk with
//!   per-card prefix sums. The hot path at every iteration.
//!
//! The fast implementation rests on one identity. For a hand `i = (a, b)`
//! against an opponent reach vector `r`, the fraction of pot won is
//! `wins − losses` where
//!
//! ```text
//!   wins   = Σ_{j: rank_j < rank_i, j∩{a,b}=∅} r_j
//!   losses = Σ_{j: rank_j > rank_i, j∩{a,b}=∅} r_j
//! ```
//!
//! Inclusion-exclusion on "j contains a OR j contains b" lets each sum be
//! expressed as a difference between a global partial sum and two per-card
//! partial sums — giving one f32 subtract per suit per combo instead of 1325
//! compatibility checks.
//!
//! Output convention: the returned per-combo values are in units of
//! "fraction of pot that player X wins over player Y", i.e. each entry lies in
//! `[-1, 1]`. Callers multiply by the pot size at the terminal to get chips.
//! Combos that conflict with the board produce 0 — they can't be dealt in the
//! first place, so reach probability is always 0 there anyway.

use crate::cards::card::Card;
use crate::eval::evaluator::best_five_of_seven;
use crate::eval::rank::HandValue;
use crate::solver::range::HandRange;

/// Total number of two-card combos (52 choose 2).
pub const N_COMBOS: usize = 1326;

/// Number of cards in a standard deck.
const N_CARDS: usize = 52;

/// Precomputed per-combo hand value on a fixed 5-card board, plus the
/// sort-by-rank permutation needed by the fast evaluator.
#[derive(Clone)]
pub struct ShowdownRanker {
    /// Hand value for each combo, or `i16::MIN` if the combo conflicts with
    /// the board (and thus can never be dealt).
    rank: [i16; N_COMBOS],
    /// Bitmask over the 52 cards for each combo (`1 << card_index | 1 << ...`).
    mask: [u64; N_COMBOS],
    /// `true` if combo `i` conflicts with the board.
    blocked: [bool; N_COMBOS],
    /// `(lo_card_idx, hi_card_idx)` for each combo, precomputed to avoid the
    /// triangular decode in the hot path.
    cards: [(u8, u8); N_COMBOS],
    /// Unblocked combo indices sorted ascending by rank. Ties are adjacent so
    /// the fast evaluator can process a whole rank group together before
    /// rolling its reach into the prefix sums.
    rank_order: Vec<u16>,
}

impl ShowdownRanker {
    /// Build a ranker for the given 5-card board.
    ///
    /// Evaluates all 1326 possible two-card combos once; combos containing a
    /// board card are marked blocked and excluded from `rank_order`.
    pub fn new(board: &[Card; 5]) -> Self {
        let board_mask: u64 = board.iter().fold(0u64, |m, c| m | (1u64 << c.index()));

        let mut rank = [0i16; N_COMBOS];
        let mut mask = [0u64; N_COMBOS];
        let mut blocked = [false; N_COMBOS];
        let mut cards = [(0u8, 0u8); N_COMBOS];

        for combo_idx in 0..N_COMBOS as u16 {
            let (c1, c2) = HandRange::cards_from_index(combo_idx);
            let (lo, hi) = if c1.index() <= c2.index() {
                (c1.index(), c2.index())
            } else {
                (c2.index(), c1.index())
            };
            cards[combo_idx as usize] = (lo, hi);
            let combo_mask = (1u64 << lo) | (1u64 << hi);
            mask[combo_idx as usize] = combo_mask;

            if combo_mask & board_mask != 0 {
                blocked[combo_idx as usize] = true;
                rank[combo_idx as usize] = i16::MIN;
                continue;
            }

            let seven = [c1, c2, board[0], board[1], board[2], board[3], board[4]];
            let hv: HandValue = best_five_of_seven(&seven);
            rank[combo_idx as usize] = hv.0 as i16;
        }

        // Sort unblocked combos by rank ascending.
        let mut rank_order: Vec<u16> = (0..N_COMBOS as u16)
            .filter(|&i| !blocked[i as usize])
            .collect();
        rank_order.sort_by_key(|&i| rank[i as usize]);

        Self {
            rank,
            mask,
            blocked,
            cards,
            rank_order,
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
    /// Naive O(N²) pairwise reference — kept `pub` so tests and callers that
    /// want the simplest implementation can use it. [`terminal_ev`] is the hot
    /// path and dispatches to the fast version.
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
                let outcome = (r_i > r_j) as i32 - (r_i < r_j) as i32;
                let reach_j = reach_p1[j];
                acc += outcome as f32 * reach_j;
                ev_p1[j] -= outcome as f32 * reach_i;
            }
            ev_p0[i] = acc;
        }

        (ev_p0, ev_p1)
    }

    /// O(N·52) terminal EV evaluator.
    ///
    /// Walks combos in rank order. Maintains the global reach prefix sum
    /// `s_lt` (all ranks strictly below the current rank group) and the
    /// 52-entry per-card prefix `s_lt_c[c]` (reach of combos containing card
    /// `c` at ranks below the current group). Combos in the current group get
    /// their `wins` and `losses` computed from those sums by inclusion-
    /// exclusion on the two cards they hold — one add and two subtracts per
    /// side — and then their own reach is rolled into the prefixes so the
    /// next group sees them.
    ///
    /// Per-iteration cost: one 1326-entry radix-like walk for each direction,
    /// with 52 f32 adds per rank group for the per-card updates. Everything
    /// else is a handful of ops per combo.
    pub fn terminal_ev(
        &self,
        reach_p0: &[f32; N_COMBOS],
        reach_p1: &[f32; N_COMBOS],
    ) -> (Box<[f32; N_COMBOS]>, Box<[f32; N_COMBOS]>) {
        let mut ev_p0 = Box::new([0.0f32; N_COMBOS]);
        let mut ev_p1 = Box::new([0.0f32; N_COMBOS]);

        self.fill_ev_one_side(reach_p1, &mut ev_p0);
        self.fill_ev_one_side(reach_p0, &mut ev_p1);

        (ev_p0, ev_p1)
    }

    /// Compute ev[i] for every unblocked combo i, where ev[i] is the fraction
    /// of pot the holder of i wins against an opponent drawing from `opp_reach`.
    ///
    /// Used for both sides of [`terminal_ev`] — once with `opp_reach = reach_p1`
    /// (fills `ev_p0`), and once with `opp_reach = reach_p0` (fills `ev_p1`).
    fn fill_ev_one_side(&self, opp_reach: &[f32; N_COMBOS], ev: &mut [f32; N_COMBOS]) {
        // Total opponent reach (across unblocked combos) and per-card totals.
        let mut s_total = 0.0f32;
        let mut s_total_c = [0.0f32; N_CARDS];
        for &idx in &self.rank_order {
            let r = opp_reach[idx as usize];
            if r == 0.0 {
                continue;
            }
            let (a, b) = self.cards[idx as usize];
            s_total += r;
            s_total_c[a as usize] += r;
            s_total_c[b as usize] += r;
        }

        // Running prefix sums over combos with rank strictly below the
        // current rank group.
        let mut s_lt = 0.0f32;
        let mut s_lt_c = [0.0f32; N_CARDS];

        let order = &self.rank_order;
        let mut start = 0;
        while start < order.len() {
            let r = self.rank[order[start] as usize];

            // Find the end of this rank group (exclusive).
            let mut end = start + 1;
            while end < order.len() && self.rank[order[end] as usize] == r {
                end += 1;
            }

            // Pass 1: compute within-group reach sums (for losses computation).
            let mut s_eq = 0.0f32;
            let mut s_eq_c = [0.0f32; N_CARDS];
            for &idx in &order[start..end] {
                let reach = opp_reach[idx as usize];
                if reach == 0.0 {
                    continue;
                }
                let (a, b) = self.cards[idx as usize];
                s_eq += reach;
                s_eq_c[a as usize] += reach;
                s_eq_c[b as usize] += reach;
            }

            // Pass 2: fill wins − losses for each combo in the group.
            // wins   = s_lt  - s_lt_c[a]  - s_lt_c[b]
            // losses = s_gt  - s_gt_c[a]  - s_gt_c[b]
            //        where s_gt   = s_total   - s_lt   - s_eq
            //              s_gt_c = s_total_c - s_lt_c - s_eq_c
            for &idx in &order[start..end] {
                let (a, b) = self.cards[idx as usize];
                let wins = s_lt - s_lt_c[a as usize] - s_lt_c[b as usize];
                let s_gt = s_total - s_lt - s_eq;
                let s_gt_a = s_total_c[a as usize] - s_lt_c[a as usize] - s_eq_c[a as usize];
                let s_gt_b = s_total_c[b as usize] - s_lt_c[b as usize] - s_eq_c[b as usize];
                let losses = s_gt - s_gt_a - s_gt_b;
                ev[idx as usize] = wins - losses;
            }

            // Roll this group's reach into the prefix sums.
            s_lt += s_eq;
            for c in 0..N_CARDS {
                s_lt_c[c] += s_eq_c[c];
            }

            start = end;
        }
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
    fn rank_order_is_sorted_and_excludes_blocked() {
        let board = test_board();
        let ranker = ShowdownRanker::new(&board);
        // Ascending ranks.
        for w in ranker.rank_order.windows(2) {
            let r0 = ranker.rank[w[0] as usize];
            let r1 = ranker.rank[w[1] as usize];
            assert!(r0 <= r1, "rank_order not ascending at {r0} vs {r1}");
        }
        // Length matches number of unblocked combos, and every entry is unblocked.
        let expected = (0..N_COMBOS).filter(|&i| !ranker.blocked[i]).count();
        assert_eq!(ranker.rank_order.len(), expected);
        for &idx in &ranker.rank_order {
            assert!(!ranker.blocked[idx as usize]);
        }
    }

    #[test]
    fn strong_hand_beats_weak_hand() {
        let board = test_board();
        let ranker = ShowdownRanker::new(&board);
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
        let board = test_board();
        let ranker = ShowdownRanker::new(&board);
        let mut reach_p0 = [0.0f32; N_COMBOS];
        let mut reach_p1 = [0.0f32; N_COMBOS];
        for idx in [10u16, 42, 100, 200, 400, 800, 1200] {
            if !ranker.is_blocked(idx) {
                reach_p0[idx as usize] = 1.0;
                reach_p1[idx as usize] = 1.0;
            }
        }
        let (ev_p0, ev_p1) = ranker.terminal_ev_naive(&reach_p0, &reach_p1);
        let ev0: f32 = ev_p0.iter().zip(reach_p0.iter()).map(|(e, r)| e * r).sum();
        let ev1: f32 = ev_p1.iter().zip(reach_p1.iter()).map(|(e, r)| e * r).sum();
        assert!(
            (ev0 + ev1).abs() < 1e-4,
            "ev0={ev0}, ev1={ev1} — should sum to 0"
        );
    }

    #[test]
    fn terminal_ev_heads_up_single_combo() {
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

    /// Property test: the fast and naive evaluators must agree on arbitrary
    /// reach vectors. This is the single most important correctness check —
    /// the fast path is only useful if it matches the reference.
    ///
    /// Uses a deterministic linear-congruential RNG seeded inside the test so
    /// failures reproduce, without pulling a dev-dep for this alone.
    #[test]
    fn fast_matches_naive_on_random_reaches() {
        let board = test_board();
        let ranker = ShowdownRanker::new(&board);

        // Run a handful of independent trials with different seeds to catch
        // edge cases (sparse ranges, ranges concentrated at one rank, etc.).
        for seed in [0x1234u64, 0xdeadbeef, 0xf00dbabe, 42, 1] {
            let mut reach_p0 = [0.0f32; N_COMBOS];
            let mut reach_p1 = [0.0f32; N_COMBOS];
            let mut state = seed;
            for i in 0..N_COMBOS {
                // Simple LCG: stable, deterministic, fine for test ranges.
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                let v0 = ((state >> 40) & 0xffff) as f32 / 65535.0;
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                let v1 = ((state >> 40) & 0xffff) as f32 / 65535.0;
                if !ranker.is_blocked(i as u16) {
                    // Sparsify: zero out ~30% so we exercise the early-exit in
                    // the fast path.
                    if (state >> 16) & 0xf < 5 {
                        reach_p0[i] = 0.0;
                    } else {
                        reach_p0[i] = v0;
                    }
                    if (state >> 20) & 0xf < 5 {
                        reach_p1[i] = 0.0;
                    } else {
                        reach_p1[i] = v1;
                    }
                }
            }

            let (fast_p0, fast_p1) = ranker.terminal_ev(&reach_p0, &reach_p1);
            let (slow_p0, slow_p1) = ranker.terminal_ev_naive(&reach_p0, &reach_p1);

            // Tolerance is chosen so float summation order differences don't
            // trigger spurious failures. The fast path sums 52 per-card
            // contributions; the naive path sums 1325 terms per combo. The two
            // orders can diverge by a few ULPs.
            for i in 0..N_COMBOS {
                let df = (fast_p0[i] - slow_p0[i]).abs();
                assert!(
                    df < 1e-3,
                    "ev_p0 mismatch at combo {i}, seed {seed:#x}: fast={} naive={}",
                    fast_p0[i],
                    slow_p0[i]
                );
                let df = (fast_p1[i] - slow_p1[i]).abs();
                assert!(
                    df < 1e-3,
                    "ev_p1 mismatch at combo {i}, seed {seed:#x}: fast={} naive={}",
                    fast_p1[i],
                    slow_p1[i]
                );
            }
        }
    }

    #[test]
    fn fast_heads_up_matches_naive() {
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
        let (fast_p0, fast_p1) = ranker.terminal_ev(&reach_p0, &reach_p1);
        assert!(
            (fast_p0[aa as usize] - 1.0).abs() < 1e-6,
            "fast AA should win"
        );
        assert!(
            (fast_p1[three_three as usize] + 1.0).abs() < 1e-6,
            "fast 33 should lose"
        );
    }
}
