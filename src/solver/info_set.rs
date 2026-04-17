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
    #[inline]
    pub fn current_strategy(&self, info_set_idx: u32) -> Vec<f32> {
        let offset = self.offsets[info_set_idx as usize] as usize;
        let n = self.num_actions[info_set_idx as usize] as usize;
        let regrets = &self.regrets[offset..offset + n];

        let positive_sum: f32 = regrets.iter().map(|&r| r.max(0.0)).sum();
        if positive_sum > 0.0 {
            regrets.iter().map(|&r| r.max(0.0) / positive_sum).collect()
        } else {
            vec![1.0 / n as f32; n]
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

    /// Get the average strategy at an info set (the converged Nash equilibrium strategy).
    pub fn average_strategy(&self, info_set_idx: u32) -> Vec<f32> {
        let offset = self.offsets[info_set_idx as usize] as usize;
        let n = self.num_actions[info_set_idx as usize] as usize;
        let sums = &self.strategy_sum[offset..offset + n];

        let total: f32 = sums.iter().sum();
        if total > 0.0 {
            sums.iter().map(|&s| s / total).collect()
        } else {
            vec![1.0 / n as f32; n]
        }
    }
}
