//! Email handling for tmail

mod client;
mod smtp;
mod worker;

pub use client::MailClient;
pub use smtp::SmtpClient;
pub use worker::{mail_worker, MailCommand, MailResult};

/// Represents an email message
#[derive(Debug, Clone)]
pub struct Message {
    /// Unique message ID
    pub id: String,
    /// Sender address
    pub from: String,
    /// Recipient addresses
    pub to: Vec<String>,
    /// CC addresses
    pub cc: Vec<String>,
    /// Subject line
    pub subject: String,
    /// Message date
    pub date: String,
    /// Plain text body
    pub body: String,
    /// HTML body (if available)
    pub html_body: Option<String>,
    /// Whether the message is unread
    pub unread: bool,
    /// Whether the message is flagged/starred
    pub flagged: bool,
    /// Attachments
    pub attachments: Vec<Attachment>,
}

impl Message {
    /// Create a demo message for testing
    pub fn demo(from: &str, subject: &str, date: &str) -> Self {
        Self {
            id: uuid_simple(),
            from: from.to_string(),
            to: vec!["me@example.com".to_string()],
            cc: vec![],
            subject: subject.to_string(),
            date: date.to_string(),
            body: format!(
                "This is a demo email message.\n\n\
                Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
                Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.\n\n\
                Best regards,\n{from}"
            ),
            html_body: None,
            unread: true,
            flagged: false,
            attachments: vec![],
        }
    }
}

/// Email attachment
#[derive(Debug, Clone)]
pub struct Attachment {
    /// Filename
    pub name: String,
    /// MIME type
    pub mime_type: String,
    /// Size in bytes
    pub size: usize,
    /// Content (lazy-loaded)
    pub content: Option<Vec<u8>>,
}

/// Email folder
#[derive(Debug, Clone)]
pub struct Folder {
    /// Folder name
    pub name: String,
    /// Full path/delimiter
    pub path: String,
    /// Number of messages
    pub message_count: usize,
    /// Number of unread messages
    pub unread_count: usize,
    /// Subfolders
    pub children: Vec<Folder>,
}

/// Generate a simple UUID-like string
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", now)
}
