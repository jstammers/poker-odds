//! Central application state and event loop.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, RwLock,
};
use std::thread;
use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::Frame;

use crate::game::GameState;
use crate::sim::{run_simulation, CancelFlag, OddsResult, SimConfig};
use crate::tui::events::{is_quit, poll_event, AppEvent};
use crate::tui::screens::{
    CommunityAction, CommunityScreen, HoleCardsScreen, OddsAction, OddsDisplayScreen,
    SettingsAction, SettingsScreen, SolverConfigAction, SolverConfigScreen, SolverParams,
    SolverProgress, SolverResultsAction, SolverResultsScreen, VariantSelectScreen,
};
use crate::tui::screens::variant_select::VariantSelectResult;
use crate::tui::theme::Theme;

pub enum Screen {
    VariantSelect(VariantSelectScreen),
    HoleCards(HoleCardsScreen),
    Community(CommunityScreen),
    OddsDisplay(OddsDisplayScreen),
    Settings(SettingsScreen, Box<Screen>), // Settings overlays the previous screen
    SolverConfig(SolverConfigScreen),
    SolverResults(SolverResultsScreen),
}

pub struct App {
    pub screen: Option<Screen>,
    pub game_state: Option<GameState>,
    pub sim_config: SimConfig,
    pub odds_result: Arc<RwLock<OddsResult>>,
    pub solver_progress: Arc<RwLock<SolverProgress>>,
    pub cancel_flag: CancelFlag,
    pub running: bool,
}

impl App {
    pub fn new() -> Self {
        App {
            screen: Some(Screen::VariantSelect(VariantSelectScreen::new())),
            game_state: None,
            sim_config: SimConfig::default(),
            odds_result: Arc::new(RwLock::new(OddsResult::default())),
            solver_progress: Arc::new(RwLock::new(SolverProgress::default())),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            running: true,
        }
    }

    pub fn run(&mut self, terminal: &mut ratatui::Terminal<impl ratatui::backend::Backend>) -> anyhow::Result<()> {
        while self.running {
            terminal.draw(|frame| self.render(frame))?;

            if let Some(event) = poll_event(50)? {
                self.handle_event(event);
            }
        }
        Ok(())
    }

    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => {
                if is_quit(&key) {
                    self.running = false;
                    return;
                }
                self.handle_key(key);
            }
            AppEvent::Tick => {
                // Nothing to do on tick — odds_result is updated from background thread
            }
            AppEvent::Resize(_, _) => {
                // ratatui handles resize automatically
            }
        }
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        let screen = self.screen.take();
        self.screen = match screen {
            Some(Screen::VariantSelect(mut s)) => {
                match s.handle_key(key) {
                    Some(VariantSelectResult::Variant(variant)) => {
                        Some(Screen::HoleCards(HoleCardsScreen::new(variant)))
                    }
                    Some(VariantSelectResult::GtoSolver) => {
                        Some(Screen::SolverConfig(SolverConfigScreen::new()))
                    }
                    None => Some(Screen::VariantSelect(s)),
                }
            }
            Some(Screen::HoleCards(mut s)) => {
                if key.code == KeyCode::Esc {
                    Some(Screen::VariantSelect(VariantSelectScreen::new()))
                } else if let Some((hole_cards, opponent_count)) = s.handle_key(key) {
                    let variant = s.variant;
                    let mut state = GameState::new(variant);
                    state.hole_cards = hole_cards;
                    state.opponent_count = opponent_count;

                    if variant.has_community_cards() {
                        // Advance from preflop — show community input
                        state.advance_round();
                        let community_screen = CommunityScreen::new(&state);
                        self.game_state = Some(state);
                        Some(Screen::Community(community_screen))
                    } else {
                        // No community cards (Draw/Stud) — go directly to odds
                        self.start_simulation(&state);
                        self.game_state = Some(state.clone());
                        Some(Screen::OddsDisplay(OddsDisplayScreen::new(variant)))
                    }
                } else {
                    Some(Screen::HoleCards(s))
                }
            }
            Some(Screen::Community(mut s)) => {
                match s.handle_key(key) {
                    CommunityAction::None => Some(Screen::Community(s)),
                    CommunityAction::Back => {
                        // Go back to hole cards
                        if let Some(ref state) = self.game_state {
                            Some(Screen::HoleCards(HoleCardsScreen::new(state.variant)))
                        } else {
                            Some(Screen::VariantSelect(VariantSelectScreen::new()))
                        }
                    }
                    CommunityAction::Cards(new_cards) => {
                        if let Some(ref mut state) = self.game_state {
                            state.community_cards.extend(new_cards);
                        }
                        if let Some(ref state) = self.game_state {
                            let state_clone = state.clone();
                            let variant = state.variant;
                            self.cancel_simulation();
                            self.start_simulation(&state_clone);
                            Some(Screen::OddsDisplay(OddsDisplayScreen::new(variant)))
                        } else {
                            Some(Screen::Community(s))
                        }
                    }
                }
            }
            Some(Screen::OddsDisplay(s)) => {
                let action = if let Some(ref state) = self.game_state {
                    s.handle_key(key, state)
                } else {
                    OddsAction::None
                };
                match action {
                    OddsAction::None => Some(Screen::OddsDisplay(s)),
                    OddsAction::NextRound => {
                        if let Some(ref mut state) = self.game_state {
                            state.advance_round();
                            if state.is_complete() {
                                // No more community cards — already showing odds
                                Some(Screen::OddsDisplay(s))
                            } else {
                                let community_screen = CommunityScreen::new(state);
                                Some(Screen::Community(community_screen))
                            }
                        } else {
                            Some(Screen::OddsDisplay(s))
                        }
                    }
                    OddsAction::Restart => {
                        self.cancel_simulation();
                        self.game_state = None;
                        Some(Screen::VariantSelect(VariantSelectScreen::new()))
                    }
                    OddsAction::Back => {
                        if let Some(ref state) = self.game_state {
                            let community_screen = CommunityScreen::new(state);
                            Some(Screen::Community(community_screen))
                        } else {
                            Some(Screen::VariantSelect(VariantSelectScreen::new()))
                        }
                    }
                    OddsAction::Settings => {
                        let settings = SettingsScreen::new(self.sim_config.clone());
                        Some(Screen::Settings(settings, Box::new(Screen::OddsDisplay(s))))
                    }
                }
            }
            Some(Screen::Settings(mut s, prev)) => {
                match s.handle_key(key) {
                    SettingsAction::None => Some(Screen::Settings(s, prev)),
                    SettingsAction::Close => Some(*prev),
                    SettingsAction::Save(config) => {
                        self.sim_config = config;
                        // Restart simulation with new config
                        if let Some(ref state) = self.game_state {
                            self.cancel_simulation();
                            self.start_simulation(state);
                        }
                        Some(*prev)
                    }
                }
            }
            Some(Screen::SolverConfig(mut s)) => {
                match s.handle_key(key) {
                    SolverConfigAction::None => Some(Screen::SolverConfig(s)),
                    SolverConfigAction::Back => {
                        Some(Screen::VariantSelect(VariantSelectScreen::new()))
                    }
                    SolverConfigAction::Run(params) => {
                        self.start_solver(&params);
                        Some(Screen::SolverResults(SolverResultsScreen::new()))
                    }
                }
            }
            Some(Screen::SolverResults(mut s)) => {
                match s.handle_key(key) {
                    SolverResultsAction::None => Some(Screen::SolverResults(s)),
                    SolverResultsAction::Back => {
                        Some(Screen::VariantSelect(VariantSelectScreen::new()))
                    }
                    SolverResultsAction::Reconfigure => {
                        Some(Screen::SolverConfig(SolverConfigScreen::new()))
                    }
                }
            }
            None => None,
        };
    }

    fn start_simulation(&self, state: &GameState) {
        let state = state.clone();
        let config = self.sim_config.clone();
        let result_ref = Arc::clone(&self.odds_result);
        let cancel = Arc::clone(&self.cancel_flag);
        cancel.store(false, Ordering::SeqCst);

        // Reset result
        if let Ok(mut r) = result_ref.write() {
            *r = OddsResult::default();
        }

        let result_ref2 = Arc::clone(&result_ref);
        let cancel2 = Arc::clone(&cancel);

        thread::spawn(move || {
            let final_result = run_simulation(&state, &config, cancel2, |partial| {
                if let Ok(mut r) = result_ref2.write() {
                    *r = partial;
                }
            });
            if let Ok(mut r) = result_ref2.write() {
                *r = final_result;
            }
        });
    }

    fn cancel_simulation(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
        // Small yield to let the sim thread notice cancellation
        thread::sleep(Duration::from_millis(5));
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start_solver(&self, params: &SolverParams) {
        use crate::solver::action::BetSizingConfig;
        use crate::solver::abstraction::NoAbstraction;
        use crate::solver::cfr::{CfrSolver, SolverConfig as CfrSolverConfig};
        use crate::solver::exploitability::compute_exploitability;
        use crate::solver::postflop::{PostflopConfig, PostflopTreeBuilder};
        use crate::solver::range::HandRange;

        let progress_ref = Arc::clone(&self.solver_progress);

        // Reset progress
        if let Ok(mut p) = progress_ref.write() {
            *p = SolverProgress {
                total_iterations: params.iterations,
                algorithm: Some(params.algorithm),
                board: params.board.clone(),
                pot: params.starting_pot,
                stack: params.effective_stack,
                street_name: params.street.name().to_string(),
                range_oop: params.range_oop.clone(),
                range_ip: params.range_ip.clone(),
                ..SolverProgress::default()
            };
        }

        let board = params.board.clone();
        let algorithm = params.algorithm;
        let iterations = params.iterations;
        let bet_sizes = params.bet_sizes.clone();
        let raise_sizes = params.raise_sizes.clone();
        let starting_pot = params.starting_pot;
        let effective_stack = params.effective_stack;
        let max_raises = params.max_raises;
        let range_oop_str = params.range_oop.clone();
        let range_ip_str = params.range_ip.clone();

        let progress_ref2 = Arc::clone(&progress_ref);

        thread::spawn(move || {
            // Parse ranges (empty = full range)
            let range_oop = if range_oop_str.is_empty() {
                HandRange::full()
            } else {
                match HandRange::from_str(&range_oop_str) {
                    Ok(r) => r,
                    Err(_) => HandRange::full(),
                }
            };
            let range_ip = if range_ip_str.is_empty() {
                HandRange::full()
            } else {
                match HandRange::from_str(&range_ip_str) {
                    Ok(r) => r,
                    Err(_) => HandRange::full(),
                }
            };

            // Use the same bet sizes for all streets in the tree
            let sizing = BetSizingConfig {
                flop_bets: bet_sizes.clone(),
                flop_raises: raise_sizes.clone(),
                turn_bets: bet_sizes.clone(),
                turn_raises: raise_sizes.clone(),
                river_bets: bet_sizes.clone(),
                river_raises: raise_sizes.clone(),
                max_raises_per_street: max_raises,
                always_allow_allin: true,
            };

            let config = PostflopConfig {
                board,
                range_oop,
                range_ip,
                starting_pot,
                effective_stack,
                bet_config: sizing,
                abstraction: Box::new(NoAbstraction::new(1326)),
            };

            let tree = PostflopTreeBuilder::new(config).build();
            let num_nodes = tree.nodes.len() as u32;
            let num_info_sets = tree.num_info_sets;

            let cfr_config = CfrSolverConfig {
                algorithm,
                iterations,
                ..CfrSolverConfig::default()
            };

            let mut solver = CfrSolver::new(tree, cfr_config);

            // Update progress with tree info
            if let Ok(mut p) = progress_ref2.write() {
                p.num_nodes = num_nodes;
                p.num_info_sets = num_info_sets;
            }

            let mut game_value_sum = 0.0f64;
            for i in 0..iterations {
                let v = solver.run_iteration(i);
                game_value_sum += v;

                // Update progress every 10 iterations or on last
                if i % 10 == 0 || i == iterations - 1 {
                    if let Ok(mut p) = progress_ref2.write() {
                        p.iteration = i + 1;
                        p.game_value = game_value_sum / (i + 1) as f64;
                    }
                }
            }

            // Compute exploitability
            let exploitability = compute_exploitability(&solver.tree, &solver.store, 1.0);
            let profile = solver.average_strategy();

            // Extract strategies for display: show info sets with their action probabilities
            let mut strategies: Vec<(String, Vec<(String, f32)>)> = Vec::new();
            for info_idx in 0..num_info_sets.min(20) {
                let probs = profile.strategy_at(info_idx);
                let actions = profile.actions_at(info_idx);
                if actions.is_empty() {
                    continue;
                }
                let label = format!("Info Set {}", info_idx);
                let action_probs: Vec<(String, f32)> = actions
                    .iter()
                    .zip(probs.iter())
                    .map(|(a, &p)| (format!("{}", a), p))
                    .collect();
                strategies.push((label, action_probs));
            }

            if let Ok(mut p) = progress_ref2.write() {
                p.iteration = iterations;
                p.done = true;
                p.game_value = game_value_sum / iterations as f64;
                p.exploitability = Some(exploitability);
                p.strategies = strategies;
            }
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn start_solver(&self, _params: &SolverParams) {
        // Solver not available on wasm
    }

    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Fill background
        let bg = ratatui::widgets::Block::default()
            .style(ratatui::style::Style::default().bg(Theme::BG));
        frame.render_widget(bg, area);

        let result = self.odds_result.read().ok().map(|r| r.clone()).unwrap_or_default();

        match &mut self.screen {
            Some(Screen::VariantSelect(s)) => s.render(frame, area),
            Some(Screen::HoleCards(s)) => s.render(frame, area),
            Some(Screen::Community(s)) => s.render(frame, area),
            Some(Screen::OddsDisplay(s)) => {
                if let Some(ref state) = self.game_state {
                    s.render(frame, area, state, &result);
                }
            }
            Some(Screen::Settings(s, _)) => s.render(frame, area),
            Some(Screen::SolverConfig(s)) => s.render(frame, area),
            Some(Screen::SolverResults(s)) => {
                let progress = self.solver_progress.read().ok().map(|p| p.clone()).unwrap_or_default();
                s.render(frame, area, &progress);
            }
            None => {}
        }
    }
}
