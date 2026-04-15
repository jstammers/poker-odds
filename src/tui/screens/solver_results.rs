use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};

use crate::solver::cfr::CfrAlgorithm;
use crate::tui::theme::Theme;
use crate::tui::widgets::card_span;

pub enum SolverResultsAction {
    None,
    Back,
    Reconfigure,
}

/// Snapshot of solver progress, shared between solver thread and UI.
#[derive(Clone, Debug, Default)]
pub struct SolverProgress {
    /// Current iteration number.
    pub iteration: u32,
    /// Total iterations requested.
    pub total_iterations: u32,
    /// Game value estimate (for player 0).
    pub game_value: f64,
    /// Whether solving is complete.
    pub done: bool,
    /// Exploitability in mbb/hand (computed at end).
    pub exploitability: Option<f64>,
    /// Strategy at key info sets for display.
    /// Each entry: (info_set_label, Vec<(action_name, probability)>)
    pub strategies: Vec<(String, Vec<(String, f32)>)>,
    /// Number of info sets in the tree.
    pub num_info_sets: u32,
    /// Number of nodes in the tree.
    pub num_nodes: u32,
    /// Algorithm used.
    pub algorithm: Option<CfrAlgorithm>,
    /// Board cards for display.
    pub board: Vec<crate::cards::Card>,
    /// Pot size.
    pub pot: f32,
    /// Stack size.
    pub stack: f32,
    /// Street name (Flop/Turn/River).
    pub street_name: String,
    /// OOP range string for display.
    pub range_oop: String,
    /// IP range string for display.
    pub range_ip: String,
}

pub struct SolverResultsScreen {
    scroll_offset: usize,
}

impl SolverResultsScreen {
    pub fn new() -> Self {
        SolverResultsScreen { scroll_offset: 0 }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SolverResultsAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('b') => SolverResultsAction::Back,
            KeyCode::Char('r') => SolverResultsAction::Reconfigure,
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                SolverResultsAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_offset += 1;
                SolverResultsAction::None
            }
            _ => SolverResultsAction::None,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, progress: &SolverProgress) {
        let has_ranges = !progress.range_oop.is_empty() || !progress.range_ip.is_empty();
        let title_height = if has_ranges { 5 } else { 3 };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(title_height), // title + ranges
                Constraint::Length(3),            // progress bar
                Constraint::Length(5),            // stats
                Constraint::Fill(1),             // strategy table
                Constraint::Length(1),            // help
            ])
            .split(area);

        self.render_title(frame, chunks[0], progress);
        self.render_progress(frame, chunks[1], progress);
        self.render_stats(frame, chunks[2], progress);
        self.render_strategies(frame, chunks[3], progress);
        self.render_help(frame, chunks[4], progress);
    }

    fn render_title(&self, frame: &mut Frame, area: Rect, progress: &SolverProgress) {
        let algo_name = match progress.algorithm {
            Some(CfrAlgorithm::CfrPlus) => "CFR+",
            Some(CfrAlgorithm::Dcfr) => "DCFR",
            None => "CFR",
        };

        let street_name = if progress.street_name.is_empty() {
            "River"
        } else {
            &progress.street_name
        };

        let board_spans: Vec<Span> = progress
            .board
            .iter()
            .enumerate()
            .flat_map(|(i, &c)| {
                let mut v = if i > 0 {
                    vec![Span::raw(" ")]
                } else {
                    vec![]
                };
                v.push(card_span(c));
                v
            })
            .collect();

        let mut title_spans = vec![
            Span::styled(
                format!("  GTO Solver — {algo_name} ({street_name})  "),
                Theme::title().add_modifier(Modifier::BOLD),
            ),
            Span::styled("Board: ", Theme::dim()),
        ];
        title_spans.extend(board_spans);

        let mut lines = vec![
            Line::from(title_spans),
            Line::from(Span::styled(
                format!(
                    "  Pot: {:.0}  Stack: {:.0}",
                    progress.pot, progress.stack
                ),
                Theme::dim(),
            )),
        ];

        // Show ranges if specified
        if !progress.range_oop.is_empty() || !progress.range_ip.is_empty() {
            let oop_display = if progress.range_oop.is_empty() {
                "all hands"
            } else {
                &progress.range_oop
            };
            let ip_display = if progress.range_ip.is_empty() {
                "all hands"
            } else {
                &progress.range_ip
            };
            lines.push(Line::from(vec![
                Span::styled("  OOP: ", Theme::dim()),
                Span::styled(oop_display.to_string(), Theme::normal()),
                Span::styled("  |  IP: ", Theme::dim()),
                Span::styled(ip_display.to_string(), Theme::normal()),
            ]));
        }

        let title = Paragraph::new(lines);
        frame.render_widget(title, area);
    }

    fn render_progress(&self, frame: &mut Frame, area: Rect, progress: &SolverProgress) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(Span::styled(" Progress ", Theme::title()));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let ratio = if progress.total_iterations > 0 {
            progress.iteration as f64 / progress.total_iterations as f64
        } else {
            0.0
        };

        let label = if progress.done {
            format!(
                "Complete  {}/{} iterations",
                progress.iteration, progress.total_iterations
            )
        } else {
            format!(
                "Solving...  {}/{}  ({:.0}%)",
                progress.iteration,
                progress.total_iterations,
                ratio * 100.0
            )
        };

        let color = if progress.done {
            Theme::WIN
        } else {
            Theme::ACCENT
        };

        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(color).bg(Color::Rgb(20, 25, 40)))
            .label(label)
            .ratio(ratio.clamp(0.0, 1.0));
        frame.render_widget(gauge, inner);
    }

    fn render_stats(&self, frame: &mut Frame, area: Rect, progress: &SolverProgress) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(Span::styled(" Statistics ", Theme::title()));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut lines = vec![Line::from(vec![
            Span::styled("  Game Value (P0): ", Theme::dim()),
            Span::styled(
                format!("{:+.4}", progress.game_value),
                if progress.game_value >= 0.0 {
                    Theme::win()
                } else {
                    Theme::lose()
                },
            ),
            Span::styled("  |  Info Sets: ", Theme::dim()),
            Span::styled(format!("{}", progress.num_info_sets), Theme::highlight()),
            Span::styled("  |  Nodes: ", Theme::dim()),
            Span::styled(format!("{}", progress.num_nodes), Theme::highlight()),
        ])];

        if let Some(exp) = progress.exploitability {
            lines.push(Line::from(vec![
                Span::styled("  Exploitability: ", Theme::dim()),
                Span::styled(
                    format!("{:.2} mbb/hand", exp),
                    if exp < 10.0 {
                        Theme::win()
                    } else if exp < 50.0 {
                        Theme::tie()
                    } else {
                        Theme::lose()
                    },
                ),
                Span::styled(
                    if exp < 10.0 {
                        "  (near-optimal)"
                    } else if exp < 50.0 {
                        "  (acceptable)"
                    } else {
                        "  (needs more iterations)"
                    },
                    Theme::dim(),
                ),
            ]));
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_strategies(&self, frame: &mut Frame, area: Rect, progress: &SolverProgress) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(Span::styled(" Strategy (Root Node) ", Theme::title()));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if progress.strategies.is_empty() {
            let msg = if progress.done {
                "No strategies to display"
            } else {
                "Solving... strategies will appear when complete"
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(format!("  {msg}"), Theme::dim()))),
                inner,
            );
            return;
        }

        let mut lines: Vec<Line> = Vec::new();

        for (label, actions) in progress
            .strategies
            .iter()
            .skip(self.scroll_offset)
            .take(inner.height as usize)
        {
            let mut spans = vec![
                Span::styled(format!("  {label:<30}"), Theme::normal()),
            ];

            for (action_name, prob) in actions {
                let bar_width = (*prob * 10.0) as usize;
                let bar: String = "█".repeat(bar_width);
                let pct = *prob * 100.0;

                let color = if pct > 60.0 {
                    Theme::WIN
                } else if pct > 20.0 {
                    Theme::TIE
                } else {
                    Theme::LOSE
                };

                spans.push(Span::styled(format!("{action_name}:"), Theme::dim()));
                spans.push(Span::styled(bar, Style::default().fg(color)));
                spans.push(Span::styled(format!("{pct:5.1}%  "), Theme::highlight()));
            }

            lines.push(Line::from(spans));
        }

        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (scroll with j/k)",
                Theme::dim(),
            )));
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_help(&self, frame: &mut Frame, area: Rect, progress: &SolverProgress) {
        let mut spans = vec![];
        if progress.done {
            spans.extend_from_slice(&[
                Span::styled("r", Theme::highlight()),
                Span::styled(" Reconfigure  ", Theme::dim()),
            ]);
        }
        spans.extend_from_slice(&[
            Span::styled("↑↓", Theme::highlight()),
            Span::styled(" Scroll  ", Theme::dim()),
            Span::styled("Esc", Theme::highlight()),
            Span::styled(" Back  ", Theme::dim()),
            Span::styled("q", Theme::highlight()),
            Span::styled(" Quit", Theme::dim()),
        ]);
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}
