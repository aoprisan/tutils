//! Event handling for TUI applications

use anyhow::Result;
use crossterm::event::{self, KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use std::time::Duration;
use tokio::sync::mpsc;

/// Application events
#[derive(Debug, Clone)]
pub enum Event {
    /// Terminal tick for animations/polling
    Tick,
    /// Keyboard input
    Key(KeyEvent),
    /// Mouse input
    Mouse(MouseEvent),
    /// Terminal resize
    Resize(u16, u16),
    /// Custom app-specific event
    App(AppEvent),
}

/// Application-specific events (for async operations)
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Data loaded successfully
    DataLoaded(String),
    /// Error occurred
    Error(String),
    /// Status message update
    Status(String),
}

/// Handles terminal events asynchronously
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Event>,
    _tx: mpsc::UnboundedSender<Event>,
}

impl EventHandler {
    /// Create a new event handler with the specified tick rate
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let event_tx = tx.clone();

        // Spawn event polling task
        tokio::spawn(async move {
            loop {
                if event::poll(tick_rate).unwrap_or(false) {
                    match event::read() {
                        Ok(evt) => {
                            let event = match evt {
                                event::Event::Key(key) => Event::Key(key),
                                event::Event::Mouse(mouse) => Event::Mouse(mouse),
                                event::Event::Resize(w, h) => Event::Resize(w, h),
                                _ => continue,
                            };
                            if event_tx.send(event).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                } else {
                    // Send tick event
                    if event_tx.send(Event::Tick).is_err() {
                        break;
                    }
                }
            }
        });

        Self { rx, _tx: tx }
    }

    /// Wait for the next event
    pub async fn next(&mut self) -> Result<Event> {
        self.rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("Event channel closed"))
    }
}

/// Helper trait for common key combinations
pub trait KeyEventExt {
    fn is_quit(&self) -> bool;
    fn is_confirm(&self) -> bool;
    fn is_cancel(&self) -> bool;
    fn is_navigation(&self) -> Option<Navigation>;
}

/// Navigation direction
#[derive(Debug, Clone, Copy)]
pub enum Navigation {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
}

impl KeyEventExt for KeyEvent {
    fn is_quit(&self) -> bool {
        matches!(
            (self.code, self.modifiers),
            (KeyCode::Char('q'), KeyModifiers::NONE)
                | (KeyCode::Char('c'), KeyModifiers::CONTROL)
        )
    }

    fn is_confirm(&self) -> bool {
        matches!(self.code, KeyCode::Enter)
    }

    fn is_cancel(&self) -> bool {
        matches!(self.code, KeyCode::Esc)
    }

    fn is_navigation(&self) -> Option<Navigation> {
        match self.code {
            KeyCode::Up | KeyCode::Char('k') => Some(Navigation::Up),
            KeyCode::Down | KeyCode::Char('j') => Some(Navigation::Down),
            KeyCode::Left | KeyCode::Char('h') => Some(Navigation::Left),
            KeyCode::Right | KeyCode::Char('l') => Some(Navigation::Right),
            KeyCode::PageUp => Some(Navigation::PageUp),
            KeyCode::PageDown => Some(Navigation::PageDown),
            KeyCode::Home => Some(Navigation::Home),
            KeyCode::End => Some(Navigation::End),
            _ => None,
        }
    }
}
