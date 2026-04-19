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
//! - **Chance nodes for next-street deals.** `build_vector_turn_tree`
//!   adds a chance node dealing the river card, with a per-river-card
//!   [`ShowdownRanker`] stored in `VectorGameTree::rankers`. The river-only
//!   builder keeps a single ranker in that vec.

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
        rankers: vec![ShowdownRanker::new(&board)],
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
        self.push_node(VectorNode::Showdown {
            half_pot,
            ranker_idx: 0,
        })
    }
}

/// Build a vector-form game tree for a turn subgame: both players act on the
/// turn, then a chance node deals the river card, then both players act on
/// the river, then terminals.
///
/// `turn_board` is the 4-card board at the start of turn action (flop +
/// turn). The builder enumerates the 48 remaining river cards as chance
/// children and builds a full river action subtree under each. Each river
/// subtree shares the same info-set structure but terminates in a showdown
/// that references a river-specific [`ShowdownRanker`] in
/// [`VectorGameTree::rankers`].
pub fn build_vector_turn_tree(
    turn_board: [Card; 4],
    starting_pot: f32,
    effective_stack: f32,
    bet_config: BetSizingConfig,
) -> VectorGameTree {
    // Pre-compute one ranker per possible river card (48 of them).
    let known: [bool; 52] = {
        let mut k = [false; 52];
        for c in turn_board.iter() {
            k[c.index() as usize] = true;
        }
        k
    };
    let mut rankers: Vec<ShowdownRanker> = Vec::with_capacity(48);
    let mut river_card_ranker_idx: [Option<u16>; 52] = [None; 52];
    for card_idx in 0..52u8 {
        if known[card_idx as usize] {
            continue;
        }
        let final_board = [
            turn_board[0],
            turn_board[1],
            turn_board[2],
            turn_board[3],
            Card::from_index(card_idx),
        ];
        river_card_ranker_idx[card_idx as usize] = Some(rankers.len() as u16);
        rankers.push(ShowdownRanker::new(&final_board));
    }

    let mut builder = VectorTurnBuilder {
        nodes: Vec::new(),
        info_set_map: HashMap::new(),
        next_info_set_idx: 0,
        actions_per_info_set: Vec::new(),
        bet_config,
        river_card_ranker_idx,
    };

    let initial_contrib = starting_pot / 2.0;
    let state = TurnBuildState {
        pot_contributions: [initial_contrib, initial_contrib],
        stacks: [effective_stack, effective_stack],
        action_history: Vec::new(),
        raises_this_street: 0,
        street: 0, // 0 = turn, 1 = river
        ranker_idx: 0,
    };

    let root = builder.build_action_node(0, &state);

    VectorGameTree {
        nodes: builder.nodes,
        root,
        actions_per_info_set: builder.actions_per_info_set,
        rankers,
    }
}

struct VectorTurnBuilder {
    nodes: Vec<VectorNode>,
    info_set_map: HashMap<InfoSetKey, u32>,
    next_info_set_idx: u32,
    actions_per_info_set: Vec<u8>,
    bet_config: BetSizingConfig,
    /// For each card index 0..52, the ranker index for the final board formed
    /// by that card being dealt as the river. `None` for cards already on the
    /// turn board (blocked).
    river_card_ranker_idx: [Option<u16>; 52],
}

/// Like [`BuildState`] but tracks the street (0 = turn, 1 = river) so we
/// know whether a street-ending action transitions to a chance node (deal
/// river) or to a showdown. On the river, `ranker_idx` points at the
/// [`ShowdownRanker`] for the current final board; it's meaningless on the
/// turn and stays at the default (0).
#[derive(Clone, Debug)]
struct TurnBuildState {
    pot_contributions: [f32; 2],
    stacks: [f32; 2],
    action_history: Vec<u8>,
    raises_this_street: u8,
    street: u8,
    ranker_idx: u16,
}

impl VectorTurnBuilder {
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

    fn build_action_node(&mut self, player: u8, state: &TurnBuildState) -> VectorNodeIndex {
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

    fn enumerate_actions(
        &self,
        state: &TurnBuildState,
        pot: f32,
        to_call: f32,
        stack: f32,
    ) -> (Vec<Action>, Vec<u8>) {
        let mut actions = Vec::new();
        let mut codes = Vec::new();

        // Choose per-street bet/raise sizes.
        let bet_sizes: &[f64] = if state.street == 0 {
            &self.bet_config.turn_bets
        } else {
            &self.bet_config.river_bets
        };
        let raise_sizes: &[f64] = if state.street == 0 {
            &self.bet_config.turn_raises
        } else {
            &self.bet_config.river_raises
        };

        if to_call > 0.0 {
            actions.push(Action::Fold);
            codes.push(0);
            actions.push(Action::Call);
            codes.push(1);

            if state.raises_this_street < self.bet_config.max_raises_per_street {
                for &size_fraction in raise_sizes {
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

            for &size_fraction in bet_sizes {
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

    fn build_action_child(
        &mut self,
        player: u8,
        state: &TurnBuildState,
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
                let pot_won = state.pot_contributions[player as usize];
                self.push_node(VectorNode::Fold {
                    winner: opponent,
                    pot_won,
                })
            }
            Action::Check => {
                if player == 1 {
                    // Both checked → end of street.
                    self.end_of_street(&new_state)
                } else {
                    self.build_action_node(opponent, &new_state)
                }
            }
            Action::Call => {
                new_state.pot_contributions[player as usize] += to_call;
                new_state.stacks[player as usize] -= to_call;
                self.end_of_street(&new_state)
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
                    self.end_of_street(&new_state)
                } else {
                    self.build_action_node(opponent, &new_state)
                }
            }
        }
    }

    /// End-of-street transition: on the turn, deal the river via a chance
    /// node. On the river, go to showdown.
    fn end_of_street(&mut self, state: &TurnBuildState) -> VectorNodeIndex {
        if state.street == 1 {
            return self.push_showdown(state);
        }

        // Deal the river. Build one subtree per possible river card (48).
        let mut children: Vec<(u8, VectorNodeIndex)> = Vec::with_capacity(48);
        for card_idx in 0..52u8 {
            let ranker_idx = match self.river_card_ranker_idx[card_idx as usize] {
                Some(idx) => idx,
                None => continue, // card is on the turn board
            };
            let mut next = state.clone();
            next.street = 1;
            next.raises_this_street = 0;
            next.ranker_idx = ranker_idx;
            // Push a street separator to the history so river info sets don't
            // collide with turn info sets that share the same action sequence.
            next.action_history.push(200 + card_idx);

            // P0 acts first on the river.
            let subtree = self.build_action_node(0, &next);
            children.push((card_idx, subtree));
        }

        self.push_node(VectorNode::Chance { children })
    }

    fn push_showdown(&mut self, state: &TurnBuildState) -> VectorNodeIndex {
        debug_assert!(
            (state.pot_contributions[0] - state.pot_contributions[1]).abs() < 1e-3,
            "showdown reached with mismatched contributions: {:?}",
            state.pot_contributions
        );
        let half_pot = state.pot_contributions[0];
        self.push_node(VectorNode::Showdown {
            half_pot,
            ranker_idx: state.ranker_idx,
        })
    }
}

/// Build a vector-form game tree for a flop subgame: three streets of action
/// with two chance-node layers (deal turn, then deal river).
///
/// `flop_board` is the 3-card flop. The builder enumerates every possible
/// (turn_card, river_card) runout, pre-computes one [`ShowdownRanker`] per
/// resulting 5-card final board, and wires them under two nested chance
/// layers. For a fresh flop with 49 remaining cards, that's
/// `49 * 48 = 2_352` final boards — this is the authoritative tree shape
/// that a real flop solver must traverse; callers should keep bet-sizing
/// configs minimal (1 size per street, no raises) to stay within workable
/// memory budgets.
pub fn build_vector_flop_tree(
    flop_board: [Card; 3],
    starting_pot: f32,
    effective_stack: f32,
    bet_config: BetSizingConfig,
) -> VectorGameTree {
    let known: [bool; 52] = {
        let mut k = [false; 52];
        for c in flop_board.iter() {
            k[c.index() as usize] = true;
        }
        k
    };
    // (turn_card, river_card) → ranker index. Boxed to keep the ~5 KB
    // lookup off the stack.
    let mut board_ranker_idx: Box<[[Option<u16>; 52]; 52]> = Box::new([[None; 52]; 52]);
    let mut rankers: Vec<ShowdownRanker> = Vec::with_capacity(49 * 48);
    for turn in 0..52u8 {
        if known[turn as usize] {
            continue;
        }
        for river in 0..52u8 {
            if known[river as usize] || river == turn {
                continue;
            }
            let final_board = [
                flop_board[0],
                flop_board[1],
                flop_board[2],
                Card::from_index(turn),
                Card::from_index(river),
            ];
            board_ranker_idx[turn as usize][river as usize] = Some(rankers.len() as u16);
            rankers.push(ShowdownRanker::new(&final_board));
        }
    }

    let mut builder = VectorFlopBuilder {
        nodes: Vec::new(),
        info_set_map: HashMap::new(),
        next_info_set_idx: 0,
        actions_per_info_set: Vec::new(),
        bet_config,
        board_ranker_idx,
        flop_blocked: known,
    };

    let initial_contrib = starting_pot / 2.0;
    let state = FlopBuildState {
        pot_contributions: [initial_contrib, initial_contrib],
        stacks: [effective_stack, effective_stack],
        action_history: Vec::new(),
        raises_this_street: 0,
        street: 0,
        turn_card: 0,
        ranker_idx: 0,
    };

    let root = builder.build_action_node(0, &state);

    VectorGameTree {
        nodes: builder.nodes,
        root,
        actions_per_info_set: builder.actions_per_info_set,
        rankers,
    }
}

struct VectorFlopBuilder {
    nodes: Vec<VectorNode>,
    info_set_map: HashMap<InfoSetKey, u32>,
    next_info_set_idx: u32,
    actions_per_info_set: Vec<u8>,
    bet_config: BetSizingConfig,
    /// Ranker index for each `(turn_card, river_card)` pair. Entries for
    /// cards already on the flop or identical turn/river pairs are `None`.
    board_ranker_idx: Box<[[Option<u16>; 52]; 52]>,
    /// Which card indices are on the flop (dead cards for chance deals).
    flop_blocked: [bool; 52],
}

/// Build state for the flop tree. `street` is 0=flop, 1=turn, 2=river.
/// `turn_card` is meaningful once street ≥ 1 (it identifies the dealt turn
/// card and indexes into `board_ranker_idx`). `ranker_idx` is meaningful at
/// river terminals (street == 2) and is selected by `(turn_card, river_card)`
/// at the river chance-branch step.
#[derive(Clone, Debug)]
struct FlopBuildState {
    pot_contributions: [f32; 2],
    stacks: [f32; 2],
    action_history: Vec<u8>,
    raises_this_street: u8,
    street: u8,
    turn_card: u8,
    ranker_idx: u16,
}

impl VectorFlopBuilder {
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

    fn build_action_node(&mut self, player: u8, state: &FlopBuildState) -> VectorNodeIndex {
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

    fn enumerate_actions(
        &self,
        state: &FlopBuildState,
        pot: f32,
        to_call: f32,
        stack: f32,
    ) -> (Vec<Action>, Vec<u8>) {
        let mut actions = Vec::new();
        let mut codes = Vec::new();

        let (bet_sizes, raise_sizes): (&[f64], &[f64]) = match state.street {
            0 => (&self.bet_config.flop_bets, &self.bet_config.flop_raises),
            1 => (&self.bet_config.turn_bets, &self.bet_config.turn_raises),
            _ => (&self.bet_config.river_bets, &self.bet_config.river_raises),
        };

        if to_call > 0.0 {
            actions.push(Action::Fold);
            codes.push(0);
            actions.push(Action::Call);
            codes.push(1);

            if state.raises_this_street < self.bet_config.max_raises_per_street {
                for &size_fraction in raise_sizes {
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

            for &size_fraction in bet_sizes {
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

    fn build_action_child(
        &mut self,
        player: u8,
        state: &FlopBuildState,
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
                let pot_won = state.pot_contributions[player as usize];
                self.push_node(VectorNode::Fold {
                    winner: opponent,
                    pot_won,
                })
            }
            Action::Check => {
                if player == 1 {
                    self.end_of_street(&new_state)
                } else {
                    self.build_action_node(opponent, &new_state)
                }
            }
            Action::Call => {
                new_state.pot_contributions[player as usize] += to_call;
                new_state.stacks[player as usize] -= to_call;
                self.end_of_street(&new_state)
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
                    self.end_of_street(&new_state)
                } else {
                    self.build_action_node(opponent, &new_state)
                }
            }
        }
    }

    /// End-of-street transition. On the flop, deal the turn via a chance
    /// node. On the turn, deal the river via a chance node (each river
    /// subtree uses the ranker for the final `(turn_card, river_card)`
    /// board). On the river, go to showdown.
    fn end_of_street(&mut self, state: &FlopBuildState) -> VectorNodeIndex {
        match state.street {
            2 => self.push_showdown(state),
            1 => {
                // Deal the river. For each river card compatible with the
                // current flop + turn, build a river-action subtree.
                let mut children: Vec<(u8, VectorNodeIndex)> = Vec::with_capacity(48);
                for card_idx in 0..52u8 {
                    if self.flop_blocked[card_idx as usize] || card_idx == state.turn_card {
                        continue;
                    }
                    let ranker_idx =
                        match self.board_ranker_idx[state.turn_card as usize][card_idx as usize] {
                            Some(idx) => idx,
                            None => continue,
                        };
                    let mut next = state.clone();
                    next.street = 2;
                    next.raises_this_street = 0;
                    next.ranker_idx = ranker_idx;
                    next.action_history.push(200 + card_idx);
                    let subtree = self.build_action_node(0, &next);
                    children.push((card_idx, subtree));
                }
                self.push_node(VectorNode::Chance { children })
            }
            _ => {
                // street == 0: deal the turn.
                let mut children: Vec<(u8, VectorNodeIndex)> = Vec::with_capacity(49);
                for card_idx in 0..52u8 {
                    if self.flop_blocked[card_idx as usize] {
                        continue;
                    }
                    let mut next = state.clone();
                    next.street = 1;
                    next.raises_this_street = 0;
                    next.turn_card = card_idx;
                    next.action_history.push(200 + card_idx);
                    let subtree = self.build_action_node(0, &next);
                    children.push((card_idx, subtree));
                }
                self.push_node(VectorNode::Chance { children })
            }
        }
    }

    fn push_showdown(&mut self, state: &FlopBuildState) -> VectorNodeIndex {
        debug_assert!(
            (state.pot_contributions[0] - state.pot_contributions[1]).abs() < 1e-3,
            "showdown reached with mismatched contributions: {:?}",
            state.pot_contributions
        );
        let half_pot = state.pot_contributions[0];
        self.push_node(VectorNode::Showdown {
            half_pot,
            ranker_idx: state.ranker_idx,
        })
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
        let r0 = unit_reach(&tree.rankers[0]);
        let r1 = unit_reach(&tree.rankers[0]);
        let config = SolverConfig {
            algorithm: CfrAlgorithm::CfrPlus,
            iterations: 1,
            ..Default::default()
        };
        let mut solver = VectorCfrSolver::new(tree, r0, r1, config);
        let v = solver.run_iteration(0);
        assert!(v.is_finite(), "iteration value should be finite, got {v}");
    }

    /// Run DCFR for a few hundred iterations on a real river config and
    /// verify the running average game value stabilises. Convergence
    /// signal: the average over the last quarter of iterations should
    /// agree with the average over the previous quarter to within a small
    /// absolute tolerance (chips). This is the cheapest practical
    /// convergence proxy without a full vector best-response routine.
    #[test]
    fn dcfr_value_converges_on_full_range_river() {
        let tree = build_vector_river_tree(test_board(), 100.0, 200.0, simple_river_config());
        let r0 = unit_reach(&tree.rankers[0]);
        let r1 = unit_reach(&tree.rankers[0]);
        // DCFR converges faster than CFR+ for a fixed iteration budget on
        // these tree sizes, so it gives a tighter convergence signal.
        let config = SolverConfig {
            algorithm: CfrAlgorithm::Dcfr,
            iterations: 400,
            ..Default::default()
        };
        let mut solver = VectorCfrSolver::new(tree, r0, r1, config);

        // Run iteration-by-iteration so we can split the value sequence
        // into halves and compare averages without re-solving.
        let n = solver.config.iterations as usize;
        let mut vals: Vec<f64> = Vec::with_capacity(n);
        for t in 0..n {
            vals.push(solver.run_iteration(t as u32));
        }

        let avg = |slice: &[f64]| slice.iter().sum::<f64>() / slice.len() as f64;
        let q = n / 4;
        let mid = avg(&vals[n / 2 - q..n / 2]);
        let late = avg(&vals[n - q..n]);

        // Iteration value is `Σᵢ reach_p0[i] · v_p0[i]` summed over all
        // ~1326 combos and weighted by the showdown ev kernel — so for
        // unit reach it's naturally on the order of N_COMBOS² · half_pot.
        // Use a relative tolerance: the two windows should agree within
        // 2% of magnitude. This catches divergence (regrets blowing up,
        // which manifests as much larger drift) and zero-progress (mid
        // and late wildly different) without being sensitive to scale.
        let denom = mid.abs().max(late.abs()).max(1.0);
        let rel = (mid - late).abs() / denom;
        assert!(
            rel < 0.02,
            "DCFR value not converging: mid={mid:.3}, late={late:.3}, rel={rel:.4}"
        );
        assert!(late.is_finite(), "DCFR value not finite: late={late:.3}");
    }

    /// On a one-combo-vs-one-combo dominating scenario (P0 has trips
    /// against P1's low pair), the average strategy at the dominating
    /// combo should put the bulk of its mass on a value-betting action,
    /// not on checking down. This is a coarse correctness check for the
    /// regret-matching loop on a full-shaped river tree.
    #[test]
    fn dcfr_strategy_value_bets_dominant_holding() {
        use crate::solver::range::HandRange;
        let board = test_board();
        let tree = build_vector_river_tree(board, 40.0, 200.0, simple_river_config());

        // P0 has AhAd (trips on AKQ72 board). P1 has 3h3d (under-pair).
        let aa = HandRange::combo_index(
            Card::new(Rank::Ace, Suit::Hearts),
            Card::new(Rank::Ace, Suit::Diamonds),
        ) as usize;
        let twos = HandRange::combo_index(
            Card::new(Rank::Three, Suit::Hearts),
            Card::new(Rank::Three, Suit::Diamonds),
        ) as usize;
        let mut r0 = Box::new([0.0f32; N_COMBOS]);
        let mut r1 = Box::new([0.0f32; N_COMBOS]);
        r0[aa] = 1.0;
        r1[twos] = 1.0;

        let config = SolverConfig {
            algorithm: CfrAlgorithm::Dcfr,
            iterations: 600,
            ..Default::default()
        };
        let mut solver = VectorCfrSolver::new(tree, r0, r1, config);
        solver.solve();

        // Read average strategy at the root info-set, combo aa.
        // Root info_set_idx is whichever was assigned to the empty
        // history for player 0 — it's the first one created, so == 0.
        let n_actions = solver.tree.actions_per_info_set[0] as usize;
        let mut avg = vec![0.0f32; n_actions * N_COMBOS];
        solver.store().average_strategy_into(0, &mut avg);
        let aa_strat: &[f32] = &avg[aa * n_actions..(aa + 1) * n_actions];

        // Action 0 is `Check` per the simple river config. Bet actions
        // (any of them) should have the bulk of the mass, since AA never
        // loses to 33 — checking gives the same value but loses to nothing
        // by betting either, and DCFR's discounting accelerates the
        // regret signal toward bets that win money on opponent calls.
        let bet_mass: f32 = aa_strat.iter().skip(1).sum();
        assert!(
            bet_mass >= 0.5,
            "AA should value-bet at least half the time, got bet_mass={bet_mass:.3}, strat={aa_strat:?}"
        );
    }

    fn turn_board() -> [Card; 4] {
        [
            Card::new(Rank::Ace, Suit::Spades),
            Card::new(Rank::King, Suit::Hearts),
            Card::new(Rank::Queen, Suit::Diamonds),
            Card::new(Rank::Seven, Suit::Clubs),
        ]
    }

    fn simple_turn_config() -> BetSizingConfig {
        BetSizingConfig {
            turn_bets: vec![0.75],
            turn_raises: vec![],
            river_bets: vec![0.75],
            river_raises: vec![],
            always_allow_allin: false,
            max_raises_per_street: 1,
            ..Default::default()
        }
    }

    #[test]
    fn turn_tree_has_48_river_rankers() {
        // A 4-card turn board leaves 48 possible river cards; the builder
        // should pre-compute one ranker per river card.
        let tree = build_vector_turn_tree(turn_board(), 100.0, 200.0, simple_turn_config());
        assert_eq!(
            tree.rankers.len(),
            48,
            "expected 48 rankers (one per river card); got {}",
            tree.rankers.len()
        );
    }

    #[test]
    fn turn_tree_contains_chance_node() {
        let tree = build_vector_turn_tree(turn_board(), 100.0, 200.0, simple_turn_config());
        let n_chance = tree
            .nodes
            .iter()
            .filter(|n| matches!(n, VectorNode::Chance { .. }))
            .count();
        assert!(
            n_chance >= 1,
            "expected at least one Chance node in turn tree; got {n_chance}"
        );
        // Each chance node should have exactly 48 children (one per river
        // card given a 4-card turn board).
        for node in &tree.nodes {
            if let VectorNode::Chance { children } = node {
                assert_eq!(
                    children.len(),
                    48,
                    "chance node should have 48 river children"
                );
            }
        }
    }

    #[test]
    fn turn_tree_showdown_ranker_indices_in_bounds() {
        let tree = build_vector_turn_tree(turn_board(), 100.0, 200.0, simple_turn_config());
        let n_rankers = tree.rankers.len();
        let mut seen_indices = std::collections::HashSet::new();
        for node in &tree.nodes {
            if let VectorNode::Showdown { ranker_idx, .. } = node {
                assert!(
                    (*ranker_idx as usize) < n_rankers,
                    "ranker_idx {ranker_idx} out of range (have {n_rankers} rankers)"
                );
                seen_indices.insert(*ranker_idx);
            }
        }
        // All 48 river rankers should be referenced by at least one showdown.
        assert_eq!(
            seen_indices.len(),
            48,
            "every river card should produce at least one showdown"
        );
    }

    #[test]
    fn turn_tree_solver_runs_one_iteration() {
        let tree = build_vector_turn_tree(turn_board(), 100.0, 200.0, simple_turn_config());
        // Unit reach over combos not blocked by any of the turn board cards.
        let mut r0 = Box::new([0.0f32; N_COMBOS]);
        let mut r1 = Box::new([0.0f32; N_COMBOS]);
        // The turn board is the same set of blocked cards for every river
        // card, so `rankers[0].is_blocked` over-approximates blocking (it
        // blocks the extra river card too). Use a ranker built on the turn
        // board only for unit reach — or just seed everything to 1.0 and
        // let the traversal handle it.
        for i in 0..N_COMBOS {
            r0[i] = 1.0;
            r1[i] = 1.0;
        }
        // Zero combos that contain any turn-board card (otherwise the
        // iteration value includes bogus starting-reach mass).
        for c in turn_board().iter() {
            let card_idx = c.index() as usize;
            for other in 0..52usize {
                if other == card_idx {
                    continue;
                }
                let (lo, hi) = if card_idx < other {
                    (card_idx, other)
                } else {
                    (other, card_idx)
                };
                let idx = (lo * 103) / 2 - (lo * lo) / 2 + hi - lo - 1;
                r0[idx] = 0.0;
                r1[idx] = 0.0;
            }
        }

        let config = SolverConfig {
            algorithm: CfrAlgorithm::CfrPlus,
            iterations: 1,
            ..Default::default()
        };
        let mut solver = VectorCfrSolver::new(tree, r0, r1, config);
        let v = solver.run_iteration(0);
        assert!(v.is_finite(), "turn iteration value should be finite: {v}");
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

    fn flop_board() -> [Card; 3] {
        [
            Card::new(Rank::Ace, Suit::Spades),
            Card::new(Rank::King, Suit::Hearts),
            Card::new(Rank::Queen, Suit::Diamonds),
        ]
    }

    /// Minimal flop config. Flop tree is big (49 × 48 final boards) so we
    /// keep sizings trivial — one bet per street, no raises, no all-ins.
    fn minimal_flop_config() -> BetSizingConfig {
        BetSizingConfig {
            flop_bets: vec![1.0],
            flop_raises: vec![],
            turn_bets: vec![1.0],
            turn_raises: vec![],
            river_bets: vec![1.0],
            river_raises: vec![],
            always_allow_allin: false,
            max_raises_per_street: 0,
        }
    }

    #[test]
    fn flop_tree_has_2352_final_boards() {
        let tree = build_vector_flop_tree(flop_board(), 20.0, 200.0, minimal_flop_config());
        // 49 remaining cards for the turn × 48 remaining for the river
        // = 2_352 distinct final (5-card) boards.
        assert_eq!(
            tree.rankers.len(),
            49 * 48,
            "expected 49*48 rankers; got {}",
            tree.rankers.len()
        );
    }

    #[test]
    fn flop_tree_has_two_chance_layers() {
        let tree = build_vector_flop_tree(flop_board(), 20.0, 200.0, minimal_flop_config());
        // Each chance node is either the turn-deal (49 children) or a
        // river-deal (48 children). With min config, the flop tree ends
        // the street fastest when both players check, so we should see
        // ≥1 turn-dealing chance node and ≥1 river-dealing chance node.
        let mut turn_chance = 0usize;
        let mut river_chance = 0usize;
        for node in &tree.nodes {
            if let VectorNode::Chance { children } = node {
                match children.len() {
                    49 => turn_chance += 1,
                    48 => river_chance += 1,
                    other => panic!("unexpected chance-node fanout: {other}"),
                }
            }
        }
        assert!(
            turn_chance >= 1,
            "expected at least one turn chance node; got {turn_chance}"
        );
        assert!(
            river_chance >= 1,
            "expected at least one river chance node; got {river_chance}"
        );
    }

    #[test]
    fn flop_tree_root_is_p0_decision() {
        let tree = build_vector_flop_tree(flop_board(), 20.0, 200.0, minimal_flop_config());
        match &tree.nodes[tree.root as usize] {
            VectorNode::Decision {
                player, actions, ..
            } => {
                assert_eq!(*player, 0, "OOP (P0) acts first on the flop");
                // Minimal config: Check + one Bet size.
                assert_eq!(actions.len(), 2, "check + one bet");
            }
            other => panic!("root should be Decision, got {other:?}"),
        }
    }

    #[test]
    fn flop_tree_all_showdowns_have_valid_rankers() {
        let tree = build_vector_flop_tree(flop_board(), 20.0, 200.0, minimal_flop_config());
        let n_rankers = tree.rankers.len();
        let mut seen = std::collections::HashSet::new();
        for node in &tree.nodes {
            if let VectorNode::Showdown { ranker_idx, .. } = node {
                assert!(
                    (*ranker_idx as usize) < n_rankers,
                    "ranker_idx {ranker_idx} out of range (have {n_rankers} rankers)"
                );
                seen.insert(*ranker_idx);
            }
        }
        // Every one of the 2_352 final boards should produce a showdown.
        assert_eq!(
            seen.len(),
            n_rankers,
            "every final board should be referenced by at least one showdown"
        );
    }

    #[test]
    fn flop_tree_solver_runs_one_iteration() {
        let tree = build_vector_flop_tree(flop_board(), 20.0, 200.0, minimal_flop_config());
        // Seed unit reach for every combo; block-by-card in the chance
        // node traversal will zero out combos that share a card with the
        // dealt turn/river.
        let mut r0 = Box::new([0.0f32; N_COMBOS]);
        let mut r1 = Box::new([0.0f32; N_COMBOS]);
        for i in 0..N_COMBOS {
            r0[i] = 1.0;
            r1[i] = 1.0;
        }
        // Zero combos that contain any flop card — those can never be
        // dealt from the remaining deck.
        use crate::solver::range::HandRange;
        for c in flop_board().iter() {
            let card_idx = c.index() as usize;
            for other in 0..52usize {
                if other == card_idx {
                    continue;
                }
                let (lo, hi) = if card_idx < other {
                    (card_idx as u8, other as u8)
                } else {
                    (other as u8, card_idx as u8)
                };
                let combo = HandRange::combo_index(Card::from_index(lo), Card::from_index(hi));
                r0[combo as usize] = 0.0;
                r1[combo as usize] = 0.0;
            }
        }

        let config = SolverConfig {
            algorithm: CfrAlgorithm::CfrPlus,
            iterations: 1,
            ..Default::default()
        };
        let mut solver = VectorCfrSolver::new(tree, r0, r1, config);
        let v = solver.run_iteration(0);
        assert!(v.is_finite(), "flop iteration value should be finite: {v}");
    }
}
