//! tui-core: Shared TUI framework and components for tutils
//!
//! This crate provides:
//! - Application framework with async event loop
//! - Common widgets (status bar, help panel, dialogs)
//! - Event handling infrastructure
//! - Configuration management

pub mod app;
pub mod config;
pub mod event;
pub mod widgets;

pub use app::{App, AppResult};
pub use config::Config;
pub use event::{Event, EventHandler};

// Re-export commonly used ratatui types
pub use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};

/// Initialize logging with tracing
pub fn init_logging(app_name: &str) -> anyhow::Result<()> {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let log_dir = directories::ProjectDirs::from("com", "tutils", app_name)
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    std::fs::create_dir_all(&log_dir)?;
    let log_file = std::fs::File::create(log_dir.join(format!("{app_name}.log")))?;

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(log_file).with_ansi(false))
        .init();

    Ok(())
}

/// Version information for the tutils suite
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
