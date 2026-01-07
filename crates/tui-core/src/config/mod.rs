//! Configuration management for TUI applications

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::path::PathBuf;

/// Base configuration trait for all tutils apps
pub trait Config: Serialize + DeserializeOwned + Default {
    /// Get the application name (used for config directory)
    fn app_name() -> &'static str;

    /// Get the config file name
    fn file_name() -> &'static str {
        "config.toml"
    }

    /// Get the config directory path
    fn config_dir() -> Result<PathBuf> {
        directories::ProjectDirs::from("com", "tutils", Self::app_name())
            .map(|dirs| dirs.config_dir().to_path_buf())
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))
    }

    /// Get the full config file path
    fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join(Self::file_name()))
    }

    /// Load config from disk, or create default if not exists
    fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if path.exists() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config from {}", path.display()))?;
            toml::from_str(&content)
                .with_context(|| format!("Failed to parse config from {}", path.display()))
        } else {
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Save config to disk
    fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let dir = path.parent().unwrap();
        std::fs::create_dir_all(dir)?;

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;

        Ok(())
    }
}

/// Common theme configuration
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ThemeConfig {
    /// Primary color (hex or name)
    pub primary: String,
    /// Secondary color
    pub secondary: String,
    /// Error color
    pub error: String,
    /// Warning color
    pub warning: String,
    /// Success color
    pub success: String,
    /// Border style: "rounded", "plain", "double", "thick"
    pub border_style: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            primary: "blue".to_string(),
            secondary: "cyan".to_string(),
            error: "red".to_string(),
            warning: "yellow".to_string(),
            success: "green".to_string(),
            border_style: "rounded".to_string(),
        }
    }
}

/// Parse a color string into a ratatui Color
pub fn parse_color(s: &str) -> ratatui::style::Color {
    use ratatui::style::Color;

    match s.to_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "darkgrey" => Color::DarkGray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        s if s.starts_with('#') => {
            // Parse hex color
            if let Ok(rgb) = u32::from_str_radix(&s[1..], 16) {
                let r = ((rgb >> 16) & 0xFF) as u8;
                let g = ((rgb >> 8) & 0xFF) as u8;
                let b = (rgb & 0xFF) as u8;
                Color::Rgb(r, g, b)
            } else {
                Color::White
            }
        }
        _ => Color::White,
    }
}
