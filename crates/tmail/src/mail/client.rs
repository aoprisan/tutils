//! IMAP client wrapper using rustls

use crate::config::TmailConfig;
use crate::mail::{Folder, Message};
use anyhow::{Context, Result};
use async_imap::Session;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::{rustls, TlsConnector};
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};

type TlsStream = Compat<tokio_rustls::client::TlsStream<TcpStream>>;

/// Mail client for async IMAP operations with rustls
pub struct MailClient {
    session: Option<Session<TlsStream>>,
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

    /// Connect to the IMAP server using rustls
    pub async fn connect(&mut self) -> Result<()> {
        // Build rustls config with Mozilla root certificates
        let root_store = rustls::RootCertStore::from_iter(
            webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
        );

        let tls_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(tls_config));

        // Connect TCP
        let addr = format!("{}:{}", self.config.imap.host, self.config.imap.port);
        let tcp_stream = TcpStream::connect(&addr)
            .await
            .with_context(|| format!("Failed to connect to {addr}"))?;

        // Upgrade to TLS
        let server_name = self.config.imap.host.clone().try_into()
            .map_err(|_| anyhow::anyhow!("Invalid server name"))?;
        let tls_stream = connector
            .connect(server_name, tcp_stream)
            .await
            .context("TLS handshake failed")?;

        // Wrap with compat layer to convert tokio traits to futures traits
        let compat_stream = tls_stream.compat();

        // Create IMAP client
        let client = async_imap::Client::new(compat_stream);

        // Login
        let password = self
            .config
            .imap
            .password
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No password configured"))?;

        let session = client
            .login(&self.config.imap.username, password)
            .await
            .map_err(|e| anyhow::anyhow!("Login failed: {:?}", e.0))?;

        self.session = Some(session);
        Ok(())
    }

    /// Disconnect from the server
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(mut session) = self.session.take() {
            session.logout().await.context("Failed to logout")?;
        }
        Ok(())
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.session.is_some()
    }

    /// List all folders
    pub async fn list_folders(&mut self) -> Result<Vec<Folder>> {
        use futures::StreamExt;

        let session = self
            .session
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected"))?;

        let mut folders_stream = session
            .list(None, Some("*"))
            .await
            .context("Failed to list folders")?;

        let mut result = Vec::new();
        while let Some(folder_result) = folders_stream.next().await {
            let folder = folder_result.context("Failed to get folder")?;
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
    pub async fn select_folder(&mut self, name: &str) -> Result<usize> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected"))?;

        let mailbox = session
            .select(name)
            .await
            .with_context(|| format!("Failed to select folder: {name}"))?;

        Ok(mailbox.exists as usize)
    }

    /// Fetch messages from the current folder
    pub async fn fetch_messages(&mut self, start: usize, count: usize) -> Result<Vec<Message>> {
        use futures::StreamExt;

        let session = self
            .session
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected"))?;

        let range = format!("{}:{}", start, start + count - 1);
        let mut messages_stream = session
            .fetch(&range, "(UID FLAGS ENVELOPE BODY.PEEK[TEXT])")
            .await
            .context("Failed to fetch messages")?;

        let mut result = Vec::new();
        while let Some(msg_result) = messages_stream.next().await {
            let msg = msg_result.context("Failed to fetch message")?;

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

            let flags: Vec<_> = msg.flags().collect();
            let unread = !flags.iter().any(|f| matches!(f, async_imap::types::Flag::Seen));
            let flagged = flags.iter().any(|f| matches!(f, async_imap::types::Flag::Flagged));

            result.push(Message {
                id: msg.uid.map(|u| u.to_string()).unwrap_or_default(),
                from,
                to: vec![],
                cc: vec![],
                subject,
                date: String::new(),
                body,
                html_body: None,
                unread,
                flagged,
                attachments: vec![],
            });
        }

        Ok(result)
    }

    /// Delete a message by UID (marks as deleted and expunges)
    pub async fn delete_message(&mut self, uid: &str) -> Result<()> {
        use futures::StreamExt;

        let session = self
            .session
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected"))?;

        // Add \Deleted flag
        {
            let mut store_stream = session
                .uid_store(uid, "+FLAGS (\\Deleted)")
                .await
                .context("Failed to mark message as deleted")?;

            // Consume the stream
            while store_stream.next().await.is_some() {}
        }

        // Expunge to actually delete - drop the stream, expunge happens on call
        let _ = session.expunge().await.context("Failed to expunge")?;

        Ok(())
    }

    /// Set or clear a flag on a message
    async fn set_flag(&mut self, uid: &str, flag: &str, enable: bool) -> Result<()> {
        use futures::StreamExt;

        let session = self
            .session
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected"))?;

        let op = if enable { "+FLAGS" } else { "-FLAGS" };
        let mut store_stream = session
            .uid_store(uid, format!("{op} ({flag})"))
            .await
            .context("Failed to update flags")?;

        // Consume the stream
        while let Some(_) = store_stream.next().await {}

        Ok(())
    }

    /// Mark message as read (set \Seen flag)
    pub async fn mark_read(&mut self, uid: &str) -> Result<()> {
        self.set_flag(uid, "\\Seen", true).await
    }

    /// Mark message as unread (remove \Seen flag)
    pub async fn mark_unread(&mut self, uid: &str) -> Result<()> {
        self.set_flag(uid, "\\Seen", false).await
    }

    /// Flag/star message (set \Flagged)
    pub async fn flag_message(&mut self, uid: &str) -> Result<()> {
        self.set_flag(uid, "\\Flagged", true).await
    }

    /// Unflag/unstar message (remove \Flagged)
    pub async fn unflag_message(&mut self, uid: &str) -> Result<()> {
        self.set_flag(uid, "\\Flagged", false).await
    }
}
