use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::solver::cfr::CfrAlgorithm;
use crate::tui::theme::Theme;
use crate::tui::widgets::card_input::MultiCardInput;

pub enum SolverConfigAction {
    None,
    Back,
    Run(SolverParams),
}

/// Parameters collected by the config screen, passed to the solver.
#[derive(Clone, Debug)]
pub struct SolverParams {
    pub board: Vec<crate::cards::Card>,
    pub algorithm: CfrAlgorithm,
    pub iterations: u32,
    pub bet_sizes: Vec<f64>,
    pub raise_sizes: Vec<f64>,
    pub starting_pot: f32,
    pub effective_stack: f32,
    pub max_raises: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Field {
    Board,
    Algorithm,
    Iterations,
    StartingPot,
    EffectiveStack,
    BetSizes,
    RaiseSizes,
    MaxRaises,
}

impl Field {
    const ALL: [Field; 8] = [
        Field::Board,
        Field::Algorithm,
        Field::Iterations,
        Field::StartingPot,
        Field::EffectiveStack,
        Field::BetSizes,
        Field::RaiseSizes,
        Field::MaxRaises,
    ];

    fn label(self) -> &'static str {
        match self {
            Field::Board => "Board Cards",
            Field::Algorithm => "Algorithm",
            Field::Iterations => "Iterations",
            Field::StartingPot => "Starting Pot",
            Field::EffectiveStack => "Effective Stack",
            Field::BetSizes => "Bet Sizes (% of pot)",
            Field::RaiseSizes => "Raise Sizes (% of pot)",
            Field::MaxRaises => "Max Raises / Street",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Field::Board => "Enter 5 board cards for river solve (enter rank then suit)",
            Field::Algorithm => "CFR+ or DCFR (Discounted CFR converges faster)",
            Field::Iterations => "Number of CFR iterations (more = more precise, slower)",
            Field::StartingPot => "Total pot size in chips before this street",
            Field::EffectiveStack => "Chips remaining behind for each player",
            Field::BetSizes => "Comma-separated pot fractions, e.g. 33,50,75,100",
            Field::RaiseSizes => "Comma-separated pot fractions for raises",
            Field::MaxRaises => "Cap on raises per street (prevents infinite trees)",
        }
    }
}

pub struct SolverConfigScreen {
    active_field: usize,
    board_input: MultiCardInput,
    algorithm: CfrAlgorithm,
    iterations: u32,
    starting_pot: f32,
    effective_stack: f32,
    bet_sizes: Vec<f64>,
    raise_sizes: Vec<f64>,
    max_raises: u8,
    edit_buffer: String,
    editing: bool,
    error: Option<String>,
}

impl SolverConfigScreen {
    pub fn new() -> Self {
        SolverConfigScreen {
            active_field: 0,
            board_input: MultiCardInput::new(5, "Board"),
            algorithm: CfrAlgorithm::CfrPlus,
            iterations: 1_000,
            starting_pot: 100.0,
            effective_stack: 200.0,
            bet_sizes: vec![50.0, 75.0, 100.0],
            raise_sizes: vec![100.0],
            max_raises: 2,
            edit_buffer: String::new(),
            editing: false,
            error: None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SolverConfigAction {
        if self.editing {
            return self.handle_edit_key(key);
        }

        // When board field is active and not editing a numeric field,
        // delegate to card input
        if Field::ALL[self.active_field] == Field::Board {
            match key.code {
                KeyCode::Esc => return SolverConfigAction::Back,
                KeyCode::Tab => {
                    self.active_field = (self.active_field + 1) % Field::ALL.len();
                }
                KeyCode::BackTab => {
                    self.active_field = if self.active_field == 0 {
                        Field::ALL.len() - 1
                    } else {
                        self.active_field - 1
                    };
                }
                KeyCode::Enter if self.board_input.all_complete() => {
                    return self.try_run();
                }
                _ => {
                    self.board_input.handle_key(key, &[]);
                }
            }
            return SolverConfigAction::None;
        }

        match key.code {
            KeyCode::Esc => return SolverConfigAction::Back,
            KeyCode::Up | KeyCode::Char('k') => {
                if self.active_field > 0 {
                    self.active_field -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.active_field < Field::ALL.len() - 1 {
                    self.active_field += 1;
                }
            }
            KeyCode::Tab => {
                self.active_field = (self.active_field + 1) % Field::ALL.len();
            }
            KeyCode::BackTab => {
                self.active_field = if self.active_field == 0 {
                    Field::ALL.len() - 1
                } else {
                    self.active_field - 1
                };
            }
            KeyCode::Enter | KeyCode::Char('e') => {
                let field = Field::ALL[self.active_field];
                match field {
                    Field::Algorithm => {
                        // Toggle algorithm
                        self.algorithm = match self.algorithm {
                            CfrAlgorithm::CfrPlus => CfrAlgorithm::Dcfr,
                            CfrAlgorithm::Dcfr => CfrAlgorithm::CfrPlus,
                        };
                    }
                    Field::Board => {} // Handled above
                    _ => self.start_edit(),
                }
            }
            KeyCode::Char('r') => {
                return self.try_run();
            }
            _ => {}
        }
        SolverConfigAction::None
    }

    fn start_edit(&mut self) {
        self.editing = true;
        self.error = None;
        self.edit_buffer = match Field::ALL[self.active_field] {
            Field::Iterations => self.iterations.to_string(),
            Field::StartingPot => format!("{:.0}", self.starting_pot),
            Field::EffectiveStack => format!("{:.0}", self.effective_stack),
            Field::BetSizes => self
                .bet_sizes
                .iter()
                .map(|v| format!("{:.0}", v))
                .collect::<Vec<_>>()
                .join(","),
            Field::RaiseSizes => self
                .raise_sizes
                .iter()
                .map(|v| format!("{:.0}", v))
                .collect::<Vec<_>>()
                .join(","),
            Field::MaxRaises => self.max_raises.to_string(),
            _ => String::new(),
        };
    }

    fn handle_edit_key(&mut self, key: KeyEvent) -> SolverConfigAction {
        match key.code {
            KeyCode::Esc => {
                self.editing = false;
                self.edit_buffer.clear();
            }
            KeyCode::Enter => {
                self.apply_edit();
                self.editing = false;
                self.edit_buffer.clear();
            }
            KeyCode::Backspace => {
                self.edit_buffer.pop();
            }
            KeyCode::Char(c) if c.is_ascii_digit() || c == ',' || c == '.' => {
                self.edit_buffer.push(c);
            }
            _ => {}
        }
        SolverConfigAction::None
    }

    fn apply_edit(&mut self) {
        let field = Field::ALL[self.active_field];
        match field {
            Field::Iterations => {
                if let Ok(val) = self.edit_buffer.parse::<u32>() {
                    self.iterations = val.max(10).min(1_000_000);
                }
            }
            Field::StartingPot => {
                if let Ok(val) = self.edit_buffer.parse::<f32>() {
                    self.starting_pot = val.max(1.0).min(100_000.0);
                }
            }
            Field::EffectiveStack => {
                if let Ok(val) = self.edit_buffer.parse::<f32>() {
                    self.effective_stack = val.max(1.0).min(100_000.0);
                }
            }
            Field::BetSizes => {
                let parsed: Vec<f64> = self
                    .edit_buffer
                    .split(',')
                    .filter_map(|s| s.trim().parse().ok())
                    .filter(|&v: &f64| v > 0.0 && v <= 500.0)
                    .collect();
                if !parsed.is_empty() {
                    self.bet_sizes = parsed;
                }
            }
            Field::RaiseSizes => {
                let parsed: Vec<f64> = self
                    .edit_buffer
                    .split(',')
                    .filter_map(|s| s.trim().parse().ok())
                    .filter(|&v: &f64| v > 0.0 && v <= 500.0)
                    .collect();
                if !parsed.is_empty() {
                    self.raise_sizes = parsed;
                }
            }
            Field::MaxRaises => {
                if let Ok(val) = self.edit_buffer.parse::<u8>() {
                    self.max_raises = val.min(5);
                }
            }
            _ => {}
        }
    }

    fn try_run(&mut self) -> SolverConfigAction {
        if !self.board_input.all_complete() {
            self.error = Some("Enter all 5 board cards first".to_string());
            return SolverConfigAction::None;
        }

        let board = self.board_input.cards();
        let bet_fracs: Vec<f64> = self.bet_sizes.iter().map(|v| v / 100.0).collect();
        let raise_fracs: Vec<f64> = self.raise_sizes.iter().map(|v| v / 100.0).collect();

        SolverConfigAction::Run(SolverParams {
            board,
            algorithm: self.algorithm,
            iterations: self.iterations,
            bet_sizes: bet_fracs,
            raise_sizes: raise_fracs,
            starting_pot: self.starting_pot,
            effective_stack: self.effective_stack,
            max_raises: self.max_raises,
        })
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let outer = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border_focused())
            .title(Span::styled(" GTO Solver — Configuration ", Theme::title()));
        let inner = outer.inner(area);
        frame.render_widget(outer, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4), // Board (card input)
                Constraint::Length(3), // Algorithm
                Constraint::Length(3), // Iterations
                Constraint::Length(3), // Starting pot
                Constraint::Length(3), // Effective stack
                Constraint::Length(3), // Bet sizes
                Constraint::Length(3), // Raise sizes
                Constraint::Length(3), // Max raises
                Constraint::Length(2), // Error
                Constraint::Fill(1),  // spacer
                Constraint::Length(1), // Help
            ])
            .split(inner);

        // Board card input
        self.render_board_field(frame, chunks[0]);

        // Other fields
        for (i, field) in Field::ALL.iter().enumerate().skip(1) {
            let chunk_idx = i;
            if chunk_idx < chunks.len() {
                self.render_field(frame, chunks[chunk_idx], *field, i);
            }
        }

        // Error
        if let Some(ref err) = self.error {
            let err_para = Paragraph::new(Line::from(Span::styled(
                format!("  {err}"),
                Theme::lose(),
            )));
            frame.render_widget(err_para, chunks[8]);
        }

        // Help
        self.render_help(frame, chunks[10]);
    }

    fn render_board_field(&self, frame: &mut Frame, area: Rect) {
        let is_active = self.active_field == 0;
        let border_style = if is_active {
            Theme::border_focused()
        } else {
            Theme::border()
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(
                " Board Cards ",
                if is_active { Theme::title() } else { Theme::dim() },
            ));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut spans: Vec<Span> = vec![Span::styled("  ", Theme::normal())];
        for (i, slot) in self.board_input.slots.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            match &slot.state {
                crate::tui::widgets::card_input::InputState::AwaitingRank => {
                    if is_active && i == self.board_input.active_slot {
                        spans.push(Span::styled("__", Theme::accent()));
                    } else {
                        spans.push(Span::styled("??", Theme::dim()));
                    }
                }
                crate::tui::widgets::card_input::InputState::AwaitingSuit(rank) => {
                    spans.push(Span::styled(
                        format!("{}_", rank.to_char()),
                        Theme::accent(),
                    ));
                }
                crate::tui::widgets::card_input::InputState::Confirmed(card) => {
                    spans.push(crate::tui::widgets::card_span(*card));
                }
            }
        }

        // Show hint
        if is_active {
            if let Some(err) = self.board_input.error() {
                spans.push(Span::styled(format!("  {err}"), Theme::lose()));
            } else {
                spans.push(Span::styled("  (type rank then suit)", Theme::dim()));
            }
        }

        frame.render_widget(
            Paragraph::new(vec![
                Line::from(spans),
                Line::from(Span::styled(
                    format!("   {}", Field::Board.hint()),
                    Theme::dim(),
                )),
            ]),
            inner,
        );
    }

    fn render_field(&self, frame: &mut Frame, area: Rect, field: Field, field_idx: usize) {
        let is_active = self.active_field == field_idx;
        let is_editing = is_active && self.editing;

        let value_str = if is_editing {
            format!("{}_", self.edit_buffer)
        } else {
            match field {
                Field::Algorithm => match self.algorithm {
                    CfrAlgorithm::CfrPlus => "CFR+".to_string(),
                    CfrAlgorithm::Dcfr => "DCFR (Discounted)".to_string(),
                },
                Field::Iterations => format!("{}", self.iterations),
                Field::StartingPot => format!("{:.0}", self.starting_pot),
                Field::EffectiveStack => format!("{:.0}", self.effective_stack),
                Field::BetSizes => self
                    .bet_sizes
                    .iter()
                    .map(|v| format!("{:.0}%", v))
                    .collect::<Vec<_>>()
                    .join(", "),
                Field::RaiseSizes => self
                    .raise_sizes
                    .iter()
                    .map(|v| format!("{:.0}%", v))
                    .collect::<Vec<_>>()
                    .join(", "),
                Field::MaxRaises => self.max_raises.to_string(),
                Field::Board => String::new(), // Handled separately
            }
        };

        let label_style = if is_active {
            Theme::highlight()
        } else {
            Theme::normal()
        };
        let value_style = if is_editing {
            Theme::accent()
        } else if is_active {
            Theme::win()
        } else {
            Theme::dim()
        };
        let prefix = if is_active { "▶ " } else { "  " };

        let lines = vec![Line::from(vec![
            Span::styled(prefix, Theme::highlight()),
            Span::styled(field.label(), label_style.add_modifier(Modifier::BOLD)),
            Span::styled(": ", Theme::dim()),
            Span::styled(value_str, value_style.add_modifier(Modifier::BOLD)),
        ])];
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let spans = vec![
            Span::styled("↑↓/Tab", Theme::highlight()),
            Span::styled(" Navigate  ", Theme::dim()),
            Span::styled("Enter", Theme::highlight()),
            Span::styled(" Edit/Toggle  ", Theme::dim()),
            Span::styled("r", Theme::highlight()),
            Span::styled(" Run Solver  ", Theme::dim()),
            Span::styled("Esc", Theme::highlight()),
            Span::styled(" Back", Theme::dim()),
        ];
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}
