// TUI module - Terminal User Interface
//
// This module manages the terminal UI using ratatui. It handles:
// - Terminal initialization and cleanup
// - Event loop (keyboard input, timer ticks)
// - Rendering the UI
// - Receiving proxy events and updating the display

pub mod app;
pub mod input;
pub mod layout;
pub mod scroll;
pub mod streaming;
pub mod ui;

use crate::events::ProxyEvent;
use crate::logging::LogBuffer;
use crate::StreamingThinking;
use anyhow::{Context, Result};
use app::{App, View};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseEvent, MouseEventKind,
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
    context_limit: u64,
    theme_name: &str,
    streaming_thinking: StreamingThinking,
) -> Result<()> {
    // Set up terminal
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("Failed to setup terminal")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

    // Create app state with log buffer and config
    let mut app = App::with_log_buffer(log_buffer);
    app.stats.configured_context_limit = context_limit;
    app.theme = crate::theme::Theme::by_name(theme_name);
    app.streaming_thinking = Some(streaming_thinking);

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
            // Keyboard or mouse input
            _ = async {
                if event::poll(Duration::from_millis(10)).unwrap_or(false) {
                    match event::read() {
                        Ok(Event::Key(key_event)) => handle_key_event(app, key_event),
                        Ok(Event::Mouse(mouse_event)) => handle_mouse_event(app, mouse_event),
                        _ => {}
                    }
                }
            } => {}

            // Periodic tick for redrawing
            _ = tick_interval.tick() => {
                // Advance animation frame for spinners
                app.tick_animation();
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

/// Handle keyboard input
/// Action keys use time-based debounce, navigation keys use state tracking
fn handle_key_event(app: &mut App, key_event: KeyEvent) {
    let key = key_event.code;

    match key_event.kind {
        KeyEventKind::Press => {
            // Action keys - use time-based debounce (no release events needed)
            match key {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    if !app.should_debounce_action() {
                        app.should_quit = true;
                    }
                    return;
                }
                // View switching
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    if !app.should_debounce_action() {
                        app.set_view(View::Stats);
                    }
                    return;
                }
                KeyCode::Char('e') | KeyCode::Char('E') => {
                    if !app.should_debounce_action() {
                        app.set_view(View::Events);
                    }
                    return;
                }
                KeyCode::Char('?') => {
                    if !app.should_debounce_action() {
                        if app.view == View::Help {
                            app.set_view(View::Events);
                        } else {
                            app.set_view(View::Help);
                        }
                    }
                    return;
                }
                KeyCode::Esc => {
                    if !app.should_debounce_action() {
                        // Esc closes detail or goes back to Events
                        if app.show_detail {
                            app.toggle_detail();
                        } else if app.view != View::Events {
                            app.set_view(View::Events);
                        }
                    }
                    return;
                }
                KeyCode::Enter => {
                    if !app.should_debounce_action() && app.view == View::Events {
                        app.toggle_detail();
                    }
                    return;
                }
                KeyCode::Tab => {
                    if !app.should_debounce_action() && app.view == View::Events {
                        if key_event.modifiers.contains(KeyModifiers::SHIFT) {
                            app.focus_prev();
                        } else {
                            app.focus_next();
                        }
                    }
                    return;
                }
                // Backtab is what some terminals send for Shift+Tab
                KeyCode::BackTab => {
                    if !app.should_debounce_action() && app.view == View::Events {
                        app.focus_prev();
                    }
                    return;
                }
                _ => {}
            }

            // Navigation keys - use state tracking for hold-to-repeat
            if !app.handle_key_press(key) {
                return;
            }

            match key {
                KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                KeyCode::Home => app.scroll_to_top(),
                KeyCode::End => app.scroll_to_bottom(),
                _ => {}
            }
        }
        KeyEventKind::Release => {
            app.handle_key_release(key);
        }
        _ => {}
    }
}

/// Handle mouse input
fn handle_mouse_event(app: &mut App, mouse_event: MouseEvent) {
    match mouse_event.kind {
        MouseEventKind::ScrollUp => app.select_previous(),
        MouseEventKind::ScrollDown => app.select_next(),
        _ => {}
    }
}
