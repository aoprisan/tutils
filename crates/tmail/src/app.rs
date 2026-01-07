//! Main TUI application for tmail

use crate::config::TmailConfig;
use crate::mail::Message;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use tui_core::event::{Event, KeyEventExt, Navigation};
use tui_core::widgets::{focused_block, titled_block, HelpPanel, KeyBinding, StatusBar};
use tui_core::{App, AppResult, Color, Frame, Line, List, ListItem, Modifier, Paragraph, Span, Style, Wrap};

/// Application view state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    FolderList,
    MessageList,
    MessageView,
    Compose,
}

/// Main tmail application
pub struct TmailApp {
    config: TmailConfig,
    view: View,
    show_help: bool,

    // Folder list state
    folders: Vec<String>,
    selected_folder: usize,

    // Message list state
    messages: Vec<Message>,
    selected_message: usize,
    message_scroll: usize,

    // Message view state
    view_scroll: usize,

    // Status message
    status: String,
}

impl TmailApp {
    pub fn new(config: TmailConfig) -> Result<Self> {
        // Initialize with demo data for now
        let folders = vec![
            "INBOX".to_string(),
            "Sent".to_string(),
            "Drafts".to_string(),
            "Trash".to_string(),
            "Archive".to_string(),
        ];

        let messages = vec![
            Message::demo("john@example.com", "Welcome to tmail!", "2024-01-15 10:30"),
            Message::demo("support@rust-lang.org", "Rust 1.75 Released", "2024-01-14 15:45"),
            Message::demo("newsletter@github.com", "Your weekly digest", "2024-01-13 09:00"),
        ];

        Ok(Self {
            config,
            view: View::FolderList,
            show_help: false,
            folders,
            selected_folder: 0,
            messages,
            selected_message: 0,
            message_scroll: 0,
            view_scroll: 0,
            status: "Press ? for help".to_string(),
        })
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
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(folder, style),
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
        let items: Vec<ListItem> = self
            .messages
            .iter()
            .enumerate()
            .map(|(i, msg)| {
                let style = if i == self.selected_message {
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                } else {
                    Style::default()
                };

                let unread_marker = if msg.unread { "● " } else { "  " };
                let line = Line::from(vec![
                    Span::styled(unread_marker, Style::default().fg(Color::Cyan)),
                    Span::styled(format!("{:<20}", msg.from), style),
                    Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&msg.subject, style),
                ]);
                ListItem::new(line)
            })
            .collect();

        let block = if self.view == View::MessageList {
            focused_block(&format!("Messages - {}", self.folders[self.selected_folder]))
        } else {
            titled_block(&format!("Messages - {}", self.folders[self.selected_folder]))
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

    fn render_help(&self, frame: &mut Frame) {
        let area = tui_core::widgets::centered_rect(60, 70, frame.area());

        let help = HelpPanel::new("Help - tmail")
            .section(
                "Navigation",
                vec![
                    KeyBinding::new("j/↓", "Move down"),
                    KeyBinding::new("k/↑", "Move up"),
                    KeyBinding::new("Enter", "Open/Select"),
                    KeyBinding::new("Esc/h", "Go back"),
                    KeyBinding::new("Tab", "Switch pane"),
                ],
            )
            .section(
                "Actions",
                vec![
                    KeyBinding::new("c", "Compose new message"),
                    KeyBinding::new("r", "Reply"),
                    KeyBinding::new("d", "Delete"),
                    KeyBinding::new("s", "Sync/Refresh"),
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
        let status = StatusBar::new()
            .left(Span::styled(
                " tmail ",
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ))
            .left(Span::raw(format!(" {} ", self.status)))
            .key_hint("?", "Help")
            .key_hint("q", "Quit");

        frame.render_widget(status, area);
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
            View::Compose => {}
        }
    }
}

impl App for TmailApp {
    fn name(&self) -> &'static str {
        "tmail"
    }

    fn handle_event(&mut self, event: Event) -> AppResult<bool> {
        match event {
            Event::Key(key) => {
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
                            self.view = View::MessageList;
                            self.status = format!("Opened {}", self.folders[self.selected_folder]);
                        }
                        View::MessageList => {
                            self.view = View::MessageView;
                            self.view_scroll = 0;
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
                        self.status = "Compose not yet implemented".to_string();
                    }
                    KeyCode::Char('s') => {
                        self.status = "Syncing...".to_string();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn render(&mut self, frame: &mut Frame) {
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
                // TODO: Compose view
            }
        }

        // Status bar
        self.render_status_bar(frame, status_area);

        // Help overlay
        if self.show_help {
            self.render_help(frame);
        }
    }
}
