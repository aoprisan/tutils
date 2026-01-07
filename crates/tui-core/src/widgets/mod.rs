//! Common widgets for TUI applications

mod help;
mod input;
mod status_bar;

pub use help::{HelpPanel, KeyBinding};
pub use input::TextInput;
pub use status_bar::StatusBar;

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders},
};

/// Create a standard block with title
pub fn titled_block(title: impl Into<String>) -> Block<'static> {
    Block::default()
        .title(format!(" {} ", title.into()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
}

/// Create a focused block (highlighted border)
pub fn focused_block(title: impl Into<String>) -> Block<'static> {
    Block::default()
        .title(format!(" {} ", title.into()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
}

/// Calculate centered rect with percentage of parent
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_width = area.width * percent_x / 100;
    let popup_height = area.height * percent_y / 100;
    let x = (area.width - popup_width) / 2 + area.x;
    let y = (area.height - popup_height) / 2 + area.y;
    Rect::new(x, y, popup_width, popup_height)
}

/// Calculate centered rect with fixed dimensions
pub fn centered_rect_fixed(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = (area.width - width) / 2 + area.x;
    let y = (area.height - height) / 2 + area.y;
    Rect::new(x, y, width, height)
}
