//! IMAP client wrapper

use crate::config::TmailConfig;
use crate::mail::{Folder, Message};
use anyhow::{Context, Result};
use imap::Session;
use native_tls::TlsStream;
use std::net::TcpStream;

/// Mail client for IMAP operations
pub struct MailClient {
    session: Option<Session<TlsStream<TcpStream>>>,
    config: TmailConfig,
}

impl MailClient {
    /// Create a new mail client (not connected)
    pub fn new(config: TmailConfig) -> Self {
        Self {
            session: None,
            config,
        }
    }

    /// Connect to the IMAP server
    pub fn connect(&mut self) -> Result<()> {
        let tls = native_tls::TlsConnector::builder()
            .build()
            .context("Failed to create TLS connector")?;

        let client = imap::connect(
            (self.config.imap.host.as_str(), self.config.imap.port),
            &self.config.imap.host,
            &tls,
        )
        .context("Failed to connect to IMAP server")?;

        let password = self
            .config
            .imap
            .password
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No password configured"))?;

        let session = client
            .login(&self.config.imap.username, password)
            .map_err(|e| anyhow::anyhow!("Login failed: {:?}", e.0))?;

        self.session = Some(session);
        Ok(())
    }

    /// Disconnect from the server
    pub fn disconnect(&mut self) -> Result<()> {
        if let Some(mut session) = self.session.take() {
            session.logout().context("Failed to logout")?;
        }
        Ok(())
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.session.is_some()
    }

    /// List all folders
    pub fn list_folders(&mut self) -> Result<Vec<Folder>> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected"))?;

        let folders = session
            .list(None, Some("*"))
            .context("Failed to list folders")?;

        let mut result = Vec::new();
        for folder in folders.iter() {
            result.push(Folder {
                name: folder.name().to_string(),
                path: folder.name().to_string(),
                message_count: 0,
                unread_count: 0,
                children: vec![],
            });
        }

        Ok(result)
    }

    /// Select a folder and return message count
    pub fn select_folder(&mut self, name: &str) -> Result<usize> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected"))?;

        let mailbox = session
            .select(name)
            .with_context(|| format!("Failed to select folder: {name}"))?;

        Ok(mailbox.exists as usize)
    }

    /// Fetch messages from the current folder
    pub fn fetch_messages(&mut self, start: usize, count: usize) -> Result<Vec<Message>> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected"))?;

        let range = format!("{}:{}", start, start + count - 1);
        let messages = session
            .fetch(&range, "(UID FLAGS ENVELOPE BODY.PEEK[TEXT])")
            .context("Failed to fetch messages")?;

        let mut result = Vec::new();
        for msg in messages.iter() {
            let envelope = msg.envelope().ok_or_else(|| anyhow::anyhow!("No envelope"))?;

            let from = envelope
                .from
                .as_ref()
                .and_then(|addrs| addrs.first())
                .map(|addr| {
                    let mailbox = addr.mailbox.as_ref().map(|s| std::str::from_utf8(s).unwrap_or(""));
                    let host = addr.host.as_ref().map(|s| std::str::from_utf8(s).unwrap_or(""));
                    match (mailbox, host) {
                        (Some(m), Some(h)) => format!("{m}@{h}"),
                        _ => "unknown".to_string(),
                    }
                })
                .unwrap_or_else(|| "unknown".to_string());

            let subject = envelope
                .subject
                .as_ref()
                .map(|s| String::from_utf8_lossy(s).to_string())
                .unwrap_or_else(|| "(no subject)".to_string());

            let body = msg
                .text()
                .map(|b| String::from_utf8_lossy(b).to_string())
                .unwrap_or_default();

            let flags = msg.flags();
            let unread = !flags.iter().any(|f| matches!(f, imap::types::Flag::Seen));

            result.push(Message {
                id: msg.uid.map(|u| u.to_string()).unwrap_or_default(),
                from,
                to: vec![],
                cc: vec![],
                subject,
                date: String::new(), // TODO: Parse date
                body,
                html_body: None,
                unread,
                flagged: flags.iter().any(|f| matches!(f, imap::types::Flag::Flagged)),
                attachments: vec![],
            });
        }

        Ok(result)
    }
}
