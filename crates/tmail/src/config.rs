//! Configuration for tmail

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tui_core::config::{Config, ThemeConfig};

/// IMAP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapConfig {
    /// IMAP server hostname
    pub host: String,
    /// IMAP server port (993 for TLS)
    pub port: u16,
    /// Username/email
    pub username: String,
    /// Password (consider using system keychain in production)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Use TLS
    pub tls: bool,
}

impl Default for ImapConfig {
    fn default() -> Self {
        Self {
            host: "imap.example.com".to_string(),
            port: 993,
            username: "user@example.com".to_string(),
            password: None,
            tls: true,
        }
    }
}

/// SMTP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    /// SMTP server hostname
    pub host: String,
    /// SMTP server port (587 for STARTTLS, 465 for TLS)
    pub port: u16,
    /// Username
    pub username: String,
    /// Password
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Use TLS
    pub tls: bool,
}

impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            host: "smtp.example.com".to_string(),
            port: 587,
            username: "user@example.com".to_string(),
            password: None,
            tls: true,
        }
    }
}

/// Main tmail configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmailConfig {
    /// IMAP server settings
    pub imap: ImapConfig,
    /// SMTP server settings
    pub smtp: SmtpConfig,
    /// User's display name for outgoing mail
    pub display_name: String,
    /// Default signature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Number of messages to fetch per folder
    pub fetch_count: usize,
    /// Theme configuration
    pub theme: ThemeConfig,
    /// Auto-sync interval in seconds (0 to disable)
    pub sync_interval: u64,
}

impl Default for TmailConfig {
    fn default() -> Self {
        Self {
            imap: ImapConfig::default(),
            smtp: SmtpConfig::default(),
            display_name: "User".to_string(),
            signature: None,
            fetch_count: 50,
            theme: ThemeConfig::default(),
            sync_interval: 300, // 5 minutes
        }
    }
}

impl Config for TmailConfig {
    fn app_name() -> &'static str {
        "tmail"
    }
}

impl TmailConfig {
    /// Load config from a specific path
    pub fn load_from(path: &str) -> Result<Self> {
        let path = Path::new(path);
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("Failed to parse config from {}", path.display()))
    }
}
