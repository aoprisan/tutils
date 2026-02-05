//! tpsql: psql-like terminal PostgreSQL client
//!
//! A TUI PostgreSQL client for running queries and inspecting databases.

mod app;
mod config;
mod db;

use anyhow::Result;
use clap::Parser;
use tui_core::app::run_app;
use tui_core::config::Config;

#[derive(Parser, Debug)]
#[command(name = "tpsql")]
#[command(about = "psql-like terminal PostgreSQL client")]
#[command(version)]
struct Args {
    /// Connection string (DSN)
    #[arg(short, long)]
    dsn: Option<String>,

    /// PostgreSQL host
    #[arg(short = 'H', long, default_value = None)]
    host: Option<String>,

    /// PostgreSQL port
    #[arg(short, long, default_value = None)]
    port: Option<u16>,

    /// Database name
    #[arg(long, default_value = None)]
    dbname: Option<String>,

    /// Username
    #[arg(short = 'U', long, default_value = None)]
    user: Option<String>,

    /// Path to config file
    #[arg(short, long)]
    config: Option<String>,

    /// Enable debug logging
    #[arg(long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.debug {
        tui_core::init_logging("tpsql")?;
    }

    let mut config = if let Some(path) = args.config {
        config::TpsqlConfig::load_from(&path)?
    } else {
        config::TpsqlConfig::load()?
    };

    // Apply CLI args
    config.apply_args(
        args.dsn.as_deref(),
        args.host.as_deref(),
        args.port,
        args.dbname.as_deref(),
        args.user.as_deref(),
    );

    let app = app::TpsqlApp::new(config)?;
    run_app(app).await?;

    Ok(())
}
