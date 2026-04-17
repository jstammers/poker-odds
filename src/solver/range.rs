use crate::cards::card::{Card, CardParseError, Rank, Suit};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RangeParseError {
    #[error("invalid range token '{0}'")]
    InvalidToken(String),
    #[error("invalid card: {0}")]
    InvalidCard(#[from] CardParseError),
    #[error("invalid rank range '{0}-{1}': first rank must be higher")]
    InvalidRankRange(char, char),
}

/// A preflop hand range represented as weights for all 1326 starting hand combos.
///
/// Weight 0.0 = not in range, 1.0 = always in range, fractional = mixed.
#[derive(Clone, Debug)]
pub struct HandRange {
    /// Weights for each of the 1326 unique 2-card combos.
    pub weights: [f32; 1326],
}

impl HandRange {
    /// Empty range (no hands).
    pub fn empty() -> Self {
        Self {
            weights: [0.0; 1326],
        }
    }

    /// Full range (all hands, weight 1.0).
    pub fn full() -> Self {
        Self {
            weights: [1.0; 1326],
        }
    }

    /// Map a pair of cards to the canonical combo index (0..1326).
    ///
    /// Cards are ordered by their 0-51 index (lower first).
    /// Index formula: triangular number mapping.
    pub fn combo_index(c1: Card, c2: Card) -> u16 {
        let (lo, hi) = if c1.index() < c2.index() {
            (c1.index() as u16, c2.index() as u16)
        } else {
            (c2.index() as u16, c1.index() as u16)
        };
        // Triangular number: sum of (51 + 50 + ... + (52 - lo)) + (hi - lo - 1)
        // = lo * 52 - lo*(lo+1)/2 + hi - lo - 1
        // Simplified: lo*51 - lo*(lo-1)/2 + hi - lo - 1
        lo * 103 / 2 - lo * lo / 2 + hi - lo - 1
    }

    /// Decode a combo index back into two card indices (lo, hi).
    pub fn cards_from_index(idx: u16) -> (Card, Card) {
        // Binary search for lo
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
        (Card::from_index(lo as u8), Card::from_index(hi as u8))
    }
}

impl std::str::FromStr for HandRange {
    type Err = RangeParseError;

    /// Parse a comma-separated range string.
    ///
    /// Supported tokens:
    /// - Pairs: "AA", "QQ"
    /// - Suited: "AKs", "T9s"
    /// - Offsuit: "AKo", "T9o"
    /// - Unspecified: "AK" (both suited and offsuit)
    /// - Pair ranges: "QQ-TT" (QQ, JJ, TT)
    /// - Suited ranges: "A5s-A2s" (A5s, A4s, A3s, A2s)
    /// - Offsuit ranges: "KJo-K9o"
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut range = Self::empty();

        for token in s.split(',') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }

            if token.contains('-') {
                let parts: Vec<&str> = token.splitn(2, '-').collect();
                range.parse_range(parts[0], parts[1])?;
            } else {
                range.parse_single(token)?;
            }
        }

        Ok(range)
    }
}

impl HandRange {
    fn parse_single(&mut self, token: &str) -> Result<(), RangeParseError> {
        let chars: Vec<char> = token.chars().collect();

        if chars.len() == 4 {
            // Specific cards: "AhKs"
            let c1 = Card::new(Rank::from_char(chars[0])?, Suit::from_char(chars[1])?);
            let c2 = Card::new(Rank::from_char(chars[2])?, Suit::from_char(chars[3])?);
            let idx = Self::combo_index(c1, c2);
            self.weights[idx as usize] = 1.0;
            return Ok(());
        }

        if chars.len() < 2 || chars.len() > 3 {
            return Err(RangeParseError::InvalidToken(token.to_string()));
        }

        let r1 = Rank::from_char(chars[0])?;
        let r2 = Rank::from_char(chars[1])?;
        let suffix = chars.get(2).copied();

        if r1 == r2 {
            // Pair: "AA"
            self.add_pair(r1);
        } else {
            match suffix {
                Some('s') => self.add_suited(r1, r2),
                Some('o') => self.add_offsuit(r1, r2),
                None => {
                    self.add_suited(r1, r2);
                    self.add_offsuit(r1, r2);
                }
                _ => return Err(RangeParseError::InvalidToken(token.to_string())),
            }
        }

        Ok(())
    }

    fn parse_range(&mut self, start: &str, end: &str) -> Result<(), RangeParseError> {
        let start_chars: Vec<char> = start.chars().collect();
        let end_chars: Vec<char> = end.chars().collect();

        if start_chars.len() < 2 || end_chars.len() < 2 {
            return Err(RangeParseError::InvalidToken(format!("{start}-{end}")));
        }

        let r1_start = Rank::from_char(start_chars[0])?;
        let r2_start = Rank::from_char(start_chars[1])?;
        let r1_end = Rank::from_char(end_chars[0])?;
        let r2_end = Rank::from_char(end_chars[1])?;
        let suffix = start_chars.get(2).copied();

        if r1_start == r2_start && r1_end == r2_end {
            // Pair range: "QQ-TT"
            let hi = r1_start.index().max(r1_end.index());
            let lo = r1_start.index().min(r1_end.index());
            for ri in lo..=hi {
                self.add_pair(Rank::from_index(ri));
            }
        } else if r1_start == r1_end {
            // Same high card range: "A5s-A2s"
            let hi = r2_start.index().max(r2_end.index());
            let lo = r2_start.index().min(r2_end.index());
            for ri in lo..=hi {
                let r2 = Rank::from_index(ri);
                match suffix {
                    Some('s') => self.add_suited(r1_start, r2),
                    Some('o') => self.add_offsuit(r1_start, r2),
                    None => {
                        self.add_suited(r1_start, r2);
                        self.add_offsuit(r1_start, r2);
                    }
                    _ => return Err(RangeParseError::InvalidToken(format!("{start}-{end}"))),
                }
            }
        } else {
            return Err(RangeParseError::InvalidToken(format!("{start}-{end}")));
        }

        Ok(())
    }

    /// Add all 6 combos of a pocket pair.
    fn add_pair(&mut self, rank: Rank) {
        for i in 0..4u8 {
            for j in (i + 1)..4 {
                let c1 = Card::new(rank, Suit::ALL[i as usize]);
                let c2 = Card::new(rank, Suit::ALL[j as usize]);
                let idx = Self::combo_index(c1, c2);
                self.weights[idx as usize] = 1.0;
            }
        }
    }

    /// Add all 4 suited combos of two ranks.
    fn add_suited(&mut self, r1: Rank, r2: Rank) {
        for &suit in &Suit::ALL {
            let c1 = Card::new(r1, suit);
            let c2 = Card::new(r2, suit);
            let idx = Self::combo_index(c1, c2);
            self.weights[idx as usize] = 1.0;
        }
    }

    /// Add all 12 offsuit combos of two ranks.
    fn add_offsuit(&mut self, r1: Rank, r2: Rank) {
        for &s1 in &Suit::ALL {
            for &s2 in &Suit::ALL {
                if s1 == s2 {
                    continue;
                }
                let c1 = Card::new(r1, s1);
                let c2 = Card::new(r2, s2);
                let idx = Self::combo_index(c1, c2);
                self.weights[idx as usize] = 1.0;
            }
        }
    }

    /// Count the number of combos with non-zero weight.
    pub fn num_combos(&self) -> usize {
        self.weights.iter().filter(|&&w| w > 0.0).count()
    }

    /// Iterate over all (combo_index, weight) pairs with non-zero weight.
    pub fn iter_combos(&self) -> impl Iterator<Item = (u16, f32)> + '_ {
        self.weights
            .iter()
            .enumerate()
            .filter(|(_, &w)| w > 0.0)
            .map(|(i, &w)| (i as u16, w))
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn combo_index_roundtrip() {
        // Verify all 1326 combos have unique indices
        let mut seen = vec![false; 1326];
        let mut count = 0;
        for i in 0..52u8 {
            for j in (i + 1)..52 {
                let c1 = Card::from_index(i);
                let c2 = Card::from_index(j);
                let idx = HandRange::combo_index(c1, c2);
                assert!(
                    !seen[idx as usize],
                    "Duplicate index {idx} for cards {i},{j}"
                );
                seen[idx as usize] = true;

                // Verify roundtrip
                let (rc1, rc2) = HandRange::cards_from_index(idx);
                assert_eq!(rc1.index().min(rc2.index()), i);
                assert_eq!(rc1.index().max(rc2.index()), j);
                count += 1;
            }
        }
        assert_eq!(count, 1326);
    }

    #[test]
    fn combo_index_order_independent() {
        let ah = Card::new(Rank::Ace, Suit::Hearts);
        let ks = Card::new(Rank::King, Suit::Spades);
        assert_eq!(
            HandRange::combo_index(ah, ks),
            HandRange::combo_index(ks, ah)
        );
    }

    #[test]
    fn parse_pocket_pair() {
        let range = HandRange::from_str("AA").unwrap();
        assert_eq!(range.num_combos(), 6); // C(4,2) = 6
    }

    #[test]
    fn parse_suited() {
        let range = HandRange::from_str("AKs").unwrap();
        assert_eq!(range.num_combos(), 4); // 4 suits
    }

    #[test]
    fn parse_offsuit() {
        let range = HandRange::from_str("AKo").unwrap();
        assert_eq!(range.num_combos(), 12); // 4*3 = 12
    }

    #[test]
    fn parse_unspecified() {
        let range = HandRange::from_str("AK").unwrap();
        assert_eq!(range.num_combos(), 16); // 4 suited + 12 offsuit
    }

    #[test]
    fn parse_pair_range() {
        let range = HandRange::from_str("QQ-TT").unwrap();
        assert_eq!(range.num_combos(), 18); // 3 pairs * 6 combos
    }

    #[test]
    fn parse_suited_range() {
        let range = HandRange::from_str("A5s-A2s").unwrap();
        assert_eq!(range.num_combos(), 16); // 4 hands * 4 suits
    }

    #[test]
    fn parse_complex_range() {
        let range = HandRange::from_str("AA,AKs,QQ-TT,A5s-A2s,KJo").unwrap();
        let expected = 6 + 4 + 18 + 16 + 12;
        assert_eq!(range.num_combos(), expected);
    }

    #[test]
    fn parse_specific_cards() {
        let range = HandRange::from_str("AhKs").unwrap();
        assert_eq!(range.num_combos(), 1);
    }

    #[test]
    fn full_range_has_1326_combos() {
        let range = HandRange::full();
        assert_eq!(range.num_combos(), 1326);
    }
}
