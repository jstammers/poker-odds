use serde::{Deserialize, Serialize};

use crate::solver::game_tree::{GameTree, GameTreeNode, NodeIndex};
use crate::solver::info_set::InfoSetStore;
use crate::solver::strategy::StrategyProfile;

/// Which CFR variant to use.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum CfrAlgorithm {
    /// CFR+: clips negative regrets to zero after each update.
    CfrPlus,
    /// Discounted CFR: discounts older regrets and strategy contributions.
    Dcfr,
}

/// Configuration for the CFR solver.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SolverConfig {
    pub algorithm: CfrAlgorithm,
    pub iterations: u32,
    /// DCFR discount parameters.
    pub dcfr_alpha: f64,
    pub dcfr_beta: f64,
    pub dcfr_gamma: f64,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            algorithm: CfrAlgorithm::CfrPlus,
            iterations: 10_000,
            dcfr_alpha: 1.5,
            dcfr_beta: 0.5,
            dcfr_gamma: 2.0,
        }
    }
}

/// The CFR solver. Owns the game tree and info set storage.
pub struct CfrSolver {
    pub tree: GameTree,
    pub store: InfoSetStore,
    pub config: SolverConfig,
}

impl CfrSolver {
    /// Create a new solver for the given game tree and configuration.
    pub fn new(tree: GameTree, config: SolverConfig) -> Self {
        let store = InfoSetStore::new(&tree.actions_per_info_set);
        Self {
            tree,
            store,
            config,
        }
    }

    /// Run a single CFR iteration.
    /// Returns the game value from this iteration (player 0's expected payoff).
    /// Call this in a loop for incremental progress reporting.
    pub fn run_iteration(&mut self, iteration_number: u32) -> f64 {
        let v0 = self.cfr_traverse(self.tree.root, 1.0, 1.0, 0);
        let _v1 = self.cfr_traverse(self.tree.root, 1.0, 1.0, 1);

        if matches!(self.config.algorithm, CfrAlgorithm::Dcfr) {
            self.apply_dcfr_discounts(iteration_number + 1);
        }

        v0 as f64
    }

    /// Run CFR for the configured number of iterations.
    /// Returns the game value (expected payoff for player 0).
    pub fn solve(&mut self) -> f64 {
        let mut game_value_sum = 0.0f64;

        for t in 0..self.config.iterations {
            // Traverse for both players (alternating updates)
            let v0 = self.cfr_traverse(self.tree.root, 1.0, 1.0, 0);
            let _v1 = self.cfr_traverse(self.tree.root, 1.0, 1.0, 1);
            game_value_sum += v0 as f64;

            // Apply DCFR discounting after each iteration
            if matches!(self.config.algorithm, CfrAlgorithm::Dcfr) {
                self.apply_dcfr_discounts(t + 1);
            }
        }

        game_value_sum / self.config.iterations as f64
    }

    /// Extract the converged average strategy after solving.
    pub fn average_strategy(&self) -> StrategyProfile {
        StrategyProfile::from_store(&self.store, &self.tree)
    }

    /// Core CFR traversal. Returns the counterfactual value for `traversing_player` at this node.
    fn cfr_traverse(
        &mut self,
        node_idx: NodeIndex,
        reach_p0: f32,
        reach_p1: f32,
        traversing_player: u8,
    ) -> f32 {
        match self.tree.nodes[node_idx as usize].clone() {
            GameTreeNode::Terminal { payoff_p0 } => {
                if traversing_player == 0 {
                    payoff_p0
                } else {
                    -payoff_p0
                }
            }
            GameTreeNode::Chance { ref children } => {
                let children = children.clone();
                let weight = 1.0 / children.len() as f32;
                children
                    .iter()
                    .map(|(_, child)| {
                        weight * self.cfr_traverse(*child, reach_p0, reach_p1, traversing_player)
                    })
                    .sum()
            }
            GameTreeNode::Decision {
                player,
                ref actions,
                ref children,
                info_set_idx,
            } => {
                let n_actions = actions.len();
                let children: Vec<NodeIndex> = children.clone();
                let strategy = self.store.current_strategy(info_set_idx);

                let mut action_values = vec![0.0f32; n_actions];
                let mut node_value = 0.0f32;

                for i in 0..n_actions {
                    let (new_reach_p0, new_reach_p1) = if player == 0 {
                        (reach_p0 * strategy[i], reach_p1)
                    } else {
                        (reach_p0, reach_p1 * strategy[i])
                    };
                    action_values[i] = self.cfr_traverse(
                        children[i],
                        new_reach_p0,
                        new_reach_p1,
                        traversing_player,
                    );
                    node_value += strategy[i] * action_values[i];
                }

                if player == traversing_player {
                    let opp_reach = if player == 0 { reach_p1 } else { reach_p0 };
                    let my_reach = if player == 0 { reach_p0 } else { reach_p1 };

                    // Update regrets
                    for i in 0..n_actions {
                        let regret = action_values[i] - node_value;
                        self.store.add_regret(info_set_idx, i, opp_reach * regret);
                    }

                    // CFR+: clip negative regrets
                    if matches!(self.config.algorithm, CfrAlgorithm::CfrPlus) {
                        self.store.clip_negative_regrets(info_set_idx);
                    }

                    // Accumulate strategy for averaging
                    self.store
                        .accumulate_strategy(info_set_idx, &strategy, my_reach);
                }

                node_value
            }
        }
    }

    /// Apply DCFR discount factors to all info sets after iteration `t`.
    fn apply_dcfr_discounts(&mut self, t: u32) {
        let t = t as f64;
        let alpha = self.config.dcfr_alpha;
        let beta = self.config.dcfr_beta;
        let gamma = self.config.dcfr_gamma;

        let pos_discount = (t.powf(alpha) / (t.powf(alpha) + 1.0)) as f32;
        let neg_discount = (t.powf(beta) / (t.powf(beta) + 1.0)) as f32;
        let strat_discount = (t / (t + 1.0)).powf(gamma) as f32;

        for idx in 0..self.tree.num_info_sets {
            self.store.discount_regrets(idx, pos_discount, neg_discount);
            self.store.discount_strategy_sum(idx, strat_discount);
        }
    }
}
