// TUI module - Terminal User Interface
//
// This module manages the terminal UI using ratatui. It handles:
// - Terminal initialization and cleanup
// - Event loop (keyboard input, timer ticks)
// - Rendering the UI
// - Receiving proxy events and updating the display

pub mod app;
pub mod clipboard;
pub mod components;
pub mod input;
pub mod layout;
pub mod markdown;
pub mod modal;
pub mod preset;
pub mod scroll;
pub mod streaming;
pub mod traits;
pub mod ui;
pub mod views;

use crate::config::Config;
use crate::events::TrackedEvent;
use crate::logging::{LogBuffer, LogLevel};
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
use modal::{Modal, ModalAction};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;
use traits::{Copyable, Handled, Scrollable};
use views::format_event_detail;

/// Run the TUI
///
/// This function sets up the terminal, runs the event loop, and cleans up
/// when done. The event loop handles both keyboard input and proxy events.
pub async fn run_tui(
    mut event_rx: mpsc::Receiver<TrackedEvent>,
    log_buffer: LogBuffer,
    config: Config,
    streaming_thinking: StreamingThinking,
    shared_stats: crate::proxy::api::SharedStats,
    shared_events: crate::proxy::api::SharedEvents,
) -> Result<()> {
    // Set up terminal
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("Failed to setup terminal")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

    // Create app state with config (initializes theme, preset from config)
    let mut app = App::with_config(log_buffer, config, shared_stats, shared_events);
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
    event_rx: &mut mpsc::Receiver<TrackedEvent>,
) -> Result<()> {
    // Create a ticker for periodic redraws (20 FPS)
    let mut tick_interval = tokio::time::interval(Duration::from_millis(200));

    loop {
        // Draw the UI
        terminal
            .draw(|f| views::draw(f, app))
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
/// Layered dispatch: Modal → Global → View-specific → Component
fn handle_key_event(app: &mut App, key_event: KeyEvent) {
    // Layer 1: Modal captures all input when active
    if handle_modal_input(app, &key_event) {
        return;
    }

    // Layer 2: Global keys (work regardless of view)
    if handle_global_keys(app, &key_event) {
        return;
    }

    let key = key_event.code;

    // Layer 3: View-specific action keys (use InputHandler for debounce)
    match key_event.kind {
        KeyEventKind::Press => {
            match key {
                KeyCode::Esc => {
                    if app.handle_key_press(key) {
                        // Esc: first let focused panel handle it (clear selection)
                        // If panel didn't handle it, fall back to view navigation
                        let key_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
                        if app.dispatch_to_focused(key_event) == Handled::No {
                            // Panel had nothing to clear - go back to Events view
                            if app.view != View::Events {
                                if app.view == View::Settings {
                                    app.save_settings_if_dirty();
                                }
                                app.set_view(View::Events);
                            }
                        }
                    }
                    return;
                }
                KeyCode::Enter => {
                    if app.handle_key_press(key) {
                        match app.view {
                            View::Events => {
                                // Only open detail when focused on Events panel
                                if app.focused == scroll::FocusablePanel::Events {
                                    // Get index: use selected if in selection mode,
                                    // otherwise use last event (auto-follow mode)
                                    let idx = app
                                        .events_panel
                                        .selected
                                        .or_else(|| app.events.len().checked_sub(1));

                                    if let Some(idx) = idx {
                                        app.detail_panel.reset();
                                        // Populate cached content for clipboard copy
                                        if let Some(tracked) = app.events.get(idx) {
                                            let renderable = format_event_detail(tracked);
                                            app.detail_panel
                                                .set_content(renderable.as_str().to_string());
                                        }
                                        app.modal = Some(Modal::detail(idx));
                                    }
                                } else if app.focused == scroll::FocusablePanel::Logs {
                                    // Logs panel: dispatch Enter to the component
                                    let entries = app.log_buffer.get_all();
                                    app.logs_panel.entry_count = entries.len();
                                    if let Some(idx) = app
                                        .logs_panel
                                        .selected
                                        .or_else(|| entries.len().checked_sub(1))
                                    {
                                        // Open log detail modal
                                        if let Some(entry) = entries.get(idx) {
                                            app.detail_panel.reset();
                                            // Format like events: emoji heading, bold labels, separator, content
                                            let level_icon = match entry.level {
                                                LogLevel::Error => "❌",
                                                LogLevel::Warn => "⚠️",
                                                LogLevel::Info => "ℹ️",
                                                LogLevel::Debug => "🔍",
                                                LogLevel::Trace => "📍",
                                            };
                                            let content = format!(
                                                "## {} System Log\n\n\
                                                **Timestamp:** {}  \n\
                                                **Level:** `{:?}`  \n\
                                                **Target:** `{}`\n\n\
                                                ---\n\n\
                                                {}",
                                                level_icon,
                                                entry.timestamp.to_rfc3339(),
                                                entry.level,
                                                entry.target,
                                                entry.message
                                            );
                                            app.detail_panel.set_content(content);
                                            app.modal = Some(Modal::log_detail());
                                        }
                                    }
                                }
                            }
                            View::Settings => app.settings_apply_option(),
                            _ => {}
                        }
                    }
                    return;
                }
                KeyCode::Tab | KeyCode::Right => {
                    if app.handle_key_press(key) {
                        match app.view {
                            View::Events => {
                                if key_event.modifiers.contains(KeyModifiers::SHIFT) {
                                    app.focus_prev();
                                } else {
                                    app.focus_next();
                                }
                            }
                            View::Settings => app.settings_toggle_focus(),
                            View::Stats => {
                                // Navigate to next tab (wraps around)
                                app.stats_selected_tab = (app.stats_selected_tab + 1) % 5;
                            }
                        }
                    }
                    return;
                }
                // Backtab or Left arrow - go back
                KeyCode::BackTab | KeyCode::Left => {
                    if app.handle_key_press(key) {
                        match app.view {
                            View::Events => app.focus_prev(),
                            View::Settings => app.settings_toggle_focus(),
                            View::Stats => {
                                // Navigate to previous tab (wraps around)
                                app.stats_selected_tab = if app.stats_selected_tab == 0 {
                                    4
                                } else {
                                    app.stats_selected_tab - 1
                                };
                            }
                        }
                    }
                    return;
                }
                // Number keys 1-5 for direct tab selection in Stats view
                KeyCode::Char('1'..='5') => {
                    if app.handle_key_press(key) && app.view == View::Stats {
                        // Map '1' -> tab 0, '2' -> tab 1, etc.
                        if let KeyCode::Char(c) = key {
                            app.stats_selected_tab = (c as usize) - ('1' as usize);
                        }
                    }
                    return;
                }
                _ => {}
            }

            // Navigation keys - use state tracking for hold-to-repeat
            if !app.handle_key_press(key) {
                return;
            }

            // Dispatch to focused panel via Interactive trait
            // All views (Events, Stats, Settings) route through dispatch_to_focused()
            // Settings uses dispatch_to_settings() → settings_panel.handle_key()
            app.dispatch_to_focused(key_event);
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
        MouseEventKind::ScrollUp => {
            // If modal is open, scroll the detail panel directly
            if app.modal.is_some() {
                app.detail_panel.scroll_up();
            } else {
                // Synthesize Up key event for trait dispatch
                let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
                app.dispatch_to_focused(key_event);
            }
        }
        MouseEventKind::ScrollDown => {
            // If modal is open, scroll the detail panel directly
            if app.modal.is_some() {
                app.detail_panel.scroll_down();
            } else {
                // Synthesize Down key event for trait dispatch
                let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
                app.dispatch_to_focused(key_event);
            }
        }
        _ => {}
    }
}

/// Handle modal input - returns true if modal absorbed the input
fn handle_modal_input(app: &mut App, key_event: &KeyEvent) -> bool {
    let Some(ref mut modal) = app.modal else {
        return false;
    };

    // CRITICAL: Always process Release events to keep InputHandler in sync
    // Without this, keys get stuck in "pressed" state after modal closes
    if key_event.kind == KeyEventKind::Release {
        app.handle_key_release(key_event.code);
        return true; // Modal absorbs the event, but state is updated
    }

    if key_event.kind != KeyEventKind::Press {
        return true; // Modal absorbs other non-press events (Repeat, etc.)
    }

    match modal.handle_input(key_event.code) {
        ModalAction::None => {}
        ModalAction::Close => {
            app.detail_panel.reset();
            app.modal = None;
        }
        ModalAction::ScrollUp => app.detail_panel.scroll_up(),
        ModalAction::ScrollDown => app.detail_panel.scroll_down(),
        ModalAction::ScrollLeft => app.detail_panel.scroll_left(),
        ModalAction::ScrollRight => app.detail_panel.scroll_right(),
        ModalAction::ScrollTop => app.detail_panel.scroll_to_top(),
        ModalAction::ScrollBottom => app.detail_panel.scroll_to_bottom(),
        ModalAction::ScrollLeftmost => app.detail_panel.scroll_to_left(),
        ModalAction::PageUp => app.detail_panel.page_up(),
        ModalAction::PageDown => app.detail_panel.page_down(),
        ModalAction::CopyReadable => {
            if let Some(text) = app.detail_panel.copy_text() {
                if clipboard::copy_to_clipboard(&text).is_ok() {
                    app.show_toast("✓ Copied to clipboard");
                } else {
                    app.show_toast("✗ Failed to copy");
                }
            }
        }
        ModalAction::CopyJsonl => {
            if let Some(idx) = modal.event_index() {
                if let Some(event) = app.events.get(idx) {
                    if let Ok(json) = serde_json::to_string(event) {
                        if clipboard::copy_to_clipboard(&json).is_ok() {
                            app.show_toast("✓ Copied JSONL to clipboard");
                        } else {
                            app.show_toast("✗ Failed to copy");
                        }
                    }
                }
            }
        }
    }

    true // Modal absorbed the input
}

/// Handle global keys - returns true if handled
/// Global keys work the same regardless of current view
/// Uses InputHandler for debounce (StateChange behavior = trigger once per press)
fn handle_global_keys(app: &mut App, key_event: &KeyEvent) -> bool {
    if key_event.kind != KeyEventKind::Press {
        return false;
    }

    let key = key_event.code;

    match key {
        // Quit
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            if app.handle_key_press(key) {
                app.should_quit = true;
            }
            true
        }
        // View switching - F-keys (primary) and letter shortcuts
        KeyCode::F(1) | KeyCode::Char('e') | KeyCode::Char('E') => {
            if app.handle_key_press(key) {
                if app.view == View::Settings {
                    app.save_settings_if_dirty();
                }
                app.set_view(View::Events);
            }
            true
        }
        KeyCode::F(2) | KeyCode::Char('s') | KeyCode::Char('S') => {
            if app.handle_key_press(key) {
                if app.view == View::Settings {
                    app.save_settings_if_dirty();
                }
                app.set_view(View::Stats);
            }
            true
        }
        KeyCode::F(3) => {
            if app.handle_key_press(key) {
                app.set_view(View::Settings);
            }
            true
        }
        // Help modal
        KeyCode::Char('?') => {
            if app.handle_key_press(key) {
                app.modal = Some(Modal::help());
            }
            true
        }
        // Copy to clipboard: y = readable, Y = JSONL
        KeyCode::Char('y') => {
            if app.handle_key_press(key) {
                if let Some(text) = app.copy_current_readable() {
                    if clipboard::copy_to_clipboard(&text).is_ok() {
                        app.show_toast("✓ Copied to clipboard");
                    } else {
                        app.show_toast("✗ Failed to copy");
                    }
                }
            }
            true
        }
        KeyCode::Char('Y') => {
            if app.handle_key_press(key) {
                if let Some(json) = app.copy_current_jsonl() {
                    if clipboard::copy_to_clipboard(&json).is_ok() {
                        app.show_toast("✓ Copied JSONL to clipboard");
                    } else {
                        app.show_toast("✗ Failed to copy");
                    }
                }
            }
            true
        }
        _ => false,
    }
}
