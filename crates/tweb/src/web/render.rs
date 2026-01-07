//! Page rendering for terminal display

use crate::web::Page;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use textwrap::wrap;

/// Renders a page for terminal display
pub struct PageRenderer {
    width: usize,
}

impl PageRenderer {
    pub fn new(width: usize) -> Self {
        Self { width: width.max(20) }
    }

    /// Render page content to styled lines
    pub fn render(&self, page: &Page) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        for content_line in &page.content {
            if content_line.is_empty() {
                lines.push(Line::raw(""));
                continue;
            }

            // Wrap long lines
            let wrapped = wrap(content_line, self.width);
            for wrapped_line in wrapped {
                let line = self.style_line(&wrapped_line, page);
                lines.push(line);
            }
        }

        lines
    }

    /// Apply styling to a line (highlight links, etc.)
    fn style_line(&self, text: &str, _page: &Page) -> Line<'static> {
        let mut spans = Vec::new();
        let mut current = String::new();
        let mut chars = text.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '[' {
                // Check if this is a link reference [N]
                let mut num = String::new();
                let mut is_link = false;

                // Look ahead for digits followed by ]
                let mut peek_chars: Vec<char> = Vec::new();
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_digit() {
                        peek_chars.push(chars.next().unwrap());
                        num.push(peek_chars.last().copied().unwrap());
                    } else if next == ']' && !num.is_empty() {
                        peek_chars.push(chars.next().unwrap());
                        is_link = true;
                        break;
                    } else {
                        break;
                    }
                }

                if is_link {
                    // Flush current text
                    if !current.is_empty() {
                        spans.push(Span::raw(std::mem::take(&mut current)));
                    }
                    // Add styled link reference
                    spans.push(Span::styled(
                        format!("[{num}]"),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    // Not a link, add the bracket and consumed chars
                    current.push('[');
                    for pc in peek_chars {
                        current.push(pc);
                    }
                }
            } else {
                current.push(c);
            }
        }

        if !current.is_empty() {
            spans.push(Span::raw(current));
        }

        Line::from(spans)
    }
}
