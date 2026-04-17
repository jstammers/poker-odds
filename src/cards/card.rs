use std::fmt;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum CardParseError {
    #[error("invalid card string '{0}' — expected format like 'Ah', 'Td', '2c'")]
    InvalidFormat(String),
    #[error("unknown rank '{0}'")]
    UnknownRank(char),
    #[error("unknown suit '{0}'")]
    UnknownSuit(char),
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rank {
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
    Nine = 9,
    Ten = 10,
    Jack = 11,
    Queen = 12,
    King = 13,
    Ace = 14,
}

impl Rank {
    pub const ALL: [Rank; 13] = [
        Rank::Two,
        Rank::Three,
        Rank::Four,
        Rank::Five,
        Rank::Six,
        Rank::Seven,
        Rank::Eight,
        Rank::Nine,
        Rank::Ten,
        Rank::Jack,
        Rank::Queen,
        Rank::King,
        Rank::Ace,
    ];

    /// 0-based index (Two=0 ... Ace=12)
    pub fn index(self) -> u8 {
        self as u8 - 2
    }

    pub fn from_index(idx: u8) -> Self {
        Rank::ALL[idx as usize]
    }

    pub fn from_char(c: char) -> Result<Self, CardParseError> {
        match c.to_ascii_uppercase() {
            '2' => Ok(Rank::Two),
            '3' => Ok(Rank::Three),
            '4' => Ok(Rank::Four),
            '5' => Ok(Rank::Five),
            '6' => Ok(Rank::Six),
            '7' => Ok(Rank::Seven),
            '8' => Ok(Rank::Eight),
            '9' => Ok(Rank::Nine),
            'T' => Ok(Rank::Ten),
            'J' => Ok(Rank::Jack),
            'Q' => Ok(Rank::Queen),
            'K' => Ok(Rank::King),
            'A' => Ok(Rank::Ace),
            other => Err(CardParseError::UnknownRank(other)),
        }
    }

    pub fn to_char(self) -> char {
        match self {
            Rank::Two => '2',
            Rank::Three => '3',
            Rank::Four => '4',
            Rank::Five => '5',
            Rank::Six => '6',
            Rank::Seven => '7',
            Rank::Eight => '8',
            Rank::Nine => '9',
            Rank::Ten => 'T',
            Rank::Jack => 'J',
            Rank::Queen => 'Q',
            Rank::King => 'K',
            Rank::Ace => 'A',
        }
    }

    /// Prime number assigned to each rank for hash-based hand evaluation
    pub fn prime(self) -> u32 {
        match self {
            Rank::Two => 2,
            Rank::Three => 3,
            Rank::Four => 5,
            Rank::Five => 7,
            Rank::Six => 11,
            Rank::Seven => 13,
            Rank::Eight => 17,
            Rank::Nine => 19,
            Rank::Ten => 23,
            Rank::Jack => 29,
            Rank::Queen => 31,
            Rank::King => 37,
            Rank::Ace => 41,
        }
    }
}

impl fmt::Display for Rank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_char())
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Suit {
    Clubs = 0,
    Diamonds = 1,
    Hearts = 2,
    Spades = 3,
}

impl Suit {
    pub const ALL: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];

    pub fn from_char(c: char) -> Result<Self, CardParseError> {
        match c.to_ascii_lowercase() {
            'c' => Ok(Suit::Clubs),
            'd' => Ok(Suit::Diamonds),
            'h' => Ok(Suit::Hearts),
            's' => Ok(Suit::Spades),
            other => Err(CardParseError::UnknownSuit(other)),
        }
    }

    pub fn to_char(self) -> char {
        match self {
            Suit::Clubs => 'c',
            Suit::Diamonds => 'd',
            Suit::Hearts => 'h',
            Suit::Spades => 's',
        }
    }

    /// Unicode glyph for display
    pub fn glyph(self) -> char {
        match self {
            Suit::Clubs => '♣',
            Suit::Diamonds => '♦',
            Suit::Hearts => '♥',
            Suit::Spades => '♠',
        }
    }
}

impl fmt::Display for Suit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_char())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Card {
    pub rank: Rank,
    pub suit: Suit,
}

impl Card {
    pub fn new(rank: Rank, suit: Suit) -> Self {
        Card { rank, suit }
    }

    /// Compact 0–51 encoding: rank_index * 4 + suit_index
    pub fn index(self) -> u8 {
        self.rank.index() * 4 + self.suit as u8
    }

    pub fn from_index(idx: u8) -> Self {
        let rank = Rank::from_index(idx / 4);
        let suit = Suit::ALL[(idx % 4) as usize];
        Card { rank, suit }
    }

    /// Parse strings like "Ah", "Td", "2c", "Ks"
    pub fn from_str(s: &str) -> Result<Self, CardParseError> {
        let s = s.trim();
        if s.len() != 2 {
            return Err(CardParseError::InvalidFormat(s.to_string()));
        }
        let mut chars = s.chars();
        let rank_char = chars.next().unwrap();
        let suit_char = chars.next().unwrap();
        let rank = Rank::from_char(rank_char)?;
        let suit = Suit::from_char(suit_char)?;
        Ok(Card { rank, suit })
    }

    pub fn display_short(self) -> String {
        format!("{}{}", self.rank.to_char(), self.suit.to_char())
    }

    pub fn display_unicode(self) -> String {
        format!("{}{}", self.rank.to_char(), self.suit.glyph())
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.rank.to_char(), self.suit.glyph())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_index() {
        for i in 0..52u8 {
            assert_eq!(Card::from_index(i).index(), i);
        }
    }

    #[test]
    fn parse_cards() {
        assert_eq!(
            Card::from_str("Ah").unwrap(),
            Card::new(Rank::Ace, Suit::Hearts)
        );
        assert_eq!(
            Card::from_str("Td").unwrap(),
            Card::new(Rank::Ten, Suit::Diamonds)
        );
        assert_eq!(
            Card::from_str("2c").unwrap(),
            Card::new(Rank::Two, Suit::Clubs)
        );
        assert!(Card::from_str("Xx").is_err());
    }
}
