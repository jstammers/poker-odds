use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use crate::game::GameVariant;
use crate::tui::theme::Theme;

pub struct VariantSelectScreen {
    pub selected: usize,
    pub list_state: ListState,
}

impl VariantSelectScreen {
    pub fn new() -> Self {
        let mut s = VariantSelectScreen { selected: 0, list_state: ListState::default() };
        s.list_state.select(Some(0));
        s
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<GameVariant> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                    self.list_state.select(Some(self.selected));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected < GameVariant::ALL.len() - 1 {
                    self.selected += 1;
                    self.list_state.select(Some(self.selected));
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                return Some(GameVariant::ALL[self.selected]);
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
                Constraint::Length(18),
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
        let title_area = Rect { height: 3, ..centered };
        let content_area = Rect { y: centered.y + 3, height: centered.height.saturating_sub(3), ..centered };

        let title = Paragraph::new(vec![
            Line::from(vec![Span::styled(
                "♠ ♥ POKER ODDS CALCULATOR ♦ ♣",
                Theme::title().add_modifier(Modifier::BOLD),
            )]),
            Line::from(Span::styled("Select a Game Variant", Theme::dim())),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(title, title_area);

        // Variant list
        let items: Vec<ListItem> = GameVariant::ALL.iter().map(|v| {
            let main = Line::from(vec![
                Span::styled(format!("  {}  ", v.name()), Theme::highlight()),
            ]);
            let desc = Line::from(vec![
                Span::styled(format!("  {}", v.description()), Theme::dim()),
            ]);
            ListItem::new(vec![main, desc, Line::from("")])
        }).collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Theme::border_focused())
                    .title(Span::styled(" Choose Variant ", Theme::title())),
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
                Span::styled("q", Theme::highlight()),
                Span::styled(" Quit", Theme::dim()),
            ]))
            .alignment(Alignment::Center);
            frame.render_widget(help, Rect { y: help_y, height: 1, ..area });
        }
    }
}
