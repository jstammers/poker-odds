use ratatui::style::{Color, Modifier, Style};

pub struct Theme;

impl Theme {
    // Background / surface
    pub const BG: Color = Color::Rgb(18, 20, 28);
    pub const SURFACE: Color = Color::Rgb(30, 34, 48);
    pub const BORDER: Color = Color::Rgb(60, 70, 100);

    // Accent
    pub const ACCENT: Color = Color::Rgb(100, 180, 255);
    pub const ACCENT_DIM: Color = Color::Rgb(60, 110, 160);

    // Suits
    pub const HEARTS: Color = Color::Rgb(220, 60, 60);
    pub const DIAMONDS: Color = Color::Rgb(220, 60, 60);
    pub const SPADES: Color = Color::White;
    pub const CLUBS: Color = Color::Rgb(60, 200, 120);

    // Status
    pub const WIN: Color = Color::Rgb(80, 200, 120);
    pub const TIE: Color = Color::Rgb(220, 180, 60);
    pub const LOSE: Color = Color::Rgb(220, 80, 80);

    // Text
    pub const TEXT: Color = Color::Rgb(210, 215, 230);
    pub const TEXT_DIM: Color = Color::Rgb(110, 120, 150);
    pub const TEXT_HIGHLIGHT: Color = Color::White;

    // Styles
    pub fn title() -> Style {
        Style::default().fg(Self::ACCENT).add_modifier(Modifier::BOLD)
    }

    pub fn normal() -> Style {
        Style::default().fg(Self::TEXT)
    }

    pub fn dim() -> Style {
        Style::default().fg(Self::TEXT_DIM)
    }

    pub fn highlight() -> Style {
        Style::default().fg(Self::TEXT_HIGHLIGHT).add_modifier(Modifier::BOLD)
    }

    pub fn selected() -> Style {
        Style::default().fg(Self::BG).bg(Self::ACCENT).add_modifier(Modifier::BOLD)
    }

    pub fn border() -> Style {
        Style::default().fg(Self::BORDER)
    }

    pub fn border_focused() -> Style {
        Style::default().fg(Self::ACCENT)
    }

    pub fn win() -> Style {
        Style::default().fg(Self::WIN).add_modifier(Modifier::BOLD)
    }

    pub fn tie() -> Style {
        Style::default().fg(Self::TIE).add_modifier(Modifier::BOLD)
    }

    pub fn lose() -> Style {
        Style::default().fg(Self::LOSE).add_modifier(Modifier::BOLD)
    }

    pub fn suit_style(suit: crate::cards::Suit) -> Style {
        let color = match suit {
            crate::cards::Suit::Hearts => Self::HEARTS,
            crate::cards::Suit::Diamonds => Self::DIAMONDS,
            crate::cards::Suit::Clubs => Self::CLUBS,
            crate::cards::Suit::Spades => Self::SPADES,
        };
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }
}
