//! wasm-bindgen public API — exposes the poker odds engine to JavaScript.

use std::collections::HashMap;
use std::sync::{
    atomic::AtomicBool,
    Arc,
};

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::cards::Card;
use crate::eval::HandCategory;
use crate::game::{GameState, GameVariant};
use crate::sim::{run_simulation, SimConfig};

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

fn default_opponents() -> usize { 1 }

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
            id: variant_id(*v),
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

// ── Internals ─────────────────────────────────────────────────────────────────

fn run_calculate(input_json: &str) -> Result<SimOutput, String> {
    let input: SimInput =
        serde_json::from_str(input_json).map_err(|e| format!("Invalid JSON: {}", e))?;

    let variant = parse_variant(&input.variant)?;

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

fn parse_variant(s: &str) -> Result<GameVariant, String> {
    match s {
        "texas_holdem" => Ok(GameVariant::TexasHoldem),
        "omaha_holdem" => Ok(GameVariant::OmahaHoldem),
        "seven_card_stud" => Ok(GameVariant::SevenCardStud),
        "five_card_draw" => Ok(GameVariant::FiveCardDraw),
        _ => Err(format!("Unknown variant '{}'. Use one of: texas_holdem, omaha_holdem, seven_card_stud, five_card_draw", s)),
    }
}

fn variant_id(v: GameVariant) -> &'static str {
    match v {
        GameVariant::TexasHoldem => "texas_holdem",
        GameVariant::OmahaHoldem => "omaha_holdem",
        GameVariant::SevenCardStud => "seven_card_stud",
        GameVariant::FiveCardDraw => "five_card_draw",
    }
}

fn error_json(msg: &str) -> String {
    serde_json::to_string(&ErrorOutput { error: msg.to_string() })
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}
