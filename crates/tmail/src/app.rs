//! Main TUI application for tmail

use crate::config::TmailConfig;
use crate::mail::{mail_worker, Folder, MailCommand, MailResult, Message};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use std::time::Instant;
use tokio::sync::mpsc;
use tui_core::event::{Event, KeyEventExt, Navigation};
use tui_core::widgets::{
    centered_rect, focused_block, titled_block, HelpPanel, KeyBinding, StatusBar, TextInput,
};
use tui_core::{
    App, AppResult, Block, Color, Frame, Line, List, ListItem, Modifier, Paragraph, Span, Style,
    Wrap,
};

/// Application view state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    PasswordInput,
    FolderList,
    MessageList,
    MessageView,
    Compose,
}

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

/// Which field is focused in compose view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeFocus {
    To,
    Cc,
    Subject,
    Body,
}

/// Reply mode for compose
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyMode {
    New,
    Reply,
    ReplyAll,
    Forward,
}

/// Main tmail application
pub struct TmailApp {
    config: TmailConfig,
    view: View,
    show_help: bool,

    // Connection state
    connection_state: ConnectionState,

    // Channels for background worker
    cmd_tx: mpsc::UnboundedSender<MailCommand>,
    result_rx: mpsc::UnboundedReceiver<MailResult>,

    // Password input
    password_input: TextInput,
    needs_password: bool,

    // Sync state
    syncing: bool,
    last_sync: Option<Instant>,

    // Folder list state
    folders: Vec<Folder>,
    selected_folder: usize,

    // Message list state
    messages: Vec<Message>,
    selected_message: usize,
    message_scroll: usize,

    // Message view state
    view_scroll: usize,

    // Compose state
    compose_to: TextInput,
    compose_cc: TextInput,
    compose_subject: TextInput,
    compose_body: Vec<String>,
    compose_cursor_line: usize,
    compose_cursor_col: usize,
    compose_focus: ComposeFocus,
    compose_mode: ReplyMode,

    // Status message
    status: String,
}

impl TmailApp {
    pub fn new(config: TmailConfig) -> Result<Self> {
        // Create channels for communication with background worker
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (result_tx, result_rx) = mpsc::unbounded_channel();

        // Check if password is needed
        let needs_password = config.imap.password.is_none();

        // Spawn background mail worker
        let worker_config = config.clone();
        tokio::spawn(async move {
            mail_worker(worker_config, cmd_rx, result_tx).await;
        });

        let initial_view = if needs_password {
            View::PasswordInput
        } else {
            View::FolderList
        };

        let initial_status = if needs_password {
            format!("Enter password for {}", config.imap.username)
        } else {
            "Connecting...".to_string()
        };

        let initial_state = if needs_password {
            ConnectionState::Disconnected
        } else {
            ConnectionState::Connecting
        };

        let app = Self {
            config,
            view: initial_view,
            show_help: false,
            connection_state: initial_state,
            cmd_tx,
            result_rx,
            password_input: TextInput::new()
                .placeholder("Password...")
                .masked(true)
                .focused(true),
            needs_password,
            syncing: false,
            last_sync: None,
            folders: vec![],
            selected_folder: 0,
            messages: vec![],
            selected_message: 0,
            message_scroll: 0,
            view_scroll: 0,
            compose_to: TextInput::new().placeholder("To..."),
            compose_cc: TextInput::new().placeholder("Cc..."),
            compose_subject: TextInput::new().placeholder("Subject..."),
            compose_body: vec![String::new()],
            compose_cursor_line: 0,
            compose_cursor_col: 0,
            compose_focus: ComposeFocus::To,
            compose_mode: ReplyMode::New,
            status: initial_status,
        };

        // Auto-connect if password available
        if !needs_password {
            let _ = app.cmd_tx.send(MailCommand::Connect);
        }

        Ok(app)
    }

    /// Process results from the background mail worker
    fn process_mail_results(&mut self) {
        while let Ok(result) = self.result_rx.try_recv() {
            self.handle_mail_result(result);
        }
    }

    /// Handle a single mail result
    fn handle_mail_result(&mut self, result: MailResult) {
        match result {
            MailResult::NeedsPassword => {
                self.connection_state = ConnectionState::Disconnected;
                self.needs_password = true;
                self.view = View::PasswordInput;
                self.status = format!("Enter password for {}", self.config.imap.username);
            }
            MailResult::Connected => {
                self.connection_state = ConnectionState::Connected;
                self.status = "Connected. Fetching folders...".to_string();
                self.syncing = true;
                let _ = self.cmd_tx.send(MailCommand::FetchFolders);
            }
            MailResult::ConnectionError(e) => {
                self.connection_state = ConnectionState::Error;
                self.syncing = false;
                self.status = format!("Connection failed: {e}");
            }
            MailResult::Folders(folders) => {
                self.folders = folders;
                self.syncing = false;
                self.last_sync = Some(Instant::now());
                self.status = format!("{} folders", self.folders.len());

                // Auto-select INBOX if available
                if let Some(inbox_idx) = self
                    .folders
                    .iter()
                    .position(|f| f.name.eq_ignore_ascii_case("inbox"))
                {
                    self.selected_folder = inbox_idx;
                    self.select_current_folder();
                } else if !self.folders.is_empty() {
                    self.selected_folder = 0;
                    self.select_current_folder();
                }
            }
            MailResult::FolderSelected(name, count) => {
                self.status = format!("{}: {} messages", name, count);

                // Fetch messages
                let fetch_count = self.config.fetch_count.min(count);
                if fetch_count > 0 {
                    let start = count.saturating_sub(fetch_count) + 1; // Most recent
                    let _ = self.cmd_tx.send(MailCommand::FetchMessages(start, fetch_count));
                } else {
                    self.messages.clear();
                    self.selected_message = 0;
                    self.syncing = false;
                }
            }
            MailResult::Messages(messages) => {
                self.messages = messages;
                self.messages.reverse(); // Newest first
                self.selected_message = 0;
                self.message_scroll = 0;
                self.syncing = false;
                self.last_sync = Some(Instant::now());
                self.status = format!("{} messages", self.messages.len());
            }
            MailResult::Error(e) => {
                self.syncing = false;
                self.status = format!("Error: {e}");
            }
            MailResult::MessageDeleted(uid) => {
                self.status = "Message deleted".to_string();
                // Remove from local list
                self.messages.retain(|m| m.id != uid);
                if self.selected_message >= self.messages.len() {
                    self.selected_message = self.messages.len().saturating_sub(1);
                }
                // Go back to message list if we were viewing the deleted message
                if self.view == View::MessageView {
                    self.view = View::MessageList;
                }
            }
            MailResult::FlagsUpdated {
                uid,
                is_read,
                is_flagged,
            } => {
                // Update local message state
                if let Some(msg) = self.messages.iter_mut().find(|m| m.id == uid) {
                    if let Some(read) = is_read {
                        msg.unread = !read;
                        self.status = if read {
                            "Marked as read".to_string()
                        } else {
                            "Marked as unread".to_string()
                        };
                    }
                    if let Some(flagged) = is_flagged {
                        msg.flagged = flagged;
                        self.status = if flagged {
                            "Flagged".to_string()
                        } else {
                            "Unflagged".to_string()
                        };
                    }
                }
            }
            MailResult::MailSent => {
                self.status = "Message sent!".to_string();
                self.clear_compose();
                self.view = View::MessageList;
            }
            MailResult::SendError(e) => {
                self.status = format!("Send failed: {e}");
                // Stay in compose view so user can retry
            }
        }
    }

    /// Select the current folder and fetch its messages
    fn select_current_folder(&mut self) {
        if let Some(folder) = self.folders.get(self.selected_folder) {
            self.syncing = true;
            self.status = format!("Loading {}...", folder.name);
            let _ = self
                .cmd_tx
                .send(MailCommand::SelectFolder(folder.name.clone()));
        }
    }

    /// Trigger a sync/refresh
    fn trigger_sync(&mut self) {
        if self.connection_state == ConnectionState::Connected && !self.syncing {
            self.syncing = true;
            self.status = "Syncing...".to_string();
            let _ = self.cmd_tx.send(MailCommand::FetchFolders);
        }
    }

    /// Check if auto-sync is due
    fn check_auto_sync(&mut self) {
        if self.connection_state != ConnectionState::Connected || self.syncing {
            return;
        }

        if self.config.sync_interval == 0 {
            return;
        }

        let should_sync = self
            .last_sync
            .map(|t| t.elapsed().as_secs() >= self.config.sync_interval)
            .unwrap_or(true);

        if should_sync {
            self.trigger_sync();
        }
    }

    /// Clear compose fields
    fn clear_compose(&mut self) {
        self.compose_to = TextInput::new().placeholder("To...");
        self.compose_cc = TextInput::new().placeholder("Cc...");
        self.compose_subject = TextInput::new().placeholder("Subject...");
        self.compose_body = vec![String::new()];
        self.compose_cursor_line = 0;
        self.compose_cursor_col = 0;
        self.compose_focus = ComposeFocus::To;
        self.compose_mode = ReplyMode::New;
    }

    /// Start a new compose
    fn start_compose(&mut self) {
        self.clear_compose();
        self.compose_to = self.compose_to.clone().focused(true);
        self.view = View::Compose;
        self.status = "Composing new message".to_string();
    }

    /// Start reply/forward to a message
    fn start_reply(&mut self, mode: ReplyMode) {
        let Some(msg) = self.messages.get(self.selected_message).cloned() else {
            return;
        };

        self.clear_compose();
        self.compose_mode = mode;

        match mode {
            ReplyMode::Reply => {
                self.compose_to = TextInput::new()
                    .placeholder("To...")
                    .value(&msg.from);
                let subject = if msg.subject.starts_with("Re: ") {
                    msg.subject.clone()
                } else {
                    format!("Re: {}", msg.subject)
                };
                self.compose_subject = TextInput::new()
                    .placeholder("Subject...")
                    .value(&subject);
                self.compose_body = Self::quote_message(&msg);
                self.compose_focus = ComposeFocus::Body;
                self.status = "Reply".to_string();
            }
            ReplyMode::ReplyAll => {
                self.compose_to = TextInput::new()
                    .placeholder("To...")
                    .value(&msg.from);
                // Add other recipients to Cc (excluding self)
                let my_email = &self.config.smtp.username;
                let cc: Vec<_> = msg
                    .to
                    .iter()
                    .chain(msg.cc.iter())
                    .filter(|addr| !addr.contains(my_email))
                    .cloned()
                    .collect();
                if !cc.is_empty() {
                    self.compose_cc = TextInput::new()
                        .placeholder("Cc...")
                        .value(&cc.join(", "));
                }
                let subject = if msg.subject.starts_with("Re: ") {
                    msg.subject.clone()
                } else {
                    format!("Re: {}", msg.subject)
                };
                self.compose_subject = TextInput::new()
                    .placeholder("Subject...")
                    .value(&subject);
                self.compose_body = Self::quote_message(&msg);
                self.compose_focus = ComposeFocus::Body;
                self.status = "Reply All".to_string();
            }
            ReplyMode::Forward => {
                let subject = if msg.subject.starts_with("Fwd: ") {
                    msg.subject.clone()
                } else {
                    format!("Fwd: {}", msg.subject)
                };
                self.compose_subject = TextInput::new()
                    .placeholder("Subject...")
                    .value(&subject);
                self.compose_body = Self::forward_message(&msg);
                self.compose_to = self.compose_to.clone().focused(true);
                self.compose_focus = ComposeFocus::To;
                self.status = "Forward".to_string();
            }
            ReplyMode::New => {
                self.compose_to = self.compose_to.clone().focused(true);
                self.compose_focus = ComposeFocus::To;
            }
        }

        self.view = View::Compose;
    }

    /// Quote original message for reply
    fn quote_message(msg: &Message) -> Vec<String> {
        let mut lines = vec![
            String::new(),
            String::new(),
            format!("On {}, {} wrote:", msg.date, msg.from),
            String::new(),
        ];
        for line in msg.body.lines() {
            lines.push(format!("> {}", line));
        }
        lines
    }

    /// Format message for forwarding
    fn forward_message(msg: &Message) -> Vec<String> {
        let mut lines = vec![
            String::new(),
            String::new(),
            "---------- Forwarded message ----------".to_string(),
            format!("From: {}", msg.from),
            format!("Date: {}", msg.date),
            format!("Subject: {}", msg.subject),
            String::new(),
        ];
        for line in msg.body.lines() {
            lines.push(line.to_string());
        }
        lines
    }

    /// Send the composed message
    fn send_compose(&mut self) {
        let to_str = self.compose_to.get_value();
        let cc_str = self.compose_cc.get_value();

        if to_str.is_empty() {
            self.status = "Cannot send: No recipient".to_string();
            return;
        }

        // Parse recipients (comma-separated)
        let to: Vec<String> = to_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let cc: Vec<String> = cc_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let subject = self.compose_subject.get_value().to_string();
        let body = self.compose_body.join("\n");

        self.status = "Sending...".to_string();
        let _ = self.cmd_tx.send(MailCommand::SendMail {
            to,
            cc,
            subject,
            body,
        });
    }

    /// Handle input in compose body
    fn handle_body_input(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Char(c) => {
                if self.compose_body.is_empty() {
                    self.compose_body.push(String::new());
                }
                let line = &mut self.compose_body[self.compose_cursor_line];
                line.insert(self.compose_cursor_col, c);
                self.compose_cursor_col += 1;
            }
            KeyCode::Enter => {
                // Split line at cursor
                let current_line = self.compose_body[self.compose_cursor_line].clone();
                let (before, after) = current_line.split_at(self.compose_cursor_col);
                self.compose_body[self.compose_cursor_line] = before.to_string();
                self.compose_body
                    .insert(self.compose_cursor_line + 1, after.to_string());
                self.compose_cursor_line += 1;
                self.compose_cursor_col = 0;
            }
            KeyCode::Backspace => {
                if self.compose_cursor_col > 0 {
                    let line = &mut self.compose_body[self.compose_cursor_line];
                    line.remove(self.compose_cursor_col - 1);
                    self.compose_cursor_col -= 1;
                } else if self.compose_cursor_line > 0 {
                    // Join with previous line
                    let current = self.compose_body.remove(self.compose_cursor_line);
                    self.compose_cursor_line -= 1;
                    self.compose_cursor_col = self.compose_body[self.compose_cursor_line].len();
                    self.compose_body[self.compose_cursor_line].push_str(&current);
                }
            }
            KeyCode::Delete => {
                let line = &mut self.compose_body[self.compose_cursor_line];
                if self.compose_cursor_col < line.len() {
                    line.remove(self.compose_cursor_col);
                } else if self.compose_cursor_line < self.compose_body.len() - 1 {
                    // Join with next line
                    let next = self.compose_body.remove(self.compose_cursor_line + 1);
                    self.compose_body[self.compose_cursor_line].push_str(&next);
                }
            }
            KeyCode::Up => {
                if self.compose_cursor_line > 0 {
                    self.compose_cursor_line -= 1;
                    self.compose_cursor_col = self
                        .compose_cursor_col
                        .min(self.compose_body[self.compose_cursor_line].len());
                }
            }
            KeyCode::Down => {
                if self.compose_cursor_line < self.compose_body.len() - 1 {
                    self.compose_cursor_line += 1;
                    self.compose_cursor_col = self
                        .compose_cursor_col
                        .min(self.compose_body[self.compose_cursor_line].len());
                }
            }
            KeyCode::Left => {
                if self.compose_cursor_col > 0 {
                    self.compose_cursor_col -= 1;
                } else if self.compose_cursor_line > 0 {
                    self.compose_cursor_line -= 1;
                    self.compose_cursor_col = self.compose_body[self.compose_cursor_line].len();
                }
            }
            KeyCode::Right => {
                let line_len = self
                    .compose_body
                    .get(self.compose_cursor_line)
                    .map(|l| l.len())
                    .unwrap_or(0);
                if self.compose_cursor_col < line_len {
                    self.compose_cursor_col += 1;
                } else if self.compose_cursor_line < self.compose_body.len() - 1 {
                    self.compose_cursor_line += 1;
                    self.compose_cursor_col = 0;
                }
            }
            KeyCode::Home => {
                self.compose_cursor_col = 0;
            }
            KeyCode::End => {
                self.compose_cursor_col = self
                    .compose_body
                    .get(self.compose_cursor_line)
                    .map(|l| l.len())
                    .unwrap_or(0);
            }
            _ => {}
        }
    }

    fn render_password_input(&mut self, frame: &mut Frame) {
        let area = centered_rect(50, 30, frame.area());
        frame.render_widget(ratatui::widgets::Clear, area);

        let block = focused_block("IMAP Login");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(2), // Server info
                Constraint::Length(1), // Spacing
                Constraint::Length(1), // Password label
                Constraint::Length(3), // Password input
                Constraint::Length(1), // Spacing
                Constraint::Length(1), // Hint
            ])
            .split(inner);

        let server_info = Paragraph::new(format!(
            "Server: {}:{}\nUser: {}",
            self.config.imap.host, self.config.imap.port, self.config.imap.username
        ))
        .style(Style::default().fg(Color::Cyan));
        frame.render_widget(server_info, chunks[0]);

        let label = Paragraph::new("Password:").style(Style::default().fg(Color::White));
        frame.render_widget(label, chunks[2]);

        self.password_input.clone().render_with_block(
            chunks[3],
            frame.buffer_mut(),
            Block::bordered(),
        );

        let hint =
            Paragraph::new("Enter: connect | Esc: quit").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint, chunks[5]);
    }

    fn render_folder_list(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let items: Vec<ListItem> = self
            .folders
            .iter()
            .enumerate()
            .map(|(i, folder)| {
                let style = if i == self.selected_folder {
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let unread_text = if folder.unread_count > 0 {
                    format!(" ({})", folder.unread_count)
                } else {
                    String::new()
                };

                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(&folder.name, style),
                    Span::styled(unread_text, Style::default().fg(Color::Cyan)),
                ]))
            })
            .collect();

        let block = if self.view == View::FolderList {
            focused_block("Folders")
        } else {
            titled_block("Folders")
        };

        let list = List::new(items).block(block);
        frame.render_widget(list, area);
    }

    fn render_message_list(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let folder_name = self
            .folders
            .get(self.selected_folder)
            .map(|f| f.name.as_str())
            .unwrap_or("Messages");

        let items: Vec<ListItem> = self
            .messages
            .iter()
            .enumerate()
            .map(|(i, msg)| {
                let style = if i == self.selected_message {
                    Style::default().bg(Color::Blue).fg(Color::White)
                } else {
                    Style::default()
                };

                let unread_marker = if msg.unread { "● " } else { "  " };
                let line = Line::from(vec![
                    Span::styled(unread_marker, Style::default().fg(Color::Cyan)),
                    Span::styled(format!("{:<20}", truncate(&msg.from, 20)), style),
                    Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&msg.subject, style),
                ]);
                ListItem::new(line)
            })
            .collect();

        let block = if self.view == View::MessageList {
            focused_block(&format!("Messages - {folder_name}"))
        } else {
            titled_block(&format!("Messages - {folder_name}"))
        };

        let list = List::new(items).block(block);
        frame.render_widget(list, area);
    }

    fn render_message_view(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let block = if self.view == View::MessageView {
            focused_block("Message")
        } else {
            titled_block("Message")
        };

        if let Some(msg) = self.messages.get(self.selected_message) {
            let header_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("From: ", header_style),
                    Span::raw(&msg.from),
                ]),
                Line::from(vec![
                    Span::styled("Subject: ", header_style),
                    Span::raw(&msg.subject),
                ]),
                Line::from(vec![
                    Span::styled("Date: ", header_style),
                    Span::raw(&msg.date),
                ]),
                Line::raw(""),
                Line::raw("─".repeat(area.width.saturating_sub(4) as usize)),
                Line::raw(""),
            ];

            // Add body lines
            for line in msg.body.lines() {
                lines.push(Line::raw(line));
            }

            let paragraph = Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((self.view_scroll as u16, 0));

            frame.render_widget(paragraph, area);
        } else {
            let paragraph = Paragraph::new("No message selected").block(block);
            frame.render_widget(paragraph, area);
        }
    }

    fn render_compose(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let title = match self.compose_mode {
            ReplyMode::New => "Compose",
            ReplyMode::Reply => "Reply",
            ReplyMode::ReplyAll => "Reply All",
            ReplyMode::Forward => "Forward",
        };

        let block = focused_block(title);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // To
                Constraint::Length(3), // Cc
                Constraint::Length(3), // Subject
                Constraint::Length(1), // Separator
                Constraint::Min(5),    // Body
                Constraint::Length(1), // Hints
            ])
            .split(inner);

        // Render To field
        self.render_compose_field("To:", &self.compose_to.clone(), ComposeFocus::To, chunks[0], frame);

        // Render Cc field
        self.render_compose_field("Cc:", &self.compose_cc.clone(), ComposeFocus::Cc, chunks[1], frame);

        // Render Subject field
        self.render_compose_field(
            "Subject:",
            &self.compose_subject.clone(),
            ComposeFocus::Subject,
            chunks[2],
            frame,
        );

        // Separator
        let sep = Paragraph::new("─".repeat(chunks[3].width as usize))
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(sep, chunks[3]);

        // Body
        self.render_compose_body(chunks[4], frame);

        // Hints
        let hints = Paragraph::new("Ctrl+S: Send | Tab: Next field | Esc: Cancel")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hints, chunks[5]);
    }

    fn render_compose_field(
        &self,
        label: &str,
        input: &TextInput,
        field: ComposeFocus,
        area: ratatui::layout::Rect,
        frame: &mut Frame,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(10), Constraint::Min(1)])
            .split(area);

        let label_style = if self.compose_focus == field {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let label_widget = Paragraph::new(label).style(label_style);
        frame.render_widget(label_widget, chunks[0]);

        let border_style = if self.compose_focus == field {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let mut input = input.clone();
        if self.compose_focus == field {
            input = input.focused(true);
        } else {
            input = input.focused(false);
        }

        input.render_with_block(
            chunks[1],
            frame.buffer_mut(),
            Block::bordered().border_style(border_style),
        );
    }

    fn render_compose_body(&self, area: ratatui::layout::Rect, frame: &mut Frame) {
        let is_focused = self.compose_focus == ComposeFocus::Body;
        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::bordered()
            .title("Body")
            .border_style(border_style);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Calculate visible lines
        let visible_height = inner.height as usize;

        // Scroll to keep cursor visible
        let scroll_offset = if self.compose_cursor_line >= visible_height {
            self.compose_cursor_line - visible_height + 1
        } else {
            0
        };

        // Build text with cursor
        let mut lines: Vec<Line> = Vec::new();
        for (i, line) in self.compose_body.iter().enumerate().skip(scroll_offset) {
            if lines.len() >= visible_height {
                break;
            }

            if is_focused && i == self.compose_cursor_line {
                // Line with cursor
                let cursor_col = self.compose_cursor_col.min(line.len());
                let (before, after) = if cursor_col <= line.len() {
                    (&line[..cursor_col], &line[cursor_col..])
                } else {
                    (line.as_str(), "")
                };

                let cursor_char = after.chars().next().unwrap_or(' ');
                let after_cursor = if after.is_empty() {
                    ""
                } else {
                    &after[cursor_char.len_utf8()..]
                };

                lines.push(Line::from(vec![
                    Span::raw(before),
                    Span::styled(
                        cursor_char.to_string(),
                        Style::default().bg(Color::White).fg(Color::Black),
                    ),
                    Span::raw(after_cursor),
                ]));
            } else {
                lines.push(Line::raw(line));
            }
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }

    fn render_help(&self, frame: &mut Frame) {
        let area = centered_rect(60, 70, frame.area());

        let help = HelpPanel::new("Help - tmail")
            .section(
                "Navigation",
                vec![
                    KeyBinding::new("j/↓", "Move down"),
                    KeyBinding::new("k/↑", "Move up"),
                    KeyBinding::new("Enter", "Open/Select"),
                    KeyBinding::new("Esc/h", "Go back"),
                    KeyBinding::new("Tab", "Switch pane/field"),
                ],
            )
            .section(
                "Message Actions",
                vec![
                    KeyBinding::new("c", "Compose new message"),
                    KeyBinding::new("r", "Reply"),
                    KeyBinding::new("R", "Reply all"),
                    KeyBinding::new("f", "Forward"),
                    KeyBinding::new("d", "Delete"),
                    KeyBinding::new("u", "Toggle read/unread"),
                    KeyBinding::new("*", "Toggle flag/star"),
                ],
            )
            .section(
                "Compose",
                vec![
                    KeyBinding::new("Ctrl+S", "Send message"),
                    KeyBinding::new("Tab", "Next field"),
                    KeyBinding::new("Shift+Tab", "Previous field"),
                    KeyBinding::new("Esc", "Cancel"),
                ],
            )
            .section(
                "General",
                vec![
                    KeyBinding::new("s", "Sync/Refresh"),
                    KeyBinding::new("?", "Toggle help"),
                    KeyBinding::new("q", "Quit"),
                ],
            );

        frame.render_widget(help, area);
    }

    fn render_status_bar(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let connection_indicator = match self.connection_state {
            ConnectionState::Disconnected => Span::styled("●", Style::default().fg(Color::Red)),
            ConnectionState::Connecting => Span::styled("●", Style::default().fg(Color::Yellow)),
            ConnectionState::Connected => Span::styled("●", Style::default().fg(Color::Green)),
            ConnectionState::Error => Span::styled("●", Style::default().fg(Color::Red)),
        };

        let sync_indicator = if self.syncing {
            Span::styled(" ↻", Style::default().fg(Color::Yellow))
        } else {
            Span::raw("")
        };

        let status = StatusBar::new()
            .left(connection_indicator)
            .left(sync_indicator)
            .left(Span::styled(
                " tmail ",
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ))
            .left(Span::raw(format!(" {} ", self.status)))
            .key_hint("s", "Sync")
            .key_hint("?", "Help")
            .key_hint("q", "Quit");

        frame.render_widget(status, area);
    }

    fn handle_password_input(&mut self, key: crossterm::event::KeyEvent) -> AppResult<bool> {
        match key.code {
            KeyCode::Enter => {
                let password = self.password_input.get_value().to_string();
                if !password.is_empty() {
                    // Send password to worker
                    let _ = self.cmd_tx.send(MailCommand::SetPassword(password));
                    self.password_input.clear();
                    self.needs_password = false;
                    self.view = View::FolderList;
                    self.connection_state = ConnectionState::Connecting;
                    self.status = "Connecting...".to_string();
                    let _ = self.cmd_tx.send(MailCommand::Connect);
                }
            }
            KeyCode::Esc => {
                return Ok(true); // Quit if user cancels
            }
            _ => {
                self.password_input.handle_key(key);
            }
        }
        Ok(false)
    }

    fn handle_compose_input(&mut self, key: crossterm::event::KeyEvent) -> AppResult<bool> {
        match key.code {
            KeyCode::Esc => {
                // Cancel compose
                self.clear_compose();
                self.view = View::MessageList;
                self.status = "Compose cancelled".to_string();
            }
            KeyCode::Tab => {
                // Cycle through fields
                self.compose_focus = match self.compose_focus {
                    ComposeFocus::To => ComposeFocus::Cc,
                    ComposeFocus::Cc => ComposeFocus::Subject,
                    ComposeFocus::Subject => ComposeFocus::Body,
                    ComposeFocus::Body => ComposeFocus::To,
                };
            }
            KeyCode::BackTab => {
                // Reverse cycle
                self.compose_focus = match self.compose_focus {
                    ComposeFocus::To => ComposeFocus::Body,
                    ComposeFocus::Cc => ComposeFocus::To,
                    ComposeFocus::Subject => ComposeFocus::Cc,
                    ComposeFocus::Body => ComposeFocus::Subject,
                };
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.send_compose();
            }
            _ => {
                // Forward to focused field
                match self.compose_focus {
                    ComposeFocus::To => {
                        self.compose_to.handle_key(key);
                    }
                    ComposeFocus::Cc => {
                        self.compose_cc.handle_key(key);
                    }
                    ComposeFocus::Subject => {
                        self.compose_subject.handle_key(key);
                    }
                    ComposeFocus::Body => {
                        self.handle_body_input(key);
                    }
                }
            }
        }
        Ok(false)
    }

    fn handle_navigation(&mut self, nav: Navigation) {
        match self.view {
            View::FolderList => match nav {
                Navigation::Up => {
                    self.selected_folder = self.selected_folder.saturating_sub(1);
                }
                Navigation::Down => {
                    if self.selected_folder < self.folders.len().saturating_sub(1) {
                        self.selected_folder += 1;
                    }
                }
                _ => {}
            },
            View::MessageList => match nav {
                Navigation::Up => {
                    self.selected_message = self.selected_message.saturating_sub(1);
                }
                Navigation::Down => {
                    if self.selected_message < self.messages.len().saturating_sub(1) {
                        self.selected_message += 1;
                    }
                }
                _ => {}
            },
            View::MessageView => match nav {
                Navigation::Up => {
                    self.view_scroll = self.view_scroll.saturating_sub(1);
                }
                Navigation::Down => {
                    self.view_scroll += 1;
                }
                Navigation::PageUp => {
                    self.view_scroll = self.view_scroll.saturating_sub(10);
                }
                Navigation::PageDown => {
                    self.view_scroll += 10;
                }
                _ => {}
            },
            _ => {}
        }
    }
}

impl Drop for TmailApp {
    fn drop(&mut self) {
        // Signal background worker to disconnect
        let _ = self.cmd_tx.send(MailCommand::Disconnect);
    }
}

impl App for TmailApp {
    fn name(&self) -> &'static str {
        "tmail"
    }

    fn tick(&mut self) -> AppResult<()> {
        // Process results from background worker
        self.process_mail_results();

        // Check for auto-sync
        self.check_auto_sync();

        Ok(())
    }

    fn handle_event(&mut self, event: Event) -> AppResult<bool> {
        match event {
            Event::Key(key) => {
                // Handle password input view separately
                if self.view == View::PasswordInput {
                    return self.handle_password_input(key);
                }

                // Handle compose view separately
                if self.view == View::Compose {
                    return self.handle_compose_input(key);
                }

                // Global keys
                if key.is_quit() && !self.show_help {
                    return Ok(true);
                }

                if key.code == KeyCode::Char('?') {
                    self.show_help = !self.show_help;
                    return Ok(false);
                }

                if self.show_help {
                    if key.is_cancel() || key.code == KeyCode::Char('?') {
                        self.show_help = false;
                    }
                    return Ok(false);
                }

                // Navigation
                if let Some(nav) = key.is_navigation() {
                    self.handle_navigation(nav);
                    return Ok(false);
                }

                // View-specific keys
                match key.code {
                    KeyCode::Enter => match self.view {
                        View::FolderList => {
                            if self.connection_state == ConnectionState::Connected {
                                self.select_current_folder();
                            }
                            self.view = View::MessageList;
                        }
                        View::MessageList => {
                            if !self.messages.is_empty() {
                                self.view = View::MessageView;
                                self.view_scroll = 0;
                            }
                        }
                        _ => {}
                    },
                    KeyCode::Esc | KeyCode::Char('h') if key.modifiers == KeyModifiers::NONE => {
                        match self.view {
                            View::MessageView => self.view = View::MessageList,
                            View::MessageList => self.view = View::FolderList,
                            _ => {}
                        }
                    }
                    KeyCode::Tab => {
                        self.view = match self.view {
                            View::FolderList => View::MessageList,
                            View::MessageList => View::FolderList,
                            _ => self.view,
                        };
                    }
                    KeyCode::Char('c') => {
                        self.start_compose();
                    }
                    KeyCode::Char('r') => {
                        // Reply to selected message
                        if matches!(self.view, View::MessageList | View::MessageView) {
                            if !self.messages.is_empty() {
                                self.start_reply(ReplyMode::Reply);
                            }
                        }
                    }
                    KeyCode::Char('R') => {
                        // Reply all to selected message
                        if matches!(self.view, View::MessageList | View::MessageView) {
                            if !self.messages.is_empty() {
                                self.start_reply(ReplyMode::ReplyAll);
                            }
                        }
                    }
                    KeyCode::Char('f') => {
                        // Forward selected message
                        if matches!(self.view, View::MessageList | View::MessageView) {
                            if !self.messages.is_empty() {
                                self.start_reply(ReplyMode::Forward);
                            }
                        }
                    }
                    KeyCode::Char('d') => {
                        // Delete selected message
                        if matches!(self.view, View::MessageList | View::MessageView) {
                            if let Some(msg) = self.messages.get(self.selected_message) {
                                let uid = msg.id.clone();
                                let _ = self.cmd_tx.send(MailCommand::DeleteMessage(uid));
                                self.status = "Deleting message...".to_string();
                            }
                        }
                    }
                    KeyCode::Char('u') => {
                        // Toggle read/unread
                        if matches!(self.view, View::MessageList | View::MessageView) {
                            if let Some(msg) = self.messages.get(self.selected_message) {
                                let uid = msg.id.clone();
                                let cmd = if msg.unread {
                                    MailCommand::MarkRead(uid)
                                } else {
                                    MailCommand::MarkUnread(uid)
                                };
                                let _ = self.cmd_tx.send(cmd);
                            }
                        }
                    }
                    KeyCode::Char('*') => {
                        // Toggle flagged/starred
                        if matches!(self.view, View::MessageList | View::MessageView) {
                            if let Some(msg) = self.messages.get(self.selected_message) {
                                let uid = msg.id.clone();
                                let cmd = if msg.flagged {
                                    MailCommand::UnflagMessage(uid)
                                } else {
                                    MailCommand::FlagMessage(uid)
                                };
                                let _ = self.cmd_tx.send(cmd);
                            }
                        }
                    }
                    KeyCode::Char('s') => {
                        match self.connection_state {
                            ConnectionState::Connected => {
                                self.trigger_sync();
                            }
                            ConnectionState::Error | ConnectionState::Disconnected => {
                                // Retry connection
                                self.connection_state = ConnectionState::Connecting;
                                self.status = "Reconnecting...".to_string();
                                let _ = self.cmd_tx.send(MailCommand::Connect);
                            }
                            ConnectionState::Connecting => {
                                self.status = "Already connecting...".to_string();
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn render(&mut self, frame: &mut Frame) {
        // Handle password input view
        if self.view == View::PasswordInput {
            self.render_password_input(frame);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(frame.area());

        let main_area = chunks[0];
        let status_area = chunks[1];

        // Main layout: sidebar + content
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(20), Constraint::Min(1)])
            .split(main_area);

        // Render folder list
        self.render_folder_list(frame, main_chunks[0]);

        // Content area depends on view
        match self.view {
            View::FolderList | View::MessageList => {
                self.render_message_list(frame, main_chunks[1]);
            }
            View::MessageView => {
                // Split for message list + message view
                let content_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(10), Constraint::Min(1)])
                    .split(main_chunks[1]);

                self.render_message_list(frame, content_chunks[0]);
                self.render_message_view(frame, content_chunks[1]);
            }
            View::Compose => {
                self.render_compose(frame, main_chunks[1]);
            }
            View::PasswordInput => unreachable!(),
        }

        // Status bar
        self.render_status_bar(frame, status_area);

        // Help overlay
        if self.show_help {
            self.render_help(frame);
        }
    }
}

/// Truncate a string to max length with ellipsis
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
