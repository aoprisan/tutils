//! Application framework for TUI apps

pub mod runner;

pub use runner::{run_app, AppRunner};

use crate::event::Event;
use crate::Frame;
use anyhow::Result;

/// Result type for app operations
pub type AppResult<T> = Result<T>;

/// Core application trait that all tutils apps must implement
pub trait App: Send {
    /// Handle an input event, returning whether the app should quit
    fn handle_event(&mut self, event: Event) -> AppResult<bool>;

    /// Render the application UI
    fn render(&mut self, frame: &mut Frame);

    /// Called on each tick (for animations, polling, etc.)
    fn tick(&mut self) -> AppResult<()> {
        Ok(())
    }

    /// Get the app name for logging and config
    fn name(&self) -> &'static str;
}
