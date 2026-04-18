use rayon::prelude::*;

/// Below this total flat-array length, the bulk DCFR discount operations run
/// serially. Rayon's work-splitting has non-trivial overhead, and the discount
/// pass is memory-bandwidth-bound — parallelism only pays off on stores that
/// are much larger than L3 cache. On a 16-core box the empirical crossover
/// lives near 1–2M f32 elements; this threshold is chosen to keep Kuhn, Leduc,
/// and typical river postflop trees on the serial path while still firing on
/// million-info-set solves.
const PARALLEL_DISCOUNT_THRESHOLD: usize = 2_000_000;

/// Storage for per-information-set cumulative regrets and strategy sums.
///
/// Uses flat parallel arrays for cache efficiency. Each info set's data is stored
/// at a contiguous slice starting at `offsets[info_set_idx]` with length
/// `num_actions[info_set_idx]`.
pub struct InfoSetStore {
    /// Cumulative regrets for each (info_set, action) pair.
    pub regrets: Vec<f32>,
    /// Cumulative strategy weights for computing the average strategy.
    pub strategy_sum: Vec<f32>,
    /// Starting offset in the flat arrays for each info set.
    pub offsets: Vec<u32>,
    /// Number of actions available at each info set.
    pub num_actions: Vec<u8>,
}

impl InfoSetStore {
    /// Create a new store sized for the given info set action counts.
    pub fn new(actions_per_info_set: &[u8]) -> Self {
        let mut offsets = Vec::with_capacity(actions_per_info_set.len());
        let mut total = 0u32;
        for &n in actions_per_info_set {
            offsets.push(total);
            total += n as u32;
        }
        Self {
            regrets: vec![0.0; total as usize],
            strategy_sum: vec![0.0; total as usize],
            offsets,
            num_actions: actions_per_info_set.to_vec(),
        }
    }

    /// Compute the current strategy at an info set via regret matching.
    ///
    /// Positive regrets are normalized to a probability distribution.
    /// If all regrets are non-positive, returns uniform random.
    pub fn current_strategy(&self, info_set_idx: u32) -> Vec<f32> {
        let n = self.num_actions[info_set_idx as usize] as usize;
        let mut out = vec![0.0; n];
        self.current_strategy_into(info_set_idx, &mut out);
        out
    }

    /// Same as [`current_strategy`] but writes into a caller-provided buffer.
    ///
    /// `out.len()` must equal the number of actions at the info set. This avoids
    /// the per-call `Vec<f32>` allocation on the CFR hot path.
    #[inline]
    pub fn current_strategy_into(&self, info_set_idx: u32, out: &mut [f32]) {
        let offset = self.offsets[info_set_idx as usize] as usize;
        let n = self.num_actions[info_set_idx as usize] as usize;
        debug_assert_eq!(out.len(), n);
        let regrets = &self.regrets[offset..offset + n];

        let mut positive_sum = 0.0f32;
        for &r in regrets {
            if r > 0.0 {
                positive_sum += r;
            }
        }
        if positive_sum > 0.0 {
            let inv = 1.0 / positive_sum;
            for (o, &r) in out.iter_mut().zip(regrets.iter()) {
                *o = r.max(0.0) * inv;
            }
        } else {
            let u = 1.0 / n as f32;
            for o in out.iter_mut() {
                *o = u;
            }
        }
    }

    /// Add a regret value for a specific action at an info set.
    #[inline]
    pub fn add_regret(&mut self, info_set_idx: u32, action_idx: usize, regret: f32) {
        let offset = self.offsets[info_set_idx as usize] as usize;
        self.regrets[offset + action_idx] += regret;
    }

    /// Accumulate strategy weights for the average strategy computation.
    #[inline]
    pub fn accumulate_strategy(&mut self, info_set_idx: u32, strategy: &[f32], reach_prob: f32) {
        let offset = self.offsets[info_set_idx as usize] as usize;
        let n = self.num_actions[info_set_idx as usize] as usize;
        for (sum, &s) in self.strategy_sum[offset..offset + n]
            .iter_mut()
            .zip(strategy.iter())
        {
            *sum += reach_prob * s;
        }
    }

    /// Combined CFR update for one info set: add regrets (scaled by `opp_reach`),
    /// optionally clip negatives (CFR+), and accumulate strategy weights (scaled
    /// by `my_reach`) — all under a single offset lookup.
    ///
    /// Replaces three separate hot-path calls (`add_regret` in a loop,
    /// `clip_negative_regrets`, `accumulate_strategy`) with one method so the
    /// offset/length lookups happen once per decision-node visit instead of
    /// 2 + n_actions times.
    ///
    /// `action_values.len()` and `strategy.len()` must equal the number of
    /// actions at the info set.
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn update_regrets_and_strategy(
        &mut self,
        info_set_idx: u32,
        action_values: &[f32],
        node_value: f32,
        strategy: &[f32],
        opp_reach: f32,
        my_reach: f32,
        clip_negative: bool,
    ) {
        let offset = self.offsets[info_set_idx as usize] as usize;
        let n = self.num_actions[info_set_idx as usize] as usize;
        debug_assert_eq!(action_values.len(), n);
        debug_assert_eq!(strategy.len(), n);

        let regrets = &mut self.regrets[offset..offset + n];
        if clip_negative {
            for (r, &av) in regrets.iter_mut().zip(action_values.iter()) {
                let updated = *r + opp_reach * (av - node_value);
                *r = updated.max(0.0);
            }
        } else {
            for (r, &av) in regrets.iter_mut().zip(action_values.iter()) {
                *r += opp_reach * (av - node_value);
            }
        }

        let sums = &mut self.strategy_sum[offset..offset + n];
        for (sum, &s) in sums.iter_mut().zip(strategy.iter()) {
            *sum += my_reach * s;
        }
    }

    /// CFR+: clip all negative regrets to zero for an info set.
    #[inline]
    pub fn clip_negative_regrets(&mut self, info_set_idx: u32) {
        let offset = self.offsets[info_set_idx as usize] as usize;
        let n = self.num_actions[info_set_idx as usize] as usize;
        for i in 0..n {
            if self.regrets[offset + i] < 0.0 {
                self.regrets[offset + i] = 0.0;
            }
        }
    }

    /// DCFR: apply discount factors to regrets at an info set.
    #[inline]
    pub fn discount_regrets(
        &mut self,
        info_set_idx: u32,
        positive_discount: f32,
        negative_discount: f32,
    ) {
        let offset = self.offsets[info_set_idx as usize] as usize;
        let n = self.num_actions[info_set_idx as usize] as usize;
        for i in 0..n {
            let r = &mut self.regrets[offset + i];
            if *r > 0.0 {
                *r *= positive_discount;
            } else {
                *r *= negative_discount;
            }
        }
    }

    /// DCFR: apply discount factor to strategy sums at an info set.
    #[inline]
    pub fn discount_strategy_sum(&mut self, info_set_idx: u32, discount: f32) {
        let offset = self.offsets[info_set_idx as usize] as usize;
        let n = self.num_actions[info_set_idx as usize] as usize;
        for i in 0..n {
            self.strategy_sum[offset + i] *= discount;
        }
    }

    /// DCFR: apply discount factors to all regrets across every info set in one pass.
    ///
    /// The factors don't depend on info-set identity, so we can treat the entire
    /// flat array as one sequence and parallelize with rayon. For small stores
    /// we fall back to a serial loop to avoid rayon's per-call overhead.
    pub fn discount_regrets_all(&mut self, positive_discount: f32, negative_discount: f32) {
        let apply = |r: &mut f32| {
            if *r > 0.0 {
                *r *= positive_discount;
            } else {
                *r *= negative_discount;
            }
        };
        if self.regrets.len() >= PARALLEL_DISCOUNT_THRESHOLD {
            self.regrets.par_iter_mut().for_each(apply);
        } else {
            self.regrets.iter_mut().for_each(apply);
        }
    }

    /// DCFR: apply a discount factor to every strategy sum in one pass.
    pub fn discount_strategy_sum_all(&mut self, discount: f32) {
        if self.strategy_sum.len() >= PARALLEL_DISCOUNT_THRESHOLD {
            self.strategy_sum
                .par_iter_mut()
                .for_each(|s| *s *= discount);
        } else {
            self.strategy_sum.iter_mut().for_each(|s| *s *= discount);
        }
    }

    /// Get the average strategy at an info set (the converged Nash equilibrium strategy).
    pub fn average_strategy(&self, info_set_idx: u32) -> Vec<f32> {
        let n = self.num_actions[info_set_idx as usize] as usize;
        let mut out = vec![0.0; n];
        self.average_strategy_into(info_set_idx, &mut out);
        out
    }

    /// Same as [`average_strategy`] but writes into a caller-provided buffer.
    ///
    /// `out.len()` must equal the number of actions at the info set. This avoids
    /// per-call `Vec<f32>` allocation in exploitability/best-response traversal.
    #[inline]
    pub fn average_strategy_into(&self, info_set_idx: u32, out: &mut [f32]) {
        let offset = self.offsets[info_set_idx as usize] as usize;
        let n = self.num_actions[info_set_idx as usize] as usize;
        debug_assert_eq!(out.len(), n);
        let sums = &self.strategy_sum[offset..offset + n];

        let total: f32 = sums.iter().sum();
        if total > 0.0 {
            let inv = 1.0 / total;
            for (o, &s) in out.iter_mut().zip(sums.iter()) {
                *o = s * inv;
            }
        } else {
            let u = 1.0 / n as f32;
            for o in out.iter_mut() {
                *o = u;
            }
        }
    }
}
