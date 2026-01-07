//! Text input widget

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

/// Single-line text input widget
#[derive(Debug, Clone)]
pub struct TextInput {
    /// Current input value
    value: String,
    /// Cursor position (character index)
    cursor: usize,
    /// Scroll offset for long inputs
    scroll: usize,
    /// Whether the input is focused
    focused: bool,
    /// Placeholder text
    placeholder: String,
    /// Whether to mask input (for passwords)
    masked: bool,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            scroll: 0,
            focused: false,
            placeholder: String::new(),
            masked: false,
        }
    }

    /// Set the input value
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self.cursor = self.value.len();
        self
    }

    /// Set placeholder text
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Enable password masking
    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    /// Set focus state
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Get the current value
    pub fn get_value(&self) -> &str {
        &self.value
    }

    /// Set the value
    pub fn set_value(&mut self, value: String) {
        self.value = value;
        self.cursor = self.value.len();
    }

    /// Clear the input
    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
        self.scroll = 0;
    }

    /// Handle a key event, returns true if handled
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match c {
                        'a' => self.cursor = 0,
                        'e' => self.cursor = self.value.len(),
                        'u' => {
                            self.value.drain(..self.cursor);
                            self.cursor = 0;
                        }
                        'k' => {
                            self.value.truncate(self.cursor);
                        }
                        'w' => {
                            // Delete word backwards
                            let mut end = self.cursor;
                            while end > 0 && self.value.chars().nth(end - 1) == Some(' ') {
                                end -= 1;
                            }
                            while end > 0 && self.value.chars().nth(end - 1) != Some(' ') {
                                end -= 1;
                            }
                            self.value.drain(end..self.cursor);
                            self.cursor = end;
                        }
                        _ => return false,
                    }
                } else {
                    self.value.insert(self.cursor, c);
                    self.cursor += 1;
                }
                true
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.value.remove(self.cursor - 1);
                    self.cursor -= 1;
                }
                true
            }
            KeyCode::Delete => {
                if self.cursor < self.value.len() {
                    self.value.remove(self.cursor);
                }
                true
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                true
            }
            KeyCode::Right => {
                if self.cursor < self.value.len() {
                    self.cursor += 1;
                }
                true
            }
            KeyCode::Home => {
                self.cursor = 0;
                true
            }
            KeyCode::End => {
                self.cursor = self.value.len();
                true
            }
            _ => false,
        }
    }

    /// Render with a block wrapper
    pub fn render_with_block(&self, area: Rect, buf: &mut Buffer, block: Block<'_>) {
        let inner = block.inner(area);
        block.render(area, buf);
        self.render_inner(inner, buf);
    }

    fn render_inner(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let display_value = if self.value.is_empty() {
            if self.placeholder.is_empty() {
                String::new()
            } else {
                self.placeholder.clone()
            }
        } else if self.masked {
            "•".repeat(self.value.len())
        } else {
            self.value.clone()
        };

        let style = if self.value.is_empty() && !self.placeholder.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        };

        // Calculate scroll offset
        let width = area.width as usize;
        let cursor_pos = if self.masked {
            self.cursor
        } else {
            self.value[..self.cursor].width()
        };

        let scroll = if cursor_pos >= width {
            cursor_pos - width + 1
        } else {
            0
        };

        // Get visible portion
        let visible: String = display_value.chars().skip(scroll).take(width).collect();

        let paragraph = Paragraph::new(visible).style(style);
        paragraph.render(area, buf);

        // Draw cursor if focused
        if self.focused {
            let cursor_x = area.x + (cursor_pos - scroll) as u16;
            if cursor_x < area.x + area.width {
                buf.set_style(
                    Rect::new(cursor_x, area.y, 1, 1),
                    Style::default()
                        .bg(Color::White)
                        .fg(Color::Black)
                        .add_modifier(Modifier::SLOW_BLINK),
                );
            }
        }
    }
}

impl Default for TextInput {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for TextInput {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.render_inner(area, buf);
    }
}
