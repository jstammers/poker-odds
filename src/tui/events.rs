use anyhow::Result;
use crossterm::event::{self, Event as CEvent, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum AppEvent {
    Key(KeyEvent),
    Tick,
    Resize(u16, u16),
}

pub fn poll_event(tick_ms: u64) -> Result<Option<AppEvent>> {
    if event::poll(Duration::from_millis(tick_ms))? {
        match event::read()? {
            CEvent::Key(key) => Ok(Some(AppEvent::Key(key))),
            CEvent::Resize(w, h) => Ok(Some(AppEvent::Resize(w, h))),
            _ => Ok(None),
        }
    } else {
        Ok(Some(AppEvent::Tick))
    }
}

/// Check if a key event represents quitting
pub fn is_quit(key: &KeyEvent) -> bool {
    matches!(
        (key.code, key.modifiers),
        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL)
    )
}
