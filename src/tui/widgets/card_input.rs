//! Interactive card input widget.

use crate::cards::{Card, Rank, Suit};
use crossterm::event::{KeyCode, KeyEvent};

#[derive(Debug, Clone, PartialEq)]
pub enum InputState {
    /// Waiting for rank character
    AwaitingRank,
    /// Got rank, waiting for suit character
    AwaitingSuit(Rank),
    /// Card confirmed
    Confirmed(Card),
}

/// A single card input slot.
#[derive(Debug, Clone)]
pub struct CardInput {
    pub state: InputState,
    pub error: Option<String>,
}

impl CardInput {
    pub fn new() -> Self {
        CardInput {
            state: InputState::AwaitingRank,
            error: None,
        }
    }

    pub fn clear(&mut self) {
        self.state = InputState::AwaitingRank;
        self.error = None;
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.state, InputState::Confirmed(_))
    }

    pub fn card(&self) -> Option<Card> {
        if let InputState::Confirmed(c) = self.state {
            Some(c)
        } else {
            None
        }
    }

    /// Handle a keypress. Returns the completed card if just confirmed.
    pub fn handle_key(&mut self, key: KeyEvent, already_used: &[Card]) -> Option<Card> {
        self.error = None;
        match &self.state {
            InputState::AwaitingRank => {
                if let KeyCode::Char(c) = key.code {
                    match Rank::from_char(c) {
                        Ok(rank) => {
                            self.state = InputState::AwaitingSuit(rank);
                        }
                        Err(_) => {
                            self.error =
                                Some(format!("Unknown rank '{}'. Use 2-9, T, J, Q, K, A", c));
                        }
                    }
                }
            }
            InputState::AwaitingSuit(rank) => {
                let rank = *rank;
                match key.code {
                    KeyCode::Backspace => {
                        self.state = InputState::AwaitingRank;
                    }
                    KeyCode::Char(c) => match Suit::from_char(c) {
                        Ok(suit) => {
                            let card = Card::new(rank, suit);
                            if already_used.contains(&card) {
                                self.error = Some(format!("{} is already in use", card));
                                self.state = InputState::AwaitingRank;
                            } else {
                                self.state = InputState::Confirmed(card);
                                return Some(card);
                            }
                        }
                        Err(_) => {
                            self.error = Some(format!("Unknown suit '{}'. Use c, d, h, s", c));
                        }
                    },
                    _ => {}
                }
            }
            InputState::Confirmed(_) => {
                if key.code == KeyCode::Backspace || key.code == KeyCode::Delete {
                    self.state = InputState::AwaitingRank;
                }
            }
        }
        None
    }

    /// Display hint for this input slot
    pub fn hint(&self) -> String {
        match &self.state {
            InputState::AwaitingRank => "_ _".to_string(),
            InputState::AwaitingSuit(r) => format!("{}_", r.to_char()),
            InputState::Confirmed(c) => format!("{}{}", c.rank.to_char(), c.suit.glyph()),
        }
    }
}

impl Default for CardInput {
    fn default() -> Self {
        Self::new()
    }
}

/// Multi-slot card input (for entering multiple cards)
#[derive(Debug, Clone)]
pub struct MultiCardInput {
    pub slots: Vec<CardInput>,
    pub active_slot: usize,
    pub label: String,
}

impl MultiCardInput {
    pub fn new(count: usize, label: impl Into<String>) -> Self {
        MultiCardInput {
            slots: vec![CardInput::new(); count],
            active_slot: 0,
            label: label.into(),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, already_used: &[Card]) {
        if self.active_slot >= self.slots.len() {
            return;
        }

        let slot = &mut self.slots[self.active_slot];
        let completed = slot.handle_key(key, already_used);

        if completed.is_some() {
            // Auto-advance to next incomplete slot
            if let Some(next) = self.next_incomplete_slot() {
                self.active_slot = next;
            }
        }

        // Tab/Shift+Tab to navigate slots
        if key.code == KeyCode::Tab {
            self.active_slot = (self.active_slot + 1) % self.slots.len();
        }
        if key.code == KeyCode::BackTab {
            self.active_slot = (self.active_slot + self.slots.len() - 1) % self.slots.len();
        }
    }

    fn next_incomplete_slot(&self) -> Option<usize> {
        // Find first incomplete slot after current
        let n = self.slots.len();
        for i in 1..=n {
            let idx = (self.active_slot + i) % n;
            if !self.slots[idx].is_complete() {
                return Some(idx);
            }
        }
        None
    }

    pub fn all_complete(&self) -> bool {
        self.slots.iter().all(|s| s.is_complete())
    }

    pub fn cards(&self) -> Vec<Card> {
        self.slots.iter().filter_map(|s| s.card()).collect()
    }

    pub fn error(&self) -> Option<&str> {
        self.slots.iter().find_map(|s| s.error.as_deref())
    }

    pub fn reset(&mut self) {
        for slot in &mut self.slots {
            slot.clear();
        }
        self.active_slot = 0;
    }
}
