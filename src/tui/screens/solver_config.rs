use std::str::FromStr;

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

/// Which street to solve from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Street {
    Flop,
    Turn,
    River,
}

impl Street {
    pub fn board_cards(self) -> usize {
        match self {
            Street::Flop => 3,
            Street::Turn => 4,
            Street::River => 5,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Street::Flop => "Flop",
            Street::Turn => "Turn",
            Street::River => "River",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Street::Flop => Street::Turn,
            Street::Turn => Street::River,
            Street::River => Street::Flop,
        }
    }
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
    pub street: Street,
    pub range_oop: String,
    pub range_ip: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Field {
    Street,
    Board,
    RangeOOP,
    RangeIP,
    Algorithm,
    Iterations,
    StartingPot,
    EffectiveStack,
    BetSizes,
    RaiseSizes,
    MaxRaises,
}

impl Field {
    const ALL: [Field; 11] = [
        Field::Street,
        Field::Board,
        Field::RangeOOP,
        Field::RangeIP,
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
            Field::Street => "Street",
            Field::Board => "Board Cards",
            Field::RangeOOP => "OOP Range",
            Field::RangeIP => "IP Range",
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
            Field::Street => "Flop (3 cards), Turn (4 cards), or River (5 cards)",
            Field::Board => "Enter board cards (rank then suit, e.g. Ah Kd Qs)",
            Field::RangeOOP => "Out-of-position range, e.g. AA,AKs,QQ-TT,AJs-A9s",
            Field::RangeIP => "In-position range, e.g. AA-22,AKs-A2s,KQs-KTs",
            Field::Algorithm => "CFR+ or DCFR (Discounted CFR converges faster)",
            Field::Iterations => "Number of CFR iterations (more = more precise, slower)",
            Field::StartingPot => "Total pot size in chips before this street",
            Field::EffectiveStack => "Chips remaining behind for each player",
            Field::BetSizes => "Comma-separated pot fractions, e.g. 33,50,75,100",
            Field::RaiseSizes => "Comma-separated pot fractions for raises",
            Field::MaxRaises => "Cap on raises per street (prevents infinite trees)",
        }
    }

    /// Whether this field uses text/range editing mode.
    fn is_text_edit(self) -> bool {
        matches!(self, Field::RangeOOP | Field::RangeIP)
    }
}

pub struct SolverConfigScreen {
    active_field: usize,
    street: Street,
    board_input: MultiCardInput,
    range_oop: String,
    range_ip: String,
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

impl Default for SolverConfigScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl SolverConfigScreen {
    pub fn new() -> Self {
        SolverConfigScreen {
            active_field: 0,
            street: Street::River,
            board_input: MultiCardInput::new(5, "Board"),
            range_oop: String::new(),
            range_ip: String::new(),
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

    fn set_street(&mut self, street: Street) {
        if self.street != street {
            self.street = street;
            self.board_input = MultiCardInput::new(street.board_cards(), "Board");
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SolverConfigAction {
        if self.editing {
            return self.handle_edit_key(key);
        }

        let current_field = Field::ALL[self.active_field];

        // When board field is active, delegate to card input
        if current_field == Field::Board {
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
                    // Move to next field instead of running
                    self.active_field = (self.active_field + 1) % Field::ALL.len();
                }
                _ => {
                    self.board_input.handle_key(key, &[]);
                }
            }
            return SolverConfigAction::None;
        }

        match key.code {
            KeyCode::Esc => return SolverConfigAction::Back,
            KeyCode::Up | KeyCode::Char('k') if self.active_field > 0 => {
                self.active_field -= 1;
            }
            KeyCode::Down | KeyCode::Char('j') if self.active_field < Field::ALL.len() - 1 => {
                self.active_field += 1;
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
                match current_field {
                    Field::Street => {
                        self.set_street(self.street.next());
                    }
                    Field::Algorithm => {
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
            Field::RangeOOP => self.range_oop.clone(),
            Field::RangeIP => self.range_ip.clone(),
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
        let current_field = Field::ALL[self.active_field];
        let is_text = current_field.is_text_edit();

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
            KeyCode::Char(c) => {
                if is_text {
                    // Range fields accept alphanumeric + commas + dashes
                    if c.is_ascii_alphanumeric() || c == ',' || c == '-' || c == ' ' {
                        self.edit_buffer.push(c);
                    }
                } else if c.is_ascii_digit() || c == ',' || c == '.' {
                    self.edit_buffer.push(c);
                }
            }
            _ => {}
        }
        SolverConfigAction::None
    }

    fn apply_edit(&mut self) {
        let field = Field::ALL[self.active_field];
        match field {
            Field::RangeOOP => {
                // Validate range string
                let trimmed = self.edit_buffer.trim().to_string();
                if trimmed.is_empty() {
                    self.range_oop.clear();
                } else {
                    match crate::solver::range::HandRange::from_str(&trimmed) {
                        Ok(_) => {
                            self.range_oop = trimmed;
                            self.error = None;
                        }
                        Err(e) => {
                            self.error = Some(format!("Invalid OOP range: {}", e));
                        }
                    }
                }
            }
            Field::RangeIP => {
                let trimmed = self.edit_buffer.trim().to_string();
                if trimmed.is_empty() {
                    self.range_ip.clear();
                } else {
                    match crate::solver::range::HandRange::from_str(&trimmed) {
                        Ok(_) => {
                            self.range_ip = trimmed;
                            self.error = None;
                        }
                        Err(e) => {
                            self.error = Some(format!("Invalid IP range: {}", e));
                        }
                    }
                }
            }
            Field::Iterations => {
                if let Ok(val) = self.edit_buffer.parse::<u32>() {
                    self.iterations = val.clamp(10, 1_000_000);
                }
            }
            Field::StartingPot => {
                if let Ok(val) = self.edit_buffer.parse::<f32>() {
                    self.starting_pot = val.clamp(1.0, 100_000.0);
                }
            }
            Field::EffectiveStack => {
                if let Ok(val) = self.edit_buffer.parse::<f32>() {
                    self.effective_stack = val.clamp(1.0, 100_000.0);
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
        let required_cards = self.street.board_cards();
        if !self.board_input.all_complete() {
            self.error = Some(format!(
                "Enter all {} board cards for {} solve",
                required_cards,
                self.street.name()
            ));
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
            street: self.street,
            range_oop: self.range_oop.clone(),
            range_ip: self.range_ip.clone(),
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
                Constraint::Length(2), // Street
                Constraint::Length(4), // Board (card input)
                Constraint::Length(3), // OOP Range
                Constraint::Length(3), // IP Range
                Constraint::Length(2), // Algorithm
                Constraint::Length(2), // Iterations
                Constraint::Length(2), // Starting pot
                Constraint::Length(2), // Effective stack
                Constraint::Length(2), // Bet sizes
                Constraint::Length(2), // Raise sizes
                Constraint::Length(2), // Max raises
                Constraint::Length(2), // Error
                Constraint::Fill(1),   // spacer
                Constraint::Length(1), // Help
            ])
            .split(inner);

        // Render each field
        for (i, field) in Field::ALL.iter().enumerate() {
            if i < chunks.len() {
                match *field {
                    Field::Board => self.render_board_field(frame, chunks[i]),
                    Field::RangeOOP | Field::RangeIP => {
                        self.render_range_field(frame, chunks[i], *field, i);
                    }
                    _ => self.render_field(frame, chunks[i], *field, i),
                }
            }
        }

        // Error (chunk index = Field::ALL.len() = 11)
        if let Some(ref err) = self.error {
            let err_para =
                Paragraph::new(Line::from(Span::styled(format!("  {err}"), Theme::lose())));
            frame.render_widget(err_para, chunks[11]);
        }

        // Help (last chunk)
        self.render_help(frame, chunks[13]);
    }

    fn render_board_field(&self, frame: &mut Frame, area: Rect) {
        let field_idx = Field::ALL.iter().position(|f| *f == Field::Board).unwrap();
        let is_active = self.active_field == field_idx;
        let border_style = if is_active {
            Theme::border_focused()
        } else {
            Theme::border()
        };
        let title_text = format!(
            " Board Cards ({} for {}) ",
            self.street.board_cards(),
            self.street.name()
        );
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(
                title_text,
                if is_active {
                    Theme::title()
                } else {
                    Theme::dim()
                },
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

    fn render_range_field(&self, frame: &mut Frame, area: Rect, field: Field, field_idx: usize) {
        let is_active = self.active_field == field_idx;
        let is_editing = is_active && self.editing;

        let range_str = match field {
            Field::RangeOOP => &self.range_oop,
            Field::RangeIP => &self.range_ip,
            _ => unreachable!(),
        };

        let display_value = if is_editing {
            format!("{}_", self.edit_buffer)
        } else if range_str.is_empty() {
            "(all hands — press Enter to set range)".to_string()
        } else {
            // Show range string and combo count
            match crate::solver::range::HandRange::from_str(range_str) {
                Ok(r) => {
                    let combos = r.weights.iter().filter(|&&w| w > 0.0).count();
                    format!("{range_str}  ({combos} combos)")
                }
                Err(_) => format!("{range_str}  (invalid)"),
            }
        };

        let label_style = if is_active {
            Theme::highlight()
        } else {
            Theme::normal()
        };
        let value_style = if is_editing {
            Theme::accent()
        } else if range_str.is_empty() {
            Theme::dim()
        } else if is_active {
            Theme::win()
        } else {
            Theme::normal()
        };
        let prefix = if is_active { "▶ " } else { "  " };

        let lines = vec![
            Line::from(vec![
                Span::styled(prefix, Theme::highlight()),
                Span::styled(field.label(), label_style.add_modifier(Modifier::BOLD)),
                Span::styled(": ", Theme::dim()),
                Span::styled(display_value, value_style),
            ]),
            Line::from(Span::styled(format!("   {}", field.hint()), Theme::dim())),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_field(&self, frame: &mut Frame, area: Rect, field: Field, field_idx: usize) {
        let is_active = self.active_field == field_idx;
        let is_editing = is_active && self.editing;

        let value_str = if is_editing {
            format!("{}_", self.edit_buffer)
        } else {
            match field {
                Field::Street => format!("{}  (Enter to toggle)", self.street.name()),
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
                _ => String::new(),
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
