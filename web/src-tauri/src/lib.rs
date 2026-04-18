use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use poker_odds::cards::Card;
use poker_odds::eval::HandCategory;
use poker_odds::game::{GameState, GameVariant};
use poker_odds::sim::{run_simulation, SimConfig};

use poker_odds::solver::abstraction::NoAbstraction;
use poker_odds::solver::action::BetSizingConfig;
use poker_odds::solver::cfr::{CfrAlgorithm, CfrSolver, SolverConfig};
use poker_odds::solver::exploitability::compute_exploitability;
use poker_odds::solver::postflop::{PostflopConfig, PostflopTreeBuilder};
use poker_odds::solver::range::HandRange;

// ── App State ────────────────────────────────────────────────────────────────

struct AppState {
    /// Active solver cancellation tokens, keyed by solve ID.
    cancel_tokens: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

// ── Odds Calculator DTOs ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SimInputDto {
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

#[derive(Serialize, Clone)]
struct SimOutputDto {
    win: f64,
    tie: f64,
    lose: f64,
    simulations_run: u64,
    method: String,
    hand_distribution: HashMap<String, f64>,
}

#[derive(Serialize)]
struct VariantInfoDto {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    hole_card_count: usize,
    community_card_count: usize,
    max_players: usize,
}

// ── Solver DTOs ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SolverConfigDto {
    board: Vec<String>,
    range_oop: String,
    range_ip: String,
    algorithm: String,
    iterations: u32,
    starting_pot: f32,
    effective_stack: f32,
    flop_bets: Vec<f64>,
    flop_raises: Vec<f64>,
    turn_bets: Vec<f64>,
    turn_raises: Vec<f64>,
    river_bets: Vec<f64>,
    river_raises: Vec<f64>,
    max_raises: u8,
}

#[derive(Serialize, Clone)]
struct SolverProgressDto {
    solve_id: String,
    iteration: u32,
    total: u32,
    game_value: f64,
}

#[derive(Serialize, Clone)]
struct SolverResultDto {
    solve_id: String,
    game_value: f64,
    exploitability_mbb: f64,
    num_info_sets: u32,
    num_nodes: u32,
    strategies: Vec<StrategyEntryDto>,
}

#[derive(Serialize, Clone)]
struct StrategyEntryDto {
    label: String,
    actions: Vec<ActionProbDto>,
}

#[derive(Serialize, Clone)]
struct ActionProbDto {
    name: String,
    probability: f32,
}

// ── Tauri Commands: Odds Calculator ──────────────────────────────────────────

#[tauri::command]
fn calculate_odds(input: SimInputDto) -> Result<SimOutputDto, String> {
    let variant = poker_odds::game::GameVariant::from_id(&input.variant)?;

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

    // Duplicate check
    let mut seen = std::collections::HashSet::new();
    for c in state.known_cards() {
        if !seen.insert(c) {
            return Err(format!("Duplicate card: {}", c));
        }
    }

    let mut config = SimConfig::default();
    if let Some(iter) = input.iterations {
        config.iterations = iter.clamp(1_000, 2_000_000);
    }
    if let Some(et) = input.exact_threshold {
        config.exact_threshold = et;
    }
    config.rng_seed = input.rng_seed;

    let cancel = Arc::new(AtomicBool::new(false));
    let result = run_simulation(&state, &config, cancel, |_| {});

    if !result.is_ready() {
        return Err("Simulation produced no results".to_string());
    }

    let hand_distribution = HandCategory::ALL
        .iter()
        .map(|cat| (cat.name().to_string(), result.hand_pct(*cat) / 100.0))
        .collect();

    Ok(SimOutputDto {
        win: result.win,
        tie: result.tie,
        lose: result.lose,
        simulations_run: result.simulations_run,
        method: result.method.to_string(),
        hand_distribution,
    })
}

#[tauri::command]
fn get_variants() -> Vec<VariantInfoDto> {
    GameVariant::ALL
        .iter()
        .map(|v| VariantInfoDto {
            id: v.id(),
            name: v.name(),
            description: v.description(),
            hole_card_count: v.hole_card_count(),
            community_card_count: v.community_card_count(),
            max_players: v.max_players(),
        })
        .collect()
}

#[tauri::command]
fn validate_card(card: String) -> bool {
    Card::from_str(&card).is_ok()
}

// ── Tauri Commands: GTO Solver ───────────────────────────────────────────────

#[tauri::command]
fn validate_range(range: String) -> Result<u32, String> {
    let r = HandRange::from_str(&range).map_err(|e| format!("{}", e))?;
    let combos = r.weights.iter().filter(|&&w| w > 0.0).count() as u32;
    Ok(combos)
}

#[tauri::command]
fn start_solve(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    config: SolverConfigDto,
) -> Result<String, String> {
    let solve_id = uuid::Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));

    // Store cancellation token
    state
        .cancel_tokens
        .lock()
        .map_err(|e| e.to_string())?
        .insert(solve_id.clone(), Arc::clone(&cancel));

    // Parse board cards
    let board: Vec<Card> = config
        .board
        .iter()
        .map(|s| Card::from_str(s).map_err(|e| format!("Invalid board card '{}': {}", s, e)))
        .collect::<Result<Vec<_>, _>>()?;

    if ![3, 4, 5].contains(&board.len()) {
        return Err(format!(
            "Board must have 3, 4, or 5 cards, got {}",
            board.len()
        ));
    }

    // Parse ranges
    let range_oop = if config.range_oop.is_empty() {
        HandRange::full()
    } else {
        HandRange::from_str(&config.range_oop).map_err(|e| format!("OOP range: {}", e))?
    };
    let range_ip = if config.range_ip.is_empty() {
        HandRange::full()
    } else {
        HandRange::from_str(&config.range_ip).map_err(|e| format!("IP range: {}", e))?
    };

    // Parse algorithm
    let algorithm = match config.algorithm.as_str() {
        "cfr_plus" => CfrAlgorithm::CfrPlus,
        "dcfr" => CfrAlgorithm::Dcfr,
        _ => CfrAlgorithm::CfrPlus,
    };

    let iterations = config.iterations;
    let id = solve_id.clone();

    // Spawn solver thread
    std::thread::spawn(move || {
        let sizing = BetSizingConfig {
            flop_bets: config.flop_bets,
            flop_raises: config.flop_raises,
            turn_bets: config.turn_bets,
            turn_raises: config.turn_raises,
            river_bets: config.river_bets,
            river_raises: config.river_raises,
            max_raises_per_street: config.max_raises,
            always_allow_allin: true,
        };

        let postflop_config = PostflopConfig {
            board,
            range_oop,
            range_ip,
            starting_pot: config.starting_pot,
            effective_stack: config.effective_stack,
            bet_config: sizing,
            abstraction: Box::new(NoAbstraction::new(1326)),
        };

        let tree = PostflopTreeBuilder::new(postflop_config).build();
        let num_nodes = tree.nodes.len() as u32;
        let num_info_sets = tree.num_info_sets;

        let cfr_config = SolverConfig {
            algorithm,
            iterations,
            ..SolverConfig::default()
        };

        let mut solver = CfrSolver::new(tree, cfr_config);
        let mut game_value_sum = 0.0f64;

        for t in 0..iterations {
            if cancel.load(Ordering::Relaxed) {
                break;
            }

            let v = solver.run_iteration(t);
            game_value_sum += v;

            // Emit progress every 10 iterations or on last
            if t % 10 == 0 || t == iterations - 1 {
                let _ = app.emit(
                    "solver-progress",
                    SolverProgressDto {
                        solve_id: id.clone(),
                        iteration: t + 1,
                        total: iterations,
                        game_value: game_value_sum / (t + 1) as f64,
                    },
                );
            }
        }

        if cancel.load(Ordering::Relaxed) {
            return;
        }

        // Compute final results
        let exploitability = compute_exploitability(&solver.tree, &solver.store, 1.0);
        let profile = solver.average_strategy();

        let mut strategies = Vec::new();
        for idx in 0..num_info_sets.min(50) {
            let probs = profile.strategy_at(idx);
            let actions = profile.actions_at(idx);
            if actions.is_empty() {
                continue;
            }
            let action_probs: Vec<ActionProbDto> = actions
                .iter()
                .zip(probs.iter())
                .map(|(a, &p)| ActionProbDto {
                    name: format!("{}", a),
                    probability: p,
                })
                .collect();
            strategies.push(StrategyEntryDto {
                label: format!("Info Set {}", idx),
                actions: action_probs,
            });
        }

        let _ = app.emit(
            "solver-complete",
            SolverResultDto {
                solve_id: id,
                game_value: game_value_sum / iterations as f64,
                exploitability_mbb: exploitability,
                num_info_sets,
                num_nodes,
                strategies,
            },
        );
    });

    Ok(solve_id)
}

#[tauri::command]
fn cancel_solve(state: tauri::State<'_, AppState>, solve_id: String) -> Result<(), String> {
    if let Some(cancel) = state
        .cancel_tokens
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&solve_id)
    {
        cancel.store(true, Ordering::SeqCst);
    }
    Ok(())
}

// ── Tauri Setup ──────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            cancel_tokens: Mutex::new(HashMap::new()),
        })
        .invoke_handler(tauri::generate_handler![
            calculate_odds,
            get_variants,
            validate_card,
            validate_range,
            start_solve,
            cancel_solve,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ── Unit Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use poker_odds::cards::Card;
    use poker_odds::game::GameVariant;
    use poker_odds::solver::range::HandRange;

    // ── Card parsing ─────────────────────────────────────────────────────────

    #[test]
    fn validate_card_valid() {
        assert!(Card::from_str("Ah").is_ok());
        assert!(Card::from_str("2c").is_ok());
        assert!(Card::from_str("Td").is_ok());
        assert!(Card::from_str("Ks").is_ok());
    }

    #[test]
    fn validate_card_invalid() {
        assert!(Card::from_str("XX").is_err());
        assert!(Card::from_str("").is_err());
        assert!(Card::from_str("AhKs").is_err());
    }

    // ── Range parsing ────────────────────────────────────────────────────────

    #[test]
    fn validate_range_aces() {
        let r = HandRange::from_str("AA").unwrap();
        let combos = r.weights.iter().filter(|&&w| w > 0.0).count() as u32;
        assert_eq!(combos, 6);
    }

    #[test]
    fn validate_range_suited() {
        let r = HandRange::from_str("AKs").unwrap();
        let combos = r.weights.iter().filter(|&&w| w > 0.0).count() as u32;
        assert_eq!(combos, 4);
    }

    #[test]
    fn validate_range_invalid() {
        assert!(HandRange::from_str("notarange!!").is_err());
    }

    // ── Variant ID round-trip ────────────────────────────────────────────────

    #[test]
    fn variant_id_roundtrip() {
        for v in GameVariant::ALL {
            let id = v.id();
            assert_eq!(GameVariant::from_id(id).unwrap(), v);
        }
    }

    #[test]
    fn variant_from_id_rejects_unknown() {
        assert!(GameVariant::from_id("bad_variant").is_err());
    }

    // ── calculate_odds integration ───────────────────────────────────────────

    #[test]
    fn calculate_odds_texas_holdem_two_cards() {
        use super::*;
        let input = SimInputDto {
            variant: "texas_holdem".to_string(),
            hole_cards: vec!["Ah".to_string(), "Kh".to_string()],
            community_cards: vec![],
            opponent_count: 1,
            iterations: Some(10_000),
            exact_threshold: None,
            rng_seed: Some(42),
        };
        let result = calculate_odds(input);
        assert!(result.is_ok(), "calculate_odds failed: {:?}", result.err());
        let out = result.unwrap();
        assert!((out.win + out.tie + out.lose - 1.0).abs() < 1e-6);
    }

    #[test]
    fn calculate_odds_rejects_unknown_variant() {
        use super::*;
        let input = SimInputDto {
            variant: "bad_variant".to_string(),
            hole_cards: vec!["Ah".to_string(), "Kh".to_string()],
            community_cards: vec![],
            opponent_count: 1,
            iterations: None,
            exact_threshold: None,
            rng_seed: None,
        };
        assert!(calculate_odds(input).is_err());
    }

    #[test]
    fn calculate_odds_rejects_invalid_card() {
        use super::*;
        let input = SimInputDto {
            variant: "texas_holdem".to_string(),
            hole_cards: vec!["XX".to_string(), "Kh".to_string()],
            community_cards: vec![],
            opponent_count: 1,
            iterations: None,
            exact_threshold: None,
            rng_seed: None,
        };
        assert!(calculate_odds(input).is_err());
    }

    #[test]
    fn calculate_odds_rejects_duplicate_card() {
        use super::*;
        let input = SimInputDto {
            variant: "texas_holdem".to_string(),
            hole_cards: vec!["Ah".to_string(), "Ah".to_string()],
            community_cards: vec![],
            opponent_count: 1,
            iterations: None,
            exact_threshold: None,
            rng_seed: None,
        };
        assert!(calculate_odds(input).is_err());
    }

    #[test]
    fn get_variants_returns_all_four() {
        use super::*;
        let variants = get_variants();
        assert_eq!(variants.len(), 4);
        let ids: Vec<&str> = variants.iter().map(|v| v.id).collect();
        assert!(ids.contains(&"texas_holdem"));
        assert!(ids.contains(&"omaha_holdem"));
        assert!(ids.contains(&"seven_card_stud"));
        assert!(ids.contains(&"five_card_draw"));
    }
}
