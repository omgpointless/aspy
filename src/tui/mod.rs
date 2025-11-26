// TUI module - Terminal User Interface
//
// This module manages the terminal UI using ratatui. It handles:
// - Terminal initialization and cleanup
// - Event loop (keyboard input, timer ticks)
// - Rendering the UI
// - Receiving proxy events and updating the display

pub mod app;
pub mod input;
pub mod ui;

use crate::events::ProxyEvent;
use crate::logging::LogBuffer;
use anyhow::{Context, Result};
use app::App;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

/// Run the TUI
///
/// This function sets up the terminal, runs the event loop, and cleans up
/// when done. The event loop handles both keyboard input and proxy events.
pub async fn run_tui(
    mut event_rx: mpsc::Receiver<ProxyEvent>,
    log_buffer: LogBuffer,
) -> Result<()> {
    // Set up terminal
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("Failed to setup terminal")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

    // Create app state with log buffer
    let mut app = App::with_log_buffer(log_buffer);

    // Run the event loop
    let result = run_event_loop(&mut terminal, &mut app, &mut event_rx).await;

    // Restore terminal
    disable_raw_mode().context("Failed to disable raw mode")?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .context("Failed to restore terminal")?;
    terminal.show_cursor().context("Failed to show cursor")?;

    result
}

/// Main event loop
///
/// This loop handles three types of events:
/// 1. Keyboard input (for navigation and commands)
/// 2. Timer ticks (for periodic redraws)
/// 3. Proxy events (for updating the display)
///
/// The use of tokio::select! allows us to wait on multiple async operations
/// simultaneously, responding to whichever one completes first.
async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    event_rx: &mut mpsc::Receiver<ProxyEvent>,
) -> Result<()> {
    // Create a ticker for periodic redraws (10 FPS)
    let mut tick_interval = tokio::time::interval(Duration::from_millis(100));

    loop {
        // Draw the UI
        terminal
            .draw(|f| ui::draw(f, app))
            .context("Failed to draw terminal")?;

        // Wait for events using tokio::select!
        // This is non-blocking and efficient - we only wake up when something happens
        tokio::select! {
            // Keyboard input
            _ = async {
                // crossterm::event::poll is blocking, so we run it in a separate task
                if event::poll(Duration::from_millis(10)).unwrap_or(false) {
                    if let Ok(Event::Key(key_event)) = event::read() {
                        handle_key_event(app, key_event);
                    }
                }
            } => {}

            // Periodic tick for redrawing
            _ = tick_interval.tick() => {
                // Just redraw, handled at the top of the loop
            }

            // Proxy events
            Some(proxy_event) = event_rx.recv() => {
                app.add_event(proxy_event);
            }
        }

        // Check if we should quit
        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Handle keyboard input using state tracking
/// Only triggers actions on key press (not on release or repeat)
fn handle_key_event(app: &mut App, key_event: KeyEvent) {
    let key = key_event.code;

    // Handle key press and release for proper state tracking
    match key_event.kind {
        KeyEventKind::Press => {
            // Only act if this is a NEW key press (state transition)
            if !app.handle_key_press(key) {
                return; // Key was already pressed, ignore
            }

            // Now handle the actual key action
            match key {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    app.should_quit = true;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    app.select_previous();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    app.select_next();
                }
                KeyCode::Enter => {
                    app.toggle_detail();
                }
                KeyCode::Home => {
                    app.selected = 0;
                    app.scroll_offset = 0;
                }
                KeyCode::End => {
                    if !app.events.is_empty() {
                        app.selected = app.events.len() - 1;
                    }
                }
                _ => {}
            }
        }
        KeyEventKind::Release => {
            // Track key release so it can be pressed again
            app.handle_key_release(key);
        }
        _ => {
            // Ignore repeat events
        }
    }
}
