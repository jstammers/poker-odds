use crate::game::GameVariant;
use crate::tui::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

pub enum VariantSelectResult {
    Variant(GameVariant),
    GtoSolver,
}

pub struct VariantSelectScreen {
    pub selected: usize,
    pub list_state: ListState,
}

/// Total items: 4 game variants + 1 GTO Solver separator
const TOTAL_ITEMS: usize = 5; // 4 variants + GTO Solver

impl Default for VariantSelectScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl VariantSelectScreen {
    pub fn new() -> Self {
        let mut s = VariantSelectScreen {
            selected: 0,
            list_state: ListState::default(),
        };
        s.list_state.select(Some(0));
        s
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<VariantSelectResult> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                    self.list_state.select(Some(self.selected));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected < TOTAL_ITEMS - 1 {
                    self.selected += 1;
                    self.list_state.select(Some(self.selected));
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if self.selected < GameVariant::ALL.len() {
                    return Some(VariantSelectResult::Variant(
                        GameVariant::ALL[self.selected],
                    ));
                } else {
                    return Some(VariantSelectResult::GtoSolver);
                }
            }
            KeyCode::Char('g') => {
                return Some(VariantSelectResult::GtoSolver);
            }
            _ => {}
        }
        None
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        // Center the UI vertically
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(22),
                Constraint::Fill(1),
            ])
            .split(area);

        let centered = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(58),
                Constraint::Fill(1),
            ])
            .split(outer[1])[1];

        // Title
        let title_area = Rect {
            height: 3,
            ..centered
        };
        let content_area = Rect {
            y: centered.y + 3,
            height: centered.height.saturating_sub(3),
            ..centered
        };

        let title = Paragraph::new(vec![
            Line::from(vec![Span::styled(
                "♠ ♥ POKER ODDS CALCULATOR ♦ ♣",
                Theme::title().add_modifier(Modifier::BOLD),
            )]),
            Line::from(Span::styled("Select a Mode", Theme::dim())),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(title, title_area);

        // Build list items: variants + separator + GTO Solver
        let mut items: Vec<ListItem> = GameVariant::ALL
            .iter()
            .map(|v| {
                let main = Line::from(vec![Span::styled(
                    format!("  {}  ", v.name()),
                    Theme::highlight(),
                )]);
                let desc = Line::from(vec![Span::styled(
                    format!("  {}", v.description()),
                    Theme::dim(),
                )]);
                ListItem::new(vec![main, desc, Line::from("")])
            })
            .collect();

        // GTO Solver option
        items.push(ListItem::new(vec![
            Line::from(vec![Span::styled(
                "  GTO Solver  ",
                Style::default()
                    .fg(Theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![Span::styled(
                "  Compute optimal heads-up strategies (CFR)",
                Theme::dim(),
            )]),
            Line::from(""),
        ]));

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Theme::border_focused())
                    .title(Span::styled(" Choose Mode ", Theme::title())),
            )
            .highlight_style(
                Style::default()
                    .bg(ratatui::style::Color::Rgb(30, 50, 80))
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, content_area, &mut self.list_state);

        // Help text
        let help_y = content_area.y + content_area.height;
        if help_y < area.y + area.height {
            let help = Paragraph::new(Line::from(vec![
                Span::styled("↑↓", Theme::highlight()),
                Span::styled(" Navigate  ", Theme::dim()),
                Span::styled("Enter", Theme::highlight()),
                Span::styled(" Select  ", Theme::dim()),
                Span::styled("g", Theme::highlight()),
                Span::styled(" GTO Solver  ", Theme::dim()),
                Span::styled("q", Theme::highlight()),
                Span::styled(" Quit", Theme::dim()),
            ]))
            .alignment(Alignment::Center);
            frame.render_widget(
                help,
                Rect {
                    y: help_y,
                    height: 1,
                    ..area
                },
            );
        }
    }
}
