//! Convenience facade over [`VectorCfrSolver`].
//!
//! Takes human-friendly inputs (board cards, [`HandRange`] per player, pot
//! and stack, bet config, iterations) and returns a solved strategy profile
//! plus summary stats. Analogous to the
//! [`PostflopTreeBuilder`](crate::solver::postflop::PostflopTreeBuilder) +
//! [`CfrSolver`](crate::solver::cfr::CfrSolver) pattern used by the scalar
//! path, but packaged as a single call.
//!
//! ## Scope
//!
//! - River subgame (5-card board) — uses
//!   [`crate::solver::vector_postflop::build_vector_river_tree`].
//! - Turn subgame (4-card board) — uses
//!   [`crate::solver::vector_postflop::build_vector_turn_tree`], which
//!   enumerates the 48 possible river cards as chance children.
//!
//! Flop and earlier streets need additional chance-node layers and aren't
//! yet covered by the vector builders. Callers receive
//! [`VectorSolveError::UnsupportedStreet`] in that case.
//!
//! ## Strategy aggregation
//!
//! Vector CFR stores one strategy per combo per info set. The TUI and most
//! callers want an *aggregated* view: one probability per action at each
//! info set, summed across combos weighted by their reach. This module
//! does that aggregation via [`aggregate_strategy`] and returns a list of
//! [`VectorInfoSetStrategy`]s keyed by info-set index.

use crate::cards::card::Card;
use crate::solver::action::{Action, BetSizingConfig};
use crate::solver::cfr::SolverConfig;
use crate::solver::range::HandRange;
use crate::solver::showdown::N_COMBOS;
use crate::solver::vector_cfr::{VectorCfrSolver, VectorNode};
use crate::solver::vector_exploitability::compute_vector_exploitability;
use crate::solver::vector_postflop::{build_vector_river_tree, build_vector_turn_tree};

/// Errors from the vector-CFR convenience API.
#[derive(Debug, thiserror::Error)]
pub enum VectorSolveError {
    /// The board size isn't yet supported by the vector builder. Only river
    /// (5 cards) and turn (4 cards) are wired up.
    #[error("unsupported street: vector CFR currently supports turn (4) and river (5) boards, got {0} cards")]
    UnsupportedStreet(usize),
}

/// Input configuration for a vector CFR solve.
#[derive(Clone, Debug)]
pub struct VectorSolverConfig {
    pub board: Vec<Card>,
    pub range_oop: HandRange,
    pub range_ip: HandRange,
    pub starting_pot: f32,
    pub effective_stack: f32,
    pub bet_config: BetSizingConfig,
    pub cfr_config: SolverConfig,
    /// Normalisation unit for exploitability (milli-ante per hand). Set to
    /// `starting_pot / 2.0` to get mbb/hand in a HU pot, or `1.0` to get
    /// milli-chip EV. Defaults to `starting_pot / 2.0` when 0.
    pub ante: f32,
}

/// Averaged strategy at one info set, reduced to per-action probabilities by
/// summing combo-conditional strategy mass and normalising.
///
/// `history_label` is a compact rendering of the action history bytes so the
/// TUI can show something more meaningful than "Info Set 37". It's a
/// human-visible summary, not a canonical identifier.
#[derive(Clone, Debug)]
pub struct VectorInfoSetStrategy {
    pub info_set_idx: u32,
    pub player: u8,
    pub actions: Vec<Action>,
    pub probs: Vec<f32>,
    pub history_label: String,
}

/// Output of [`solve_vector`]: game value, exploitability, tree size, and one
/// aggregated strategy per info set.
#[derive(Clone, Debug)]
pub struct VectorSolverOutput {
    /// Time-averaged root value for player 0 (Σᵢ reach_p0[i] · v_p0[i]).
    /// With unit-weight ranges this is chip EV at the root to player 0.
    pub game_value: f64,
    /// Exploitability in milli-ante per hand (see [`VectorSolverConfig::ante`]).
    /// < 10 is near-Nash; < 50 is reasonable for display.
    pub exploitability: f64,
    pub num_info_sets: u32,
    pub num_nodes: u32,
    pub strategies: Vec<VectorInfoSetStrategy>,
}

/// Build the requested tree, run CFR for the configured iterations, and
/// return aggregated output.
pub fn solve_vector(cfg: VectorSolverConfig) -> Result<VectorSolverOutput, VectorSolveError> {
    let tree = match cfg.board.len() {
        5 => {
            let board: [Card; 5] = [
                cfg.board[0],
                cfg.board[1],
                cfg.board[2],
                cfg.board[3],
                cfg.board[4],
            ];
            build_vector_river_tree(
                board,
                cfg.starting_pot,
                cfg.effective_stack,
                cfg.bet_config.clone(),
            )
        }
        4 => {
            let board: [Card; 4] = [cfg.board[0], cfg.board[1], cfg.board[2], cfg.board[3]];
            build_vector_turn_tree(
                board,
                cfg.starting_pot,
                cfg.effective_stack,
                cfg.bet_config.clone(),
            )
        }
        n => return Err(VectorSolveError::UnsupportedStreet(n)),
    };

    let num_nodes = tree.nodes.len() as u32;
    let num_info_sets = tree.actions_per_info_set.len() as u32;

    let reach_p0 = hand_range_to_reach(&cfg.range_oop, &cfg.board);
    let reach_p1 = hand_range_to_reach(&cfg.range_ip, &cfg.board);

    let ante = if cfg.ante > 0.0 {
        cfg.ante
    } else {
        (cfg.starting_pot / 2.0).max(1.0)
    };

    let mut solver = VectorCfrSolver::new(tree, reach_p0, reach_p1, cfg.cfr_config);
    let game_value = solver.solve();

    let exploitability = compute_vector_exploitability(
        &solver.tree,
        &solver.store,
        &solver.starting_reach[0],
        &solver.starting_reach[1],
        ante,
    );

    let strategies = aggregate_strategy(&solver);

    Ok(VectorSolverOutput {
        game_value,
        exploitability,
        num_info_sets,
        num_nodes,
        strategies,
    })
}

/// Convert a [`HandRange`] (per-combo weights over all 1326 combos) into the
/// boxed reach vector used by vector CFR. Combos blocked by the board are
/// forced to 0 — a player can't hold cards that are already on the board.
pub fn hand_range_to_reach(range: &HandRange, board: &[Card]) -> Box<[f32; N_COMBOS]> {
    let mut reach = Box::new([0.0f32; N_COMBOS]);
    let board_mask: u64 = board.iter().fold(0u64, |m, c| m | (1u64 << c.index()));

    for (i, &w) in range.weights.iter().enumerate() {
        if w <= 0.0 {
            continue;
        }
        let (lo, hi) = combo_cards(i as u16);
        let combo_mask = (1u64 << lo) | (1u64 << hi);
        if combo_mask & board_mask != 0 {
            continue; // blocked by board
        }
        reach[i] = w;
    }
    reach
}

/// Decode a combo index into `(lo_card_index, hi_card_index)` using the
/// triangular encoding. Inlined here to avoid a dependency on private state
/// of `HandRange::cards_from_index`.
#[inline]
fn combo_cards(idx: u16) -> (u8, u8) {
    let mut lo: u16 = 0;
    let mut remaining = idx;
    loop {
        let combos_for_lo = 51 - lo;
        if remaining < combos_for_lo {
            break;
        }
        remaining -= combos_for_lo;
        lo += 1;
    }
    let hi = lo + 1 + remaining;
    (lo as u8, hi as u8)
}

/// Walk the tree's Decision nodes and produce one aggregated
/// [`VectorInfoSetStrategy`] per info set.
///
/// Aggregation sums the combo-dimension of the per-combo strategy sum and
/// normalises per info set. For an info set with 3 actions and 1326 combos
/// the output is a `[p_a, p_b, p_c]` with `p_a + p_b + p_c = 1`. Combos
/// with zero strategy mass (e.g. board-blocked or unreachable under the
/// starting range) contribute nothing.
fn aggregate_strategy(solver: &VectorCfrSolver) -> Vec<VectorInfoSetStrategy> {
    let n_info_sets = solver.tree.actions_per_info_set.len();
    // Collect per-info-set metadata from the tree in a single pass.
    let mut metadata: Vec<Option<(u8, Vec<Action>, String)>> = vec![None; n_info_sets];
    for node in &solver.tree.nodes {
        if let VectorNode::Decision {
            player,
            actions,
            info_set_idx,
            ..
        } = node
        {
            let idx = *info_set_idx as usize;
            if metadata[idx].is_none() {
                // Label: actions joined by '-'. Compact and informative for
                // the root-ish info sets. Longer histories get truncated.
                let label = format!("info#{} P{}", idx, player);
                metadata[idx] = Some((*player, actions.clone(), label));
            }
        }
    }

    let mut out = Vec::with_capacity(n_info_sets);
    for (idx, meta) in metadata.iter_mut().enumerate() {
        let (player, actions, history_label) = match meta.take() {
            Some(m) => m,
            None => continue, // info set never appeared — skip
        };
        let n_actions = actions.len();
        if n_actions == 0 {
            continue;
        }
        let mut strat_buf = vec![0.0f32; n_actions * N_COMBOS];
        solver
            .store
            .average_strategy_into(idx as u32, &mut strat_buf);

        // Sum across combos. Layout is combo-major: strat_buf[combo *
        // n_actions + a] is the probability of action `a` at `combo`.
        let mut action_totals = vec![0.0f32; n_actions];
        for combo in 0..N_COMBOS {
            for a in 0..n_actions {
                action_totals[a] += strat_buf[combo * n_actions + a];
            }
        }
        let total: f32 = action_totals.iter().sum();
        let probs: Vec<f32> = if total > 0.0 {
            action_totals.iter().map(|v| v / total).collect()
        } else {
            // Fallback to uniform if nothing learned (shouldn't happen on
            // reachable info sets but guards against divide-by-zero).
            vec![1.0 / n_actions as f32; n_actions]
        };

        out.push(VectorInfoSetStrategy {
            info_set_idx: idx as u32,
            player,
            actions,
            probs,
            history_label,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::card::{Rank, Suit};
    use crate::solver::cfr::CfrAlgorithm;

    fn test_board_river() -> Vec<Card> {
        vec![
            Card::new(Rank::Ace, Suit::Spades),
            Card::new(Rank::King, Suit::Hearts),
            Card::new(Rank::Queen, Suit::Diamonds),
            Card::new(Rank::Seven, Suit::Clubs),
            Card::new(Rank::Two, Suit::Spades),
        ]
    }

    fn test_board_turn() -> Vec<Card> {
        vec![
            Card::new(Rank::Ace, Suit::Spades),
            Card::new(Rank::King, Suit::Hearts),
            Card::new(Rank::Queen, Suit::Diamonds),
            Card::new(Rank::Seven, Suit::Clubs),
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

    #[test]
    fn hand_range_full_maps_to_unit_reach() {
        let range = HandRange::full();
        let board = test_board_river();
        let reach = hand_range_to_reach(&range, &board);

        // Exactly 47 choose 2 = 1081 combos should be non-zero (52 - 5
        // board cards = 47 remaining cards).
        let count = reach.iter().filter(|&&x| x > 0.0).count();
        assert_eq!(count, 47 * 46 / 2, "expected C(47,2) non-blocked combos");
        // Every non-zero should be exactly 1.0 (full range).
        for &v in reach.iter() {
            assert!(v == 0.0 || v == 1.0);
        }
    }

    #[test]
    fn hand_range_respects_per_combo_weights() {
        let mut range = HandRange::empty();
        // Put weight 1.0 on AhAd (not blocked by AKQ72 board).
        let ah = Card::new(Rank::Ace, Suit::Hearts);
        let ad = Card::new(Rank::Ace, Suit::Diamonds);
        let idx = HandRange::combo_index(ah, ad);
        range.weights[idx as usize] = 1.0;

        // Put weight on AsAh — As is on the board, so this must be dropped.
        let as_card = Card::new(Rank::Ace, Suit::Spades);
        let blocked_idx = HandRange::combo_index(as_card, ah);
        range.weights[blocked_idx as usize] = 1.0;

        let reach = hand_range_to_reach(&range, &test_board_river());
        assert_eq!(reach[idx as usize], 1.0, "AhAd should pass through");
        assert_eq!(
            reach[blocked_idx as usize], 0.0,
            "combo blocked by As on board should be zeroed"
        );
    }

    #[test]
    fn solve_vector_river_returns_strategies() {
        let cfg = VectorSolverConfig {
            board: test_board_river(),
            range_oop: HandRange::full(),
            range_ip: HandRange::full(),
            starting_pot: 100.0,
            effective_stack: 200.0,
            bet_config: simple_river_config(),
            cfr_config: SolverConfig {
                algorithm: CfrAlgorithm::CfrPlus,
                iterations: 50,
                ..Default::default()
            },
            ante: 0.0,
        };
        let out = solve_vector(cfg).expect("river solve should succeed");

        assert!(out.game_value.is_finite());
        assert!(out.exploitability.is_finite());
        assert!(out.exploitability >= 0.0);
        assert!(out.num_info_sets > 0);
        assert!(out.num_nodes > 0);
        assert!(
            !out.strategies.is_empty(),
            "expected at least one aggregated info-set strategy"
        );
        for s in &out.strategies {
            assert_eq!(s.probs.len(), s.actions.len());
            let sum: f32 = s.probs.iter().sum();
            assert!(
                (sum - 1.0).abs() < 1e-3,
                "aggregated probs should sum to 1.0, got {sum}"
            );
        }
    }

    #[test]
    fn solve_vector_turn_builds_chance() {
        let cfg = VectorSolverConfig {
            board: test_board_turn(),
            range_oop: HandRange::full(),
            range_ip: HandRange::full(),
            starting_pot: 100.0,
            effective_stack: 200.0,
            bet_config: BetSizingConfig {
                turn_bets: vec![0.75],
                turn_raises: vec![],
                river_bets: vec![0.75],
                river_raises: vec![],
                always_allow_allin: false,
                max_raises_per_street: 1,
                ..Default::default()
            },
            cfr_config: SolverConfig {
                algorithm: CfrAlgorithm::CfrPlus,
                iterations: 5,
                ..Default::default()
            },
            ante: 0.0,
        };
        let out = solve_vector(cfg).expect("turn solve should succeed");
        assert!(out.game_value.is_finite());
        assert!(
            out.num_info_sets > 1,
            "turn tree should have many info sets"
        );
    }

    #[test]
    fn solve_vector_rejects_flop() {
        let cfg = VectorSolverConfig {
            board: vec![
                Card::new(Rank::Ace, Suit::Spades),
                Card::new(Rank::King, Suit::Hearts),
                Card::new(Rank::Queen, Suit::Diamonds),
            ],
            range_oop: HandRange::full(),
            range_ip: HandRange::full(),
            starting_pot: 100.0,
            effective_stack: 200.0,
            bet_config: BetSizingConfig::default(),
            cfr_config: SolverConfig::default(),
            ante: 0.0,
        };
        let err = solve_vector(cfg).unwrap_err();
        assert!(matches!(err, VectorSolveError::UnsupportedStreet(3)));
    }
}
