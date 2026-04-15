use std::collections::HashMap;
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
    let variant = parse_variant(&input.variant)?;

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
            id: variant_id(*v),
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

// ── Helpers ──────────────────────────────────────────────────────────────────

fn parse_variant(s: &str) -> Result<GameVariant, String> {
    match s {
        "texas_holdem" => Ok(GameVariant::TexasHoldem),
        "omaha_holdem" => Ok(GameVariant::OmahaHoldem),
        "seven_card_stud" => Ok(GameVariant::SevenCardStud),
        "five_card_draw" => Ok(GameVariant::FiveCardDraw),
        _ => Err(format!("Unknown variant '{}'", s)),
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
