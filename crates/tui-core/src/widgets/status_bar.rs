//! Status bar widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

/// Status bar showing app info and key hints
pub struct StatusBar<'a> {
    left: Vec<Span<'a>>,
    center: Vec<Span<'a>>,
    right: Vec<Span<'a>>,
    style: Style,
}

impl<'a> StatusBar<'a> {
    pub fn new() -> Self {
        Self {
            left: Vec::new(),
            center: Vec::new(),
            right: Vec::new(),
            style: Style::default().bg(Color::DarkGray).fg(Color::White),
        }
    }

    /// Add content to the left side
    pub fn left<S: Into<Span<'a>>>(mut self, span: S) -> Self {
        self.left.push(span.into());
        self
    }

    /// Add content to the center
    pub fn center<S: Into<Span<'a>>>(mut self, span: S) -> Self {
        self.center.push(span.into());
        self
    }

    /// Add content to the right side
    pub fn right<S: Into<Span<'a>>>(mut self, span: S) -> Self {
        self.right.push(span.into());
        self
    }

    /// Add a key hint (formatted as [key] action)
    pub fn key_hint(mut self, key: &'a str, action: &'a str) -> Self {
        self.right.push(Span::styled(
            format!("[{key}]"),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
        self.right.push(Span::raw(format!(" {action} ")));
        self
    }

    /// Set the background style
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

impl<'a> Default for StatusBar<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Fill background
        buf.set_style(area, self.style);

        // Calculate widths
        let left_width: usize = self.left.iter().map(|s| s.width()).sum();
        let center_width: usize = self.center.iter().map(|s| s.width()).sum();
        let right_width: usize = self.right.iter().map(|s| s.width()).sum();

        // Render left
        let left_line = Line::from(self.left);
        let left_para = Paragraph::new(left_line);
        let left_area = Rect::new(area.x + 1, area.y, left_width as u16, 1);
        left_para.render(left_area, buf);

        // Render center
        if !self.center.is_empty() {
            let center_x = area.x + (area.width.saturating_sub(center_width as u16)) / 2;
            let center_line = Line::from(self.center);
            let center_para = Paragraph::new(center_line);
            let center_area = Rect::new(center_x, area.y, center_width as u16, 1);
            center_para.render(center_area, buf);
        }

        // Render right
        if !self.right.is_empty() {
            let right_x = area.x + area.width.saturating_sub(right_width as u16 + 1);
            let right_line = Line::from(self.right);
            let right_para = Paragraph::new(right_line);
            let right_area = Rect::new(right_x, area.y, right_width as u16, 1);
            right_para.render(right_area, buf);
        }
    }
}
