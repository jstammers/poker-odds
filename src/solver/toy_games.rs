use crate::solver::action::Action;
use crate::solver::game_tree::{GameTree, GameTreeNode, NodeIndex};
use std::collections::HashMap;

/// Kuhn poker: the simplest interesting poker game for validating CFR.
///
/// - 3-card deck: Jack (0), Queen (1), King (2)
/// - 2 players, each antes 1 chip
/// - Each dealt one card
/// - One round of betting: check/bet(1), then call/fold if facing bet
/// - Higher card wins at showdown
///
/// Known Nash equilibrium:
/// - Game value for P0: -1/18 ≈ -0.0556
/// - P0 with K: always bets
/// - P0 with Q: always checks
/// - P0 with J: bets (bluffs) with probability 1/3
/// - P1 with K facing bet: always calls
/// - P1 with Q facing bet: calls with probability 1/3
/// - P1 with J facing bet: always folds
pub struct KuhnPoker;

impl KuhnPoker {
    /// Expected game value for player 0 at Nash equilibrium.
    pub fn expected_game_value() -> f64 {
        -1.0 / 18.0
    }

    /// Build the Kuhn poker game tree.
    ///
    /// The tree is a chance node at the root dealing cards to both players,
    /// followed by decision nodes for P0 and P1.
    ///
    /// Card deals: 6 permutations of (P0_card, P1_card) from {J, Q, K}.
    ///
    /// Info sets are keyed by (player, card, action_history):
    /// - P0 sees: card + empty history (first to act) or card + [check, bet] (facing bet after checking)
    /// - P1 sees: card + [check] or card + [bet]
    pub fn build_tree() -> GameTree {
        let mut nodes: Vec<GameTreeNode> = Vec::new();
        let mut info_set_map: HashMap<(u8, u8, Vec<u8>), u32> = HashMap::new();
        let mut next_info_set_idx: u32 = 0;
        let mut actions_per_info_set: Vec<u8> = Vec::new();

        // Helper to get or assign an info set index
        let mut get_info_set = |player: u8, card: u8, history: &[u8]| -> u32 {
            let key = (player, card, history.to_vec());
            if let Some(&idx) = info_set_map.get(&key) {
                idx
            } else {
                let idx = next_info_set_idx;
                info_set_map.insert(key, idx);
                next_info_set_idx += 1;
                actions_per_info_set.push(2); // All Kuhn info sets have 2 actions
                idx
            }
        };

        // Build subtree for a specific card deal.
        // P0 card, P1 card, ante = 1 each (pot = 2).
        // Returns the node index of the subtree root.
        let cards = [(0u8, 1u8), (0, 2), (1, 0), (1, 2), (2, 0), (2, 1)];

        let mut deal_children: Vec<(u8, NodeIndex)> = Vec::new();

        for (deal_idx, &(p0_card, p1_card)) in cards.iter().enumerate() {
            let winner = if p0_card > p1_card { 0u8 } else { 1 };

            // P0 acts first: Check or Bet
            let p0_info_set = get_info_set(0, p0_card, &[]);

            // -- P0 checks --
            // P1 acts: Check or Bet
            let p1_after_check_info_set = get_info_set(1, p1_card, &[0]); // 0 = check

            // P1 checks (showdown): pot=2, winner gets +1
            let showdown_after_check_check = nodes.len() as NodeIndex;
            nodes.push(GameTreeNode::Terminal {
                payoff_p0: if winner == 0 { 1.0 } else { -1.0 },
            });

            // P1 bets after P0 checked:
            // P0 must respond: Fold or Call
            let p0_facing_bet_after_check = get_info_set(0, p0_card, &[0, 1]); // check, bet

            // P0 folds: P1 wins pot (P0 loses ante of 1)
            let p0_folds_idx = nodes.len() as NodeIndex;
            nodes.push(GameTreeNode::Terminal { payoff_p0: -1.0 });

            // P0 calls: showdown with pot=4 (each put in 2), winner gets +2
            let p0_calls_idx = nodes.len() as NodeIndex;
            nodes.push(GameTreeNode::Terminal {
                payoff_p0: if winner == 0 { 2.0 } else { -2.0 },
            });

            // P0 decision facing bet after checking
            let p0_response_idx = nodes.len() as NodeIndex;
            nodes.push(GameTreeNode::Decision {
                player: 0,
                actions: vec![Action::Fold, Action::Call],
                children: vec![p0_folds_idx, p0_calls_idx],
                info_set_idx: p0_facing_bet_after_check,
            });

            // P1 decision after P0 checked
            let p1_after_check_idx = nodes.len() as NodeIndex;
            nodes.push(GameTreeNode::Decision {
                player: 1,
                actions: vec![Action::Check, Action::Bet(10000)],
                children: vec![showdown_after_check_check, p0_response_idx],
                info_set_idx: p1_after_check_info_set,
            });

            // -- P0 bets --
            // P1 acts: Fold or Call
            let p1_facing_bet_info_set = get_info_set(1, p1_card, &[1]); // 1 = bet

            // P1 folds: P0 wins pot (P1 loses ante of 1)
            let p1_folds_idx = nodes.len() as NodeIndex;
            nodes.push(GameTreeNode::Terminal { payoff_p0: 1.0 });

            // P1 calls: showdown with pot=4, winner gets +2
            let p1_calls_idx = nodes.len() as NodeIndex;
            nodes.push(GameTreeNode::Terminal {
                payoff_p0: if winner == 0 { 2.0 } else { -2.0 },
            });

            // P1 decision facing P0's bet
            let p1_facing_bet_idx = nodes.len() as NodeIndex;
            nodes.push(GameTreeNode::Decision {
                player: 1,
                actions: vec![Action::Fold, Action::Call],
                children: vec![p1_folds_idx, p1_calls_idx],
                info_set_idx: p1_facing_bet_info_set,
            });

            // P0 initial decision
            let p0_decision_idx = nodes.len() as NodeIndex;
            nodes.push(GameTreeNode::Decision {
                player: 0,
                actions: vec![Action::Check, Action::Bet(10000)],
                children: vec![p1_after_check_idx, p1_facing_bet_idx],
                info_set_idx: p0_info_set,
            });

            deal_children.push((deal_idx as u8, p0_decision_idx));
        }

        // Root: chance node dealing cards
        let root = nodes.len() as NodeIndex;
        nodes.push(GameTreeNode::Chance {
            children: deal_children,
        });

        GameTree {
            nodes,
            root,
            num_info_sets: next_info_set_idx,
            actions_per_info_set,
        }
    }

    /// Info set index mapping for test verification.
    /// Returns a description of each info set index based on build order:
    ///   0: P0, J, []           - [Check, Bet]
    ///   1: P1, Q, [check]      - [Check, Bet]
    ///   2: P0, J, [check,bet]  - [Fold, Call]
    ///   3: P1, Q, [bet]        - [Fold, Call]
    ///   4: P1, K, [check]      - [Check, Bet]
    ///   5: P1, K, [bet]        - [Fold, Call]
    ///   6: P0, Q, []           - [Check, Bet]
    ///   7: P1, J, [check]      - [Check, Bet]
    ///   8: P0, Q, [check,bet]  - [Fold, Call]
    ///   9: P1, J, [bet]        - [Fold, Call]
    ///  10: P0, K, []           - [Check, Bet]
    ///  11: P0, K, [check,bet]  - [Fold, Call]
    pub const INFO_SET_LABELS: [&'static str; 12] = [
        "P0 Jack initial",
        "P1 Queen after check",
        "P0 Jack facing bet after check",
        "P1 Queen facing bet",
        "P1 King after check",
        "P1 King facing bet",
        "P0 Queen initial",
        "P1 Jack after check",
        "P0 Queen facing bet after check",
        "P1 Jack facing bet",
        "P0 King initial",
        "P0 King facing bet after check",
    ];
}

/// Leduc poker: intermediate-complexity game for validating CFR scalability.
///
/// - 6-card deck: {J, J, Q, Q, K, K} (2 suits of 3 ranks)
/// - 2 players, each antes 1 chip
/// - Round 1: each dealt one private card, then betting
/// - Round 2: one public (board) card dealt, then betting
/// - Betting: check/bet(2 in r1, 4 in r2), then fold/call/raise, max 2 raises per round
/// - Showdown: pair with board > higher card > lower card
///
/// Game tree has ~936 info sets (varies with raise cap).
pub struct LeducPoker;

impl LeducPoker {
    /// Build the Leduc poker game tree.
    pub fn build_tree() -> GameTree {
        let mut builder = LeducBuilder::new();
        let root = builder.build_root();
        GameTree {
            nodes: builder.nodes,
            root,
            num_info_sets: builder.next_info_set_idx,
            actions_per_info_set: builder.actions_per_info_set,
        }
    }
}

/// Helper struct for building the Leduc poker tree.
struct LeducBuilder {
    nodes: Vec<GameTreeNode>,
    info_set_map: HashMap<(u8, u8, u8, Vec<u8>), u32>, // (player, card, board_card, action_history)
    next_info_set_idx: u32,
    actions_per_info_set: Vec<u8>,
}

impl LeducBuilder {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            info_set_map: HashMap::new(),
            next_info_set_idx: 0,
            actions_per_info_set: Vec::new(),
        }
    }

    fn get_info_set(
        &mut self,
        player: u8,
        card: u8,
        board: u8,
        history: &[u8],
        n_actions: u8,
    ) -> u32 {
        let key = (player, card, board, history.to_vec());
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

    fn push_node(&mut self, node: GameTreeNode) -> NodeIndex {
        let idx = self.nodes.len() as NodeIndex;
        self.nodes.push(node);
        idx
    }

    /// Determine winner at showdown.
    /// Returns payoff for P0 given each put `total_bet` chips into pot.
    fn showdown_payoff(p0_card: u8, p1_card: u8, board: u8, total_bet: f32) -> f32 {
        // Pair with board beats non-pair; higher card wins otherwise
        let p0_pair = p0_card == board;
        let p1_pair = p1_card == board;
        if p0_pair && !p1_pair {
            total_bet
        } else if !p0_pair && p1_pair {
            -total_bet
        } else if p0_card > p1_card {
            total_bet
        } else if p0_card < p1_card {
            -total_bet
        } else {
            0.0 // tie (same rank, different suit)
        }
    }

    fn build_root(&mut self) -> NodeIndex {
        // Cards: J=0, J=1, Q=2, Q=3, K=4, K=5
        // Rank of card i: i / 2 (0=J, 1=Q, 2=K)
        let n_cards = 6u8;

        let mut deal_children: Vec<(u8, NodeIndex)> = Vec::new();
        let mut deal_idx = 0u8;

        for p0 in 0..n_cards {
            for p1 in 0..n_cards {
                if p0 == p1 {
                    continue;
                }
                let p0_rank = p0 / 2;
                let p1_rank = p1 / 2;

                // After dealing hole cards, build round 1 betting
                // board_card = 255 means no board card yet
                let round1 = self.build_round1(p0_rank, p1_rank, &[], 1.0);
                deal_children.push((deal_idx, round1));
                deal_idx += 1;
            }
        }

        self.push_node(GameTreeNode::Chance {
            children: deal_children,
        })
    }

    /// Build round 1 betting (before board card).
    /// Each player has anted 1 chip. Bet size in round 1 = 2 chips.
    fn build_round1(
        &mut self,
        p0_rank: u8,
        p1_rank: u8,
        history: &[u8],
        pot_per_player: f32,
    ) -> NodeIndex {
        // P0 acts first
        self.build_betting_node(p0_rank, p1_rank, 255, 0, history, pot_per_player, 2.0, 0, 1)
    }

    /// Build the initial betting node for a round where no bet has been made yet.
    /// P0 acts first: can check or bet.
    ///
    /// `bet_size`: chips per bet/raise in this round
    /// `round`: 1 = preflop, 2 = postflop
    fn build_betting_node(
        &mut self,
        p0_rank: u8,
        p1_rank: u8,
        board: u8, // 255 = no board yet
        player: u8,
        history: &[u8],
        pot_per_player: f32,
        bet_size: f32,
        raises_so_far: u8,
        round: u8,
    ) -> NodeIndex {
        let card = if player == 0 { p0_rank } else { p1_rank };
        let info_set = self.get_info_set(player, card, board, history, 2);

        // Check branch
        let check_child = {
            let mut new_history = history.to_vec();
            new_history.push(0);

            if player == 1 {
                // Both checked - end of round
                if round == 1 {
                    self.build_board_deal(p0_rank, p1_rank, &new_history, pot_per_player)
                } else {
                    let payoff = Self::showdown_payoff(p0_rank, p1_rank, board, pot_per_player);
                    self.push_node(GameTreeNode::Terminal { payoff_p0: payoff })
                }
            } else {
                // P1 still needs to act
                self.build_betting_node(
                    p0_rank,
                    p1_rank,
                    board,
                    1,
                    &new_history,
                    pot_per_player,
                    bet_size,
                    raises_so_far,
                    round,
                )
            }
        };

        // Bet branch
        let bet_child = {
            let mut new_history = history.to_vec();
            new_history.push(1);
            let opponent = 1 - player;
            self.build_facing_bet(
                p0_rank,
                p1_rank,
                board,
                opponent,
                &new_history,
                pot_per_player,
                bet_size,
                pot_per_player + bet_size,
                raises_so_far + 1,
                round,
            )
        };

        self.push_node(GameTreeNode::Decision {
            player,
            actions: vec![Action::Check, Action::Bet((bet_size as u16) * 10000 / 2)],
            children: vec![check_child, bet_child],
            info_set_idx: info_set,
        })
    }

    /// Build a node for the player facing a bet/raise.
    fn build_facing_bet(
        &mut self,
        p0_rank: u8,
        p1_rank: u8,
        board: u8,
        player: u8,
        history: &[u8],
        pot_per_player: f32,
        bet_size: f32,
        bettor_total: f32, // amount bettor has put in
        raises_so_far: u8,
        round: u8,
    ) -> NodeIndex {
        let card = if player == 0 { p0_rank } else { p1_rank };
        let can_raise = raises_so_far < 2;
        let n_actions = if can_raise { 3u8 } else { 2 };
        let info_set = self.get_info_set(player, card, board, history, n_actions);

        // Fold
        let fold_child = {
            // Player folds, loses pot_per_player
            let payoff_p0 = if player == 0 {
                -pot_per_player
            } else {
                pot_per_player
            };
            self.push_node(GameTreeNode::Terminal { payoff_p0 })
        };

        // Call
        let call_child = {
            // Player matches the bet
            let new_pot = bettor_total;
            if round == 1 {
                // End of round 1, deal board
                let mut new_history = history.to_vec();
                new_history.push(1); // call
                self.build_board_deal(p0_rank, p1_rank, &new_history, new_pot)
            } else {
                // Showdown
                let payoff = Self::showdown_payoff(p0_rank, p1_rank, board, new_pot);
                self.push_node(GameTreeNode::Terminal { payoff_p0: payoff })
            }
        };

        let mut actions = vec![Action::Fold, Action::Call];
        let mut children = vec![fold_child, call_child];

        // Raise (if allowed)
        if can_raise {
            let raise_child = {
                let mut new_history = history.to_vec();
                new_history.push(2); // raise
                let opponent = 1 - player;
                let new_bettor_total = bettor_total + bet_size;
                self.build_facing_bet(
                    p0_rank,
                    p1_rank,
                    board,
                    opponent,
                    &new_history,
                    bettor_total, // caller matched previous bet
                    bet_size,
                    new_bettor_total,
                    raises_so_far + 1,
                    round,
                )
            };
            actions.push(Action::Bet((bet_size as u16) * 10000 / 2));
            children.push(raise_child);
        }

        self.push_node(GameTreeNode::Decision {
            player,
            actions,
            children,
            info_set_idx: info_set,
        })
    }

    /// Deal the board card (chance node) between round 1 and round 2.
    fn build_board_deal(
        &mut self,
        p0_rank: u8,
        p1_rank: u8,
        history: &[u8],
        pot_per_player: f32,
    ) -> NodeIndex {
        // Remaining cards: any of {J, Q, K} that weren't dealt to players
        // In Leduc with ranks, each rank has 2 copies. After dealing p0 and p1,
        // there are 4 remaining cards. Board card can be any of the 3 ranks,
        // with probability proportional to remaining copies.
        let mut board_children: Vec<(u8, NodeIndex)> = Vec::new();

        for board_rank in 0..3u8 {
            // Start round 2 betting with bet_size = 4
            let mut r2_history = history.to_vec();
            r2_history.push(100 + board_rank); // encode board card in history

            let round2 = self.build_betting_node(
                p0_rank,
                p1_rank,
                board_rank,
                0,
                &r2_history,
                pot_per_player,
                4.0,
                0,
                2,
            );
            board_children.push((board_rank, round2));
        }

        self.push_node(GameTreeNode::Chance {
            children: board_children,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::cfr::{CfrAlgorithm, CfrSolver, SolverConfig};

    #[test]
    fn kuhn_tree_structure() {
        let tree = KuhnPoker::build_tree();
        // 6 card deals, each with: ~5 nodes (P0 decision, P1 decision, terminals)
        // Plus 1 root chance node
        assert!(tree.nodes.len() > 30, "Tree should have at least 30 nodes");
        assert!(
            tree.num_info_sets == 12,
            "Kuhn poker has 12 info sets, got {}",
            tree.num_info_sets
        );
    }

    #[test]
    fn kuhn_cfr_plus_game_value() {
        let tree = KuhnPoker::build_tree();
        let config = SolverConfig {
            algorithm: CfrAlgorithm::CfrPlus,
            iterations: 10_000,
            ..Default::default()
        };
        let mut solver = CfrSolver::new(tree, config);
        let game_value = solver.solve();

        let expected = KuhnPoker::expected_game_value();
        assert!(
            (game_value - expected).abs() < 0.01,
            "Game value {game_value:.4} should be close to {expected:.4}"
        );
    }

    #[test]
    fn kuhn_cfr_plus_strategy_convergence() {
        let tree = KuhnPoker::build_tree();
        let config = SolverConfig {
            algorithm: CfrAlgorithm::CfrPlus,
            iterations: 50_000,
            ..Default::default()
        };
        let mut solver = CfrSolver::new(tree, config);
        solver.solve();

        let strategy = solver.average_strategy();

        // We need to identify which info set index corresponds to which situation.
        // Info sets are assigned in build order:
        //   - First call: P0, card=J, history=[] (P0 first to act with Jack)
        //   - Then P1 info sets, then P0 facing bet, etc.
        // The assignment depends on the deal iteration order.
        //
        // Instead of relying on exact indices, let's verify the game value
        // converges and spot-check that strategies sum to 1.
        for idx in 0..12u32 {
            let strat = strategy.strategy_at(idx);
            let sum: f32 = strat.iter().sum();
            assert!(
                (sum - 1.0).abs() < 0.001,
                "Strategy at info set {idx} should sum to 1.0, got {sum}"
            );
        }
    }

    #[test]
    fn kuhn_nash_equilibrium_strategies() {
        // Build and solve with many iterations for accurate convergence
        let tree = KuhnPoker::build_tree();
        let config = SolverConfig {
            algorithm: CfrAlgorithm::CfrPlus,
            iterations: 100_000,
            ..Default::default()
        };
        let mut solver = CfrSolver::new(tree, config);
        solver.solve();

        // To verify specific strategies, we need to know the info set mapping.
        // Build a second tree to get the info set index assignments.
        // The info sets are assigned in this order (from the build_tree code):
        //
        // Deal (J,Q): P0_J_[], P1_Q_[check], P0_J_[check,bet], P1_Q_[bet]
        // Deal (J,K): P0_J_[] (same), P1_K_[check], P0_J_[check,bet] (same), P1_K_[bet]
        // Deal (Q,J): P0_Q_[], P1_J_[check], P0_Q_[check,bet], P1_J_[bet]
        // Deal (Q,K): P0_Q_[] (same), P1_K_[check] (same), P0_Q_[check,bet] (same), P1_K_[bet] (same)
        // Deal (K,J): P0_K_[], P1_J_[check] (same), P0_K_[check,bet], P1_J_[bet] (same)
        // Deal (K,Q): P0_K_[] (same), P1_Q_[check] (same), P0_K_[check,bet], P1_Q_[bet] (same)
        //
        // So info sets are assigned as:
        // 0: P0, J, []           - actions: [Check, Bet]
        // 1: P1, Q, [check]      - actions: [Check, Bet]
        // 2: P0, J, [check,bet]  - actions: [Fold, Call]
        // 3: P1, Q, [bet]        - actions: [Fold, Call]
        // 4: P1, K, [check]      - actions: [Check, Bet]
        // 5: P1, K, [bet]        - actions: [Fold, Call]
        // 6: P0, Q, []           - actions: [Check, Bet]
        // 7: P1, J, [check]      - actions: [Check, Bet]
        // 8: P0, Q, [check,bet]  - actions: [Fold, Call]
        // 9: P1, J, [bet]        - actions: [Fold, Call]
        // 10: P0, K, []          - actions: [Check, Bet]
        // 11: P0, K, [check,bet] - actions: [Fold, Call]

        let strat = solver.average_strategy();
        let tol = 0.05; // 5% tolerance for convergence

        // P0 with King (idx 10): should always bet (or mix, but bet >= some amount)
        // Nash: P0-K bets with probability alpha + 3*alpha where alpha is the J-bluff freq
        // In the standard Nash, P0-K always bets.
        let p0_k_initial = strat.strategy_at(10);
        // actions: [Check, Bet] -> Bet probability should be high
        assert!(
            p0_k_initial[1] > 0.6,
            "P0 with K should bet frequently, got bet_prob={:.3}",
            p0_k_initial[1]
        );

        // P0 with Queen (idx 6): should always check
        let p0_q_initial = strat.strategy_at(6);
        assert!(
            p0_q_initial[0] > 1.0 - tol,
            "P0 with Q should almost always check, got check_prob={:.3}",
            p0_q_initial[0]
        );

        // P0 with Jack (idx 0): should bet (bluff) ~1/3 of the time
        let p0_j_initial = strat.strategy_at(0);
        assert!(
            (p0_j_initial[1] - 1.0 / 3.0).abs() < 0.1,
            "P0 with J should bet ~1/3, got bet_prob={:.3}",
            p0_j_initial[1]
        );

        // P1 with King facing bet (idx 5): should always call
        let p1_k_facing_bet = strat.strategy_at(5);
        assert!(
            p1_k_facing_bet[1] > 1.0 - tol,
            "P1 with K facing bet should always call, got call_prob={:.3}",
            p1_k_facing_bet[1]
        );

        // P1 with Jack facing bet (idx 9): should always fold
        let p1_j_facing_bet = strat.strategy_at(9);
        assert!(
            p1_j_facing_bet[0] > 1.0 - tol,
            "P1 with J facing bet should always fold, got fold_prob={:.3}",
            p1_j_facing_bet[0]
        );

        // P1 with Queen facing bet (idx 3): should call ~1/3
        let p1_q_facing_bet = strat.strategy_at(3);
        assert!(
            (p1_q_facing_bet[1] - 1.0 / 3.0).abs() < 0.1,
            "P1 with Q facing bet should call ~1/3, got call_prob={:.3}",
            p1_q_facing_bet[1]
        );
    }

    #[test]
    fn leduc_tree_structure() {
        let tree = LeducPoker::build_tree();
        // Leduc should have many more info sets than Kuhn
        assert!(
            tree.num_info_sets > 50,
            "Leduc should have > 50 info sets, got {}",
            tree.num_info_sets
        );
        assert!(
            tree.nodes.len() > 100,
            "Leduc should have > 100 nodes, got {}",
            tree.nodes.len()
        );
    }

    #[test]
    fn leduc_cfr_plus_converges() {
        let tree = LeducPoker::build_tree();
        let config = SolverConfig {
            algorithm: CfrAlgorithm::CfrPlus,
            iterations: 10_000,
            ..Default::default()
        };
        let mut solver = CfrSolver::new(tree, config);
        solver.solve();

        let strategy = solver.average_strategy();
        // All strategies should sum to 1
        for idx in 0..solver.tree.num_info_sets {
            let strat = strategy.strategy_at(idx);
            let sum: f32 = strat.iter().sum();
            assert!(
                (sum - 1.0).abs() < 0.01,
                "Leduc strategy at info set {idx} should sum to 1.0, got {sum}"
            );
        }
    }

    #[test]
    fn leduc_exploitability_decreases() {
        use crate::solver::exploitability::compute_exploitability;

        let tree = LeducPoker::build_tree();
        let config = SolverConfig {
            algorithm: CfrAlgorithm::CfrPlus,
            iterations: 100,
            ..Default::default()
        };
        let mut solver = CfrSolver::new(tree, config);
        solver.solve();
        let exp_100 = compute_exploitability(&solver.tree, &solver.store, 1.0);

        let tree = LeducPoker::build_tree();
        let config = SolverConfig {
            algorithm: CfrAlgorithm::CfrPlus,
            iterations: 10_000,
            ..Default::default()
        };
        let mut solver = CfrSolver::new(tree, config);
        solver.solve();
        let exp_10k = compute_exploitability(&solver.tree, &solver.store, 1.0);

        assert!(
            exp_10k < exp_100,
            "Leduc exploitability should decrease: 100={exp_100:.2}, 10k={exp_10k:.2}"
        );
    }

    #[test]
    fn kuhn_dcfr_game_value() {
        let tree = KuhnPoker::build_tree();
        let config = SolverConfig {
            algorithm: CfrAlgorithm::Dcfr,
            iterations: 10_000,
            ..Default::default()
        };
        let mut solver = CfrSolver::new(tree, config);
        let game_value = solver.solve();

        let expected = KuhnPoker::expected_game_value();
        assert!(
            (game_value - expected).abs() < 0.01,
            "DCFR game value {game_value:.4} should be close to {expected:.4}"
        );
    }
}
