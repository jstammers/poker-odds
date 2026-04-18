use serde::{Deserialize, Serialize};

use crate::solver::game_tree::{GameTree, GameTreeNode, NodeIndex};
use crate::solver::info_set::InfoSetStore;
use crate::solver::strategy::StrategyProfile;

/// Maximum number of actions expected at any decision node.
/// Used to size stack-allocated scratch buffers on the CFR hot path.
const MAX_ACTIONS: usize = 16;

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
        let use_cfr_plus = matches!(self.config.algorithm, CfrAlgorithm::CfrPlus);
        let v0 = cfr_traverse(
            &self.tree,
            &mut self.store,
            use_cfr_plus,
            self.tree.root,
            1.0,
            1.0,
            0,
        );
        let _v1 = cfr_traverse(
            &self.tree,
            &mut self.store,
            use_cfr_plus,
            self.tree.root,
            1.0,
            1.0,
            1,
        );

        if matches!(self.config.algorithm, CfrAlgorithm::Dcfr) {
            self.apply_dcfr_discounts(iteration_number + 1);
        }

        v0 as f64
    }

    /// Run CFR for the configured number of iterations.
    /// Returns the game value (expected payoff for player 0).
    pub fn solve(&mut self) -> f64 {
        let mut game_value_sum = 0.0f64;
        let use_cfr_plus = matches!(self.config.algorithm, CfrAlgorithm::CfrPlus);

        for t in 0..self.config.iterations {
            // Traverse for both players (alternating updates)
            let v0 = cfr_traverse(
                &self.tree,
                &mut self.store,
                use_cfr_plus,
                self.tree.root,
                1.0,
                1.0,
                0,
            );
            let _v1 = cfr_traverse(
                &self.tree,
                &mut self.store,
                use_cfr_plus,
                self.tree.root,
                1.0,
                1.0,
                1,
            );
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

    /// Apply DCFR discount factors to all info sets after iteration `t`.
    ///
    /// The factors are uniform across info sets, so this is just an elementwise
    /// sweep over the flat regret and strategy-sum arrays — parallelized with
    /// rayon for large trees via `discount_*_all`.
    fn apply_dcfr_discounts(&mut self, t: u32) {
        let t = t as f64;
        let alpha = self.config.dcfr_alpha;
        let beta = self.config.dcfr_beta;
        let gamma = self.config.dcfr_gamma;

        let pos_discount = (t.powf(alpha) / (t.powf(alpha) + 1.0)) as f32;
        let neg_discount = (t.powf(beta) / (t.powf(beta) + 1.0)) as f32;
        let strat_discount = (t / (t + 1.0)).powf(gamma) as f32;

        self.store.discount_regrets_all(pos_discount, neg_discount);
        self.store.discount_strategy_sum_all(strat_discount);
    }
}

/// Core CFR traversal. Returns the counterfactual value for `traversing_player`
/// at `node_idx`.
///
/// Split out of `CfrSolver` as a free function so the tree and info-set store
/// can be borrowed disjointly during recursion — this lets us match on
/// `&tree.nodes[idx]` without cloning the node's `actions`/`children` vectors.
fn cfr_traverse(
    tree: &GameTree,
    store: &mut InfoSetStore,
    use_cfr_plus: bool,
    node_idx: NodeIndex,
    reach_p0: f32,
    reach_p1: f32,
    traversing_player: u8,
) -> f32 {
    match &tree.nodes[node_idx as usize] {
        GameTreeNode::Terminal { payoff_p0 } => {
            if traversing_player == 0 {
                *payoff_p0
            } else {
                -*payoff_p0
            }
        }
        GameTreeNode::Chance { children } => {
            let weight = 1.0 / children.len() as f32;
            let mut sum = 0.0f32;
            for &(_, child) in children {
                sum += weight
                    * cfr_traverse(
                        tree,
                        store,
                        use_cfr_plus,
                        child,
                        reach_p0,
                        reach_p1,
                        traversing_player,
                    );
            }
            sum
        }
        GameTreeNode::Decision {
            player,
            children,
            info_set_idx,
            ..
        } => {
            let player = *player;
            let info_set_idx = *info_set_idx;
            let n_actions = children.len();
            debug_assert!(n_actions <= MAX_ACTIONS);

            let mut strategy = [0.0f32; MAX_ACTIONS];
            store.current_strategy_into(info_set_idx, &mut strategy[..n_actions]);

            let mut action_values = [0.0f32; MAX_ACTIONS];
            let mut node_value = 0.0f32;

            for i in 0..n_actions {
                let child = children[i];
                let s = strategy[i];
                let (new_reach_p0, new_reach_p1) = if player == 0 {
                    (reach_p0 * s, reach_p1)
                } else {
                    (reach_p0, reach_p1 * s)
                };
                let v = cfr_traverse(
                    tree,
                    store,
                    use_cfr_plus,
                    child,
                    new_reach_p0,
                    new_reach_p1,
                    traversing_player,
                );
                action_values[i] = v;
                node_value += s * v;
            }

            if player == traversing_player {
                let opp_reach = if player == 0 { reach_p1 } else { reach_p0 };
                let my_reach = if player == 0 { reach_p0 } else { reach_p1 };

                // Combined regret update + optional CFR+ clip + strategy
                // accumulation under a single offset lookup.
                store.update_regrets_and_strategy(
                    info_set_idx,
                    &action_values[..n_actions],
                    node_value,
                    &strategy[..n_actions],
                    opp_reach,
                    my_reach,
                    use_cfr_plus,
                );
            }

            node_value
        }
    }
}
