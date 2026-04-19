//! wasm-bindgen public API — exposes the poker odds engine to JavaScript.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{atomic::AtomicBool, Arc};

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::cards::Card;
use crate::eval::HandCategory;
use crate::game::{GameState, GameVariant};
use crate::sim::{run_simulation, SimConfig};
use crate::solver::action::BetSizingConfig;
use crate::solver::cfr::{CfrAlgorithm, SolverConfig};
use crate::solver::range::HandRange;
use crate::solver::vector_api::{solve_vector, VectorSolverConfig};

// ── Panic hook ───────────────────────────────────────────────────────────────

/// Called automatically when the wasm module loads.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

// ── Types crossing the JS boundary (JSON strings) ────────────────────────────

#[derive(Deserialize)]
struct SimInput {
    variant: String,
    hole_cards: Vec<String>,
    #[serde(default)]
    community_cards: Vec<String>,
    #[serde(default = "default_opponents")]
    opponent_count: usize,
    iterations: Option<u64>,
    exact_threshold: Option<u64>,
    rng_seed: Option<u64>,
}

fn default_opponents() -> usize {
    1
}

#[derive(Serialize)]
struct SimOutput {
    win: f64,
    tie: f64,
    lose: f64,
    simulations_run: u64,
    method: String,
    /// Probability 0‒1 for each hand category, keyed by category name.
    hand_distribution: HashMap<String, f64>,
}

#[derive(Serialize)]
struct ErrorOutput {
    error: String,
}

#[derive(Serialize)]
pub struct VariantInfo {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    hole_card_count: usize,
    community_card_count: usize,
    max_players: usize,
}

// ── Public wasm-bindgen functions ─────────────────────────────────────────────

/// Calculate poker odds.
///
/// **Input JSON:**
/// ```json
/// {
///   "variant": "texas_holdem",
///   "hole_cards": ["Ah", "Kh"],
///   "community_cards": ["Qh", "Jh", "Th"],
///   "opponent_count": 2,
///   "iterations": 100000
/// }
/// ```
///
/// **Output JSON:** `{ win, tie, lose, simulations_run, method, hand_distribution }`
/// or `{ "error": "..." }` on failure.
#[wasm_bindgen]
pub fn calculate_odds(input_json: &str) -> String {
    match run_calculate(input_json) {
        Ok(out) => serde_json::to_string(&out).unwrap_or_else(|e| error_json(&e.to_string())),
        Err(e) => error_json(&e),
    }
}

/// Return a JSON array of all supported variants with their metadata.
#[wasm_bindgen]
pub fn get_variants() -> String {
    let variants: Vec<VariantInfo> = GameVariant::ALL
        .iter()
        .map(|v| VariantInfo {
            id: v.id(),
            name: v.name(),
            description: v.description(),
            hole_card_count: v.hole_card_count(),
            community_card_count: v.community_card_count(),
            max_players: v.max_players(),
        })
        .collect();
    serde_json::to_string(&variants).unwrap_or_else(|e| error_json(&e.to_string()))
}

/// Return `true` if the string parses as a valid card (e.g. "Ah", "Td", "2c").
#[wasm_bindgen]
pub fn validate_card(s: &str) -> bool {
    Card::from_str(s).is_ok()
}

// ── Vector CFR solver ────────────────────────────────────────────────────────

/// Run the PIOSolver-style vector CFR on a postflop subgame.
///
/// **Input JSON:**
/// ```json
/// {
///   "board": ["Ah", "Kd", "5c"],
///   "range_oop": "22+,A2s+,KTs+",
///   "range_ip":  "22+,ATs+,KJs+",
///   "starting_pot": 10.0,
///   "effective_stack": 100.0,
///   "iterations": 500,
///   "algorithm": "cfr_plus",
///   "bet_config": { ...optional... }
/// }
/// ```
///
/// Board length must be 3 (flop), 4 (turn) or 5 (river). Ranges accept the
/// same notation as [`HandRange::from_str`] (e.g. `"AA,KK,AKs"`); use
/// `"all"` as a shortcut for the full 1326-combo range.
///
/// **Output JSON:** [`VectorSolveOutput`] on success, `{ "error": "..." }`
/// on failure.
#[wasm_bindgen]
pub fn solve_postflop(input_json: &str) -> String {
    match run_solve_postflop(input_json) {
        Ok(out) => serde_json::to_string(&out).unwrap_or_else(|e| error_json(&e.to_string())),
        Err(e) => error_json(&e),
    }
}

// ── Internals ─────────────────────────────────────────────────────────────────

fn run_calculate(input_json: &str) -> Result<SimOutput, String> {
    let input: SimInput =
        serde_json::from_str(input_json).map_err(|e| format!("Invalid JSON: {}", e))?;

    let variant = GameVariant::from_id(&input.variant)?;

    // Build game state
    let mut state = GameState::new(variant);

    state.hole_cards = input
        .hole_cards
        .iter()
        .map(|s| Card::from_str(s).map_err(|e| format!("Invalid card '{}': {}", s, e)))
        .collect::<Result<Vec<_>, _>>()?;

    state.community_cards = input
        .community_cards
        .iter()
        .map(|s| Card::from_str(s).map_err(|e| format!("Invalid card '{}': {}", s, e)))
        .collect::<Result<Vec<_>, _>>()?;

    state.opponent_count = input.opponent_count.max(1);

    if !state.hole_cards_complete() {
        return Err(format!(
            "{} requires {} hole cards, got {}",
            variant.name(),
            variant.hole_card_count(),
            state.hole_cards.len()
        ));
    }

    // Duplicate card check
    let mut seen = std::collections::HashSet::new();
    for c in state.known_cards() {
        if !seen.insert(c) {
            return Err(format!("Duplicate card: {}", c));
        }
    }

    // Build sim config
    let mut config = SimConfig::default();
    if let Some(iter) = input.iterations {
        config.iterations = iter.clamp(1_000, 2_000_000);
    }
    if let Some(et) = input.exact_threshold {
        config.exact_threshold = et;
    }
    config.rng_seed = input.rng_seed;

    // Run (no cancellation in WASM — simulation is called from a Web Worker)
    let cancel = Arc::new(AtomicBool::new(false));
    let result = run_simulation(&state, &config, cancel, |_| {});

    if !result.is_ready() {
        return Err("Simulation produced no results — check your inputs".to_string());
    }

    // Build hand distribution map
    let hand_distribution = HandCategory::ALL
        .iter()
        .map(|cat| (cat.name().to_string(), result.hand_pct(*cat) / 100.0))
        .collect();

    Ok(SimOutput {
        win: result.win,
        tie: result.tie,
        lose: result.lose,
        simulations_run: result.simulations_run,
        method: result.method.to_string(),
        hand_distribution,
    })
}

fn error_json(msg: &str) -> String {
    serde_json::to_string(&ErrorOutput {
        error: msg.to_string(),
    })
    .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}

// ── Vector solver internals ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct SolveInput {
    board: Vec<String>,
    range_oop: String,
    range_ip: String,
    starting_pot: f32,
    effective_stack: f32,
    #[serde(default = "default_iterations")]
    iterations: u32,
    #[serde(default)]
    algorithm: AlgorithmTag,
    #[serde(default)]
    bet_config: Option<BetConfigInput>,
    #[serde(default)]
    ante: Option<f32>,
}

fn default_iterations() -> u32 {
    200
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum AlgorithmTag {
    #[default]
    CfrPlus,
    Dcfr,
}

impl From<AlgorithmTag> for CfrAlgorithm {
    fn from(t: AlgorithmTag) -> Self {
        match t {
            AlgorithmTag::CfrPlus => CfrAlgorithm::CfrPlus,
            AlgorithmTag::Dcfr => CfrAlgorithm::Dcfr,
        }
    }
}

#[derive(Deserialize)]
struct BetConfigInput {
    #[serde(default)]
    flop_bets: Option<Vec<f64>>,
    #[serde(default)]
    flop_raises: Option<Vec<f64>>,
    #[serde(default)]
    turn_bets: Option<Vec<f64>>,
    #[serde(default)]
    turn_raises: Option<Vec<f64>>,
    #[serde(default)]
    river_bets: Option<Vec<f64>>,
    #[serde(default)]
    river_raises: Option<Vec<f64>>,
    #[serde(default)]
    always_allow_allin: Option<bool>,
    #[serde(default)]
    max_raises_per_street: Option<u8>,
}

impl BetConfigInput {
    fn into_config(self) -> BetSizingConfig {
        let mut out = BetSizingConfig::default();
        if let Some(v) = self.flop_bets {
            out.flop_bets = v;
        }
        if let Some(v) = self.flop_raises {
            out.flop_raises = v;
        }
        if let Some(v) = self.turn_bets {
            out.turn_bets = v;
        }
        if let Some(v) = self.turn_raises {
            out.turn_raises = v;
        }
        if let Some(v) = self.river_bets {
            out.river_bets = v;
        }
        if let Some(v) = self.river_raises {
            out.river_raises = v;
        }
        if let Some(v) = self.always_allow_allin {
            out.always_allow_allin = v;
        }
        if let Some(v) = self.max_raises_per_street {
            out.max_raises_per_street = v;
        }
        out
    }
}

#[derive(Serialize)]
struct InfoSetOut {
    info_set_idx: u32,
    player: u8,
    actions: Vec<String>,
    probs: Vec<f32>,
    history_label: String,
}

#[derive(Serialize)]
struct SolveOutput {
    game_value: f64,
    exploitability: f64,
    num_info_sets: u32,
    num_nodes: u32,
    strategies: Vec<InfoSetOut>,
}

fn parse_range(s: &str) -> Result<HandRange, String> {
    let trimmed = s.trim();
    if trimmed.eq_ignore_ascii_case("all") || trimmed.is_empty() {
        return Ok(HandRange::full());
    }
    HandRange::from_str(trimmed).map_err(|e| format!("invalid range '{}': {}", s, e))
}

fn run_solve_postflop(input_json: &str) -> Result<SolveOutput, String> {
    let input: SolveInput =
        serde_json::from_str(input_json).map_err(|e| format!("Invalid JSON: {}", e))?;

    let board: Vec<Card> = input
        .board
        .iter()
        .map(|s| Card::from_str(s).map_err(|e| format!("Invalid card '{}': {}", s, e)))
        .collect::<Result<Vec<_>, _>>()?;

    if !matches!(board.len(), 3..=5) {
        return Err(format!(
            "unsupported board length {}: expected 3 (flop), 4 (turn) or 5 (river)",
            board.len()
        ));
    }

    let range_oop = parse_range(&input.range_oop)?;
    let range_ip = parse_range(&input.range_ip)?;

    let bet_config = input
        .bet_config
        .map(|b| b.into_config())
        .unwrap_or_default();

    let cfr_config = SolverConfig {
        algorithm: input.algorithm.into(),
        iterations: input.iterations.max(1),
        ..Default::default()
    };

    let ante = input
        .ante
        .filter(|&a| a > 0.0)
        .unwrap_or(input.starting_pot / 2.0)
        .max(1.0);

    let cfg = VectorSolverConfig {
        board,
        range_oop,
        range_ip,
        starting_pot: input.starting_pot,
        effective_stack: input.effective_stack,
        bet_config,
        cfr_config,
        ante,
    };

    let out = solve_vector(cfg).map_err(|e| e.to_string())?;

    Ok(SolveOutput {
        game_value: out.game_value,
        exploitability: out.exploitability,
        num_info_sets: out.num_info_sets,
        num_nodes: out.num_nodes,
        strategies: out
            .strategies
            .into_iter()
            .map(|s| InfoSetOut {
                info_set_idx: s.info_set_idx,
                player: s.player,
                actions: s.actions.iter().map(|a| a.to_string()).collect(),
                probs: s.probs,
                history_label: s.history_label,
            })
            .collect(),
    })
}
