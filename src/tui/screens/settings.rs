use crate::sim::SimConfig;
use crate::tui::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub enum SettingsAction {
    None,
    Close,
    Save(SimConfig),
}

#[derive(Clone, Copy, Debug)]
enum Field {
    Iterations,
    ExactThreshold,
    Threads,
}

impl Field {
    const ALL: [Field; 3] = [Field::Iterations, Field::ExactThreshold, Field::Threads];

    fn label(self) -> &'static str {
        match self {
            Field::Iterations => "Monte Carlo Iterations",
            Field::ExactThreshold => "Exact Enumeration Threshold",
            Field::Threads => "Threads (0 = auto)",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Field::Iterations => "Number of random simulations (higher = more accurate)",
            Field::ExactThreshold => "Switch to exact when combinations ≤ this value",
            Field::Threads => "CPU threads for simulation (0 = use all available)",
        }
    }
}

pub struct SettingsScreen {
    pub config: SimConfig,
    active_field: usize,
    edit_buffer: String,
    editing: bool,
}

impl SettingsScreen {
    pub fn new(config: SimConfig) -> Self {
        SettingsScreen {
            config,
            active_field: 0,
            edit_buffer: String::new(),
            editing: false,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SettingsAction {
        if self.editing {
            return self.handle_edit_key(key);
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return SettingsAction::Close,
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
            KeyCode::Enter | KeyCode::Char('e') => {
                self.start_edit();
            }
            KeyCode::Char('s') => {
                return SettingsAction::Save(self.config.clone());
            }
            _ => {}
        }
        SettingsAction::None
    }

    fn start_edit(&mut self) {
        self.editing = true;
        self.edit_buffer = match Field::ALL[self.active_field] {
            Field::Iterations => self.config.iterations.to_string(),
            Field::ExactThreshold => self.config.exact_threshold.to_string(),
            Field::Threads => self.config.threads.to_string(),
        };
    }

    fn handle_edit_key(&mut self, key: KeyEvent) -> SettingsAction {
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
            KeyCode::Char(c) if c.is_ascii_digit() => {
                self.edit_buffer.push(c);
            }
            _ => {}
        }
        SettingsAction::None
    }

    fn apply_edit(&mut self) {
        if let Ok(val) = self.edit_buffer.parse::<u64>() {
            match Field::ALL[self.active_field] {
                Field::Iterations => self.config.iterations = val.clamp(1000, 10_000_000),
                Field::ExactThreshold => self.config.exact_threshold = val.clamp(10, 1_000_000),
                Field::Threads => self.config.threads = (val as usize).min(64),
            }
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border_focused())
            .title(Span::styled(" Settings ", Theme::title()));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                std::iter::repeat_n(Constraint::Length(4), Field::ALL.len())
                    .chain([Constraint::Fill(1), Constraint::Length(1)])
                    .collect::<Vec<_>>(),
            )
            .split(inner);

        for (i, field) in Field::ALL.iter().enumerate() {
            let is_active = i == self.active_field;
            let is_editing = is_active && self.editing;

            let value_str = if is_editing {
                format!("{}_", self.edit_buffer)
            } else {
                match field {
                    Field::Iterations => self.config.iterations.to_string(),
                    Field::ExactThreshold => self.config.exact_threshold.to_string(),
                    Field::Threads => {
                        let t = self.config.threads;
                        if t == 0 {
                            format!(
                                "auto ({})",
                                std::thread::available_parallelism()
                                    .map(|n| n.get())
                                    .unwrap_or(4)
                            )
                        } else {
                            t.to_string()
                        }
                    }
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
            let border_style = if is_active {
                Theme::border_focused()
            } else {
                Theme::border()
            };
            let prefix = if is_active { "▶ " } else { "  " };

            let field_block = Block::default()
                .borders(Borders::ALL)
                .border_style(border_style);
            let field_inner = field_block.inner(chunks[i]);
            frame.render_widget(field_block, chunks[i]);

            let lines = vec![
                Line::from(vec![
                    Span::styled(prefix, Theme::highlight()),
                    Span::styled(field.label(), label_style.add_modifier(Modifier::BOLD)),
                    Span::styled(": ", Theme::dim()),
                    Span::styled(value_str, value_style.add_modifier(Modifier::BOLD)),
                ]),
                Line::from(Span::styled(format!("   {}", field.hint()), Theme::dim())),
            ];
            frame.render_widget(Paragraph::new(lines), field_inner);
        }

        // Help
        let help_idx = Field::ALL.len() + 1;
        if help_idx < chunks.len() {
            let help = Paragraph::new(Line::from(vec![
                Span::styled("↑↓", Theme::highlight()),
                Span::styled(" Navigate  ", Theme::dim()),
                Span::styled("Enter", Theme::highlight()),
                Span::styled(" Edit  ", Theme::dim()),
                Span::styled("s", Theme::highlight()),
                Span::styled(" Save  ", Theme::dim()),
                Span::styled("Esc", Theme::highlight()),
                Span::styled(" Close", Theme::dim()),
            ]));
            frame.render_widget(help, chunks[help_idx]);
        }
    }
}
