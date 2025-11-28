// TUI application state
//
// This module manages the state of the TUI application, including the list
// of events, selected item, statistics, and UI state.

use super::input::InputHandler;
use super::scroll::{FocusablePanel, PanelStates};
use super::streaming::StreamingStateMachine;
use crate::events::{ProxyEvent, Stats};
use crate::logging::LogBuffer;
use crate::theme::Theme;
use crate::StreamingThinking;
use std::time::{Duration, Instant};

// Re-export StreamingState for backward compatibility with ui.rs
pub use super::streaming::StreamingState;

/// Debounce duration for action keys (Enter, Esc, q)
/// Prevents rapid-fire triggers on terminals that don't send release events
const ACTION_DEBOUNCE: Duration = Duration::from_millis(150);

/// Active view in the TUI
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Events,
    Stats,
    Help,
}

/// Topic info extracted from Haiku's summarization
#[derive(Debug, Clone, Default)]
pub struct TopicInfo {
    pub title: Option<String>,
    pub is_new_topic: bool,
}

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

    /// Scroll state for all panels (events, detail, thinking, logs)
    pub panels: PanelStates,

    /// Input handler for flexible key behavior
    input_handler: InputHandler,

    /// Log buffer for system logs display
    pub log_buffer: LogBuffer,

    /// Last time an action key was triggered (for debouncing)
    last_action_time: Option<Instant>,

    /// Current conversation topic (from Haiku summarization)
    pub topic: TopicInfo,

    /// Active view
    pub view: View,

    /// Currently focused panel (receives scroll input)
    pub focused: FocusablePanel,

    /// Color theme for the UI
    pub theme: Theme,

    /// Streaming state machine (for header animation)
    streaming_sm: StreamingStateMachine,

    /// Animation frame counter (increments each render tick)
    pub animation_frame: usize,

    /// Shared buffer for real-time streaming thinking content
    /// Proxy writes to this, TUI reads from it each frame
    pub streaming_thinking: Option<StreamingThinking>,
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
            panels: PanelStates::default(),
            input_handler: InputHandler::default(),
            log_buffer,
            last_action_time: None,
            topic: TopicInfo::default(),
            view: View::default(),
            focused: FocusablePanel::default(),
            theme: Theme::default(),
            streaming_sm: StreamingStateMachine::new(),
            animation_frame: 0,
            streaming_thinking: None,
        }
    }

    /// Get current streaming state (for UI display)
    pub fn streaming_state(&self) -> StreamingState {
        self.streaming_sm.state()
    }

    /// Advance animation frame (call on each render tick)
    pub fn tick_animation(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);
    }

    /// Get current spinner frame for animations
    pub fn spinner_char(&self) -> char {
        const SPINNER: [char; 4] = ['◐', '◓', '◑', '◒'];
        SPINNER[self.animation_frame % SPINNER.len()]
    }

    /// Get current thinking content for display
    /// Returns streaming content if available, otherwise last completed thinking block
    pub fn current_thinking_content(&self) -> Option<String> {
        // First try streaming content (real-time)
        if let Some(ref streaming) = self.streaming_thinking {
            if let Ok(guard) = streaming.lock() {
                if !guard.is_empty() {
                    return Some(guard.clone());
                }
            }
        }
        // Fall back to completed thinking
        self.stats.current_thinking.clone()
    }

    /// Check if there's any thinking content to display
    pub fn has_thinking_content(&self) -> bool {
        // Check streaming buffer first
        if let Some(ref streaming) = self.streaming_thinking {
            if let Ok(guard) = streaming.lock() {
                if !guard.is_empty() {
                    return true;
                }
            }
        }
        // Fall back to completed thinking
        self.stats.current_thinking.is_some()
    }

    /// Set the active view
    pub fn set_view(&mut self, view: View) {
        self.view = view;
        // Reset view-specific state when switching
        self.show_detail = false;
        self.focused = FocusablePanel::Events;
        self.panels.detail.scroll_to_top();
    }

    /// Cycle focus to next panel (Tab)
    pub fn focus_next(&mut self) {
        self.focused = self.focused.next();
    }

    /// Cycle focus to previous panel (Shift+Tab)
    pub fn focus_prev(&mut self) {
        self.focused = self.focused.prev();
    }

    /// Check if a panel is currently focused
    pub fn is_focused(&self, panel: FocusablePanel) -> bool {
        self.focused == panel
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
        // Set session start time on first event
        if self.stats.session_started.is_none() {
            self.stats.session_started = Some(chrono::Utc::now());
        }

        // Update statistics based on event type
        match &event {
            ProxyEvent::Request { .. } => {
                self.stats.total_requests += 1;
                self.streaming_sm.on_request();
            }
            ProxyEvent::Response {
                status, ttfb, body, ..
            } => {
                // Track TTFB for latency monitoring
                self.stats.total_ttfb += *ttfb;
                self.stats.response_count += 1;

                // Track failures - success is derived from (total - failed)
                // This avoids false success rate dips during pending requests
                if !(200..300).contains(status) {
                    self.stats.failed_requests += 1;
                }

                // Extract topic from Haiku summarization responses
                if let Some(topic_info) = Self::extract_topic_from_response(body) {
                    self.topic = topic_info;
                }

                self.streaming_sm.on_response();
            }
            ProxyEvent::ToolCall { tool_name, .. } => {
                self.stats.total_tool_calls += 1;
                // Track tool calls by name for distribution
                *self
                    .stats
                    .tool_calls_by_name
                    .entry(tool_name.clone())
                    .or_insert(0) += 1;

                self.streaming_sm.on_tool_call(tool_name);
            }
            ProxyEvent::ToolResult {
                tool_name,
                duration,
                success,
                ..
            } => {
                // Track tool execution duration in milliseconds
                let duration_ms = duration.as_millis() as u64;
                self.stats
                    .tool_durations_ms
                    .entry(tool_name.clone())
                    .or_default()
                    .push(duration_ms);

                // Track failed/rejected tool calls
                if !success {
                    self.stats.failed_tool_calls += 1;
                }

                self.streaming_sm.on_tool_result();
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

                // Track context only for non-Haiku models (Opus/Sonnet carry the conversation)
                // Haiku is used for quick side-tasks and doesn't reflect actual context usage
                // Note: Compact detection moved to Parser layer for proper logging
                let is_haiku = model.contains("haiku");
                if !is_haiku {
                    let cache = *cache_read_tokens as u64;
                    self.stats.current_context_tokens = *input_tokens as u64 + cache;
                    self.stats.last_cached_tokens = cache;
                }

                // Track model calls for distribution
                *self.stats.model_calls.entry(model.clone()).or_insert(0) += 1;

                // Track per-model token usage
                let model_tokens = self.stats.model_tokens.entry(model.clone()).or_default();
                model_tokens.input += *input_tokens as u64;
                model_tokens.output += *output_tokens as u64;
                model_tokens.cache_read += *cache_read_tokens as u64;
                model_tokens.cache_creation += *cache_creation_tokens as u64;
                model_tokens.calls += 1;

                self.streaming_sm.on_api_usage();
            }
            ProxyEvent::Thinking {
                content,
                token_estimate,
                ..
            } => {
                // Track thinking blocks (stats only - no state transition)
                // This event arrives post-stream from the parser with complete content.
                // ThinkingStarted handles real-time state; ApiUsage is the terminal event.
                self.stats.thinking_blocks += 1;
                self.stats.thinking_tokens += *token_estimate as u64;

                // Store current thinking for the dedicated panel
                self.stats.current_thinking = Some(content.clone());
            }
            ProxyEvent::ThinkingStarted { .. } => {
                self.streaming_sm.on_thinking_started();
            }
            ProxyEvent::ContextCompact { new_context, .. } => {
                // Context was compacted - update stats
                self.stats.compact_count += 1;
                self.stats.current_context_tokens = *new_context;
                self.stats.last_cached_tokens = 0;
            }
            _ => {}
        }

        // Skip ThinkingStarted (just a spinner signal) - but keep Thinking events
        // so completed thinking blocks appear in the list for inspection
        if matches!(event, ProxyEvent::ThinkingStarted { .. }) {
            return;
        }

        self.events.push(event);

        // Auto-scroll to bottom when new events arrive
        if self.selected == self.events.len().saturating_sub(2) {
            self.selected = self.events.len().saturating_sub(1);
        }
    }

    /// Get the currently selected event
    pub fn selected_event(&self) -> Option<&ProxyEvent> {
        self.events.get(self.selected)
    }

    /// Scroll up / select previous based on focused panel
    pub fn select_previous(&mut self) {
        match self.focused {
            FocusablePanel::Events => {
                // Events panel: move selection (not scroll)
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            FocusablePanel::Detail => self.panels.detail.scroll_up(),
            FocusablePanel::Thinking => self.panels.thinking.scroll_up(),
            FocusablePanel::Logs => self.panels.logs.scroll_up(),
        }
    }

    /// Scroll down / select next based on focused panel
    pub fn select_next(&mut self) {
        match self.focused {
            FocusablePanel::Events => {
                // Events panel: move selection (not scroll)
                if self.selected < self.events.len().saturating_sub(1) {
                    self.selected += 1;
                }
            }
            FocusablePanel::Detail => self.panels.detail.scroll_down(),
            FocusablePanel::Thinking => self.panels.thinking.scroll_down(),
            FocusablePanel::Logs => self.panels.logs.scroll_down(),
        }
    }

    /// Toggle detail view and switch focus
    pub fn toggle_detail(&mut self) {
        self.show_detail = !self.show_detail;
        if self.show_detail {
            // Entering detail: focus it and reset scroll
            self.focused = FocusablePanel::Detail;
            self.panels.detail.scroll_to_top();
        } else {
            // Exiting detail: return focus to events
            self.focused = FocusablePanel::Events;
        }
    }

    /// Jump to top based on focused panel
    pub fn scroll_to_top(&mut self) {
        match self.focused {
            FocusablePanel::Events => self.selected = 0,
            FocusablePanel::Detail => self.panels.detail.scroll_to_top(),
            FocusablePanel::Thinking => self.panels.thinking.scroll_to_top(),
            FocusablePanel::Logs => self.panels.logs.scroll_to_top(),
        }
    }

    /// Jump to bottom based on focused panel
    pub fn scroll_to_bottom(&mut self) {
        match self.focused {
            FocusablePanel::Events => {
                if !self.events.is_empty() {
                    self.selected = self.events.len() - 1;
                }
            }
            FocusablePanel::Detail => self.panels.detail.scroll_to_bottom(),
            FocusablePanel::Thinking => self.panels.thinking.scroll_to_bottom(),
            FocusablePanel::Logs => self.panels.logs.scroll_to_bottom(),
        }
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

    /// Extract topic info from a Haiku response body
    fn extract_topic_from_response(body: &Option<serde_json::Value>) -> Option<TopicInfo> {
        let body = body.as_ref()?;

        // Check if this is a Haiku model response
        let model = body.get("model")?.as_str()?;
        if !model.contains("haiku") {
            return None;
        }

        // Get the text content: body.content[0].text
        let content = body.get("content")?.as_array()?;
        let first = content.first()?;
        let text = first.get("text")?.as_str()?;

        // Parse the JSON from the text
        // Haiku sometimes returns JSON without opening brace, so we fix it up
        let trimmed = text.trim();
        let json_str = if trimmed.starts_with('{') {
            trimmed.to_string()
        } else if trimmed.contains("isNewTopic") {
            format!("{{{}", trimmed)
        } else {
            return None;
        };
        let topic_json: serde_json::Value = serde_json::from_str(&json_str).ok()?;

        let title = topic_json
            .get("title")
            .and_then(|v| v.as_str())
            .map(String::from);
        let is_new_topic = topic_json
            .get("isNewTopic")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Some(TopicInfo {
            title,
            is_new_topic,
        })
    }

    /// Calculate visible range for the event list given viewport height
    /// Keeps selected item visible by computing scroll offset from selection
    pub fn visible_range(&self, height: usize) -> (usize, usize) {
        let total = self.events.len();
        if total == 0 {
            return (0, 0);
        }

        // Compute offset to keep selected item visible
        // Selection drives scroll position (selection-based scrolling)
        let offset = if self.selected >= height {
            self.selected.saturating_sub(height - 1)
        } else {
            0
        };

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
