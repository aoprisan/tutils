//! Main TUI application for tweb

use crate::config::TwebConfig;
use crate::web::{HistoryEntry, Page, PageRenderer, WebClient};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use tokio::sync::mpsc;
use tui_core::event::{Event, KeyEventExt, Navigation};
use tui_core::widgets::{centered_rect, focused_block, HelpPanel, KeyBinding, StatusBar, TextInput};
use tui_core::{App, AppResult, Color, Frame, Line, Modifier, Paragraph, Span, Style, Wrap};

/// Navigation command sent to background worker
enum NavCommand {
    Navigate(String),
}

/// Navigation result from background worker
enum NavResult {
    PageLoaded(Page),
    Error(String),
}

/// A search match location
#[derive(Debug, Clone)]
struct SearchMatch {
    line_idx: usize,
    start_col: usize,
    end_col: usize,
}

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

/// Background worker that handles navigation requests
async fn nav_worker(
    client: WebClient,
    mut cmd_rx: mpsc::UnboundedReceiver<NavCommand>,
    result_tx: mpsc::UnboundedSender<NavResult>,
) {
    while let Some(cmd) = cmd_rx.recv().await {
        let result = match cmd {
            NavCommand::Navigate(url) => match client.fetch(&url).await {
                Ok(page) => NavResult::PageLoaded(page),
                Err(e) => NavResult::Error(e.to_string()),
            },
        };
        if result_tx.send(result).is_err() {
            break;
        }
    }
}

/// Main tweb application
pub struct TwebApp {
    config: TwebConfig,

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
    is_navigating_history: bool,

    // Navigation channels
    nav_cmd_tx: mpsc::UnboundedSender<NavCommand>,
    nav_result_rx: mpsc::UnboundedReceiver<NavResult>,
    pending_scroll: Option<usize>,

    // Input
    url_input: TextInput,
    link_input: String,

    // Search state
    search_input: TextInput,
    search_query: String,
    search_matches: Vec<SearchMatch>,
    current_match: usize,

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
        let search_input = TextInput::new()
            .placeholder("Search...")
            .focused(true);

        // Create navigation channels
        let (nav_cmd_tx, nav_cmd_rx) = mpsc::unbounded_channel();
        let (nav_result_tx, nav_result_rx) = mpsc::unbounded_channel();

        // Spawn navigation worker
        tokio::spawn(nav_worker(client, nav_cmd_rx, nav_result_tx));

        Ok(Self {
            config,
            mode: Mode::Normal,
            show_help: false,
            page: None,
            rendered_lines: Vec::new(),
            scroll: 0,
            selected_link: 0,
            history: Vec::new(),
            history_index: 0,
            is_navigating_history: false,
            nav_cmd_tx,
            nav_result_rx,
            pending_scroll: None,
            url_input,
            link_input: String::new(),
            search_input,
            search_query: String::new(),
            search_matches: Vec::new(),
            current_match: 0,
            status: "Press 'g' to enter URL, '?' for help".to_string(),
            loading: false,
        })
    }

    /// Navigate to a URL (sends command to worker)
    pub fn navigate(&mut self, url: &str) {
        self.loading = true;
        self.status = format!("Loading {}...", url);

        // Save current page to history before navigating (if not going back/forward)
        if !self.is_navigating_history {
            if let Some(current) = &self.page {
                // Truncate forward history when navigating to new page
                self.history.truncate(self.history_index);
                self.history.push(HistoryEntry {
                    url: current.url.clone(),
                    title: current.title.clone(),
                    scroll_position: self.scroll,
                });
                self.history_index = self.history.len();
            }
        }
        self.is_navigating_history = false;

        // Send navigation command to worker
        let _ = self.nav_cmd_tx.send(NavCommand::Navigate(url.to_string()));
    }

    /// Go back in history
    fn go_back(&mut self) {
        if self.history_index > 0 {
            // Save current scroll position
            if let Some(entry) = self.history.get_mut(self.history_index) {
                entry.scroll_position = self.scroll;
            } else if let Some(current) = &self.page {
                // We're at the end, add current page first
                self.history.push(HistoryEntry {
                    url: current.url.clone(),
                    title: current.title.clone(),
                    scroll_position: self.scroll,
                });
            }

            self.history_index -= 1;
            if let Some(entry) = self.history.get(self.history_index) {
                let url = entry.url.clone();
                self.pending_scroll = Some(entry.scroll_position);
                self.is_navigating_history = true;
                self.navigate(&url);
            }
        } else {
            self.status = "No previous page".to_string();
        }
    }

    /// Go forward in history
    fn go_forward(&mut self) {
        if self.history_index + 1 < self.history.len() {
            // Save current scroll position
            if let Some(entry) = self.history.get_mut(self.history_index) {
                entry.scroll_position = self.scroll;
            }

            self.history_index += 1;
            if let Some(entry) = self.history.get(self.history_index) {
                let url = entry.url.clone();
                self.pending_scroll = Some(entry.scroll_position);
                self.is_navigating_history = true;
                self.navigate(&url);
            }
        } else {
            self.status = "No next page".to_string();
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

            // Apply search highlighting if there are matches
            let visible_lines: Vec<Line> = self
                .rendered_lines
                .iter()
                .enumerate()
                .skip(self.scroll)
                .take(visible_height)
                .map(|(idx, line)| {
                    if !self.search_query.is_empty() && !self.search_matches.is_empty() {
                        self.highlight_line(line.clone(), idx)
                    } else {
                        line.clone()
                    }
                })
                .collect();

            let paragraph = Paragraph::new(visible_lines)
                .block(block)
                .wrap(Wrap { trim: false });

            frame.render_widget(paragraph, area);
        }
    }

    fn highlight_line(&self, line: Line<'static>, line_idx: usize) -> Line<'static> {
        // Find matches on this line
        let matches_on_line: Vec<&SearchMatch> = self
            .search_matches
            .iter()
            .filter(|m| m.line_idx == line_idx)
            .collect();

        if matches_on_line.is_empty() {
            return line;
        }

        // Get full text of the line
        let text: String = line.spans.iter().map(|span| span.content.as_ref()).collect();

        // Build new spans with highlighting
        let mut new_spans = Vec::new();
        let mut pos = 0;

        for m in &matches_on_line {
            // Add text before match
            if m.start_col > pos {
                new_spans.push(Span::raw(text[pos..m.start_col].to_string()));
            }

            // Determine if this is the current match
            let is_current = self
                .search_matches
                .get(self.current_match)
                .map(|cm| cm.line_idx == line_idx && cm.start_col == m.start_col)
                .unwrap_or(false);

            // Add highlighted match
            let highlight_style = if is_current {
                Style::default().bg(Color::Yellow).fg(Color::Black)
            } else {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            };

            new_spans.push(Span::styled(
                text[m.start_col..m.end_col].to_string(),
                highlight_style,
            ));

            pos = m.end_col;
        }

        // Add remaining text
        if pos < text.len() {
            new_spans.push(Span::raw(text[pos..].to_string()));
        }

        Line::from(new_spans)
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
                "Search",
                vec![
                    KeyBinding::new("/", "Search in page"),
                    KeyBinding::new("n", "Next match"),
                    KeyBinding::new("N", "Previous match"),
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
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.search_input = TextInput::new()
                    .placeholder("Search...")
                    .focused(true);
            }
            KeyCode::Char('n') if !self.search_matches.is_empty() => {
                self.current_match = (self.current_match + 1) % self.search_matches.len();
                self.scroll_to_match(self.current_match);
                self.status = format!(
                    "{}/{} matches for '{}'",
                    self.current_match + 1,
                    self.search_matches.len(),
                    self.search_query
                );
            }
            KeyCode::Char('N') if !self.search_matches.is_empty() => {
                if self.current_match == 0 {
                    self.current_match = self.search_matches.len() - 1;
                } else {
                    self.current_match -= 1;
                }
                self.scroll_to_match(self.current_match);
                self.status = format!(
                    "{}/{} matches for '{}'",
                    self.current_match + 1,
                    self.search_matches.len(),
                    self.search_query
                );
            }
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

    fn handle_search_mode(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let query = self.search_input.get_value().to_string();
                self.mode = Mode::Normal;

                if !query.is_empty() {
                    self.search_query = query.clone();
                    self.find_matches(&query);
                    if !self.search_matches.is_empty() {
                        self.current_match = 0;
                        self.scroll_to_match(0);
                        self.status = format!(
                            "{}/{} matches for '{}'",
                            self.current_match + 1,
                            self.search_matches.len(),
                            self.search_query
                        );
                    } else {
                        self.status = format!("No matches for '{query}'");
                    }
                }
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                // Keep existing search results visible
            }
            _ => {
                self.search_input.handle_key(key);
            }
        }
    }

    fn find_matches(&mut self, query: &str) {
        self.search_matches.clear();
        let query_lower = query.to_lowercase();

        for (line_idx, line) in self.rendered_lines.iter().enumerate() {
            // Get the plain text from the Line (spans combined)
            let text: String = line.spans.iter().map(|span| span.content.as_ref()).collect();
            let text_lower = text.to_lowercase();

            let mut start = 0;
            while let Some(pos) = text_lower[start..].find(&query_lower) {
                let match_start = start + pos;
                let match_end = match_start + query.len();

                self.search_matches.push(SearchMatch {
                    line_idx,
                    start_col: match_start,
                    end_col: match_end,
                });

                start = match_end;
            }
        }
    }

    fn scroll_to_match(&mut self, match_idx: usize) {
        if let Some(m) = self.search_matches.get(match_idx) {
            // Try to center the match in the visible area
            let visible_height = 20; // Approximate, will be clamped by render
            self.scroll = m.line_idx.saturating_sub(visible_height / 2);
        }
    }

    fn render_search_input(&mut self, frame: &mut Frame) {
        let area = centered_rect(60, 10, frame.area());
        let block = focused_block("Search");

        frame.render_widget(ratatui::widgets::Clear, area);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        self.search_input.render_with_block(
            inner,
            frame.buffer_mut(),
            ratatui::widgets::Block::default(),
        );
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
                    Mode::Search => {
                        self.handle_search_mode(key);
                        None
                    }
                };
            }
            Event::Resize(_, _) => {
                // Re-render on resize
                self.rendered_lines.clear();
            }
            _ => {}
        }

        // Navigate if needed
        if let Some(url) = navigate_url {
            self.navigate(&url);
        }

        Ok(false)
    }

    fn tick(&mut self) -> AppResult<()> {
        // Process navigation results from worker
        while let Ok(result) = self.nav_result_rx.try_recv() {
            match result {
                NavResult::PageLoaded(page) => {
                    self.status = format!("{} - {} links", page.title, page.links.len());
                    self.rendered_lines.clear();
                    self.selected_link = 0;

                    // Restore scroll position if going back/forward
                    if let Some(scroll) = self.pending_scroll.take() {
                        self.scroll = scroll;
                    } else {
                        self.scroll = 0;
                    }

                    // Clear search when navigating to new page
                    self.search_query.clear();
                    self.search_matches.clear();

                    self.page = Some(page);
                    self.loading = false;
                }
                NavResult::Error(e) => {
                    self.status = format!("Error: {e}");
                    self.loading = false;
                }
            }
        }
        Ok(())
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
            Mode::Search => self.render_search_input(frame),
            _ => {}
        }

        if self.show_help {
            self.render_help(frame);
        }
    }
}
