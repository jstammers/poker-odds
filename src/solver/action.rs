use serde::{Deserialize, Serialize};

/// A discrete action a player can take at a decision node.
///
/// Bet sizes are encoded as basis points (1/10000) of the pot to avoid
/// floating-point equality issues in hash maps and enable trivial Eq + Hash.
/// For example, 5000 = 0.5x pot, 10000 = 1.0x pot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    Fold,
    Check,
    Call,
    /// Bet/raise as a fraction of pot in basis points.
    Bet(u16),
    AllIn,
}

impl Action {
    /// Convert a pot-fraction bet size (e.g. 0.5) to the basis-point encoding.
    pub fn bet_from_fraction(fraction: f64) -> Self {
        Action::Bet((fraction * 10000.0).round() as u16)
    }

    /// Get the pot fraction for a Bet action, or None for other actions.
    pub fn bet_fraction(&self) -> Option<f64> {
        match self {
            Action::Bet(bp) => Some(*bp as f64 / 10000.0),
            _ => None,
        }
    }
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::Fold => write!(f, "Fold"),
            Action::Check => write!(f, "Check"),
            Action::Call => write!(f, "Call"),
            Action::Bet(bp) => write!(f, "Bet({:.0}%)", *bp as f64 / 100.0),
            Action::AllIn => write!(f, "AllIn"),
        }
    }
}

/// Configuration for available bet sizes at each street.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BetSizingConfig {
    /// Bet sizes as fractions of pot (e.g., [0.33, 0.5, 0.75, 1.0]).
    pub flop_bets: Vec<f64>,
    pub flop_raises: Vec<f64>,
    pub turn_bets: Vec<f64>,
    pub turn_raises: Vec<f64>,
    pub river_bets: Vec<f64>,
    pub river_raises: Vec<f64>,
    /// Whether all-in is always available.
    pub always_allow_allin: bool,
    /// Max number of raises per street (prevents infinite trees).
    pub max_raises_per_street: u8,
}

impl Default for BetSizingConfig {
    fn default() -> Self {
        Self {
            flop_bets: vec![0.33, 0.67, 1.0],
            flop_raises: vec![0.5, 1.0],
            turn_bets: vec![0.5, 0.75, 1.0],
            turn_raises: vec![0.5, 1.0],
            river_bets: vec![0.5, 0.75, 1.0],
            river_raises: vec![0.5, 1.0],
            always_allow_allin: true,
            max_raises_per_street: 3,
        }
    }
}
