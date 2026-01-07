//! Main TUI application for tweb

use crate::config::TwebConfig;
use crate::web::{HistoryEntry, Page, PageRenderer, WebClient};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use tui_core::event::{Event, KeyEventExt, Navigation};
use tui_core::widgets::{centered_rect, focused_block, HelpPanel, KeyBinding, StatusBar, TextInput};
use tui_core::{App, AppResult, Color, Frame, Line, Modifier, Paragraph, Span, Style, Wrap};

/// Application mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Normal browsing
    Normal,
    /// URL input
    UrlInput,
    /// Search input
    Search,
    /// Link number input
    GotoLink,
    /// Viewing bookmarks
    Bookmarks,
}

/// Main tweb application
pub struct TwebApp {
    config: TwebConfig,
    client: WebClient,

    // Current state
    mode: Mode,
    show_help: bool,
    page: Option<Page>,
    rendered_lines: Vec<Line<'static>>,
    scroll: usize,
    selected_link: usize,

    // History
    history: Vec<HistoryEntry>,
    history_index: usize,

    // Input
    url_input: TextInput,
    link_input: String,

    // Status
    status: String,
    loading: bool,
}

impl TwebApp {
    pub fn new(config: TwebConfig) -> Result<Self> {
        let client = WebClient::new(&config);
        let url_input = TextInput::new()
            .placeholder("Enter URL or search...")
            .focused(true);

        Ok(Self {
            config,
            client,
            mode: Mode::Normal,
            show_help: false,
            page: None,
            rendered_lines: Vec::new(),
            scroll: 0,
            selected_link: 0,
            history: Vec::new(),
            history_index: 0,
            url_input,
            link_input: String::new(),
            status: "Press 'g' to enter URL, '?' for help".to_string(),
            loading: false,
        })
    }

    /// Navigate to a URL
    pub async fn navigate(&mut self, url: &str) -> Result<()> {
        self.loading = true;
        self.status = format!("Loading {}...", url);

        match self.client.fetch(url).await {
            Ok(page) => {
                // Add to history
                if let Some(current) = &self.page {
                    self.history.push(HistoryEntry {
                        url: current.url.clone(),
                        title: current.title.clone(),
                        scroll_position: self.scroll,
                    });
                    self.history_index = self.history.len();
                }

                self.status = format!("{} - {} links", page.title, page.links.len());
                self.scroll = 0;
                self.selected_link = 0;
                self.rendered_lines = Vec::new(); // Will be rendered on next frame
                self.page = Some(page);
            }
            Err(e) => {
                self.status = format!("Error: {e}");
            }
        }

        self.loading = false;
        Ok(())
    }

    /// Go back in history
    fn go_back(&mut self) {
        if self.history_index > 0 {
            self.history_index -= 1;
            // TODO: Actually navigate back
            self.status = "Back navigation not yet implemented".to_string();
        }
    }

    /// Go forward in history
    fn go_forward(&mut self) {
        if self.history_index < self.history.len() {
            self.history_index += 1;
            self.status = "Forward navigation not yet implemented".to_string();
        }
    }

    fn render_page(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let block = focused_block("Content");
        let inner = block.inner(area);

        // Re-render if needed
        if self.rendered_lines.is_empty() {
            if let Some(page) = &self.page {
                let renderer = PageRenderer::new(inner.width as usize);
                self.rendered_lines = renderer.render(page);
            }
        }

        if self.rendered_lines.is_empty() {
            let welcome = vec![
                Line::raw(""),
                Line::styled(
                    "  Welcome to tweb",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Line::raw(""),
                Line::raw("  Press 'g' to enter a URL"),
                Line::raw("  Press 'o' to open home page"),
                Line::raw("  Press 'b' for bookmarks"),
                Line::raw("  Press '?' for help"),
            ];
            let paragraph = Paragraph::new(welcome).block(block);
            frame.render_widget(paragraph, area);
        } else {
            let visible_height = inner.height as usize;
            let max_scroll = self.rendered_lines.len().saturating_sub(visible_height);
            self.scroll = self.scroll.min(max_scroll);

            let visible_lines: Vec<Line> = self
                .rendered_lines
                .iter()
                .skip(self.scroll)
                .take(visible_height)
                .cloned()
                .collect();

            let paragraph = Paragraph::new(visible_lines)
                .block(block)
                .wrap(Wrap { trim: false });

            frame.render_widget(paragraph, area);
        }
    }

    fn render_url_bar(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let url_text = if let Some(page) = &self.page {
            &page.url
        } else {
            ""
        };

        let title = if let Some(page) = &self.page {
            &page.title
        } else {
            "tweb"
        };

        let spans = vec![
            Span::styled(
                format!(" {title} "),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" │ "),
            Span::raw(url_text),
        ];

        let paragraph = Paragraph::new(Line::from(spans))
            .style(Style::default().bg(Color::DarkGray));

        frame.render_widget(paragraph, area);
    }

    fn render_url_input(&mut self, frame: &mut Frame) {
        let area = centered_rect(80, 15, frame.area());
        let block = focused_block("Enter URL");

        // Clear background
        frame.render_widget(ratatui::widgets::Clear, area);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        self.url_input.render_with_block(
            inner,
            frame.buffer_mut(),
            ratatui::widgets::Block::default(),
        );
    }

    fn render_link_input(&self, frame: &mut Frame) {
        let area = centered_rect(40, 10, frame.area());
        let block = focused_block("Go to Link #");

        frame.render_widget(ratatui::widgets::Clear, area);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let text = Paragraph::new(format!("> {}_", self.link_input))
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(text, inner);
    }

    fn render_bookmarks(&self, frame: &mut Frame) {
        let area = centered_rect(60, 50, frame.area());
        let block = focused_block("Bookmarks");

        frame.render_widget(ratatui::widgets::Clear, area);

        let mut lines = Vec::new();
        for (i, bookmark) in self.config.bookmarks.iter().enumerate() {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("[{}] ", i + 1),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::raw(&bookmark.name),
            ]));
            lines.push(Line::styled(
                format!("    {}", bookmark.url),
                Style::default().fg(Color::DarkGray),
            ));
        }

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }

    fn render_help(&self, frame: &mut Frame) {
        let area = centered_rect(60, 70, frame.area());

        let help = HelpPanel::new("Help - tweb")
            .section(
                "Navigation",
                vec![
                    KeyBinding::new("j/↓", "Scroll down"),
                    KeyBinding::new("k/↑", "Scroll up"),
                    KeyBinding::new("Space/PgDn", "Page down"),
                    KeyBinding::new("PgUp", "Page up"),
                    KeyBinding::new("gg", "Go to top"),
                    KeyBinding::new("G", "Go to bottom"),
                ],
            )
            .section(
                "Links",
                vec![
                    KeyBinding::new("f", "Follow link by number"),
                    KeyBinding::new("Tab", "Next link"),
                    KeyBinding::new("Enter", "Follow selected link"),
                ],
            )
            .section(
                "Browsing",
                vec![
                    KeyBinding::new("g", "Go to URL"),
                    KeyBinding::new("o", "Open home page"),
                    KeyBinding::new("b", "Bookmarks"),
                    KeyBinding::new("H", "Back"),
                    KeyBinding::new("L", "Forward"),
                    KeyBinding::new("r", "Reload"),
                ],
            )
            .section(
                "General",
                vec![
                    KeyBinding::new("?", "Toggle help"),
                    KeyBinding::new("q", "Quit"),
                ],
            );

        frame.render_widget(help, area);
    }

    fn render_status_bar(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let mode_text = match self.mode {
            Mode::Normal => "NORMAL",
            Mode::UrlInput => "URL",
            Mode::Search => "SEARCH",
            Mode::GotoLink => "LINK",
            Mode::Bookmarks => "BOOKMARKS",
        };

        let scroll_info = if !self.rendered_lines.is_empty() {
            let percent = if self.rendered_lines.len() <= 1 {
                100
            } else {
                (self.scroll * 100) / self.rendered_lines.len().saturating_sub(1).max(1)
            };
            format!("{}%", percent.min(100))
        } else {
            String::new()
        };

        let status = StatusBar::new()
            .left(Span::styled(
                format!(" {mode_text} "),
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ))
            .left(Span::raw(format!(" {} ", self.status)))
            .right(Span::raw(format!("{scroll_info} ")))
            .key_hint("?", "Help")
            .key_hint("q", "Quit");

        frame.render_widget(status, area);
    }

    fn handle_normal_mode(&mut self, key: crossterm::event::KeyEvent) -> Option<String> {
        if let Some(nav) = key.is_navigation() {
            match nav {
                Navigation::Up => self.scroll = self.scroll.saturating_sub(1),
                Navigation::Down => self.scroll += 1,
                Navigation::PageUp => self.scroll = self.scroll.saturating_sub(20),
                Navigation::PageDown => self.scroll += 20,
                Navigation::Home => self.scroll = 0,
                Navigation::End => self.scroll = self.rendered_lines.len(),
                _ => {}
            }
            return None;
        }

        match key.code {
            KeyCode::Char(' ') => self.scroll += 20,
            KeyCode::Char('g') if key.modifiers == KeyModifiers::NONE => {
                self.mode = Mode::UrlInput;
                self.url_input = TextInput::new()
                    .placeholder("Enter URL or search...")
                    .focused(true);
            }
            KeyCode::Char('G') => {
                self.scroll = self.rendered_lines.len();
            }
            KeyCode::Char('o') => {
                return Some(self.config.home_url.clone());
            }
            KeyCode::Char('b') => {
                self.mode = Mode::Bookmarks;
            }
            KeyCode::Char('f') => {
                self.mode = Mode::GotoLink;
                self.link_input.clear();
            }
            KeyCode::Char('r') => {
                if let Some(page) = &self.page {
                    return Some(page.url.clone());
                }
            }
            KeyCode::Char('H') => self.go_back(),
            KeyCode::Char('L') => self.go_forward(),
            KeyCode::Char('?') => self.show_help = !self.show_help,
            KeyCode::Tab => {
                if let Some(page) = &self.page {
                    if !page.links.is_empty() {
                        self.selected_link = (self.selected_link + 1) % page.links.len();
                        self.status = format!(
                            "Link {}: {}",
                            self.selected_link + 1,
                            page.links[self.selected_link].text
                        );
                    }
                }
            }
            KeyCode::Enter => {
                if let Some(page) = &self.page {
                    if let Some(link) = page.links.get(self.selected_link) {
                        return Some(link.url.clone());
                    }
                }
            }
            _ => {}
        }
        None
    }

    fn handle_url_input(&mut self, key: crossterm::event::KeyEvent) -> Option<String> {
        match key.code {
            KeyCode::Enter => {
                let input = self.url_input.get_value().to_string();
                self.mode = Mode::Normal;

                if input.is_empty() {
                    return None;
                }

                // Check if it's a URL or a search
                if input.contains('.') && !input.contains(' ') {
                    return Some(input);
                } else {
                    return Some(self.config.search_url_for(&input));
                }
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            _ => {
                self.url_input.handle_key(key);
            }
        }
        None
    }

    fn handle_goto_link(&mut self, key: crossterm::event::KeyEvent) -> Option<String> {
        match key.code {
            KeyCode::Enter => {
                if let Ok(num) = self.link_input.parse::<usize>() {
                    if let Some(page) = &self.page {
                        if num > 0 && num <= page.links.len() {
                            let url = page.links[num - 1].url.clone();
                            self.mode = Mode::Normal;
                            return Some(url);
                        }
                    }
                }
                self.status = "Invalid link number".to_string();
                self.mode = Mode::Normal;
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                self.link_input.push(c);
            }
            KeyCode::Backspace => {
                self.link_input.pop();
            }
            _ => {}
        }
        None
    }

    fn handle_bookmarks(&mut self, key: crossterm::event::KeyEvent) -> Option<String> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let num = c.to_digit(10).unwrap() as usize;
                if num > 0 && num <= self.config.bookmarks.len() {
                    let url = self.config.bookmarks[num - 1].url.clone();
                    self.mode = Mode::Normal;
                    return Some(url);
                }
            }
            _ => {}
        }
        None
    }
}

impl App for TwebApp {
    fn name(&self) -> &'static str {
        "tweb"
    }

    fn handle_event(&mut self, event: Event) -> AppResult<bool> {
        // Handle navigation result
        let mut navigate_url: Option<String> = None;

        match event {
            Event::Key(key) => {
                // Quit check (only in normal mode, not in input modes)
                if key.is_quit() && self.mode == Mode::Normal && !self.show_help {
                    return Ok(true);
                }

                // Help toggle (works in any mode)
                if key.code == KeyCode::Char('?') && self.mode == Mode::Normal {
                    self.show_help = !self.show_help;
                    return Ok(false);
                }

                // Close help
                if self.show_help && (key.is_cancel() || key.code == KeyCode::Char('?')) {
                    self.show_help = false;
                    return Ok(false);
                }

                if self.show_help {
                    return Ok(false);
                }

                // Mode-specific handling
                navigate_url = match self.mode {
                    Mode::Normal => self.handle_normal_mode(key),
                    Mode::UrlInput => self.handle_url_input(key),
                    Mode::GotoLink => self.handle_goto_link(key),
                    Mode::Bookmarks => self.handle_bookmarks(key),
                    Mode::Search => None, // TODO
                };
            }
            Event::Resize(_, _) => {
                // Re-render on resize
                self.rendered_lines.clear();
            }
            _ => {}
        }

        // Navigate if needed (this is a hack since we can't use async in handle_event directly)
        if let Some(url) = navigate_url {
            // We'll store the URL and navigate in tick()
            self.status = format!("Navigating to {}...", url);
            // For now, just store in status - actual navigation would need runtime integration
            tokio::spawn({
                let url = url.clone();
                async move {
                    tracing::info!("Would navigate to: {}", url);
                }
            });
        }

        Ok(false)
    }

    fn render(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // URL bar
                Constraint::Min(1),    // Content
                Constraint::Length(1), // Status bar
            ])
            .split(frame.area());

        // URL bar
        self.render_url_bar(frame, chunks[0]);

        // Main content
        self.render_page(frame, chunks[1]);

        // Status bar
        self.render_status_bar(frame, chunks[2]);

        // Overlays
        match self.mode {
            Mode::UrlInput => self.render_url_input(frame),
            Mode::GotoLink => self.render_link_input(frame),
            Mode::Bookmarks => self.render_bookmarks(frame),
            _ => {}
        }

        if self.show_help {
            self.render_help(frame);
        }
    }
}
