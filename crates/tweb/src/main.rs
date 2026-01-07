//! tweb: Lynx-like terminal web browser
//!
//! A simplified, modern terminal web browser inspired by Lynx.

mod app;
mod config;
mod web;

use anyhow::Result;
use clap::Parser;
use tui_core::app::run_app;
use tui_core::config::Config;

#[derive(Parser, Debug)]
#[command(name = "tweb")]
#[command(about = "Lynx-like terminal web browser")]
#[command(version)]
struct Args {
    /// URL to open
    url: Option<String>,

    /// Path to config file
    #[arg(short, long)]
    config: Option<String>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Dump page content to stdout and exit (non-interactive)
    #[arg(long)]
    dump: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    if args.debug {
        tui_core::init_logging("tweb")?;
    }

    // Load configuration
    let config = if let Some(path) = args.config {
        config::TwebConfig::load_from(&path)?
    } else {
        config::TwebConfig::load()?
    };

    // Dump mode (like lynx -dump)
    if args.dump {
        if let Some(url) = &args.url {
            let client = web::WebClient::new(&config);
            let page = client.fetch(url).await?;
            println!("{}", page.text_content());
            return Ok(());
        } else {
            anyhow::bail!("URL required for --dump mode");
        }
    }

    // Create and run the app
    let mut app = app::TwebApp::new(config)?;

    // Load initial URL if provided
    if let Some(url) = args.url {
        app.navigate(&url).await?;
    }

    run_app(app).await?;

    Ok(())
}
