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
    SettingsAction, SettingsScreen, VariantSelectScreen,
};
use crate::tui::theme::Theme;

pub enum Screen {
    VariantSelect(VariantSelectScreen),
    HoleCards(HoleCardsScreen),
    Community(CommunityScreen),
    OddsDisplay(OddsDisplayScreen),
    Settings(SettingsScreen, Box<Screen>), // Settings overlays the previous screen
}

pub struct App {
    pub screen: Option<Screen>,
    pub game_state: Option<GameState>,
    pub sim_config: SimConfig,
    pub odds_result: Arc<RwLock<OddsResult>>,
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
                if let Some(variant) = s.handle_key(key) {
                    Some(Screen::HoleCards(HoleCardsScreen::new(variant)))
                } else {
                    Some(Screen::VariantSelect(s))
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
            None => {}
        }
    }
}
