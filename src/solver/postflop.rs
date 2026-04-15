use std::collections::HashMap;

use crate::cards::card::Card;
use crate::solver::abstraction::CardAbstraction;
use crate::solver::action::{Action, BetSizingConfig};
use crate::solver::game_tree::{GameTree, GameTreeNode, NodeIndex};
use crate::solver::range::HandRange;

/// Configuration for building a postflop game tree.
pub struct PostflopConfig {
    /// Board cards (3 for flop, 4 for turn, 5 for river).
    pub board: Vec<Card>,
    /// Player 0's (OOP / out of position) preflop range.
    pub range_oop: HandRange,
    /// Player 1's (IP / in position) preflop range.
    pub range_ip: HandRange,
    /// Starting pot size (sum of both players' contributions so far).
    pub starting_pot: f32,
    /// Effective stack remaining (chips behind for each player, assumed equal).
    pub effective_stack: f32,
    /// Bet sizing configuration.
    pub bet_config: BetSizingConfig,
    /// Card abstraction for bucketing hands.
    pub abstraction: Box<dyn CardAbstraction>,
}

/// Builds a postflop game tree from a configuration.
pub struct PostflopTreeBuilder {
    nodes: Vec<GameTreeNode>,
    info_set_map: HashMap<InfoSetKey, u32>,
    next_info_set_idx: u32,
    actions_per_info_set: Vec<u8>,
    config: PostflopConfig,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct InfoSetKey {
    player: u8,
    card_bucket: u16,
    history: Vec<u8>,
}

/// State tracked during tree construction.
#[derive(Clone, Debug)]
struct BuildState {
    /// Current street: 0=flop, 1=turn, 2=river.
    street: u8,
    /// Current board cards.
    board: Vec<Card>,
    /// Amount each player has put into the pot so far.
    pot_contributions: [f32; 2],
    /// Stack remaining for each player.
    stacks: [f32; 2],
    /// Action history for info set keying (encoded per street).
    action_history: Vec<u8>,
    /// Number of raises so far on the current street.
    raises_this_street: u8,
    /// Whether the current street's action is complete
    /// (both players have acted and no outstanding bet).
    last_action: Option<LastAction>,
}

#[derive(Clone, Debug)]
enum LastAction {
    Check,
    Bet,
    Call,
    Raise,
}

impl PostflopTreeBuilder {
    pub fn new(config: PostflopConfig) -> Self {
        Self {
            nodes: Vec::new(),
            info_set_map: HashMap::new(),
            next_info_set_idx: 0,
            actions_per_info_set: Vec::new(),
            config,
        }
    }

    /// Build the game tree and return it.
    pub fn build(mut self) -> GameTree {
        let initial_contrib = self.config.starting_pot / 2.0;
        let state = BuildState {
            street: match self.config.board.len() {
                3 => 0, // flop
                4 => 1, // turn
                5 => 2, // river
                _ => panic!("Board must have 3, 4, or 5 cards"),
            },
            board: self.config.board.clone(),
            pot_contributions: [initial_contrib, initial_contrib],
            stacks: [self.config.effective_stack, self.config.effective_stack],
            action_history: Vec::new(),
            raises_this_street: 0,
            last_action: None,
        };

        // P0 (OOP) acts first on each street
        let root = self.build_action_node(0, &state);

        GameTree {
            nodes: self.nodes,
            root,
            num_info_sets: self.next_info_set_idx,
            actions_per_info_set: self.actions_per_info_set,
        }
    }

    fn push_node(&mut self, node: GameTreeNode) -> NodeIndex {
        let idx = self.nodes.len() as NodeIndex;
        self.nodes.push(node);
        idx
    }

    fn get_or_create_info_set(&mut self, player: u8, card_bucket: u16, history: &[u8], n_actions: u8) -> u32 {
        let key = InfoSetKey {
            player,
            card_bucket,
            history: history.to_vec(),
        };
        if let Some(&idx) = self.info_set_map.get(&key) {
            idx
        } else {
            let idx = self.next_info_set_idx;
            self.info_set_map.insert(key, idx);
            self.next_info_set_idx += 1;
            self.actions_per_info_set.push(n_actions);
            idx
        }
    }

    /// Build an action node where `player` must act.
    fn build_action_node(&mut self, player: u8, state: &BuildState) -> NodeIndex {
        let pot = state.pot_contributions[0] + state.pot_contributions[1];
        let to_call = state.pot_contributions[1 - player as usize] - state.pot_contributions[player as usize];
        let stack = state.stacks[player as usize];

        // Determine available actions
        let mut actions: Vec<Action> = Vec::new();
        let mut action_codes: Vec<u8> = Vec::new(); // for history encoding

        if to_call > 0.0 {
            // Facing a bet/raise: can fold, call, or raise
            actions.push(Action::Fold);
            action_codes.push(0);

            actions.push(Action::Call);
            action_codes.push(1);

            // Raise options (if allowed)
            if state.raises_this_street < self.config.bet_config.max_raises_per_street {
                let raise_sizes = self.get_raise_sizes(state.street);
                for &size_fraction in raise_sizes {
                    let raise_amount = (pot + to_call) * size_fraction as f32;
                    let total_to_put_in = to_call + raise_amount;
                    if total_to_put_in < stack {
                        actions.push(Action::bet_from_fraction(size_fraction));
                        action_codes.push(2 + (size_fraction * 100.0) as u8);
                    }
                }
                // All-in
                if self.config.bet_config.always_allow_allin && stack > to_call {
                    actions.push(Action::AllIn);
                    action_codes.push(255);
                }
            }
        } else {
            // No bet to face: can check or bet
            actions.push(Action::Check);
            action_codes.push(0);

            let bet_sizes = self.get_bet_sizes(state.street);
            for &size_fraction in bet_sizes {
                let bet_amount = pot * size_fraction as f32;
                if bet_amount < stack {
                    actions.push(Action::bet_from_fraction(size_fraction));
                    action_codes.push(1 + (size_fraction * 100.0) as u8);
                }
            }
            // All-in
            if self.config.bet_config.always_allow_allin && stack > 0.0 {
                actions.push(Action::AllIn);
                action_codes.push(255);
            }
        }

        let n_actions = actions.len() as u8;

        // Use a placeholder bucket (0) - in the actual CFR traversal,
        // the bucket depends on the player's specific hand.
        // For tree construction, we use a single representative info set per
        // (player, history) pair since all hands at the same decision point
        // have the same available actions.
        let info_set_idx = self.get_or_create_info_set(player, 0, &state.action_history, n_actions);

        // Build children
        let mut children: Vec<NodeIndex> = Vec::new();
        for (i, action) in actions.iter().enumerate() {
            let child = self.build_action_child(player, state, action, action_codes[i]);
            children.push(child);
        }

        self.push_node(GameTreeNode::Decision {
            player,
            actions: actions.clone(),
            children,
            info_set_idx,
        })
    }

    /// Build the child node resulting from `player` taking `action`.
    fn build_action_child(
        &mut self,
        player: u8,
        state: &BuildState,
        action: &Action,
        action_code: u8,
    ) -> NodeIndex {
        let opponent = 1 - player;
        let pot = state.pot_contributions[0] + state.pot_contributions[1];
        let to_call = state.pot_contributions[opponent as usize] - state.pot_contributions[player as usize];

        let mut new_state = state.clone();
        new_state.action_history.push(action_code);

        match action {
            Action::Fold => {
                // Player folds - opponent wins the pot
                let payoff_p0 = if player == 0 {
                    -(state.pot_contributions[0]) // P0 loses what they put in
                } else {
                    state.pot_contributions[1] // P0 wins what P1 put in
                };
                self.push_node(GameTreeNode::Terminal { payoff_p0 })
            }
            Action::Check => {
                if player == 1 {
                    // Both checked - end of street
                    self.end_of_street(&new_state)
                } else {
                    // P1 still to act
                    new_state.last_action = Some(LastAction::Check);
                    self.build_action_node(opponent, &new_state)
                }
            }
            Action::Call => {
                // Match the bet
                new_state.pot_contributions[player as usize] += to_call;
                new_state.stacks[player as usize] -= to_call;
                new_state.last_action = Some(LastAction::Call);
                // End of street (call closes the action)
                self.end_of_street(&new_state)
            }
            Action::Bet(bp) => {
                let size_fraction = *bp as f32 / 10000.0;
                let amount = if to_call > 0.0 {
                    // Raise: call + raise
                    let raise_amount = (pot + to_call) * size_fraction;
                    to_call + raise_amount
                } else {
                    // Open bet
                    pot * size_fraction
                };
                let amount = amount.min(new_state.stacks[player as usize]);
                new_state.pot_contributions[player as usize] += amount;
                new_state.stacks[player as usize] -= amount;
                new_state.raises_this_street += 1;
                new_state.last_action = Some(LastAction::Bet);
                self.build_action_node(opponent, &new_state)
            }
            Action::AllIn => {
                let amount = new_state.stacks[player as usize];
                new_state.pot_contributions[player as usize] += amount;
                new_state.stacks[player as usize] = 0.0;
                new_state.raises_this_street += 1;
                new_state.last_action = Some(LastAction::Raise);

                // If opponent is also all-in or has nothing to call, go to showdown
                if new_state.stacks[opponent as usize] == 0.0 {
                    self.run_out_board(&new_state)
                } else {
                    self.build_action_node(opponent, &new_state)
                }
            }
        }
    }

    /// End of street: either deal next card or go to showdown.
    fn end_of_street(&mut self, state: &BuildState) -> NodeIndex {
        if state.street == 2 || state.board.len() == 5 {
            // River - showdown
            self.showdown(state)
        } else if state.stacks[0] == 0.0 || state.stacks[1] == 0.0 {
            // Someone is all-in - run out remaining cards
            self.run_out_board(state)
        } else {
            // Deal next card
            self.deal_next_street(state)
        }
    }

    /// Deal the next community card (chance node).
    fn deal_next_street(&mut self, state: &BuildState) -> NodeIndex {
        let new_street = state.street + 1;
        // For simplicity, use a fixed set of representative cards
        // (full implementation would enumerate all possible cards minus dead cards).
        // Here we use the abstraction: group cards into buckets.

        // Deal all possible next cards (52 minus known cards)
        let known: Vec<u8> = state.board.iter().map(|c| c.index()).collect();
        // Note: in a full implementation, we'd also exclude player hole cards,
        // but the tree is hand-independent. Cards are handled at solve time.

        let possible_cards: Vec<Card> = (0..52u8)
            .filter(|i| !known.contains(i))
            .map(Card::from_index)
            .collect();

        let mut children: Vec<(u8, NodeIndex)> = Vec::new();

        for &card in &possible_cards {
            let mut new_state = state.clone();
            new_state.board.push(card);
            new_state.street = new_street;
            new_state.raises_this_street = 0;
            new_state.last_action = None;
            // Add a street separator to the history
            new_state.action_history.push(200 + card.index());

            // P0 (OOP) acts first on the new street
            let child = self.build_action_node(0, &new_state);
            children.push((card.index(), child));
        }

        self.push_node(GameTreeNode::Chance { children })
    }

    /// Run out remaining board cards when all-in.
    fn run_out_board(&mut self, state: &BuildState) -> NodeIndex {
        if state.board.len() >= 5 {
            return self.showdown(state);
        }

        // Deal remaining cards as chance nodes
        let known: Vec<u8> = state.board.iter().map(|c| c.index()).collect();
        let possible_cards: Vec<Card> = (0..52u8)
            .filter(|i| !known.contains(i))
            .map(Card::from_index)
            .collect();

        let mut children: Vec<(u8, NodeIndex)> = Vec::new();

        for &card in &possible_cards {
            let mut new_state = state.clone();
            new_state.board.push(card);
            new_state.street = (new_state.board.len() as u8).saturating_sub(3);
            let child = self.run_out_board(&new_state);
            children.push((card.index(), child));
        }

        self.push_node(GameTreeNode::Chance { children })
    }

    /// Showdown: terminal node with payoff determined at solve time.
    /// For tree construction, we use a payoff of 0 (actual payoff depends on
    /// hands and is computed during CFR traversal).
    fn showdown(&mut self, state: &BuildState) -> NodeIndex {
        // Payoff = what P0 wins. At showdown, the winner gets the opponent's contribution.
        // The actual payoff depends on hand comparison and is handled during solve-time.
        // For now, store the pot size; the solver will multiply by +1 or -1 based on who wins.
        let pot_won = state.pot_contributions[1]; // What P0 wins if they have the best hand
        self.push_node(GameTreeNode::Terminal {
            payoff_p0: pot_won, // Placeholder: will be adjusted at solve time
        })
    }

    fn get_bet_sizes(&self, street: u8) -> &[f64] {
        match street {
            0 => &self.config.bet_config.flop_bets,
            1 => &self.config.bet_config.turn_bets,
            _ => &self.config.bet_config.river_bets,
        }
    }

    fn get_raise_sizes(&self, street: u8) -> &[f64] {
        match street {
            0 => &self.config.bet_config.flop_raises,
            1 => &self.config.bet_config.turn_raises,
            _ => &self.config.bet_config.river_raises,
        }
    }
}

/// Build a postflop game tree for a river spot (simplest real-world scenario).
///
/// This builds a tree for a single street with no further cards to deal,
/// making it equivalent to a solved river subgame.
pub fn build_river_tree(
    board: [Card; 5],
    starting_pot: f32,
    effective_stack: f32,
    bet_config: BetSizingConfig,
) -> GameTree {
    use crate::solver::abstraction::NoAbstraction;

    let config = PostflopConfig {
        board: board.to_vec(),
        range_oop: HandRange::full(),
        range_ip: HandRange::full(),
        starting_pot,
        effective_stack,
        bet_config,
        abstraction: Box::new(NoAbstraction::new(1326)),
    };

    PostflopTreeBuilder::new(config).build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::card::{Rank, Suit};
    use crate::solver::action::BetSizingConfig;

    fn simple_river_config() -> BetSizingConfig {
        BetSizingConfig {
            river_bets: vec![0.5, 1.0],
            river_raises: vec![1.0],
            always_allow_allin: false,
            max_raises_per_street: 1,
            ..Default::default()
        }
    }

    fn test_board() -> [Card; 5] {
        [
            Card::new(Rank::Ace, Suit::Spades),
            Card::new(Rank::King, Suit::Hearts),
            Card::new(Rank::Queen, Suit::Diamonds),
            Card::new(Rank::Seven, Suit::Clubs),
            Card::new(Rank::Two, Suit::Spades),
        ]
    }

    #[test]
    fn river_tree_builds_successfully() {
        let tree = build_river_tree(test_board(), 100.0, 200.0, simple_river_config());
        assert!(tree.nodes.len() > 5, "River tree should have multiple nodes");
        assert!(tree.num_info_sets > 0, "Should have info sets");
    }

    #[test]
    fn river_tree_has_correct_actions() {
        let tree = build_river_tree(test_board(), 100.0, 200.0, simple_river_config());

        // Root should be a decision node for P0 (OOP)
        match &tree.nodes[tree.root as usize] {
            GameTreeNode::Decision {
                player, actions, ..
            } => {
                assert_eq!(*player, 0, "OOP (P0) should act first");
                // Should have: Check, Bet(50%), Bet(100%)
                assert_eq!(actions.len(), 3, "Should have check + 2 bet sizes");
                assert_eq!(actions[0], Action::Check);
            }
            _ => panic!("Root should be a decision node"),
        }
    }

    #[test]
    fn river_tree_has_terminal_nodes() {
        let tree = build_river_tree(test_board(), 100.0, 200.0, simple_river_config());

        let terminal_count = tree
            .nodes
            .iter()
            .filter(|n| matches!(n, GameTreeNode::Terminal { .. }))
            .count();
        assert!(
            terminal_count > 5,
            "Should have multiple terminal nodes, got {terminal_count}"
        );
    }

    #[test]
    fn river_tree_fold_payoff() {
        let tree = build_river_tree(test_board(), 100.0, 200.0, simple_river_config());

        // Find a terminal node from a fold
        // After P0 checks, P1 bets, P0 folds: P0 loses their contribution (50)
        // Starting pot = 100, so each contributed 50
        let has_fold_terminal = tree.nodes.iter().any(|n| {
            matches!(n, GameTreeNode::Terminal { payoff_p0 } if *payoff_p0 < 0.0)
        });
        assert!(has_fold_terminal, "Should have fold terminal nodes with negative payoff for P0");
    }

    #[test]
    fn river_tree_solves_with_cfr() {
        use crate::solver::cfr::{CfrAlgorithm, CfrSolver, SolverConfig};

        let tree = build_river_tree(test_board(), 100.0, 200.0, simple_river_config());
        let config = SolverConfig {
            algorithm: CfrAlgorithm::CfrPlus,
            iterations: 1_000,
            ..Default::default()
        };
        let mut solver = CfrSolver::new(tree, config);
        solver.solve();

        // Verify strategies are valid probability distributions
        let strategy = solver.average_strategy();
        for idx in 0..solver.tree.num_info_sets {
            let strat = strategy.strategy_at(idx);
            let sum: f32 = strat.iter().sum();
            assert!(
                (sum - 1.0).abs() < 0.01,
                "Strategy at info set {idx} should sum to 1.0, got {sum}"
            );
        }
    }

    #[test]
    fn minimal_river_tree() {
        // Simplest possible: 1 bet size, no raises, no all-in
        let config = BetSizingConfig {
            river_bets: vec![1.0],
            river_raises: vec![],
            always_allow_allin: false,
            max_raises_per_street: 0,
            ..Default::default()
        };
        let tree = build_river_tree(test_board(), 100.0, 200.0, config);

        // P0: check or bet(pot)
        // If P0 checks:
        //   P1: check (showdown) or bet(pot)
        //   If P1 bets:
        //     P0: fold or call (showdown) -- no raise since max_raises=0
        // If P0 bets:
        //   P1: fold or call (showdown) -- no raise since max_raises=0
        //
        // Terminal nodes: check-check, check-bet-fold, check-bet-call, bet-fold, bet-call = 5
        let terminal_count = tree
            .nodes
            .iter()
            .filter(|n| matches!(n, GameTreeNode::Terminal { .. }))
            .count();
        assert_eq!(terminal_count, 5, "Minimal river tree should have 5 terminal nodes");
    }
}
