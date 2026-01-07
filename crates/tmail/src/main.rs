//! tmail: Pine-like terminal mail client
//!
//! A simplified, modern terminal email client inspired by Pine/Alpine.

mod app;
mod config;
mod mail;

use anyhow::Result;
use clap::Parser;
use tui_core::app::run_app;
use tui_core::config::Config;

#[derive(Parser, Debug)]
#[command(name = "tmail")]
#[command(about = "Pine-like terminal mail client")]
#[command(version)]
struct Args {
    /// Path to config file
    #[arg(short, long)]
    config: Option<String>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    if args.debug {
        tui_core::init_logging("tmail")?;
    }

    // Load configuration
    let config = if let Some(path) = args.config {
        config::TmailConfig::load_from(&path)?
    } else {
        config::TmailConfig::load()?
    };

    // Create and run the app
    let app = app::TmailApp::new(config)?;
    run_app(app).await?;

    Ok(())
}
