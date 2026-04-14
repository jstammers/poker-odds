use crate::solver::action::Action;

/// Index into the flat node arena.
pub type NodeIndex = u32;

/// A single node in the game tree, stored in a flat arena for cache efficiency.
#[derive(Clone, Debug)]
pub enum GameTreeNode {
    /// Terminal node: game is over.
    Terminal {
        /// Payoff to player 0 (player 1 gets the negation in a zero-sum game).
        payoff_p0: f32,
    },
    /// Chance node: nature deals a card.
    Chance {
        /// (card_index, child_node_index) pairs. Sparse representation.
        children: Vec<(u8, NodeIndex)>,
    },
    /// Decision node: a player chooses an action.
    Decision {
        /// Which player acts (0 or 1 for heads-up).
        player: u8,
        /// Available actions at this node.
        actions: Vec<Action>,
        /// Child node indices, one per action (same order as `actions`).
        children: Vec<NodeIndex>,
        /// Index into InfoSetStore for this node's information set.
        info_set_idx: u32,
    },
}

/// The complete game tree as a flat arena of nodes.
pub struct GameTree {
    /// All nodes stored contiguously.
    pub nodes: Vec<GameTreeNode>,
    /// Index of the root node (usually 0).
    pub root: NodeIndex,
    /// Total number of distinct information sets.
    pub num_info_sets: u32,
    /// Number of actions available at each information set.
    pub actions_per_info_set: Vec<u8>,
}

impl GameTree {
    /// Get a reference to a node by index.
    #[inline]
    pub fn node(&self, idx: NodeIndex) -> &GameTreeNode {
        &self.nodes[idx as usize]
    }
}
