//! Help panel widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget, Wrap},
};

/// A keyboard shortcut entry
#[derive(Debug, Clone)]
pub struct KeyBinding {
    pub keys: String,
    pub description: String,
}

impl KeyBinding {
    pub fn new(keys: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            keys: keys.into(),
            description: description.into(),
        }
    }
}

/// Help panel showing keyboard shortcuts
pub struct HelpPanel<'a> {
    title: &'a str,
    bindings: Vec<KeyBinding>,
    sections: Vec<(&'a str, Vec<KeyBinding>)>,
}

impl<'a> HelpPanel<'a> {
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            bindings: Vec::new(),
            sections: Vec::new(),
        }
    }

    /// Add a single key binding
    pub fn binding(mut self, keys: impl Into<String>, description: impl Into<String>) -> Self {
        self.bindings.push(KeyBinding::new(keys, description));
        self
    }

    /// Add a section with multiple bindings
    pub fn section(mut self, name: &'a str, bindings: Vec<KeyBinding>) -> Self {
        self.sections.push((name, bindings));
        self
    }
}

impl Widget for HelpPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear the area first
        Clear.render(area, buf);

        let mut lines: Vec<Line> = Vec::new();

        // Add top-level bindings
        for binding in &self.bindings {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:>12}", binding.keys),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::raw(&binding.description),
            ]));
        }

        // Add sections
        for (section_name, bindings) in &self.sections {
            if !lines.is_empty() {
                lines.push(Line::raw(""));
            }
            lines.push(Line::styled(
                format!("── {section_name} ──"),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));

            for binding in bindings {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:>12}", binding.keys),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::raw(&binding.description),
                ]));
            }
        }

        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });

        paragraph.render(area, buf);
    }
}
