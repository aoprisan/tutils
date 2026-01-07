//! Application runner with terminal setup and event loop

use crate::app::{App, AppResult};
use crate::event::{Event, EventHandler};
use crate::Terminal;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use std::io::{self, stdout, Stdout};
use std::time::Duration;

/// Runs a TUI application with proper terminal setup/teardown
pub struct AppRunner {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    event_handler: EventHandler,
}

impl AppRunner {
    /// Create a new app runner with the specified tick rate
    pub fn new(tick_rate: Duration) -> AppResult<Self> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        let event_handler = EventHandler::new(tick_rate);

        Ok(Self {
            terminal,
            event_handler,
        })
    }

    /// Run the application event loop
    pub async fn run<A: App>(&mut self, app: &mut A) -> AppResult<()> {
        loop {
            // Render
            self.terminal.draw(|frame| app.render(frame))?;

            // Handle events
            let event = self.event_handler.next().await?;

            match &event {
                Event::Tick => {
                    app.tick()?;
                }
                _ => {
                    if app.handle_event(event)? {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Restore terminal state
    fn restore(&mut self) -> io::Result<()> {
        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        self.terminal.show_cursor()?;
        Ok(())
    }
}

impl Drop for AppRunner {
    fn drop(&mut self) {
        if let Err(e) = self.restore() {
            eprintln!("Failed to restore terminal: {e}");
        }
    }
}

/// Convenience function to run an app with default settings
pub async fn run_app<A: App>(mut app: A) -> AppResult<()> {
    let tick_rate = Duration::from_millis(250);
    let mut runner = AppRunner::new(tick_rate)?;
    runner.run(&mut app).await
}
