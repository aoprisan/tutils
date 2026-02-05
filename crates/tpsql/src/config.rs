//! Configuration for tpsql

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tui_core::config::{Config, ThemeConfig};

/// PostgreSQL connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// Display name for this connection
    pub name: String,
    /// PostgreSQL host
    pub host: String,
    /// PostgreSQL port
    pub port: u16,
    /// Database name
    pub dbname: String,
    /// Username
    pub user: String,
    /// Password (optional, will prompt if not set)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Use TLS
    pub sslmode: String,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            host: "localhost".to_string(),
            port: 5432,
            dbname: "postgres".to_string(),
            user: "postgres".to_string(),
            password: None,
            sslmode: "prefer".to_string(),
        }
    }
}

impl ConnectionConfig {
    /// Build a DSN connection string
    #[allow(dead_code)]
    pub fn to_dsn(&self) -> String {
        let mut dsn = format!(
            "host={} port={} dbname={} user={}",
            self.host, self.port, self.dbname, self.user
        );
        if let Some(ref password) = self.password {
            dsn.push_str(&format!(" password={}", password));
        }
        if self.sslmode != "disable" {
            dsn.push_str(&format!(" sslmode={}", self.sslmode));
        }
        dsn
    }
}

/// Main tpsql configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpsqlConfig {
    /// Default connection
    pub connection: ConnectionConfig,
    /// Saved connections
    #[serde(default)]
    pub saved_connections: Vec<ConnectionConfig>,
    /// Theme configuration
    pub theme: ThemeConfig,
    /// Maximum rows to display per query
    pub max_rows: usize,
    /// Query history size
    pub history_size: usize,
}

impl Default for TpsqlConfig {
    fn default() -> Self {
        Self {
            connection: ConnectionConfig::default(),
            saved_connections: vec![],
            theme: ThemeConfig::default(),
            max_rows: 1000,
            history_size: 100,
        }
    }
}

impl Config for TpsqlConfig {
    fn app_name() -> &'static str {
        "tpsql"
    }
}

impl TpsqlConfig {
    /// Load config from a specific path
    pub fn load_from(path: &str) -> Result<Self> {
        let path = Path::new(path);
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("Failed to parse config from {}", path.display()))
    }

    /// Apply CLI args to the config
    pub fn apply_args(
        &mut self,
        dsn: Option<&str>,
        host: Option<&str>,
        port: Option<u16>,
        dbname: Option<&str>,
        user: Option<&str>,
    ) {
        if let Some(dsn) = dsn {
            // Parse DSN and override individual fields
            self.connection = ConnectionConfig::from_dsn(dsn);
            return;
        }
        if let Some(host) = host {
            self.connection.host = host.to_string();
        }
        if let Some(port) = port {
            self.connection.port = port;
        }
        if let Some(dbname) = dbname {
            self.connection.dbname = dbname.to_string();
        }
        if let Some(user) = user {
            self.connection.user = user.to_string();
        }
    }
}

impl ConnectionConfig {
    /// Parse a DSN string into a ConnectionConfig
    fn from_dsn(dsn: &str) -> Self {
        let mut config = Self::default();

        // Handle both URI and key=value format
        if dsn.starts_with("postgresql://") || dsn.starts_with("postgres://") {
            // URI format — store as-is, will be passed directly to connect
            config.name = dsn.to_string();
            return config;
        }

        // key=value format
        for part in dsn.split_whitespace() {
            if let Some((key, value)) = part.split_once('=') {
                match key {
                    "host" => config.host = value.to_string(),
                    "port" => {
                        if let Ok(p) = value.parse() {
                            config.port = p;
                        }
                    }
                    "dbname" => config.dbname = value.to_string(),
                    "user" => config.user = value.to_string(),
                    "password" => config.password = Some(value.to_string()),
                    "sslmode" => config.sslmode = value.to_string(),
                    _ => {}
                }
            }
        }
        config
    }
}
