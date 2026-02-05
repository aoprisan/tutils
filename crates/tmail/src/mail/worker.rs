//! Background mail worker for async IMAP operations

use crate::config::TmailConfig;
use crate::mail::{Folder, MailClient, Message, SmtpClient};
use tokio::sync::mpsc;

/// Commands sent from the UI to the mail worker
#[derive(Debug)]
pub enum MailCommand {
    /// Connect to the IMAP server
    Connect,
    /// Fetch folder list
    FetchFolders,
    /// Select a folder by name
    SelectFolder(String),
    /// Fetch messages from current folder (start, count)
    FetchMessages(usize, usize),
    /// Disconnect and shutdown worker
    Disconnect,
    /// Update password (sent when user enters password in UI)
    SetPassword(String),
    /// Delete a message by UID
    DeleteMessage(String),
    /// Mark message as read
    MarkRead(String),
    /// Mark message as unread
    MarkUnread(String),
    /// Flag/star a message
    FlagMessage(String),
    /// Unflag/unstar a message
    UnflagMessage(String),
    /// Send an email via SMTP
    SendMail {
        to: Vec<String>,
        cc: Vec<String>,
        subject: String,
        body: String,
    },
    /// Set SMTP password (if different from IMAP)
    SetSmtpPassword(String),
}

/// Results sent from the mail worker back to the UI
#[derive(Debug)]
pub enum MailResult {
    /// Successfully connected
    Connected,
    /// Connection failed
    ConnectionError(String),
    /// Folder list fetched
    Folders(Vec<Folder>),
    /// Folder selected with message count
    FolderSelected(String, usize),
    /// Messages fetched
    Messages(Vec<Message>),
    /// General error
    Error(String),
    /// Password needed (config had no password)
    NeedsPassword,
    /// Message deleted successfully
    MessageDeleted(String),
    /// Message flags updated (uid, is_read, is_flagged)
    FlagsUpdated {
        uid: String,
        is_read: Option<bool>,
        is_flagged: Option<bool>,
    },
    /// Email sent successfully
    MailSent,
    /// Email send failed
    SendError(String),
}

/// Run the mail worker in a background task
pub async fn mail_worker(
    mut config: TmailConfig,
    mut cmd_rx: mpsc::UnboundedReceiver<MailCommand>,
    result_tx: mpsc::UnboundedSender<MailResult>,
) {
    let mut client: Option<MailClient> = None;
    let mut smtp_client: Option<SmtpClient> = None;

    while let Some(cmd) = cmd_rx.recv().await {
        let result = match cmd {
            MailCommand::SetPassword(password) => {
                config.imap.password = Some(password.clone());
                // Also set SMTP password if not separately configured
                if config.smtp.password.is_none() {
                    config.smtp.password = Some(password);
                }
                // Don't send a result, wait for Connect command
                continue;
            }
            MailCommand::SetSmtpPassword(password) => {
                config.smtp.password = Some(password);
                // Don't send a result
                continue;
            }
            MailCommand::Connect => {
                if config.imap.password.is_none() {
                    MailResult::NeedsPassword
                } else {
                    let mut new_client = MailClient::new(config.clone());
                    match new_client.connect().await {
                        Ok(()) => {
                            client = Some(new_client);
                            MailResult::Connected
                        }
                        Err(e) => MailResult::ConnectionError(e.to_string()),
                    }
                }
            }
            MailCommand::FetchFolders => {
                if let Some(ref mut c) = client {
                    match c.list_folders().await {
                        Ok(folders) => MailResult::Folders(folders),
                        Err(e) => MailResult::Error(format!("Failed to fetch folders: {e}")),
                    }
                } else {
                    MailResult::Error("Not connected".to_string())
                }
            }
            MailCommand::SelectFolder(name) => {
                if let Some(ref mut c) = client {
                    match c.select_folder(&name).await {
                        Ok(count) => MailResult::FolderSelected(name, count),
                        Err(e) => MailResult::Error(format!("Failed to select folder: {e}")),
                    }
                } else {
                    MailResult::Error("Not connected".to_string())
                }
            }
            MailCommand::FetchMessages(start, count) => {
                if let Some(ref mut c) = client {
                    if count == 0 {
                        MailResult::Messages(vec![])
                    } else {
                        match c.fetch_messages(start, count).await {
                            Ok(messages) => MailResult::Messages(messages),
                            Err(e) => MailResult::Error(format!("Failed to fetch messages: {e}")),
                        }
                    }
                } else {
                    MailResult::Error("Not connected".to_string())
                }
            }
            MailCommand::Disconnect => {
                if let Some(mut c) = client.take() {
                    let _ = c.disconnect().await;
                }
                break;
            }
            MailCommand::DeleteMessage(uid) => {
                if let Some(ref mut c) = client {
                    match c.delete_message(&uid).await {
                        Ok(()) => MailResult::MessageDeleted(uid),
                        Err(e) => MailResult::Error(format!("Delete failed: {e}")),
                    }
                } else {
                    MailResult::Error("Not connected".to_string())
                }
            }
            MailCommand::MarkRead(uid) => {
                if let Some(ref mut c) = client {
                    match c.mark_read(&uid).await {
                        Ok(()) => MailResult::FlagsUpdated {
                            uid,
                            is_read: Some(true),
                            is_flagged: None,
                        },
                        Err(e) => MailResult::Error(format!("Mark read failed: {e}")),
                    }
                } else {
                    MailResult::Error("Not connected".to_string())
                }
            }
            MailCommand::MarkUnread(uid) => {
                if let Some(ref mut c) = client {
                    match c.mark_unread(&uid).await {
                        Ok(()) => MailResult::FlagsUpdated {
                            uid,
                            is_read: Some(false),
                            is_flagged: None,
                        },
                        Err(e) => MailResult::Error(format!("Mark unread failed: {e}")),
                    }
                } else {
                    MailResult::Error("Not connected".to_string())
                }
            }
            MailCommand::FlagMessage(uid) => {
                if let Some(ref mut c) = client {
                    match c.flag_message(&uid).await {
                        Ok(()) => MailResult::FlagsUpdated {
                            uid,
                            is_read: None,
                            is_flagged: Some(true),
                        },
                        Err(e) => MailResult::Error(format!("Flag failed: {e}")),
                    }
                } else {
                    MailResult::Error("Not connected".to_string())
                }
            }
            MailCommand::UnflagMessage(uid) => {
                if let Some(ref mut c) = client {
                    match c.unflag_message(&uid).await {
                        Ok(()) => MailResult::FlagsUpdated {
                            uid,
                            is_read: None,
                            is_flagged: Some(false),
                        },
                        Err(e) => MailResult::Error(format!("Unflag failed: {e}")),
                    }
                } else {
                    MailResult::Error("Not connected".to_string())
                }
            }
            MailCommand::SendMail {
                to,
                cc,
                subject,
                body,
            } => {
                // Lazy-connect SMTP if needed
                if smtp_client.is_none() {
                    let mut new_smtp = SmtpClient::new(config.clone());
                    match new_smtp.connect().await {
                        Ok(()) => {
                            smtp_client = Some(new_smtp);
                        }
                        Err(e) => {
                            let _ = result_tx.send(MailResult::SendError(format!(
                                "SMTP connection failed: {e}"
                            )));
                            continue;
                        }
                    }
                }

                if let Some(ref smtp) = smtp_client {
                    match smtp.send(&to, &cc, &subject, &body).await {
                        Ok(()) => MailResult::MailSent,
                        Err(e) => MailResult::SendError(e.to_string()),
                    }
                } else {
                    MailResult::SendError("SMTP not available".to_string())
                }
            }
        };

        // Send result back to UI
        if result_tx.send(result).is_err() {
            // UI has closed, exit
            break;
        }
    }

    // Clean up on exit
    if let Some(mut c) = client.take() {
        let _ = c.disconnect().await;
    }
}
