use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use crate::cards::Card;
use crate::game::GameVariant;
use crate::tui::theme::Theme;
use crate::tui::widgets::{card_span, empty_card_span, MultiCardInput};

pub struct HoleCardsScreen {
    pub variant: GameVariant,
    pub input: MultiCardInput,
    pub opponent_count: usize,
    pub editing_opponents: bool,
}

impl HoleCardsScreen {
    pub fn new(variant: GameVariant) -> Self {
        let count = variant.hole_card_count();
        HoleCardsScreen {
            variant,
            input: MultiCardInput::new(count, "Your Hole Cards"),
            opponent_count: 1,
            editing_opponents: false,
        }
    }

    /// Returns (hole_cards, opponent_count) when confirmed
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<(Vec<Card>, usize)> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('b') => {
                // Back — signal with None; caller handles navigation
                return None;
            }
            KeyCode::Enter if self.input.all_complete() => {
                return Some((self.input.cards(), self.opponent_count));
            }
            KeyCode::Tab if !self.editing_opponents => {
                if self.input.all_complete() {
                    self.editing_opponents = true;
                } else {
                    self.input.handle_key(key, &[]);
                }
            }
            KeyCode::Tab if self.editing_opponents => {
                self.editing_opponents = false;
            }
            KeyCode::Up if self.editing_opponents => {
                let max = self.variant.max_players() - 1;
                if self.opponent_count < max { self.opponent_count += 1; }
            }
            KeyCode::Down if self.editing_opponents => {
                if self.opponent_count > 1 { self.opponent_count -= 1; }
            }
            _ => {
                if !self.editing_opponents {
                    let already: Vec<Card> = self.input.cards();
                    self.input.handle_key(key, &already);
                }
            }
        }
        None
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // title
                Constraint::Length(5),  // card display
                Constraint::Length(3),  // input prompt
                Constraint::Length(3),  // opponents
                Constraint::Length(2),  // error
                Constraint::Fill(1),    // help
            ])
            .split(area);

        // Title
        let title = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("  {} — Enter Your Hand", self.variant.name()),
                Theme::title().add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("  Enter {} hole cards (e.g. Ah, Td, 2c)", self.variant.hole_card_count()),
                Theme::dim(),
            )),
        ]);
        frame.render_widget(title, chunks[0]);

        // Card display
        let card_block = Block::default()
            .borders(Borders::ALL)
            .border_style(if !self.editing_opponents { Theme::border_focused() } else { Theme::border() })
            .title(Span::styled(" Your Cards ", Theme::title()));
        let inner = card_block.inner(chunks[1]);
        frame.render_widget(card_block, chunks[1]);

        // Render card slots
        let mut spans: Vec<Span> = Vec::new();
        let _completed = self.input.cards();
        for (i, slot) in self.input.slots.iter().enumerate() {
            if i > 0 { spans.push(Span::raw("  ")); }

            let is_active = i == self.input.active_slot && !self.editing_opponents;
            let prefix = if is_active { "▶ " } else { "  " };
            spans.push(Span::styled(prefix, Theme::highlight()));

            match slot.card() {
                Some(c) => spans.push(card_span(c)),
                None => {
                    if let crate::tui::widgets::card_input::InputState::AwaitingSuit(r) = slot.state {
                        spans.push(Span::styled(format!("{}_", r.to_char()), Theme::highlight()));
                    } else {
                        spans.push(empty_card_span());
                    }
                }
            }
        }
        let card_line = Paragraph::new(Line::from(spans)).alignment(Alignment::Center);
        frame.render_widget(card_line, inner);

        // Input prompt
        let prompt_style = if !self.editing_opponents { Theme::highlight() } else { Theme::dim() };
        let prompt = if self.input.all_complete() {
            Paragraph::new(Line::from(Span::styled("  All cards entered. Press Tab to edit opponents or Enter to continue.", Theme::dim())))
        } else {
            let active = &self.input.slots[self.input.active_slot];
            let hint = match &active.state {
                crate::tui::widgets::card_input::InputState::AwaitingRank =>
                    "  Enter rank: 2-9, T, J, Q, K, A".to_string(),
                crate::tui::widgets::card_input::InputState::AwaitingSuit(r) =>
                    format!("  Got rank {}. Enter suit: c(lubs), d(iamonds), h(earts), s(pades)", r.to_char()),
                crate::tui::widgets::card_input::InputState::Confirmed(_) =>
                    "  Press Backspace to change card, Tab for next slot".to_string(),
            };
            Paragraph::new(Line::from(Span::styled(hint, prompt_style)))
        };
        frame.render_widget(prompt, chunks[2]);

        // Opponents
        let opp_style = if self.editing_opponents { Theme::border_focused() } else { Theme::border() };
        let opp_block = Block::default()
            .borders(Borders::ALL)
            .border_style(opp_style)
            .title(Span::styled(" Opponents ", Theme::title()));
        let opp_inner = opp_block.inner(chunks[3]);
        frame.render_widget(opp_block, chunks[3]);

        let opp_indicator = if self.editing_opponents { "▶ " } else { "  " };
        let opp_text = Paragraph::new(Line::from(vec![
            Span::styled(opp_indicator, Theme::highlight()),
            Span::styled(format!("{}", self.opponent_count), Theme::highlight().add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(" opponent{}", if self.opponent_count == 1 { "" } else { "s" }),
                Theme::normal(),
            ),
            Span::styled(
                format!("  (max {})", self.variant.max_players() - 1),
                Theme::dim(),
            ),
        ]));
        frame.render_widget(opp_text, opp_inner);

        // Error
        if let Some(err) = self.input.error() {
            let error_text = Paragraph::new(Line::from(Span::styled(
                format!("  ⚠ {}", err),
                Theme::lose(),
            )));
            frame.render_widget(error_text, chunks[4]);
        }

        // Help
        let help = Paragraph::new(Line::from(vec![
            Span::styled("Tab", Theme::highlight()),
            Span::styled(" Switch focus  ", Theme::dim()),
            Span::styled("↑↓", Theme::highlight()),
            Span::styled(" Opponents  ", Theme::dim()),
            Span::styled("Enter", Theme::highlight()),
            Span::styled(" Continue  ", Theme::dim()),
            Span::styled("Esc", Theme::highlight()),
            Span::styled(" Back", Theme::dim()),
        ]));
        frame.render_widget(help, chunks[5]);
    }
}
