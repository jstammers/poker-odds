use crate::eval::HandCategory;

#[derive(Clone, Debug, Default)]
pub struct OddsResult {
    pub win: f64,
    pub tie: f64,
    pub lose: f64,
    pub simulations_run: u64,
    pub method: SimMethod,
    /// Probability of achieving each hand category (indexed by HandCategory ordinal)
    pub hand_distribution: [f64; 10],
}

impl OddsResult {
    pub fn win_pct(&self) -> f64 { self.win * 100.0 }
    pub fn tie_pct(&self) -> f64 { self.tie * 100.0 }
    pub fn lose_pct(&self) -> f64 { self.lose * 100.0 }

    pub fn hand_pct(&self, cat: HandCategory) -> f64 {
        self.hand_distribution[cat as usize] * 100.0
    }

    pub fn is_ready(&self) -> bool {
        self.simulations_run > 0
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum SimMethod {
    #[default]
    NotStarted,
    MonteCarlo,
    Exact,
}

impl std::fmt::Display for SimMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SimMethod::NotStarted => write!(f, "—"),
            SimMethod::MonteCarlo => write!(f, "Monte Carlo"),
            SimMethod::Exact => write!(f, "Exact"),
        }
    }
}

/// Accumulator used internally during simulation
#[derive(Clone, Debug, Default)]
pub struct SimAccumulator {
    pub wins: u64,
    pub ties: u64,
    pub losses: u64,
    pub total: u64,
    pub hand_counts: [u64; 10],
}

impl SimAccumulator {
    pub fn record_win(&mut self, cat: HandCategory) {
        self.wins += 1;
        self.total += 1;
        self.hand_counts[cat as usize] += 1;
    }

    pub fn record_tie(&mut self, cat: HandCategory) {
        self.ties += 1;
        self.total += 1;
        self.hand_counts[cat as usize] += 1;
    }

    pub fn record_loss(&mut self, cat: HandCategory) {
        self.losses += 1;
        self.total += 1;
        self.hand_counts[cat as usize] += 1;
    }

    pub fn merge(&mut self, other: &SimAccumulator) {
        self.wins += other.wins;
        self.ties += other.ties;
        self.losses += other.losses;
        self.total += other.total;
        for (a, b) in self.hand_counts.iter_mut().zip(other.hand_counts.iter()) {
            *a += *b;
        }
    }

    pub fn to_result(&self, method: SimMethod) -> OddsResult {
        if self.total == 0 {
            return OddsResult::default();
        }
        let t = self.total as f64;
        let mut hand_distribution = [0.0f64; 10];
        for (i, &c) in self.hand_counts.iter().enumerate() {
            hand_distribution[i] = c as f64 / t;
        }
        OddsResult {
            win: self.wins as f64 / t,
            tie: self.ties as f64 / t,
            lose: self.losses as f64 / t,
            simulations_run: self.total,
            method,
            hand_distribution,
        }
    }
}
