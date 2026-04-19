//! Vector CFR on a river subgame.
//!
//! This is the PIOSolver-style formulation of CFR: instead of enumerating
//! every combo pair at the root and running a scalar traversal per pair, the
//! tree carries per-combo reach vectors `[f32; N_COMBOS]` down to terminals
//! and evaluates all 1326 combos at once. Showdown terminals use the
//! [`ShowdownRanker`] O(N·52) evaluator; fold terminals use inclusion-
//! exclusion over the opponent reach to get a per-combo "compatible opponent
//! mass" value.
//!
//! ## Scope
//!
//! A *river subgame*: fixed 5-card board, no chance nodes, both players act
//! with known starting ranges. Every leaf is either:
//!
//! - [`VectorNode::Showdown`] — the two remaining ranges are compared via
//!   [`ShowdownRanker::ev_vs_opponent`], scaled by `half_pot`.
//! - [`VectorNode::Fold`] — one player folded; winner collects `pot_won`,
//!   loser pays it, modulated per combo by "how much of opponent reach is
//!   compatible with this combo".
//!
//! Generalising to earlier streets with chance nodes is a follow-up: the
//! traversal signature stays the same, but tree construction needs to
//! enumerate turn/river deals as chance branches.
//!
//! ## Output
//!
//! Per-combo EV vectors are allocated as `Box<[f32; N_COMBOS]>` so the
//! recursion doesn't smash the 4kB default stack. `VectorCfrSolver::solve`
//! returns the *root node value*, averaged over iterations — the vector
//! analogue of the scalar solver's game-value output. Currently this is
//! Σᵢ reach_p0[i] · v_p0[i], the unweighted sum across combos; callers
//! with unit-weight starting ranges get a direct chip-EV number.

use crate::solver::cfr::{CfrAlgorithm, SolverConfig};
use crate::solver::showdown::{ShowdownRanker, N_COMBOS};
use crate::solver::vector_info_set::VectorInfoSetStore;

/// Per-combo coefficient of 1.0 used as the "opponent reach" factor in
/// vector-CFR regret updates. Terminal evaluators integrate opponent reach
/// into per-combo action values, so the regret accumulation step must not
/// scale by opponent reach a second time. Materialising the vector once
/// (rather than special-casing the update) keeps
/// `VectorInfoSetStore::update_regrets_and_strategy` API-compatible with
/// the scalar store.
static ONES_REACH: [f32; N_COMBOS] = [1.0; N_COMBOS];

/// Index into [`VectorGameTree::nodes`].
pub type VectorNodeIndex = u32;

/// Vector-form game tree node.
///
/// Decision nodes mirror the scalar [`crate::solver::game_tree::GameTreeNode`]
/// shape (player, per-action children, info set index). Terminals split by
/// semantics so the traversal doesn't have to carry scalar payoffs that don't
/// apply at the vector level.
#[derive(Clone, Debug)]
pub enum VectorNode {
    /// Decision node. `children[a]` is the sub-tree reached by action `a`;
    /// `info_set_idx` indexes the [`VectorInfoSetStore`], which holds 1326
    /// regret/strategy-sum slots per action here.
    Decision {
        player: u8,
        children: Vec<VectorNodeIndex>,
        info_set_idx: u32,
    },
    /// Showdown terminal. `half_pot` is the pot at the terminal divided by 2
    /// — the amount each side gains (wins) or loses against the other at a
    /// full showdown, in chips.
    Showdown { half_pot: f32 },
    /// Fold terminal. The `winner` player collects `pot_won` chips (= the
    /// amount the folder put in already, which is theirs to lose). Per-combo
    /// EV is scaled by the opponent's combo-compatible reach mass.
    Fold { winner: u8, pot_won: f32 },
}

/// Flat-arena vector-form game tree for a river subgame.
///
/// The tree is paired with a single [`ShowdownRanker`] because every showdown
/// terminal shares the same 5-card board.
#[derive(Clone)]
pub struct VectorGameTree {
    pub nodes: Vec<VectorNode>,
    pub root: VectorNodeIndex,
    /// Number of actions at each info set, indexed by `info_set_idx`. Shape
    /// mirrors [`crate::solver::game_tree::GameTree::actions_per_info_set`].
    pub actions_per_info_set: Vec<u8>,
    /// Precomputed showdown evaluator for the fixed board.
    pub ranker: ShowdownRanker,
}

/// Vector CFR solver. Analogous to [`crate::solver::cfr::CfrSolver`] but
/// keeps per-combo regrets and strategy sums.
pub struct VectorCfrSolver {
    pub tree: VectorGameTree,
    pub store: VectorInfoSetStore,
    pub config: SolverConfig,
    /// Per-combo starting reach for each player (e.g. the normalised opening
    /// range). Carried down the tree along with the strategy-scaled updates.
    pub starting_reach: [Box<[f32; N_COMBOS]>; 2],
}

impl VectorCfrSolver {
    /// Create a solver from a prebuilt tree, starting reach for both players,
    /// and configuration. Allocates the per-combo info-set store.
    pub fn new(
        tree: VectorGameTree,
        starting_reach_p0: Box<[f32; N_COMBOS]>,
        starting_reach_p1: Box<[f32; N_COMBOS]>,
        config: SolverConfig,
    ) -> Self {
        let store = VectorInfoSetStore::new(&tree.actions_per_info_set);
        Self {
            tree,
            store,
            config,
            starting_reach: [starting_reach_p0, starting_reach_p1],
        }
    }

    /// Run one CFR iteration (two traversals, one per traversing player).
    ///
    /// Returns the aggregate root value for player 0: Σᵢ reach_p0[i] · v_p0[i].
    /// With unit-weight ranges this is chip EV of the root to player 0.
    pub fn run_iteration(&mut self, _iteration_number: u32) -> f64 {
        let use_cfr_plus = matches!(self.config.algorithm, CfrAlgorithm::CfrPlus);
        let reach_p0 = self.starting_reach[0].clone();
        let reach_p1 = self.starting_reach[1].clone();

        let v0 = vector_cfr_traverse(
            &self.tree,
            &mut self.store,
            use_cfr_plus,
            self.tree.root,
            &reach_p0,
            &reach_p1,
            0,
        );
        let _v1 = vector_cfr_traverse(
            &self.tree,
            &mut self.store,
            use_cfr_plus,
            self.tree.root,
            &reach_p0,
            &reach_p1,
            1,
        );

        let mut acc = 0.0f64;
        for (r, v) in reach_p0.iter().zip(v0.iter()) {
            acc += (*r as f64) * (*v as f64);
        }
        acc
    }

    /// Run CFR for the configured number of iterations. Returns the time-
    /// averaged root value (same units as [`run_iteration`]).
    pub fn solve(&mut self) -> f64 {
        let mut sum = 0.0f64;
        for t in 0..self.config.iterations {
            sum += self.run_iteration(t);
        }
        sum / self.config.iterations as f64
    }
}

/// Core vector CFR traversal. Returns a per-combo counterfactual value
/// vector for `traversing_player` at `node_idx`.
///
/// Mirrors the scalar traversal's recursion structure. The `_use_cfr_plus`
/// flag is threaded through as a single bool so the traversal doesn't have
/// to re-read it from the solver each call.
fn vector_cfr_traverse(
    tree: &VectorGameTree,
    store: &mut VectorInfoSetStore,
    use_cfr_plus: bool,
    node_idx: VectorNodeIndex,
    reach_p0: &[f32; N_COMBOS],
    reach_p1: &[f32; N_COMBOS],
    traversing_player: u8,
) -> Box<[f32; N_COMBOS]> {
    match &tree.nodes[node_idx as usize] {
        VectorNode::Showdown { half_pot } => {
            let opp_reach = if traversing_player == 0 {
                reach_p1
            } else {
                reach_p0
            };
            let mut ev = tree.ranker.ev_vs_opponent(opp_reach);
            if *half_pot != 1.0 {
                for v in ev.iter_mut() {
                    *v *= half_pot;
                }
            }
            ev
        }
        VectorNode::Fold { winner, pot_won } => {
            let opp_reach = if traversing_player == 0 {
                reach_p1
            } else {
                reach_p0
            };
            let compat = tree.ranker.compatible_reach_sum(opp_reach);
            let sign = if *winner == traversing_player {
                1.0
            } else {
                -1.0
            };
            let scale = sign * *pot_won;
            let mut ev = compat;
            for v in ev.iter_mut() {
                *v *= scale;
            }
            ev
        }
        VectorNode::Decision {
            player,
            children,
            info_set_idx,
        } => {
            let player = *player;
            let info_set_idx = *info_set_idx;
            let n_actions = children.len();

            // Regret-match the current strategy for every combo in one shot.
            let mut strategy = vec![0.0f32; n_actions * N_COMBOS];
            store.current_strategy_into(info_set_idx, &mut strategy);

            // Per-action per-combo values from recursion.
            let mut action_values = vec![0.0f32; n_actions * N_COMBOS];
            // Node value per combo: Σ_a strategy[combo, a] · action_values[combo, a].
            let mut node_value = Box::new([0.0f32; N_COMBOS]);

            // Children are visited in order; each visit needs the per-combo
            // reach updated by this player's strategy at that action.
            for a in 0..n_actions {
                // Build this action's reach vectors.
                let (new_reach_p0, new_reach_p1) = if player == 0 {
                    (
                        scale_reach(reach_p0, &strategy, a, n_actions),
                        Box::new(*reach_p1),
                    )
                } else {
                    (
                        Box::new(*reach_p0),
                        scale_reach(reach_p1, &strategy, a, n_actions),
                    )
                };

                let v = vector_cfr_traverse(
                    tree,
                    store,
                    use_cfr_plus,
                    children[a],
                    &new_reach_p0,
                    &new_reach_p1,
                    traversing_player,
                );

                // Write this action's per-combo values into the flat buffer,
                // combo-major layout to match the strategy buffer.
                for combo in 0..N_COMBOS {
                    action_values[combo * n_actions + a] = v[combo];
                }
            }

            // Compute node_value per combo from strategy * action_values.
            for combo in 0..N_COMBOS {
                let base = combo * n_actions;
                let mut acc = 0.0f32;
                for a in 0..n_actions {
                    acc += strategy[base + a] * action_values[base + a];
                }
                node_value[combo] = acc;
            }

            // If this is the traversing player's decision, apply the CFR
            // regret update and strategy accumulation — vector-form.
            //
            // Key convention: terminal evaluators (showdown, fold) already
            // integrate the opponent reach into the returned per-combo value,
            // so the per-combo regret update must NOT multiply by opponent
            // reach a second time. Pass an all-ones vector for the regret
            // coefficient; scale strategy-sum by the acting player's own
            // per-combo reach.
            if player == traversing_player {
                let my_reach = if player == 0 { reach_p0 } else { reach_p1 };

                store.update_regrets_and_strategy(
                    info_set_idx,
                    &action_values,
                    node_value.as_ref(),
                    &strategy,
                    &ONES_REACH,
                    my_reach,
                    use_cfr_plus,
                );
            }

            node_value
        }
    }
}

/// Per-combo reach scaled by strategy at one action.
///
/// Reach is combo-indexed (`[f32; N_COMBOS]`); strategy is combo-major with
/// `n_actions` entries per combo (`strategy[combo * n_actions + a]`). The
/// output is the reach vector you'd pass into the child reached by action
/// `a`: `out[combo] = reach[combo] * strategy[combo * n_actions + a]`.
///
/// Factored into a helper to keep the recursion body concise; returning an
/// owned `Box<[f32; N_COMBOS]>` keeps the large array off the recursion
/// stack.
#[inline]
fn scale_reach(
    reach: &[f32; N_COMBOS],
    strategy: &[f32],
    action: usize,
    n_actions: usize,
) -> Box<[f32; N_COMBOS]> {
    let mut out = Box::new([0.0f32; N_COMBOS]);
    for (combo, (slot, &r)) in out.iter_mut().zip(reach.iter()).enumerate() {
        *slot = r * strategy[combo * n_actions + action];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::card::{Card, Rank, Suit};

    fn test_board() -> [Card; 5] {
        [
            Card::new(Rank::Ace, Suit::Spades),
            Card::new(Rank::King, Suit::Hearts),
            Card::new(Rank::Queen, Suit::Diamonds),
            Card::new(Rank::Seven, Suit::Clubs),
            Card::new(Rank::Two, Suit::Spades),
        ]
    }

    /// Build the simplest non-trivial river tree: P0 decides "check" (→ showdown)
    /// or "fold" (→ P1 wins a tiny pot). Only one info set, two actions. P1
    /// has no decisions.
    fn tiny_tree(half_pot: f32, fold_pot: f32) -> VectorGameTree {
        let board = test_board();
        let ranker = ShowdownRanker::new(&board);
        let showdown_idx = 0;
        let fold_idx = 1;
        let decision_idx = 2;
        let nodes = vec![
            VectorNode::Showdown { half_pot },
            VectorNode::Fold {
                winner: 1,
                pot_won: fold_pot,
            },
            VectorNode::Decision {
                player: 0,
                children: vec![showdown_idx, fold_idx],
                info_set_idx: 0,
            },
        ];
        VectorGameTree {
            nodes,
            root: decision_idx,
            actions_per_info_set: vec![2],
            ranker,
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
    fn showdown_terminal_returns_scaled_ev() {
        let tree = tiny_tree(10.0, 0.0);
        let store_input: Vec<u8> = tree.actions_per_info_set.clone();
        let mut store = VectorInfoSetStore::new(&store_input);
        let r0 = unit_reach(&tree.ranker);
        let r1 = unit_reach(&tree.ranker);

        // Call the showdown node directly.
        let v = vector_cfr_traverse(&tree, &mut store, true, 0, &r0, &r1, 0);

        // Must match ranker.ev_vs_opponent(r1) * 10.0 on every unblocked combo.
        let ref_ev = tree.ranker.ev_vs_opponent(&r1);
        for i in 0..N_COMBOS {
            assert!(
                (v[i] - ref_ev[i] * 10.0).abs() < 1e-4,
                "combo {i}: got {} expected {}",
                v[i],
                ref_ev[i] * 10.0
            );
        }
    }

    #[test]
    fn fold_terminal_sign_matches_winner() {
        // P1 wins fold of pot 4.0.
        let tree = tiny_tree(0.0, 4.0);
        let mut store = VectorInfoSetStore::new(&tree.actions_per_info_set);
        let r0 = unit_reach(&tree.ranker);
        let r1 = unit_reach(&tree.ranker);

        // Traversing = P0 (the loser): value should be -4 * compat_sum.
        let v0 = vector_cfr_traverse(&tree, &mut store, true, 1, &r0, &r1, 0);
        // Traversing = P1 (the winner): value should be +4 * compat_sum.
        let v1 = vector_cfr_traverse(&tree, &mut store, true, 1, &r0, &r1, 1);

        for i in 0..N_COMBOS {
            if tree.ranker.is_blocked(i as u16) {
                continue;
            }
            // v0[i] = -v1[i] (zero-sum at fold given symmetric reaches).
            assert!(
                (v0[i] + v1[i]).abs() < 1e-4,
                "combo {i}: v0={} v1={}",
                v0[i],
                v1[i]
            );
            // Sign check: folder (P0) should get non-positive values.
            assert!(v0[i] <= 1e-4, "combo {i}: folder got positive {}", v0[i]);
        }
    }

    #[test]
    fn one_iteration_pushes_decision_toward_showdown_if_strong() {
        // When P0's range dominates P1's at showdown, the solver should
        // allocate strictly positive strategy mass to the showdown action
        // after a single update — because regret for that action is
        // positive.
        //
        // Use AA vs 22 heads-up (P0 holds AA, P1 holds 22). Pot-on-river
        // = 10 chips (half_pot = 5.0). Folding gives P0 -2 (small pot) —
        // so "showdown" is clearly better.
        let board = test_board();
        let ranker = ShowdownRanker::new(&board);
        use crate::solver::range::HandRange;
        let aa = HandRange::combo_index(
            Card::new(Rank::Ace, Suit::Hearts),
            Card::new(Rank::Ace, Suit::Diamonds),
        ) as usize;
        let twos = HandRange::combo_index(
            Card::new(Rank::Two, Suit::Hearts),
            Card::new(Rank::Two, Suit::Diamonds),
        ) as usize;
        let mut r0 = Box::new([0.0f32; N_COMBOS]);
        let mut r1 = Box::new([0.0f32; N_COMBOS]);
        r0[aa] = 1.0;
        r1[twos] = 1.0;

        let nodes = vec![
            VectorNode::Showdown { half_pot: 5.0 },
            VectorNode::Fold {
                winner: 1,
                pot_won: 2.0,
            },
            VectorNode::Decision {
                player: 0,
                children: vec![0, 1],
                info_set_idx: 0,
            },
        ];
        let tree = VectorGameTree {
            nodes,
            root: 2,
            actions_per_info_set: vec![2],
            ranker,
        };
        let mut store = VectorInfoSetStore::new(&tree.actions_per_info_set);

        // Uniform start → each action is 0.5. Node value for combo aa:
        // 0.5 * (+5) + 0.5 * (-2) = 1.5.
        // Regret for showdown action = +5 - 1.5 = +3.5 > 0.
        // Regret for fold action   = -2 - 1.5 = -3.5 (clipped to 0 by CFR+).
        let _ = vector_cfr_traverse(&tree, &mut store, true, 2, &r0, &r1, 0);

        // After update, regrets at combo aa are [3.5, 0.0]. Read them back.
        let regrets = store.regrets_of(0);
        let base = aa * 2;
        assert!(
            (regrets[base] - 3.5).abs() < 1e-3,
            "showdown regret at AA: got {}",
            regrets[base]
        );
        assert!(
            regrets[base + 1].abs() < 1e-3,
            "fold regret (clipped) at AA: got {}",
            regrets[base + 1]
        );

        // Strategy sum after one update with uniform strategy and unit
        // reach: [0.5, 0.5] at combo aa.
        let sums = store.strategy_sum_of(0);
        assert!((sums[base] - 0.5).abs() < 1e-3);
        assert!((sums[base + 1] - 0.5).abs() < 1e-3);
    }
}
