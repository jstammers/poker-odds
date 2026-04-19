use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimConfig {
    /// Number of Monte Carlo iterations
    pub iterations: u64,
    /// If remaining combinations <= this, use exact enumeration
    pub exact_threshold: u64,
    /// How many threads to use (0 = auto-detect)
    pub threads: usize,
    /// How often (ms) to emit partial results to the TUI
    pub update_interval_ms: u64,
    /// Optional fixed RNG seed for reproducibility
    pub rng_seed: Option<u64>,
}

impl Default for SimConfig {
    fn default() -> Self {
        SimConfig {
            iterations: 500_000, // ~9ms native (parallel), ~50ms WASM (single-threaded)
            exact_threshold: 50_000,
            threads: 0,
            update_interval_ms: 100,
            rng_seed: None,
        }
    }
}

impl SimConfig {
    #[cfg(target_arch = "wasm32")]
    pub fn effective_threads(&self) -> usize {
        1
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn effective_threads(&self) -> usize {
        if self.threads == 0 {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        } else {
            self.threads
        }
    }
}
