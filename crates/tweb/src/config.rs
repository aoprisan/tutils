//! Configuration for tweb

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tui_core::config::{Config, ThemeConfig};

/// Main tweb configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwebConfig {
    /// Home page URL
    pub home_url: String,
    /// Search engine URL template (%s = query)
    pub search_url: String,
    /// User agent string
    pub user_agent: String,
    /// Enable JavaScript (limited support)
    pub javascript: bool,
    /// Enable images (rendered as placeholders)
    pub images: bool,
    /// Maximum redirects to follow
    pub max_redirects: usize,
    /// Connection timeout in seconds
    pub timeout: u64,
    /// Theme configuration
    pub theme: ThemeConfig,
    /// Bookmarks
    pub bookmarks: Vec<Bookmark>,
    /// History size
    pub history_size: usize,
}

/// A bookmark entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub name: String,
    pub url: String,
}

impl Default for TwebConfig {
    fn default() -> Self {
        Self {
            home_url: "https://lite.duckduckgo.com/lite".to_string(),
            search_url: "https://lite.duckduckgo.com/lite?q=%s".to_string(),
            user_agent: format!("tweb/{} (terminal browser)", env!("CARGO_PKG_VERSION")),
            javascript: false,
            images: false,
            max_redirects: 10,
            timeout: 30,
            theme: ThemeConfig::default(),
            bookmarks: vec![
                Bookmark {
                    name: "DuckDuckGo Lite".to_string(),
                    url: "https://lite.duckduckgo.com/lite".to_string(),
                },
                Bookmark {
                    name: "Wikipedia".to_string(),
                    url: "https://en.wikipedia.org/".to_string(),
                },
                Bookmark {
                    name: "Hacker News".to_string(),
                    url: "https://news.ycombinator.com/".to_string(),
                },
            ],
            history_size: 100,
        }
    }
}

impl Config for TwebConfig {
    fn app_name() -> &'static str {
        "tweb"
    }
}

impl TwebConfig {
    /// Load config from a specific path
    pub fn load_from(path: &str) -> Result<Self> {
        let path = Path::new(path);
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("Failed to parse config from {}", path.display()))
    }

    /// Get search URL for a query
    pub fn search_url_for(&self, query: &str) -> String {
        self.search_url.replace("%s", &urlencoding_encode(query))
    }
}

/// Simple URL encoding for search queries
fn urlencoding_encode(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push('+'),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}
