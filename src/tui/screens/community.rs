use crate::cards::Card;
use crate::game::{BettingRound, GameState, GameVariant};
use crate::tui::theme::Theme;
use crate::tui::widgets::card_input::InputState;
use crate::tui::widgets::{card_span, empty_card_span, MultiCardInput};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub struct CommunityScreen {
    pub round: BettingRound,
    pub variant: GameVariant,
    pub existing_community: Vec<Card>,
    pub input: MultiCardInput,
    pub all_known: Vec<Card>,
}

impl CommunityScreen {
    pub fn new(state: &GameState) -> Self {
        let round = state.current_round;
        let cards_this_round = round.community_cards_revealed();
        let mut all_known = state.hole_cards.clone();
        all_known.extend_from_slice(&state.community_cards);

        CommunityScreen {
            round,
            variant: state.variant,
            existing_community: state.community_cards.clone(),
            input: MultiCardInput::new(cards_this_round, round.name()),
            all_known,
        }
    }

    /// Returns the new community cards for this round when confirmed.
    /// Returns empty vec if skipping (no cards this round).
    pub fn handle_key(&mut self, key: KeyEvent) -> CommunityAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('b') => return CommunityAction::Back,
            KeyCode::Enter if self.input.all_complete() => {
                return CommunityAction::Cards(self.input.cards());
            }
            KeyCode::Enter if self.input.slots.is_empty() => {
                return CommunityAction::Cards(vec![]);
            }
            _ => {
                let known = self.all_known.clone();
                self.input.handle_key(key, &known);
            }
        }
        CommunityAction::None
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(5),
                Constraint::Length(5),
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Fill(1),
            ])
            .split(area);

        // Title
        let title = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("  {} — {}", self.variant.name(), self.round.name()),
                Theme::title().add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!(
                    "  Enter {} community card{}",
                    self.input.slots.len(),
                    if self.input.slots.len() == 1 { "" } else { "s" }
                ),
                Theme::dim(),
            )),
        ]);
        frame.render_widget(title, chunks[0]);

        // Full board (existing + new)
        let board_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(Span::styled(" Board ", Theme::title()));
        let inner = board_block.inner(chunks[1]);
        frame.render_widget(board_block, chunks[1]);

        let total_community = self.variant.community_card_count();
        let mut board_spans: Vec<Span> = vec![Span::raw("  ")];
        for (i, &c) in self.existing_community.iter().enumerate() {
            if i > 0 {
                board_spans.push(Span::raw(" "));
            }
            board_spans.push(card_span(c));
        }
        // New cards being entered
        for (_i, slot) in self.input.slots.iter().enumerate() {
            board_spans.push(Span::raw(" "));
            match slot.card() {
                Some(c) => board_spans.push(card_span(c)),
                None => {
                    if let InputState::AwaitingSuit(r) = slot.state {
                        board_spans.push(Span::styled(
                            format!("{}_", r.to_char()),
                            Theme::highlight(),
                        ));
                    } else {
                        board_spans.push(Span::styled("??", Theme::accent()));
                    }
                }
            }
        }
        // Remaining empty slots
        let shown = self.existing_community.len() + self.input.slots.len();
        for _ in shown..total_community {
            board_spans.push(Span::raw(" "));
            board_spans.push(empty_card_span());
        }
        let board_line = Paragraph::new(Line::from(board_spans)).alignment(Alignment::Center);
        frame.render_widget(board_line, inner);

        // Your hole cards
        let hole_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(Span::styled(" Your Hand ", Theme::title()));
        let hole_inner = hole_block.inner(chunks[2]);
        frame.render_widget(hole_block, chunks[2]);

        let hole_spans: Vec<Span> = self
            .all_known
            .iter()
            .take(self.variant.hole_card_count())
            .enumerate()
            .flat_map(|(i, &c)| {
                let mut v = if i > 0 { vec![Span::raw("  ")] } else { vec![] };
                v.push(card_span(c));
                v
            })
            .collect();
        let hole_line = Paragraph::new(Line::from(hole_spans)).alignment(Alignment::Center);
        frame.render_widget(hole_line, hole_inner);

        // Prompt
        let prompt = if self.input.slots.is_empty() {
            Paragraph::new(Line::from(Span::styled(
                "  Press Enter to continue",
                Theme::dim(),
            )))
        } else if self.input.all_complete() {
            Paragraph::new(Line::from(Span::styled(
                "  All cards entered. Press Enter to calculate odds.",
                Theme::dim(),
            )))
        } else {
            let active = &self.input.slots[self.input.active_slot];
            let hint = match &active.state {
                InputState::AwaitingRank => "  Enter rank: 2-9, T, J, Q, K, A".to_string(),
                InputState::AwaitingSuit(r) => {
                    format!("  Got {}. Enter suit: c, d, h, s", r.to_char())
                }
                InputState::Confirmed(_) => "  Backspace to change, Tab for next".to_string(),
            };
            Paragraph::new(Line::from(Span::styled(hint, Theme::highlight())))
        };
        frame.render_widget(prompt, chunks[3]);

        // Error
        if let Some(err) = self.input.error() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("  ⚠ {}", err),
                    Theme::lose(),
                ))),
                chunks[4],
            );
        }

        // Help
        let help = Paragraph::new(Line::from(vec![
            Span::styled("Enter", Theme::highlight()),
            Span::styled(" Continue  ", Theme::dim()),
            Span::styled("Esc", Theme::highlight()),
            Span::styled(" Back  ", Theme::dim()),
            Span::styled("Tab", Theme::highlight()),
            Span::styled(" Next slot", Theme::dim()),
        ]));
        frame.render_widget(help, chunks[5]);
    }
}

pub enum CommunityAction {
    None,
    Cards(Vec<Card>),
    Back,
}

// Extension for Theme to add accent
impl Theme {
    pub fn accent() -> ratatui::style::Style {
        ratatui::style::Style::default()
            .fg(Self::ACCENT)
            .add_modifier(ratatui::style::Modifier::BOLD)
    }
}
