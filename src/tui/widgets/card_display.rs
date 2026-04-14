//! Widget for rendering cards visually.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Modifier,
    text::Span,
};
use crate::cards::Card;
use crate::tui::theme::Theme;

/// Render a row of cards as ASCII art boxes.
pub fn render_cards(cards: &[Card], empty_slots: usize, buf: &mut Buffer, area: Rect) {
    let card_width = 5u16;
    let card_height = 3u16;
    let mut x = area.x;

    // Render known cards
    for card in cards {
        if x + card_width > area.x + area.width { break; }
        render_card(Some(*card), buf, Rect { x, y: area.y, width: card_width, height: card_height.min(area.height) });
        x += card_width + 1;
    }

    // Render empty slots
    for _ in 0..empty_slots {
        if x + card_width > area.x + area.width { break; }
        render_card(None, buf, Rect { x, y: area.y, width: card_width, height: card_height.min(area.height) });
        x += card_width + 1;
    }
}

fn render_card(card: Option<Card>, buf: &mut Buffer, area: Rect) {
    if area.height < 3 || area.width < 5 { return; }

    let (rank_str, suit_char, style) = match card {
        Some(c) => {
            let r = c.rank.to_char().to_string();
            let s = c.suit.glyph();
            let style = Theme::suit_style(c.suit);
            (r, s, style)
        }
        None => ("?".to_string(), ' ', Theme::dim()),
    };

    // Border
    let border_style = if card.is_some() { Theme::border() } else { Theme::dim() };

    // Top row: ┌───┐
    buf[(area.x, area.y)].set_char('┌').set_style(border_style);
    for i in 1..4 { buf[(area.x + i, area.y)].set_char('─').set_style(border_style); }
    buf[(area.x + 4, area.y)].set_char('┐').set_style(border_style);

    // Middle row: │R s│
    buf[(area.x, area.y + 1)].set_char('│').set_style(border_style);
    buf[(area.x + 1, area.y + 1)].set_char(rank_str.chars().next().unwrap_or(' ')).set_style(style);
    buf[(area.x + 2, area.y + 1)].set_char(' ').set_style(style);
    buf[(area.x + 3, area.y + 1)].set_char(suit_char).set_style(style);
    buf[(area.x + 4, area.y + 1)].set_char('│').set_style(border_style);

    // Bottom row: └───┘
    if area.height >= 3 {
        buf[(area.x, area.y + 2)].set_char('└').set_style(border_style);
        for i in 1..4 { buf[(area.x + i, area.y + 2)].set_char('─').set_style(border_style); }
        buf[(area.x + 4, area.y + 2)].set_char('┘').set_style(border_style);
    }
}

/// Render a single card as compact inline span text like "A♠"
pub fn card_span(card: Card) -> Span<'static> {
    let text = format!("{}{}", card.rank.to_char(), card.suit.glyph());
    Span::styled(text, Theme::suit_style(card.suit).add_modifier(Modifier::BOLD))
}

/// Render a placeholder for an unknown card
pub fn empty_card_span() -> Span<'static> {
    Span::styled("??", Theme::dim())
}
