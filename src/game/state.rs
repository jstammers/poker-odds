use crate::cards::Card;
use crate::game::variant::{BettingRound, GameVariant};

/// The full game state shared between TUI, simulation, and rules.
#[derive(Clone, Debug)]
pub struct GameState {
    pub variant: GameVariant,
    /// The player's hole cards (known exactly)
    pub hole_cards: Vec<Card>,
    /// Community cards visible on the board (Hold'em)
    pub community_cards: Vec<Card>,
    /// Number of opponents (not counting the player)
    pub opponent_count: usize,
    /// Current betting round
    pub current_round: BettingRound,
    /// For Stud: face-up cards visible for each opponent (indexed by opponent)
    pub stud_visible: Vec<Vec<Card>>,
}

impl GameState {
    pub fn new(variant: GameVariant) -> Self {
        let current_round = *variant.rounds().first().unwrap();
        let stud_visible = vec![Vec::new(); 6]; // up to 6 opponents
        GameState {
            variant,
            hole_cards: Vec::new(),
            community_cards: Vec::new(),
            opponent_count: 1,
            current_round,
            stud_visible,
        }
    }

    /// All cards that are definitively known (to remove from deck before simulation)
    pub fn known_cards(&self) -> Vec<Card> {
        let mut known = self.hole_cards.clone();
        known.extend_from_slice(&self.community_cards);
        for visible in &self.stud_visible {
            known.extend_from_slice(visible);
        }
        known
    }

    /// How many more community cards need to be revealed to reach showdown
    pub fn community_cards_remaining(&self) -> usize {
        self.variant
            .community_card_count()
            .saturating_sub(self.community_cards.len())
    }

    /// How many hole cards per opponent are unknown (for simulation)
    pub fn unknown_hole_cards_per_opponent(&self) -> usize {
        match self.variant {
            GameVariant::TexasHoldem => 2,
            GameVariant::OmahaHoldem => 4,
            GameVariant::FiveCardDraw => 5,
            GameVariant::SevenCardStud => {
                // Each opponent has 7 cards total; some face-up are known
                7 // simplified — Stud visible cards handled separately
            }
        }
    }

    /// Total cards still needed from the deck per simulation run
    pub fn cards_to_simulate(&self) -> usize {
        let community_needed = self.community_cards_remaining();
        let opponent_needed = self.opponent_count * self.unknown_hole_cards_per_opponent();
        community_needed + opponent_needed
    }

    /// Advance to the next betting round. Returns false if already at the last round.
    pub fn advance_round(&mut self) -> bool {
        let rounds = self.variant.rounds();
        let current_pos = rounds.iter().position(|&r| r == self.current_round);
        if let Some(pos) = current_pos {
            if pos + 1 < rounds.len() {
                self.current_round = rounds[pos + 1];
                return true;
            }
        }
        false
    }

    /// Whether the hand has reached showdown (all cards revealed)
    pub fn is_complete(&self) -> bool {
        match self.variant {
            GameVariant::TexasHoldem | GameVariant::OmahaHoldem => self.community_cards.len() == 5,
            GameVariant::SevenCardStud => self.hole_cards.len() == 7,
            GameVariant::FiveCardDraw => self.hole_cards.len() == 5,
        }
    }

    pub fn hole_cards_complete(&self) -> bool {
        self.hole_cards.len() == self.variant.hole_card_count()
    }
}
