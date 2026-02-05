# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
# Build all crates
cargo build

# Build a specific binary
cargo build -p tweb
cargo build -p tmail
cargo build -p tpsql

# Run binaries
cargo run -p tweb -- https://example.com
cargo run -p tweb -- --dump https://example.com  # Non-interactive mode
cargo run -p tmail
cargo run -p tpsql -- -H localhost -U postgres --dbname postgres

# Tests
cargo test                 # All workspace tests
cargo test -p tui-core     # Single crate

# Code quality
cargo clippy
cargo fmt
```

## Architecture

Rust workspace of terminal UI apps built on Ratatui/crossterm. All apps share the `tui-core` framework library.

### Workspace Crates

- **tui-core** — Shared framework: `App` trait, event system, config management, reusable widgets
- **tweb** — Terminal web browser (reqwest + scraper)
- **tmail** — Terminal email client (async-imap + lettre for SMTP)
- **tpsql** — Terminal PostgreSQL client (tokio-postgres)

### App Trait (`tui-core`)

Every app implements `App` (in `crates/tui-core/src/app/mod.rs`):

```rust
pub trait App: Send {
    fn handle_event(&mut self, event: Event) -> AppResult<bool>;  // true = quit
    fn render(&mut self, frame: &mut Frame);
    fn tick(&mut self) -> AppResult<()> { Ok(()) }  // polling, animations
    fn name(&self) -> &'static str;
}
```

Entry point: `tui_core::app::run_app(app).await` — handles terminal setup/teardown, event loop, and cleanup via Drop.

### Background Worker Pattern

Apps with network I/O (tmail, tpsql) use the same async worker pattern:

1. **Command/Result enums** define the protocol (`DbCommand`/`DbResult`, `MailCommand`/`MailResult`)
2. **Two unbounded mpsc channels**: `cmd_tx`/`cmd_rx` (UI→worker), `result_tx`/`result_rx` (worker→UI)
3. Worker is spawned via `tokio::spawn` in the app constructor
4. `tick()` calls `self.result_rx.try_recv()` in a loop to process results without blocking
5. `Drop` sends a disconnect command to the worker

Reference implementations: `crates/tmail/src/mail/worker.rs`, `crates/tpsql/src/db/worker.rs`

### Config Pattern

Apps define a config struct implementing `Config` trait (from `tui-core/src/config/mod.rs`). Provides `load()` (auto-creates default if missing) and `save()`. Configs are TOML files stored at `~/.config/tutils/<app_name>/config.toml` via the `directories` crate.

### Main Entry Point Pattern

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();                    // clap derive
    if args.debug { tui_core::init_logging("app_name")?; }
    let config = AppConfig::load()?;
    let app = MyApp::new(config)?;
    run_app(app).await
}
```

### UI Conventions

- **Modal views**: Apps use a `View` enum as state machine (e.g., ConnectionInput → QueryEditor → Results)
- **Navigation**: vim keys (hjkl) + arrows + PageUp/Down/Home/End via `KeyEventExt::is_navigation()`
- **Global keys**: `q`/`Ctrl+C` quit, `?` toggles help overlay, `Esc` goes back
- **Layout**: status bar is always bottom row (`Constraint::Length(1)`), content above (`Constraint::Min(1)`)
- **Focused/unfocused blocks**: `focused_block()` (cyan border) vs `titled_block()` (default border)
- **Multi-line editing**: manual `Vec<String>` with cursor line/col tracking (see tmail compose, tpsql query editor)
- **Single-line input**: `TextInput` widget with `.masked(true)` for passwords

### TLS

All TLS connections use **rustls with the ring crypto provider** (not OpenSSL). The correct crate for PostgreSQL TLS is `tokio-postgres-rustls` (not `postgres-rustls` or `postgres_rustls`).

### Clippy Notes

- Avoid enum variant names ending with the enum name (e.g., `ResultsView` in `enum View` — use `Results`)
- Use `.clamp(min, max)` instead of `.max(min).min(max)`
- Suppress dead_code warnings on data model structs with `#[allow(dead_code)]`
