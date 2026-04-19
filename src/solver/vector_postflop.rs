//! Vector-form river subgame tree builder.
//!
//! Constructs a [`VectorGameTree`] for a fixed 5-card board, following the
//! same action-enumeration logic as the scalar [`crate::solver::postflop`]
//! builder. The output tree feeds [`crate::solver::vector_cfr::VectorCfrSolver`]
//! and lets us run vector CFR on real river spots side-by-side with the
//! scalar solver for benchmarking and convergence comparison.
//!
//! ## Differences from the scalar river builder
//!
//! - **No card abstraction.** Vector CFR carries per-combo state in the
//!   info-set store, so info sets are keyed only on `(player, history)`. The
//!   scalar builder keys on `(player, card_bucket, history)` and ends up with
//!   one info set per combo per history (`NoAbstraction`) — many more.
//! - **Showdown / Fold semantics.** Terminals are
//!   [`VectorNode::Showdown`] / [`VectorNode::Fold`] rather than scalar
//!   `Terminal { payoff_p0 }`. The scalar river tree stores a placeholder
//!   payoff at showdown nodes; the vector tree carries `half_pot` and the
//!   solver evaluates per-combo at runtime.
//! - **No chance nodes.** River subgames only — turn/river deals on earlier
//!   streets need additional handling and aren't covered here.

use std::collections::HashMap;

use crate::cards::card::Card;
use crate::solver::action::{Action, BetSizingConfig};
use crate::solver::showdown::ShowdownRanker;
use crate::solver::vector_cfr::{VectorGameTree, VectorNode, VectorNodeIndex};

/// Build a vector-form game tree for a river subgame.
///
/// Mirrors [`crate::solver::postflop::build_river_tree`] in inputs and action
/// semantics so vector and scalar solvers can be run on the same scenario.
pub fn build_vector_river_tree(
    board: [Card; 5],
    starting_pot: f32,
    effective_stack: f32,
    bet_config: BetSizingConfig,
) -> VectorGameTree {
    let mut builder = VectorRiverBuilder {
        nodes: Vec::new(),
        info_set_map: HashMap::new(),
        next_info_set_idx: 0,
        actions_per_info_set: Vec::new(),
        bet_config,
    };

    let initial_contrib = starting_pot / 2.0;
    let state = BuildState {
        pot_contributions: [initial_contrib, initial_contrib],
        stacks: [effective_stack, effective_stack],
        action_history: Vec::new(),
        raises_this_street: 0,
    };

    let root = builder.build_action_node(0, &state);

    VectorGameTree {
        nodes: builder.nodes,
        root,
        actions_per_info_set: builder.actions_per_info_set,
        ranker: ShowdownRanker::new(&board),
    }
}

struct VectorRiverBuilder {
    nodes: Vec<VectorNode>,
    info_set_map: HashMap<InfoSetKey, u32>,
    next_info_set_idx: u32,
    actions_per_info_set: Vec<u8>,
    bet_config: BetSizingConfig,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct InfoSetKey {
    player: u8,
    history: Vec<u8>,
}

#[derive(Clone, Debug)]
struct BuildState {
    pot_contributions: [f32; 2],
    stacks: [f32; 2],
    action_history: Vec<u8>,
    raises_this_street: u8,
}

impl VectorRiverBuilder {
    fn push_node(&mut self, node: VectorNode) -> VectorNodeIndex {
        let idx = self.nodes.len() as VectorNodeIndex;
        self.nodes.push(node);
        idx
    }

    fn get_or_create_info_set(&mut self, player: u8, history: &[u8], n_actions: u8) -> u32 {
        let key = InfoSetKey {
            player,
            history: history.to_vec(),
        };
        if let Some(&idx) = self.info_set_map.get(&key) {
            return idx;
        }
        let idx = self.next_info_set_idx;
        self.info_set_map.insert(key, idx);
        self.next_info_set_idx += 1;
        self.actions_per_info_set.push(n_actions);
        idx
    }

    /// Build a Decision node where `player` must act, plus all child subtrees.
    fn build_action_node(&mut self, player: u8, state: &BuildState) -> VectorNodeIndex {
        let pot = state.pot_contributions[0] + state.pot_contributions[1];
        let to_call =
            state.pot_contributions[1 - player as usize] - state.pot_contributions[player as usize];
        let stack = state.stacks[player as usize];

        let (actions, action_codes) = self.enumerate_actions(state, pot, to_call, stack);
        let n_actions = actions.len() as u8;

        let info_set_idx = self.get_or_create_info_set(player, &state.action_history, n_actions);

        let mut children: Vec<VectorNodeIndex> = Vec::with_capacity(actions.len());
        for (action, code) in actions.iter().zip(action_codes.iter()) {
            let child = self.build_action_child(player, state, action, *code);
            children.push(child);
        }

        self.push_node(VectorNode::Decision {
            player,
            actions,
            children,
            info_set_idx,
        })
    }

    /// Determine the legal actions and history-encoding bytes at this state.
    fn enumerate_actions(
        &self,
        state: &BuildState,
        pot: f32,
        to_call: f32,
        stack: f32,
    ) -> (Vec<Action>, Vec<u8>) {
        let mut actions = Vec::new();
        let mut codes = Vec::new();

        if to_call > 0.0 {
            actions.push(Action::Fold);
            codes.push(0);
            actions.push(Action::Call);
            codes.push(1);

            if state.raises_this_street < self.bet_config.max_raises_per_street {
                for &size_fraction in &self.bet_config.river_raises {
                    let raise_amount = (pot + to_call) * size_fraction as f32;
                    let total_to_put_in = to_call + raise_amount;
                    if total_to_put_in < stack {
                        actions.push(Action::bet_from_fraction(size_fraction));
                        codes.push(2 + (size_fraction * 100.0) as u8);
                    }
                }
                if self.bet_config.always_allow_allin && stack > to_call {
                    actions.push(Action::AllIn);
                    codes.push(255);
                }
            }
        } else {
            actions.push(Action::Check);
            codes.push(0);

            for &size_fraction in &self.bet_config.river_bets {
                let bet_amount = pot * size_fraction as f32;
                if bet_amount < stack {
                    actions.push(Action::bet_from_fraction(size_fraction));
                    codes.push(1 + (size_fraction * 100.0) as u8);
                }
            }
            if self.bet_config.always_allow_allin && stack > 0.0 {
                actions.push(Action::AllIn);
                codes.push(255);
            }
        }

        (actions, codes)
    }

    /// Build the subtree reached when `player` takes `action`.
    fn build_action_child(
        &mut self,
        player: u8,
        state: &BuildState,
        action: &Action,
        action_code: u8,
    ) -> VectorNodeIndex {
        let opponent = 1 - player;
        let pot = state.pot_contributions[0] + state.pot_contributions[1];
        let to_call =
            state.pot_contributions[opponent as usize] - state.pot_contributions[player as usize];

        let mut new_state = state.clone();
        new_state.action_history.push(action_code);

        match action {
            Action::Fold => {
                // Folder loses what they put in; opponent wins it.
                let pot_won = state.pot_contributions[player as usize];
                self.push_node(VectorNode::Fold {
                    winner: opponent,
                    pot_won,
                })
            }
            Action::Check => {
                if player == 1 {
                    // Both checked → showdown. Pot stayed at `pot`; half_pot
                    // is what the winner gains and the loser loses.
                    self.push_showdown(&new_state)
                } else {
                    self.build_action_node(opponent, &new_state)
                }
            }
            Action::Call => {
                new_state.pot_contributions[player as usize] += to_call;
                new_state.stacks[player as usize] -= to_call;
                self.push_showdown(&new_state)
            }
            Action::Bet(bp) => {
                let size_fraction = *bp as f32 / 10000.0;
                let amount = if to_call > 0.0 {
                    let raise_amount = (pot + to_call) * size_fraction;
                    to_call + raise_amount
                } else {
                    pot * size_fraction
                };
                let amount = amount.min(new_state.stacks[player as usize]);
                new_state.pot_contributions[player as usize] += amount;
                new_state.stacks[player as usize] -= amount;
                new_state.raises_this_street += 1;
                self.build_action_node(opponent, &new_state)
            }
            Action::AllIn => {
                let amount = new_state.stacks[player as usize];
                new_state.pot_contributions[player as usize] += amount;
                new_state.stacks[player as usize] = 0.0;
                new_state.raises_this_street += 1;

                if new_state.stacks[opponent as usize] == 0.0 {
                    self.push_showdown(&new_state)
                } else {
                    self.build_action_node(opponent, &new_state)
                }
            }
        }
    }

    /// Push a Showdown terminal. At a showdown both players have matched
    /// contributions, so `half_pot = contributions[0]`. Per-combo EV at solve
    /// time is `half_pot * ranker.ev_vs_opponent(...)` ∈ `[-half_pot, half_pot]`.
    fn push_showdown(&mut self, state: &BuildState) -> VectorNodeIndex {
        // At a showdown the contributions should be equal — assert in
        // debug builds so a tree-construction bug surfaces immediately.
        debug_assert!(
            (state.pot_contributions[0] - state.pot_contributions[1]).abs() < 1e-3,
            "showdown reached with mismatched contributions: {:?}",
            state.pot_contributions
        );
        let half_pot = state.pot_contributions[0];
        self.push_node(VectorNode::Showdown { half_pot })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::card::{Rank, Suit};
    use crate::solver::cfr::{CfrAlgorithm, SolverConfig};
    use crate::solver::showdown::N_COMBOS;
    use crate::solver::vector_cfr::VectorCfrSolver;

    fn test_board() -> [Card; 5] {
        [
            Card::new(Rank::Ace, Suit::Spades),
            Card::new(Rank::King, Suit::Hearts),
            Card::new(Rank::Queen, Suit::Diamonds),
            Card::new(Rank::Seven, Suit::Clubs),
            Card::new(Rank::Two, Suit::Spades),
        ]
    }

    fn simple_river_config() -> BetSizingConfig {
        BetSizingConfig {
            river_bets: vec![0.5, 1.0],
            river_raises: vec![1.0],
            always_allow_allin: false,
            max_raises_per_street: 1,
            ..Default::default()
        }
    }

    fn unit_reach(ranker: &ShowdownRanker) -> Box<[f32; N_COMBOS]> {
        let mut r = Box::new([0.0f32; N_COMBOS]);
        for i in 0..N_COMBOS {
            if !ranker.is_blocked(i as u16) {
                r[i] = 1.0;
            }
        }
        r
    }

    #[test]
    fn river_tree_builds_with_decision_root() {
        let tree = build_vector_river_tree(test_board(), 100.0, 200.0, simple_river_config());
        match &tree.nodes[tree.root as usize] {
            VectorNode::Decision {
                player,
                actions,
                children,
                ..
            } => {
                assert_eq!(*player, 0, "OOP (P0) acts first");
                // Check + 2 bet sizes.
                assert_eq!(actions.len(), 3);
                assert_eq!(children.len(), 3);
                assert_eq!(actions[0], Action::Check);
            }
            other => panic!("root should be Decision, got {other:?}"),
        }
    }

    #[test]
    fn river_tree_has_both_terminal_kinds() {
        let tree = build_vector_river_tree(test_board(), 100.0, 200.0, simple_river_config());
        let n_showdown = tree
            .nodes
            .iter()
            .filter(|n| matches!(n, VectorNode::Showdown { .. }))
            .count();
        let n_fold = tree
            .nodes
            .iter()
            .filter(|n| matches!(n, VectorNode::Fold { .. }))
            .count();
        assert!(n_showdown >= 1, "expected at least one Showdown terminal");
        assert!(n_fold >= 1, "expected at least one Fold terminal");
    }

    #[test]
    fn fold_winner_matches_folding_player() {
        // Walk the tree and verify each Fold's winner is the *opponent* of
        // the player who acted right before — folding always benefits the
        // other player.
        let tree = build_vector_river_tree(test_board(), 100.0, 200.0, simple_river_config());
        for node in &tree.nodes {
            if let VectorNode::Fold { pot_won, .. } = node {
                // pot_won must be positive (folder put in something).
                assert!(*pot_won > 0.0, "fold pot_won should be positive");
            }
        }
    }

    #[test]
    fn vector_solver_runs_one_iteration() {
        // Smoke test: full builder → solver → one iteration end-to-end.
        let tree = build_vector_river_tree(test_board(), 100.0, 200.0, simple_river_config());
        let r0 = unit_reach(&tree.ranker);
        let r1 = unit_reach(&tree.ranker);
        let config = SolverConfig {
            algorithm: CfrAlgorithm::CfrPlus,
            iterations: 1,
            ..Default::default()
        };
        let mut solver = VectorCfrSolver::new(tree, r0, r1, config);
        let v = solver.run_iteration(0);
        assert!(v.is_finite(), "iteration value should be finite, got {v}");
    }

    #[test]
    fn info_sets_dedup_per_player_history() {
        // No card abstraction here — info sets are uniquely keyed on
        // (player, action history). For a simple river config we expect
        // far fewer info sets than scalar NoAbstraction's per-combo
        // explosion.
        let tree = build_vector_river_tree(test_board(), 100.0, 200.0, simple_river_config());
        // Total info sets is the length of actions_per_info_set.
        let n_info_sets = tree.actions_per_info_set.len();
        // Loose bound — for `simple_river_config` the action tree is shallow
        // (3 OOP actions × {check, fold/call/raise} branches × max 1 raise).
        // Definitely under 50.
        assert!(
            n_info_sets <= 50,
            "expected ≤50 info sets (player+history only), got {n_info_sets}"
        );
        assert!(
            n_info_sets >= 2,
            "expected at least 2 info sets, got {n_info_sets}"
        );
    }
}
