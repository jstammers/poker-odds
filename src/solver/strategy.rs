use serde::{Deserialize, Serialize};

use crate::solver::action::Action;
use crate::solver::game_tree::{GameTree, GameTreeNode};
use crate::solver::info_set::InfoSetStore;

/// The converged average strategy across all CFR iterations.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StrategyProfile {
    /// Probability of each action at each info set (flat layout).
    pub action_probs: Vec<f32>,
    /// Starting offset for each info set.
    pub offsets: Vec<u32>,
    /// Number of actions at each info set.
    pub num_actions: Vec<u8>,
    /// Action labels for each info set.
    pub action_labels: Vec<Vec<Action>>,
}

impl StrategyProfile {
    /// Extract the average strategy from the solver's cumulative strategy sums.
    pub fn from_store(store: &InfoSetStore, tree: &GameTree) -> Self {
        let mut action_probs = Vec::with_capacity(store.strategy_sum.len());
        let mut action_labels: Vec<Vec<Action>> = vec![Vec::new(); tree.num_info_sets as usize];

        // Collect action labels from tree nodes
        for node in &tree.nodes {
            if let GameTreeNode::Decision {
                actions,
                info_set_idx,
                ..
            } = node
            {
                if action_labels[*info_set_idx as usize].is_empty() {
                    action_labels[*info_set_idx as usize] = actions.clone();
                }
            }
        }

        // Normalize strategy sums to probabilities
        for idx in 0..store.offsets.len() {
            let avg = store.average_strategy(idx as u32);
            action_probs.extend_from_slice(&avg);
        }

        Self {
            action_probs,
            offsets: store.offsets.clone(),
            num_actions: store.num_actions.clone(),
            action_labels,
        }
    }

    /// Get the strategy (action probabilities) at an info set.
    pub fn strategy_at(&self, info_set_idx: u32) -> &[f32] {
        let offset = self.offsets[info_set_idx as usize] as usize;
        let n = self.num_actions[info_set_idx as usize] as usize;
        &self.action_probs[offset..offset + n]
    }

    /// Get the action labels at an info set.
    pub fn actions_at(&self, info_set_idx: u32) -> &[Action] {
        &self.action_labels[info_set_idx as usize]
    }
}
