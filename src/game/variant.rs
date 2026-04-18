use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameVariant {
    TexasHoldem,
    OmahaHoldem,
    SevenCardStud,
    FiveCardDraw,
}

impl GameVariant {
    pub const ALL: [GameVariant; 4] = [
        GameVariant::TexasHoldem,
        GameVariant::OmahaHoldem,
        GameVariant::SevenCardStud,
        GameVariant::FiveCardDraw,
    ];

    /// Stable string identifier used in the JS/Tauri API (e.g. `"texas_holdem"`).
    pub fn id(self) -> &'static str {
        match self {
            GameVariant::TexasHoldem => "texas_holdem",
            GameVariant::OmahaHoldem => "omaha_holdem",
            GameVariant::SevenCardStud => "seven_card_stud",
            GameVariant::FiveCardDraw => "five_card_draw",
        }
    }

    /// Parse the stable string identifier produced by [`GameVariant::id`].
    pub fn from_id(s: &str) -> Result<Self, String> {
        match s {
            "texas_holdem" => Ok(GameVariant::TexasHoldem),
            "omaha_holdem" => Ok(GameVariant::OmahaHoldem),
            "seven_card_stud" => Ok(GameVariant::SevenCardStud),
            "five_card_draw" => Ok(GameVariant::FiveCardDraw),
            _ => Err(format!(
                "Unknown variant '{}'. Use one of: texas_holdem, omaha_holdem, seven_card_stud, five_card_draw",
                s
            )),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            GameVariant::TexasHoldem => "Texas Hold'em",
            GameVariant::OmahaHoldem => "Omaha Hold'em",
            GameVariant::SevenCardStud => "7-Card Stud",
            GameVariant::FiveCardDraw => "5-Card Draw",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            GameVariant::TexasHoldem => "2 hole cards + 5 community cards. Best 5-card hand wins.",
            GameVariant::OmahaHoldem => {
                "4 hole cards + 5 community cards. Must use exactly 2 hole + 3 board."
            }
            GameVariant::SevenCardStud => {
                "7 cards dealt individually (3 down, 4 up). No community cards."
            }
            GameVariant::FiveCardDraw => "5 hole cards. No community cards. Can draw new cards.",
        }
    }

    pub fn hole_card_count(self) -> usize {
        match self {
            GameVariant::TexasHoldem => 2,
            GameVariant::OmahaHoldem => 4,
            GameVariant::SevenCardStud => 7,
            GameVariant::FiveCardDraw => 5,
        }
    }

    pub fn community_card_count(self) -> usize {
        match self {
            GameVariant::TexasHoldem => 5,
            GameVariant::OmahaHoldem => 5,
            GameVariant::SevenCardStud => 0,
            GameVariant::FiveCardDraw => 0,
        }
    }

    pub fn has_community_cards(self) -> bool {
        self.community_card_count() > 0
    }

    pub fn max_players(self) -> usize {
        match self {
            GameVariant::TexasHoldem => 9,
            GameVariant::OmahaHoldem => 9,
            GameVariant::SevenCardStud => 7,
            GameVariant::FiveCardDraw => 6,
        }
    }

    pub fn rounds(self) -> &'static [BettingRound] {
        match self {
            GameVariant::TexasHoldem => &[
                BettingRound::Preflop,
                BettingRound::Flop,
                BettingRound::Turn,
                BettingRound::River,
            ],
            GameVariant::OmahaHoldem => &[
                BettingRound::Preflop,
                BettingRound::Flop,
                BettingRound::Turn,
                BettingRound::River,
            ],
            GameVariant::SevenCardStud => &[
                BettingRound::StudStreet3,
                BettingRound::StudStreet4,
                BettingRound::StudStreet5,
                BettingRound::StudStreet6,
                BettingRound::StudStreet7,
            ],
            GameVariant::FiveCardDraw => &[BettingRound::DrawInitial, BettingRound::DrawAfter],
        }
    }
}

impl fmt::Display for GameVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BettingRound {
    // Hold'em rounds
    Preflop,
    Flop,
    Turn,
    River,
    // Stud rounds
    StudStreet3,
    StudStreet4,
    StudStreet5,
    StudStreet6,
    StudStreet7,
    // Draw rounds
    DrawInitial,
    DrawAfter,
}

impl BettingRound {
    pub fn name(self) -> &'static str {
        match self {
            BettingRound::Preflop => "Pre-Flop",
            BettingRound::Flop => "Flop",
            BettingRound::Turn => "Turn",
            BettingRound::River => "River",
            BettingRound::StudStreet3 => "3rd Street",
            BettingRound::StudStreet4 => "4th Street",
            BettingRound::StudStreet5 => "5th Street",
            BettingRound::StudStreet6 => "6th Street",
            BettingRound::StudStreet7 => "7th Street (Showdown)",
            BettingRound::DrawInitial => "Initial Deal",
            BettingRound::DrawAfter => "After Draw",
        }
    }

    /// How many community cards are revealed in this round (Hold'em only)
    pub fn community_cards_revealed(self) -> usize {
        match self {
            BettingRound::Flop => 3,
            BettingRound::Turn => 1,
            BettingRound::River => 1,
            _ => 0,
        }
    }

    /// Total community cards on board after this round
    pub fn total_community_after(self) -> usize {
        match self {
            BettingRound::Preflop => 0,
            BettingRound::Flop => 3,
            BettingRound::Turn => 4,
            BettingRound::River => 5,
            _ => 0,
        }
    }
}

impl fmt::Display for BettingRound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_id_roundtrip() {
        for variant in GameVariant::ALL {
            let id = variant.id();
            let parsed = GameVariant::from_id(id).unwrap();
            assert_eq!(variant, parsed, "round-trip failed for {id}");
        }
    }

    #[test]
    fn variant_from_id_unknown() {
        assert!(GameVariant::from_id("unknown_game").is_err());
    }
}
