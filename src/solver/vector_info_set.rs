//! Per-combo info-set storage for vector CFR.
//!
//! The scalar [`crate::solver::info_set::InfoSetStore`] stores one regret and
//! one strategy-sum value per `(info_set, action)` pair. In vector CFR every
//! information set has 1326 *copies* of those — one per 2-card combo a player
//! might be holding — and the traversal carries a per-combo reach vector.
//!
//! This module provides [`VectorInfoSetStore`], the vector analogue: each
//! `(info_set, action)` pair holds a `[f32; 1326]`-shaped slab of regrets and
//! strategy sums. Layout within one info set is **combo-major**, action-minor:
//!
//! ```text
//!   slab[combo * n_actions + action]
//! ```
//!
//! The reason is the CFR hot path — regret matching normalises a single
//! combo's actions into a probability distribution, so reading all `n_actions`
//! entries for a given combo should be a unit-stride access. Similarly the
//! update step iterates per combo and touches all its actions.
//!
//! Across info sets the flat layout follows the same `offsets[idx] ..
//! offsets[idx] + N_COMBOS * num_actions[idx]` slice convention as the scalar
//! store, so allocation accounting and downstream views stay simple.
//!
//! **Memory footprint.** With `N_COMBOS = 1326`, a 4-action info set occupies
//! `4 * 1326 * 4 = 21,216 bytes` of regrets plus the same for strategy sums.
//! A 10k-info-set river tree with average 4 actions therefore needs ~420 MB
//! total. For river subgames this is fine; for larger trees (full flop
//! solves), range bucketing or compressed storage will be the follow-up.

use crate::solver::showdown::N_COMBOS;

/// Per-combo info-set store for vector CFR.
///
/// See the module docs for layout and memory notes.
pub struct VectorInfoSetStore {
    /// Cumulative regrets. Total length `sum(n_actions[i] * N_COMBOS)`.
    pub regrets: Vec<f32>,
    /// Cumulative strategy weights for the average strategy. Same shape as
    /// `regrets`.
    pub strategy_sum: Vec<f32>,
    /// Start offset in the flat arrays for each info set.
    pub offsets: Vec<u32>,
    /// Number of actions at each info set.
    pub num_actions: Vec<u8>,
}

impl VectorInfoSetStore {
    /// Create a new store sized for the given info set action counts.
    ///
    /// Each info set consumes `n_actions * N_COMBOS` slots in the flat
    /// regret and strategy-sum arrays.
    pub fn new(actions_per_info_set: &[u8]) -> Self {
        let mut offsets = Vec::with_capacity(actions_per_info_set.len());
        let mut total: u64 = 0;
        for &n in actions_per_info_set {
            // Cast up to u64 during accumulation so we catch overflow at
            // allocation time rather than silently wrapping to u32.
            offsets.push(total as u32);
            total += n as u64 * N_COMBOS as u64;
        }
        assert!(
            total <= u32::MAX as u64,
            "vector info-set store too large ({total} slots > u32::MAX); \
             consider range bucketing"
        );
        let total = total as usize;
        Self {
            regrets: vec![0.0; total],
            strategy_sum: vec![0.0; total],
            offsets,
            num_actions: actions_per_info_set.to_vec(),
        }
    }

    /// Total number of f32 slots consumed by regrets (and by strategy_sum).
    #[inline]
    pub fn total_slots(&self) -> usize {
        self.regrets.len()
    }

    /// Slice the regret slab of one info set. Layout: combo-major.
    #[inline]
    pub fn regrets_of(&self, info_set_idx: u32) -> &[f32] {
        let offset = self.offsets[info_set_idx as usize] as usize;
        let n = self.num_actions[info_set_idx as usize] as usize;
        &self.regrets[offset..offset + n * N_COMBOS]
    }

    /// Mutable slice of one info set's regret slab. Layout: combo-major.
    #[inline]
    pub fn regrets_of_mut(&mut self, info_set_idx: u32) -> &mut [f32] {
        let offset = self.offsets[info_set_idx as usize] as usize;
        let n = self.num_actions[info_set_idx as usize] as usize;
        &mut self.regrets[offset..offset + n * N_COMBOS]
    }

    /// Slice the strategy-sum slab of one info set. Layout: combo-major.
    #[inline]
    pub fn strategy_sum_of(&self, info_set_idx: u32) -> &[f32] {
        let offset = self.offsets[info_set_idx as usize] as usize;
        let n = self.num_actions[info_set_idx as usize] as usize;
        &self.strategy_sum[offset..offset + n * N_COMBOS]
    }

    /// Regret-match into `out`, one distribution per combo.
    ///
    /// `out` must have length `N_COMBOS * n_actions`, combo-major layout.
    /// For each combo, positive regrets are normalised; if all are
    /// non-positive the distribution falls back to uniform.
    ///
    /// Combos whose strategy is ignored by the caller (e.g. board-blocked
    /// combos) can be left to the uniform fallback — the fact that their
    /// reach is 0 means the strategy they produce never contributes.
    #[inline]
    pub fn current_strategy_into(&self, info_set_idx: u32, out: &mut [f32]) {
        let n = self.num_actions[info_set_idx as usize] as usize;
        debug_assert_eq!(out.len(), N_COMBOS * n);
        let regrets = self.regrets_of(info_set_idx);
        let uniform = 1.0f32 / n as f32;

        for combo in 0..N_COMBOS {
            let base = combo * n;
            let r_slice = &regrets[base..base + n];
            let o_slice = &mut out[base..base + n];

            let mut positive_sum = 0.0f32;
            for &r in r_slice {
                if r > 0.0 {
                    positive_sum += r;
                }
            }
            if positive_sum > 0.0 {
                let inv = 1.0 / positive_sum;
                for (o, &r) in o_slice.iter_mut().zip(r_slice.iter()) {
                    *o = r.max(0.0) * inv;
                }
            } else {
                for o in o_slice.iter_mut() {
                    *o = uniform;
                }
            }
        }
    }

    /// Combined CFR update at one info set: add per-combo regrets (scaled by
    /// per-combo `opp_reach`), optionally clip negatives (CFR+), and
    /// accumulate strategy weights (scaled by per-combo `my_reach`).
    ///
    /// All four combo-major slabs (`action_values`, `strategy`) must have
    /// length `N_COMBOS * n_actions`; the per-combo slices (`node_value`,
    /// `opp_reach`, `my_reach`) must have length `N_COMBOS`.
    ///
    /// Mirrors the scalar `update_regrets_and_strategy` signature so the CFR
    /// traversal code can dispatch uniformly.
    #[allow(clippy::too_many_arguments)]
    pub fn update_regrets_and_strategy(
        &mut self,
        info_set_idx: u32,
        action_values: &[f32],
        node_value: &[f32],
        strategy: &[f32],
        opp_reach: &[f32],
        my_reach: &[f32],
        clip_negative: bool,
    ) {
        let offset = self.offsets[info_set_idx as usize] as usize;
        let n = self.num_actions[info_set_idx as usize] as usize;
        debug_assert_eq!(action_values.len(), N_COMBOS * n);
        debug_assert_eq!(strategy.len(), N_COMBOS * n);
        debug_assert_eq!(node_value.len(), N_COMBOS);
        debug_assert_eq!(opp_reach.len(), N_COMBOS);
        debug_assert_eq!(my_reach.len(), N_COMBOS);

        let regrets = &mut self.regrets[offset..offset + n * N_COMBOS];
        let sums = &mut self.strategy_sum[offset..offset + n * N_COMBOS];

        if clip_negative {
            for combo in 0..N_COMBOS {
                let base = combo * n;
                let orc = opp_reach[combo];
                let mrc = my_reach[combo];
                let nvc = node_value[combo];
                for a in 0..n {
                    let updated = regrets[base + a] + orc * (action_values[base + a] - nvc);
                    regrets[base + a] = updated.max(0.0);
                    sums[base + a] += mrc * strategy[base + a];
                }
            }
        } else {
            for combo in 0..N_COMBOS {
                let base = combo * n;
                let orc = opp_reach[combo];
                let mrc = my_reach[combo];
                let nvc = node_value[combo];
                for a in 0..n {
                    regrets[base + a] += orc * (action_values[base + a] - nvc);
                    sums[base + a] += mrc * strategy[base + a];
                }
            }
        }
    }

    /// Normalise the strategy sum into `out`, one distribution per combo.
    ///
    /// `out` must have length `N_COMBOS * n_actions`. Uniform fallback when a
    /// combo's total strategy sum is 0 (e.g. never-visited combos).
    #[inline]
    pub fn average_strategy_into(&self, info_set_idx: u32, out: &mut [f32]) {
        let n = self.num_actions[info_set_idx as usize] as usize;
        debug_assert_eq!(out.len(), N_COMBOS * n);
        let sums = self.strategy_sum_of(info_set_idx);
        let uniform = 1.0f32 / n as f32;

        for combo in 0..N_COMBOS {
            let base = combo * n;
            let s_slice = &sums[base..base + n];
            let o_slice = &mut out[base..base + n];
            let total: f32 = s_slice.iter().sum();
            if total > 0.0 {
                let inv = 1.0 / total;
                for (o, &s) in o_slice.iter_mut().zip(s_slice.iter()) {
                    *o = s * inv;
                }
            } else {
                for o in o_slice.iter_mut() {
                    *o = uniform;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_allocates_expected_size() {
        let actions = vec![3u8, 5, 2];
        let store = VectorInfoSetStore::new(&actions);
        let expected = (3 + 5 + 2) * N_COMBOS;
        assert_eq!(store.total_slots(), expected);
        assert_eq!(store.strategy_sum.len(), expected);
        assert_eq!(
            store.offsets,
            vec![0, 3 * N_COMBOS as u32, 8 * N_COMBOS as u32]
        );
        assert_eq!(store.num_actions, vec![3, 5, 2]);
    }

    #[test]
    fn current_strategy_falls_back_to_uniform_on_zero_regrets() {
        let store = VectorInfoSetStore::new(&[4u8]);
        let mut out = vec![0.0f32; 4 * N_COMBOS];
        store.current_strategy_into(0, &mut out);
        for combo in 0..N_COMBOS {
            for a in 0..4 {
                assert!(
                    (out[combo * 4 + a] - 0.25).abs() < 1e-6,
                    "combo {combo} action {a}: expected 0.25, got {}",
                    out[combo * 4 + a]
                );
            }
        }
    }

    #[test]
    fn current_strategy_normalises_positive_regrets() {
        let mut store = VectorInfoSetStore::new(&[3u8]);
        // Hand-set regrets for combo 42: [1.0, 3.0, 0.0]
        let base = 42 * 3;
        store.regrets[base] = 1.0;
        store.regrets[base + 1] = 3.0;
        store.regrets[base + 2] = 0.0;

        let mut out = vec![0.0f32; 3 * N_COMBOS];
        store.current_strategy_into(0, &mut out);

        assert!((out[base] - 0.25).abs() < 1e-6, "action 0");
        assert!((out[base + 1] - 0.75).abs() < 1e-6, "action 1");
        assert!((out[base + 2] - 0.0).abs() < 1e-6, "action 2");

        // A different combo (untouched) should still produce uniform 1/3.
        let other_base = 100 * 3;
        for a in 0..3 {
            assert!(
                (out[other_base + a] - 1.0 / 3.0).abs() < 1e-6,
                "other combo uniform"
            );
        }
    }

    #[test]
    fn current_strategy_clips_negatives() {
        let mut store = VectorInfoSetStore::new(&[3u8]);
        // combo 7: regrets = [-2.0, 1.5, 0.5] → positive sum = 2.0,
        // strategy = [0, 0.75, 0.25].
        let base = 7 * 3;
        store.regrets[base] = -2.0;
        store.regrets[base + 1] = 1.5;
        store.regrets[base + 2] = 0.5;
        let mut out = vec![0.0f32; 3 * N_COMBOS];
        store.current_strategy_into(0, &mut out);
        assert!(out[base].abs() < 1e-6);
        assert!((out[base + 1] - 0.75).abs() < 1e-6);
        assert!((out[base + 2] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn update_round_trip_accumulates() {
        let mut store = VectorInfoSetStore::new(&[2u8]);
        // Uniform strategy for a single combo, node_value = 0.5,
        // action_values = [1.0, 0.0], opp_reach = 1.0, my_reach = 1.0.
        // After one update: regrets = [0.5, -0.5], strategy_sum = [0.5, 0.5].
        // With clip_negative=true, regrets → [0.5, 0.0].
        let n = 2;
        let mut action_values = vec![0.0f32; N_COMBOS * n];
        let mut strategy = vec![0.5f32; N_COMBOS * n];
        let mut node_value = vec![0.5f32; N_COMBOS];
        let mut opp_reach = vec![0.0f32; N_COMBOS];
        let mut my_reach = vec![0.0f32; N_COMBOS];

        // Put data for one specific combo only, zero elsewhere.
        let c = 300;
        action_values[c * n] = 1.0;
        action_values[c * n + 1] = 0.0;
        strategy[c * n] = 0.5;
        strategy[c * n + 1] = 0.5;
        node_value[c] = 0.5;
        opp_reach[c] = 1.0;
        my_reach[c] = 1.0;

        store.update_regrets_and_strategy(
            0,
            &action_values,
            &node_value,
            &strategy,
            &opp_reach,
            &my_reach,
            true,
        );

        assert!((store.regrets[c * n] - 0.5).abs() < 1e-6);
        assert!(store.regrets[c * n + 1].abs() < 1e-6, "clipped");
        assert!((store.strategy_sum[c * n] - 0.5).abs() < 1e-6);
        assert!((store.strategy_sum[c * n + 1] - 0.5).abs() < 1e-6);

        // Combo whose reach was 0 should be untouched.
        let other = 17;
        assert!(store.regrets[other * n].abs() < 1e-6);
        assert!(store.strategy_sum[other * n].abs() < 1e-6);
    }

    #[test]
    fn average_strategy_normalises_strategy_sum() {
        let mut store = VectorInfoSetStore::new(&[3u8]);
        // Seed strategy_sum for combo 5: [2.0, 6.0, 2.0]. Expected average
        // = [0.2, 0.6, 0.2].
        let base = 5 * 3;
        store.strategy_sum[base] = 2.0;
        store.strategy_sum[base + 1] = 6.0;
        store.strategy_sum[base + 2] = 2.0;
        let mut out = vec![0.0f32; 3 * N_COMBOS];
        store.average_strategy_into(0, &mut out);
        assert!((out[base] - 0.2).abs() < 1e-6);
        assert!((out[base + 1] - 0.6).abs() < 1e-6);
        assert!((out[base + 2] - 0.2).abs() < 1e-6);

        // Never-updated combo → uniform.
        let other_base = 200 * 3;
        for a in 0..3 {
            assert!((out[other_base + a] - 1.0 / 3.0).abs() < 1e-6);
        }
    }

    #[test]
    fn clip_negative_off_preserves_negatives() {
        let mut store = VectorInfoSetStore::new(&[2u8]);
        let n = 2;
        let mut action_values = vec![0.0f32; N_COMBOS * n];
        let strategy = vec![0.5f32; N_COMBOS * n];
        let node_value = vec![1.0f32; N_COMBOS];
        let mut opp_reach = vec![0.0f32; N_COMBOS];
        let my_reach = vec![0.0f32; N_COMBOS];

        // combo 10: action_values = [0.0, 0.0], node_value = 1.0
        // regret = 1 * (0 - 1) = -1 for each action.
        action_values[10 * n] = 0.0;
        action_values[10 * n + 1] = 0.0;
        opp_reach[10] = 1.0;

        store.update_regrets_and_strategy(
            0,
            &action_values,
            &node_value,
            &strategy,
            &opp_reach,
            &my_reach,
            false,
        );

        assert!((store.regrets[10 * n] + 1.0).abs() < 1e-6);
        assert!((store.regrets[10 * n + 1] + 1.0).abs() < 1e-6);
    }
}
