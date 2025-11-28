// TUI application state
//
// This module manages the state of the TUI application, including the list
// of events, selected item, statistics, and UI state.

use super::input::InputHandler;
use super::theme::ThemeKind;
use crate::events::{ProxyEvent, Stats};
use crate::logging::LogBuffer;
use std::time::{Duration, Instant};

/// Different views the TUI can display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Main, // Event list with optional detail
    Stats, // Comprehensive statistics dashboard
    Help,  // Help and keybindings
}

impl View {
    /// Get the next view in cycle
    pub fn next(self) -> Self {
        match self {
            View::Main => View::Stats,
            View::Stats => View::Help,
            View::Help => View::Main,
        }
    }

    /// Get the previous view in cycle
    pub fn prev(self) -> Self {
        match self {
            View::Main => View::Help,
            View::Stats => View::Main,
            View::Help => View::Stats,
        }
    }

    /// Get display name for status bar
    pub fn name(&self) -> &'static str {
        match self {
            View::Main => "Events",
            View::Stats => "Stats",
            View::Help => "Help",
        }
    }
}

/// Debounce duration for action keys (Enter, Esc, q)
/// Prevents rapid-fire triggers on terminals that don't send release events
const ACTION_DEBOUNCE: Duration = Duration::from_millis(150);

/// Main application state for the TUI
pub struct App {
    /// All events received so far
    pub events: Vec<ProxyEvent>,

    /// Index of the currently selected event (for detail view)
    pub selected: usize,

    /// Whether to show the detail view
    pub show_detail: bool,

    /// Whether the app should quit
    pub should_quit: bool,

    /// Accumulated statistics
    pub stats: Stats,

    /// When the app started (for uptime display)
    pub start_time: Instant,

    /// Scroll offset for the event list
    pub scroll_offset: usize,

    /// Scroll offset for detail view
    pub detail_scroll: usize,

    /// Input handler for flexible key behavior
    input_handler: InputHandler,

    /// Log buffer for system logs display
    pub log_buffer: LogBuffer,

    /// Last time an action key was triggered (for debouncing)
    last_action_time: Option<Instant>,

    /// Current view being displayed
    pub current_view: View,

    /// Current color theme
    pub theme: ThemeKind,

    /// Scroll offset for stats view
    pub stats_scroll: usize,

    /// Scroll offset for help view
    pub help_scroll: usize,

    /// Whether thinking panel is collapsed
    pub thinking_collapsed: bool,
}

impl App {
    pub fn new() -> Self {
        Self::with_log_buffer(LogBuffer::new())
    }

    pub fn with_log_buffer(log_buffer: LogBuffer) -> Self {
        Self {
            events: Vec::new(),
            selected: 0,
            show_detail: false,
            should_quit: false,
            stats: Stats::default(),
            start_time: Instant::now(),
            scroll_offset: 0,
            detail_scroll: 0,
            input_handler: InputHandler::default(),
            log_buffer,
            last_action_time: None,
            current_view: View::default(),
            theme: ThemeKind::default(),
            stats_scroll: 0,
            help_scroll: 0,
            thinking_collapsed: false,
        }
    }

    /// Switch to the next view
    pub fn next_view(&mut self) {
        self.current_view = self.current_view.next();
    }

    /// Switch to the previous view
    pub fn prev_view(&mut self) {
        self.current_view = self.current_view.prev();
    }

    /// Switch to a specific view
    pub fn set_view(&mut self, view: View) {
        self.current_view = view;
    }

    /// Cycle to the next theme
    pub fn next_theme(&mut self) {
        self.theme = self.theme.next();
    }

    /// Cycle to the previous theme
    pub fn prev_theme(&mut self) {
        self.theme = self.theme.prev();
    }

    /// Toggle the thinking panel collapsed state
    pub fn toggle_thinking(&mut self) {
        self.thinking_collapsed = !self.thinking_collapsed;
    }

    /// Check if an action should be debounced
    /// Returns true if action should be blocked (too soon since last action)
    pub fn should_debounce_action(&mut self) -> bool {
        let now = Instant::now();
        if let Some(last) = self.last_action_time {
            if now.duration_since(last) < ACTION_DEBOUNCE {
                return true;
            }
        }
        self.last_action_time = Some(now);
        false
    }

    /// Handle a key press - returns true if the action should be triggered
    /// Uses the configured behavior for each key (state-change or repeatable)
    pub fn handle_key_press(&mut self, key: crossterm::event::KeyCode) -> bool {
        self.input_handler.handle_key_press(key)
    }

    /// Handle a key release
    pub fn handle_key_release(&mut self, key: crossterm::event::KeyCode) {
        self.input_handler.handle_key_release(key);
    }

    /// Add a new event and update statistics
    pub fn add_event(&mut self, event: ProxyEvent) {
        // Update statistics based on event type
        match &event {
            ProxyEvent::Request { .. } => {
                self.stats.total_requests += 1;
            }
            ProxyEvent::ToolCall { tool_name, .. } => {
                self.stats.total_tool_calls += 1;
                // Track per-tool call count
                *self
                    .stats
                    .tool_call_counts
                    .entry(tool_name.clone())
                    .or_insert(0) += 1;
            }
            ProxyEvent::ToolResult {
                tool_name,
                duration,
                success,
                ..
            } => {
                self.stats.total_duration += *duration;
                if *success {
                    self.stats.successful_calls += 1;
                } else {
                    self.stats.failed_calls += 1;
                }
                // Track per-tool duration
                *self
                    .stats
                    .tool_durations
                    .entry(tool_name.clone())
                    .or_insert(Duration::ZERO) += *duration;
            }
            ProxyEvent::ApiUsage {
                model,
                input_tokens,
                output_tokens,
                cache_creation_tokens,
                cache_read_tokens,
                ..
            } => {
                // Accumulate token usage
                self.stats.total_input_tokens += *input_tokens as u64;
                self.stats.total_output_tokens += *output_tokens as u64;
                self.stats.total_cache_creation_tokens += *cache_creation_tokens as u64;
                self.stats.total_cache_read_tokens += *cache_read_tokens as u64;

                // Update current model (use the most recent one)
                self.stats.current_model = Some(model.clone());

                // Update context window tracking
                self.stats
                    .update_context(*input_tokens as u64, *output_tokens as u64);
            }
            ProxyEvent::Thinking {
                content,
                token_estimate,
                ..
            } => {
                // Track thinking blocks
                self.stats.thinking_blocks += 1;
                self.stats.thinking_tokens += *token_estimate as u64;

                // Store current thinking for the dedicated panel
                self.stats.current_thinking = Some(content.clone());
            }
            _ => {}
        }

        self.events.push(event);

        // Auto-scroll to bottom when new events arrive (only in Main view)
        if self.current_view == View::Main && self.selected == self.events.len().saturating_sub(2) {
            self.selected = self.events.len().saturating_sub(1);
        }
    }

    /// Get the currently selected event
    pub fn selected_event(&self) -> Option<&ProxyEvent> {
        self.events.get(self.selected)
    }

    /// Move selection up
    pub fn select_previous(&mut self) {
        if self.show_detail {
            // In detail view, scroll the detail content
            if self.detail_scroll > 0 {
                self.detail_scroll -= 1;
            }
        } else {
            // In list view, move selection
            if self.selected > 0 {
                self.selected -= 1;
                // Adjust scroll if needed
                if self.selected < self.scroll_offset {
                    self.scroll_offset = self.selected;
                }
            }
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if self.show_detail {
            // In detail view, scroll the detail content
            self.detail_scroll += 1;
        } else {
            // In list view, move selection
            if self.selected < self.events.len().saturating_sub(1) {
                self.selected += 1;
            }
        }
    }

    /// Toggle detail view
    pub fn toggle_detail(&mut self) {
        self.show_detail = !self.show_detail;
        // Reset detail scroll when toggling
        self.detail_scroll = 0;
    }

    /// Get uptime as a formatted string
    pub fn uptime(&self) -> String {
        let elapsed = self.start_time.elapsed();
        let seconds = elapsed.as_secs();
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;

        format!("{:02}:{:02}:{:02}", hours, minutes, secs)
    }

    /// Calculate visible range for the event list given viewport height
    pub fn visible_range(&self, height: usize) -> (usize, usize) {
        let total = self.events.len();
        if total == 0 {
            return (0, 0);
        }

        // Adjust scroll offset to keep selected item visible
        let mut offset = self.scroll_offset;
        if self.selected >= offset + height {
            offset = self.selected.saturating_sub(height - 1);
        } else if self.selected < offset {
            offset = self.selected;
        }

        let start = offset;
        let end = (offset + height).min(total);

        (start, end)
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
