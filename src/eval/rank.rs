use std::fmt;

/// Hand rank value — higher is better.
/// The u16 payload encodes hand strength + tiebreakers (following Cactus Kev ordering,
/// mapped so that HIGHER value = STRONGER hand for idiomatic Rust Ord usage).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HandValue(pub u16);

/// Canonical hand category (independent of kickers).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HandCategory {
    HighCard = 0,
    OnePair = 1,
    TwoPair = 2,
    ThreeOfAKind = 3,
    Straight = 4,
    Flush = 5,
    FullHouse = 6,
    FourOfAKind = 7,
    StraightFlush = 8,
    RoyalFlush = 9,
}

impl HandCategory {
    pub const ALL: [HandCategory; 10] = [
        HandCategory::HighCard,
        HandCategory::OnePair,
        HandCategory::TwoPair,
        HandCategory::ThreeOfAKind,
        HandCategory::Straight,
        HandCategory::Flush,
        HandCategory::FullHouse,
        HandCategory::FourOfAKind,
        HandCategory::StraightFlush,
        HandCategory::RoyalFlush,
    ];

    pub fn name(self) -> &'static str {
        match self {
            HandCategory::HighCard => "High Card",
            HandCategory::OnePair => "One Pair",
            HandCategory::TwoPair => "Two Pair",
            HandCategory::ThreeOfAKind => "Three of a Kind",
            HandCategory::Straight => "Straight",
            HandCategory::Flush => "Flush",
            HandCategory::FullHouse => "Full House",
            HandCategory::FourOfAKind => "Four of a Kind",
            HandCategory::StraightFlush => "Straight Flush",
            HandCategory::RoyalFlush => "Royal Flush",
        }
    }
}

impl fmt::Display for HandCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

// Cactus Kev rank ranges (1=best, 7462=worst, but we INVERT so higher=better):
// Royal Flush:     1        → our value: 7462
// Straight Flush:  2-9      → 7453-7460
// Four of a Kind:  11-166   → 7296-7451
// Full House:      167-322  → 7140-7295
// Flush:           323-1599 → 5863-7139
// Straight:        1600-1609 → 5853-5862
// Three of a Kind: 1610-2467 → 4995-5852
// Two Pair:        2468-3325 → 4137-4994
// One Pair:        3326-6185 → 1277-4136
// High Card:       6186-7462 → 1-1276

impl HandValue {
    /// Convert Cactus Kev rank (1=best, 7462=worst) to our inverted value.
    pub fn from_cactus_kev(ck: u16) -> Self {
        HandValue(7463 - ck)
    }

    pub fn category(self) -> HandCategory {
        let v = self.0;
        // HandValue = 7463 - CK_rank, so higher HandValue = stronger hand.
        // CK ranges (inclusive): RF=1, SF=2-10, FOAK=11-166, FH=167-322,
        // Flush=323-1599, Straight=1600-1609, Trips=1610-2467,
        // TwoPair=2468-3325, OnePair=3326-6185, HighCard=6186-7462
        match v {
            7462 => HandCategory::RoyalFlush,           // CK 1
            7453..=7461 => HandCategory::StraightFlush, // CK 2-10
            7297..=7452 => HandCategory::FourOfAKind,   // CK 11-166
            7141..=7296 => HandCategory::FullHouse,     // CK 167-322
            5864..=7140 => HandCategory::Flush,         // CK 323-1599
            5854..=5863 => HandCategory::Straight,      // CK 1600-1609
            4996..=5853 => HandCategory::ThreeOfAKind,  // CK 1610-2467
            4138..=4995 => HandCategory::TwoPair,       // CK 2468-3325
            1278..=4137 => HandCategory::OnePair,       // CK 3326-6185
            _ => HandCategory::HighCard,                // CK 6186-7462
        }
    }
}

impl fmt::Display for HandValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (strength: {})", self.category(), self.0)
    }
}
