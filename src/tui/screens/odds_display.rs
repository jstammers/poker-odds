use crate::eval::HandCategory;
use crate::game::{GameState, GameVariant};
use crate::sim::result::OddsResult;
use crate::tui::theme::Theme;
use crate::tui::widgets::card_span;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};

pub enum OddsAction {
    None,
    NextRound,
    Restart,
    Back,
    Settings,
}

pub struct OddsDisplayScreen {
    pub variant: GameVariant,
}

impl OddsDisplayScreen {
    pub fn new(variant: GameVariant) -> Self {
        OddsDisplayScreen { variant }
    }

    pub fn handle_key(&self, key: KeyEvent, state: &GameState) -> OddsAction {
        match key.code {
            KeyCode::Enter | KeyCode::Char('n') if !state.is_complete() => OddsAction::NextRound,
            KeyCode::Char('r') => OddsAction::Restart,
            KeyCode::Esc | KeyCode::Char('b') => OddsAction::Back,
            KeyCode::Char('s') => OddsAction::Settings,
            _ => OddsAction::None,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, state: &GameState, result: &OddsResult) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // title
                Constraint::Length(5),  // cards
                Constraint::Length(5),  // odds bars
                Constraint::Length(14), // hand rank table
                Constraint::Length(2),  // sim info
                Constraint::Fill(1),    // help
            ])
            .split(area);

        self.render_title(frame, chunks[0], state);
        self.render_cards(frame, chunks[1], state);
        self.render_odds_bars(frame, chunks[2], result);
        self.render_hand_table(frame, chunks[3], result);
        self.render_sim_info(frame, chunks[4], result);
        self.render_help(frame, chunks[5], state);
    }

    fn render_title(&self, frame: &mut Frame, area: Rect, state: &GameState) {
        let round_name = state.current_round.name();
        let title = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("  {} — {}", self.variant.name(), round_name),
                Theme::title().add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!(
                    "  {} opponent{}",
                    state.opponent_count,
                    if state.opponent_count == 1 { "" } else { "s" }
                ),
                Theme::dim(),
            )),
        ]);
        frame.render_widget(title, area);
    }

    fn render_cards(&self, frame: &mut Frame, area: Rect, state: &GameState) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        // Hole cards
        let hole_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(Span::styled(" Your Hand ", Theme::title()));
        let inner = hole_block.inner(chunks[0]);
        frame.render_widget(hole_block, chunks[0]);

        let hole_spans: Vec<Span> = state
            .hole_cards
            .iter()
            .enumerate()
            .flat_map(|(i, &c)| {
                let mut v = if i > 0 { vec![Span::raw("  ")] } else { vec![] };
                v.push(card_span(c));
                v
            })
            .collect();
        frame.render_widget(
            Paragraph::new(Line::from(hole_spans)).alignment(Alignment::Center),
            inner,
        );

        // Community cards
        if self.variant.has_community_cards() {
            let comm_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Theme::border())
                .title(Span::styled(" Board ", Theme::title()));
            let inner = comm_block.inner(chunks[1]);
            frame.render_widget(comm_block, chunks[1]);

            let total = self.variant.community_card_count();
            let mut comm_spans: Vec<Span> = Vec::new();
            for (i, &c) in state.community_cards.iter().enumerate() {
                if i > 0 {
                    comm_spans.push(Span::raw(" "));
                }
                comm_spans.push(card_span(c));
            }
            for _ in state.community_cards.len()..total {
                comm_spans.push(Span::raw(" "));
                comm_spans.push(Span::styled("??", Theme::dim()));
            }
            frame.render_widget(
                Paragraph::new(Line::from(comm_spans)).alignment(Alignment::Center),
                inner,
            );
        }
    }

    fn render_odds_bars(&self, frame: &mut Frame, area: Rect, result: &OddsResult) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(Span::styled(" Win Probability ", Theme::title()));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if !result.is_ready() {
            let loading =
                Paragraph::new(Line::from(Span::styled("  Calculating...", Theme::dim())))
                    .alignment(Alignment::Center);
            frame.render_widget(loading, inner);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

        let win_pct = result.win_pct();
        let tie_pct = result.tie_pct();
        let lose_pct = result.lose_pct();

        // Win bar
        let win_gauge = Gauge::default()
            .gauge_style(Style::default().fg(Theme::WIN).bg(Color::Rgb(20, 40, 25)))
            .label(format!("WIN  {:5.1}%", win_pct))
            .ratio((win_pct / 100.0).clamp(0.0, 1.0));
        frame.render_widget(win_gauge, chunks[0]);

        // Tie bar
        let tie_gauge = Gauge::default()
            .gauge_style(Style::default().fg(Theme::TIE).bg(Color::Rgb(40, 35, 10)))
            .label(format!("TIE  {:5.1}%", tie_pct))
            .ratio((tie_pct / 100.0).clamp(0.0, 1.0));
        frame.render_widget(tie_gauge, chunks[1]);

        // Lose bar
        let lose_gauge = Gauge::default()
            .gauge_style(Style::default().fg(Theme::LOSE).bg(Color::Rgb(40, 15, 15)))
            .label(format!("LOSE {:5.1}%", lose_pct))
            .ratio((lose_pct / 100.0).clamp(0.0, 1.0));
        frame.render_widget(lose_gauge, chunks[2]);
    }

    fn render_hand_table(&self, frame: &mut Frame, area: Rect, result: &OddsResult) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(Span::styled(" Hand Distribution ", Theme::title()));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if !result.is_ready() {
            return;
        }

        // Two columns of 5 hand categories
        let col_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(inner);

        let categories = [
            HandCategory::RoyalFlush,
            HandCategory::StraightFlush,
            HandCategory::FourOfAKind,
            HandCategory::FullHouse,
            HandCategory::Flush,
            HandCategory::Straight,
            HandCategory::ThreeOfAKind,
            HandCategory::TwoPair,
            HandCategory::OnePair,
            HandCategory::HighCard,
        ];

        for (col, chunk) in col_chunks.iter().enumerate() {
            let mut lines: Vec<Line> = Vec::new();
            for i in 0..5 {
                let cat = categories[col * 5 + i];
                let pct = result.hand_pct(cat);
                let bar_width = (pct / 100.0 * 8.0) as usize;
                let bar: String = "█".repeat(bar_width) + &"░".repeat(8 - bar_width);
                let (name_style, pct_style) = if pct > 0.1 {
                    (Theme::normal(), Theme::highlight())
                } else {
                    (Theme::dim(), Theme::dim())
                };
                lines.push(Line::from(vec![
                    Span::styled(format!(" {:<18}", cat.name()), name_style),
                    Span::styled(bar, Style::default().fg(Theme::ACCENT_DIM)),
                    Span::styled(format!(" {:5.1}%", pct), pct_style),
                ]));
            }
            frame.render_widget(Paragraph::new(lines), *chunk);
        }
    }

    fn render_sim_info(&self, frame: &mut Frame, area: Rect, result: &OddsResult) {
        let info = if result.is_ready() {
            Paragraph::new(Line::from(vec![
                Span::styled("  Method: ", Theme::dim()),
                Span::styled(result.method.to_string(), Theme::highlight()),
                Span::styled("  |  Simulations: ", Theme::dim()),
                Span::styled(format!("{}", result.simulations_run), Theme::highlight()),
            ]))
        } else {
            Paragraph::new(Line::from(Span::styled(
                "  Running simulation...",
                Theme::dim(),
            )))
        };
        frame.render_widget(info, area);
    }

    fn render_help(&self, frame: &mut Frame, area: Rect, state: &GameState) {
        let mut spans = vec![];

        if !state.is_complete() {
            spans.extend_from_slice(&[
                Span::styled("n / Enter", Theme::highlight()),
                Span::styled(" Next round  ", Theme::dim()),
            ]);
        }

        spans.extend_from_slice(&[
            Span::styled("r", Theme::highlight()),
            Span::styled(" New hand  ", Theme::dim()),
            Span::styled("s", Theme::highlight()),
            Span::styled(" Settings  ", Theme::dim()),
            Span::styled("q", Theme::highlight()),
            Span::styled(" Quit", Theme::dim()),
        ]);

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}
