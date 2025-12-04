// TUI application state
//
// This module manages the state of the TUI application, including the list
// of events, selected item, statistics, and UI state.

use super::components::detail_panel::DetailPanel;
use super::components::events_panel::EventsPanel;
use super::components::logs_panel::LogsPanel;
use super::components::settings_panel::SettingsPanel;
// Re-export SettingsCategory (used in settings_apply_option)
pub use super::components::settings_panel::SettingsCategory;
use super::components::thinking_panel::ThinkingPanel;
use super::components::Toast;
use super::input::InputHandler;
use super::modal::Modal;
use super::preset::{get_preset, Preset};
use super::scroll::FocusablePanel;
use super::streaming::StreamingStateMachine;
use super::traits::{Handled, Interactive};
use crate::config::Config;
use crate::events::{ProxyEvent, Stats, TrackedEvent};
use crate::logging::LogBuffer;
use crate::theme::{Theme, ThemeConfig};
use crate::StreamingThinking;
use crossterm::event::KeyEvent;
use std::time::Instant;

// Re-export StreamingState for backward compatibility with ui.rs
pub use super::streaming::StreamingState;

/// Active view in the TUI
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Events,
    Stats,
    Settings,
}

// Note: SettingsCategory, SettingsFocus live in components/settings_panel.rs
// SettingsFocus used only by component; SettingsCategory re-exported for settings_apply_option

/// Topic info extracted from Haiku's summarization
#[derive(Debug, Clone, Default)]
pub struct TopicInfo {
    pub title: Option<String>,
    pub is_new_topic: bool,
}

/// Main application state for the TUI
///
/// # Architecture
///
/// The App struct is the central state container for Aspy's TUI.
/// It's organized into logical groups:
///
/// - **Core Data**: Events received from the proxy, accumulated statistics
/// - **Navigation**: Current view, selection, focus state
/// - **Appearance**: Theme, preset (layout), animations
/// - **Input**: Key handling, debouncing
/// - **Subsystems**: Delegated state (panels, settings, streaming)
///
/// # Usage
///
/// ```ignore
/// let mut app = App::new();
/// app.add_event(event);           // Core: receive proxy events
/// app.set_view(View::Stats);      // Navigation: switch views
/// app.dispatch_to_focused(key);   // Navigation: delegate to panel
/// app.tick_animation();           // Appearance: advance animations
/// ```
///
/// # Extension Points
///
/// - Add new views: extend `View` enum, add rendering in `views/`
/// - Add new themes: add to `theme/` module, register in `modal.rs`
/// - Add new presets: add to `preset.rs`
pub struct App {
    // ─────────────────────────────────────────────────────────────────────────
    // Core Data
    // The primary data this application manages
    // ─────────────────────────────────────────────────────────────────────────
    /// All proxy events received this session (tool calls, responses, etc.)
    pub events: Vec<TrackedEvent>,

    /// Accumulated statistics (tokens, costs, tool calls, etc.)
    pub stats: Stats,

    /// Shared statistics (synced for HTTP API access)
    shared_stats: crate::proxy::api::SharedStats,

    /// Shared events buffer (synced for HTTP API access)
    shared_events: crate::proxy::api::SharedEvents,

    /// Current conversation topic (extracted from Haiku summarization)
    pub topic: TopicInfo,

    /// System log buffer (for the logs panel)
    pub log_buffer: LogBuffer,

    // ─────────────────────────────────────────────────────────────────────────
    // Navigation & Selection
    // Where the user is in the UI and what they're looking at
    // ─────────────────────────────────────────────────────────────────────────
    /// Active view (Events, Stats, Settings)
    pub view: View,

    /// Currently focused panel (receives keyboard input)
    pub focused: FocusablePanel,

    /// Active modal dialog (Help, Detail, etc.) - captures all input when Some
    pub modal: Option<Modal>,

    /// Toast notification (copy confirmation, errors) - auto-dismisses
    pub toast: Option<Toast>,

    /// Selected tab in Stats view (0=Overview, 1=Models, 2=Tokens, 3=Tools, 4=Trends)
    pub stats_selected_tab: usize,

    // ─────────────────────────────────────────────────────────────────────────
    // Appearance & Animation
    // Visual presentation: theme, layout, streaming indicators
    // ─────────────────────────────────────────────────────────────────────────
    /// Color theme for the UI
    pub theme: Theme,

    /// Theme configuration (thinking colors, etc.)
    pub theme_config: ThemeConfig,

    /// Runtime configuration (for persistence on settings changes)
    pub config: Config,

    /// Layout preset (panel arrangement: classic, reasoning, debug)
    pub preset: Preset,

    /// Animation frame counter (for spinners, dots)
    pub animation_frame: usize,

    // ─────────────────────────────────────────────────────────────────────────
    // Input Handling
    // Keyboard event processing and debouncing
    // ─────────────────────────────────────────────────────────────────────────
    /// Input handler (tracks pressed keys, prevents double-triggers)
    input_handler: InputHandler,

    // ─────────────────────────────────────────────────────────────────────────
    // Delegated Subsystems
    // Complex state that's managed by dedicated structs (component pattern)
    // ─────────────────────────────────────────────────────────────────────────
    /// Events panel component (owns its selection + scroll state)
    pub events_panel: EventsPanel,

    /// Logs panel component (owns its scroll + selection state)
    pub logs_panel: LogsPanel,

    /// Thinking panel component (owns its scroll state)
    pub thinking_panel: ThinkingPanel,

    /// Detail panel component (owns its scroll state)
    pub detail_panel: DetailPanel,

    /// Settings panel component (owns all settings view state)
    /// This includes navigation, theme selection, and layout preset selection
    pub settings_panel: SettingsPanel,

    /// Streaming state machine (idle → thinking → generating)
    streaming_sm: StreamingStateMachine,

    /// Real-time streaming thinking content (shared with proxy)
    pub streaming_thinking: Option<StreamingThinking>,

    // ─────────────────────────────────────────────────────────────────────────
    // Lifecycle
    // Application lifecycle state
    // ─────────────────────────────────────────────────────────────────────────
    /// When the app started (for uptime display)
    pub start_time: Instant,

    /// Whether the app should quit
    pub should_quit: bool,
}

impl App {
    // ─────────────────────────────────────────────────────────────
    // Construction
    // ─────────────────────────────────────────────────────────────

    pub fn new() -> Self {
        Self::with_log_buffer(LogBuffer::new())
    }

    pub fn with_log_buffer(log_buffer: LogBuffer) -> Self {
        use crate::events::Stats;
        use crate::proxy::api::EventBuffer;
        use std::sync::{Arc, Mutex};
        // Create dummy shared stats/events for test/convenience constructors
        let shared_stats = Arc::new(Mutex::new(Stats::default()));
        let shared_events = Arc::new(Mutex::new(EventBuffer::new()));
        Self::with_config(log_buffer, Config::default(), shared_stats, shared_events)
    }

    /// Create App with log buffer and config (preferred constructor)
    pub fn with_config(
        log_buffer: LogBuffer,
        config: Config,
        shared_stats: crate::proxy::api::SharedStats,
        shared_events: crate::proxy::api::SharedEvents,
    ) -> Self {
        let theme_config = ThemeConfig {
            use_theme_background: config.use_theme_background,
        };
        let theme = Theme::by_name_with_config(&config.theme, &theme_config);
        let preset = get_preset(&config.preset);

        // Initialize local stats with context limit from config
        let stats = Stats {
            configured_context_limit: config.context_limit,
            ..Default::default()
        };

        Self {
            events: Vec::new(),
            should_quit: false,
            stats,
            shared_stats,
            shared_events,
            start_time: Instant::now(),
            events_panel: EventsPanel::new(), // Start in auto-follow mode
            logs_panel: LogsPanel::new(),
            thinking_panel: ThinkingPanel::new(),
            detail_panel: DetailPanel::new(),
            settings_panel: SettingsPanel::new(),
            input_handler: InputHandler::default(),
            log_buffer,
            topic: TopicInfo::default(),
            view: View::default(),
            focused: FocusablePanel::default(),
            stats_selected_tab: 0, // Default to Overview tab
            theme,
            theme_config,
            config,
            streaming_sm: StreamingStateMachine::new(),
            animation_frame: 0,
            streaming_thinking: None,
            modal: None,
            toast: None,
            preset,
        }
    }

    // ─────────────────────────────────────────────────────────────
    // Streaming & Animation
    // ─────────────────────────────────────────────────────────────

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

    /// Get animated dots for thinking indicator (standard AI "thinking..." pattern)
    pub fn thinking_dots(&self) -> &'static str {
        const DOTS: [&str; 4] = ["", ".", "..", "..."];
        DOTS[self.animation_frame % DOTS.len()]
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

    // ─────────────────────────────────────────────────────────────
    // View & Focus Navigation
    // ─────────────────────────────────────────────────────────────

    /// Set the active view
    pub fn set_view(&mut self, view: View) {
        self.view = view;
        // Reset view-specific state when switching
        self.modal = None; // Close any modal when switching views
        self.focused = FocusablePanel::Events;
        self.detail_panel.reset();

        // When entering Settings, scroll theme list to current theme
        if view == View::Settings {
            let themes = Theme::list_available();
            self.settings_panel
                .scroll_to_current_theme(&themes, &self.theme.name);
        }
    }

    /// Cycle focus to next panel (Tab)
    /// Uses preset's focus_order for layout-appropriate cycling
    pub fn focus_next(&mut self) {
        let order = &self.preset.focus_order;
        if let Some(pos) = order.iter().position(|&p| p == self.focused) {
            let next_pos = (pos + 1) % order.len();
            self.focused = order[next_pos];
        }
    }

    /// Cycle focus to previous panel (Shift+Tab)
    /// Uses preset's focus_order for layout-appropriate cycling
    pub fn focus_prev(&mut self) {
        let order = &self.preset.focus_order;
        if let Some(pos) = order.iter().position(|&p| p == self.focused) {
            let prev_pos = if pos == 0 { order.len() - 1 } else { pos - 1 };
            self.focused = order[prev_pos];
        }
    }

    /// Check if a panel is currently focused
    pub fn is_focused(&self, panel: FocusablePanel) -> bool {
        self.focused == panel
    }

    // ─────────────────────────────────────────────────────────────
    // Settings view navigation (delegated to SettingsPanel)
    // ─────────────────────────────────────────────────────────────

    /// Toggle focus between categories and options in Settings view
    pub fn settings_toggle_focus(&mut self) {
        self.settings_panel.toggle_focus();
    }

    /// Apply the currently selected option in Settings view
    pub fn settings_apply_option(&mut self) {
        match self.settings_panel.category {
            SettingsCategory::Appearance => {
                let themes = Theme::list_available();
                let selected = self.settings_panel.selected_theme_index();

                if selected < themes.len() {
                    // Apply selected theme
                    if let Some(theme_name) = themes.get(selected) {
                        self.theme = Theme::by_name_with_config(theme_name, &self.theme_config);
                        self.config.theme = theme_name.clone();
                        self.settings_panel.mark_dirty();
                    }
                } else {
                    // Toggle background setting (last item)
                    self.config.use_theme_background = !self.config.use_theme_background;
                    self.theme_config.use_theme_background = self.config.use_theme_background;
                    self.theme = Theme::by_name_with_config(&self.config.theme, &self.theme_config);
                    self.settings_panel.mark_dirty();
                }
            }
            SettingsCategory::Layout => {
                // Apply selected preset
                let preset_names = ["classic", "reasoning", "debug"];
                if let Some(&preset_name) =
                    preset_names.get(self.settings_panel.layout_option_index)
                {
                    self.preset = get_preset(preset_name);
                    self.config.preset = preset_name.to_string();
                    self.settings_panel.mark_dirty();
                }
            }
        }
    }

    /// Save settings to config file if any changes were made
    pub fn save_settings_if_dirty(&mut self) {
        if self.settings_panel.is_dirty() {
            match self.config.save() {
                Ok(()) => self.show_toast("✓ Settings saved"),
                Err(e) => self.show_toast(format!("✗ Save failed: {}", e)),
            }
            self.settings_panel.clear_dirty();
        }
    }

    // ─────────────────────────────────────────────────────────────
    // Input Handling
    // ─────────────────────────────────────────────────────────────

    /// Handle a key press - returns true if the action should be triggered
    /// Uses the configured behavior for each key (state-change or repeatable)
    pub fn handle_key_press(&mut self, key: crossterm::event::KeyCode) -> bool {
        self.input_handler.handle_key_press(key)
    }

    /// Handle a key release
    pub fn handle_key_release(&mut self, key: crossterm::event::KeyCode) {
        self.input_handler.handle_key_release(key);
    }

    // ─────────────────────────────────────────────────────────────
    // Event Processing
    // ─────────────────────────────────────────────────────────────

    /// Add a new event and update statistics
    pub fn add_event(&mut self, tracked_event: TrackedEvent) {
        // Set session start time on first event
        if self.stats.session_started.is_none() {
            self.stats.session_started = Some(chrono::Utc::now());
        }

        // Extract the inner ProxyEvent for stats processing
        let event = &tracked_event.event;

        // Update statistics based on event type
        // First, populate historical ring buffers for sparklines
        self.stats.update_history(event);

        // Then, handle aggregate stats and TUI-specific state updates
        match event {
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
                //
                // Context = input + cache_creation + cache_read because:
                // - When cache is being CREATED, those tokens appear in cache_creation (not cache_read)
                // - This prevents false "green" status during cache warmup
                let is_haiku = model.contains("haiku");
                if !is_haiku {
                    self.stats.current_context_tokens = *input_tokens as u64
                        + *cache_creation_tokens as u64
                        + *cache_read_tokens as u64;
                    self.stats.last_cached_tokens = *cache_read_tokens as u64;
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

        // Log milestones to system log panel
        self.check_milestones(event);

        // Sync local stats to shared stats for HTTP API access
        if let Ok(mut shared) = self.shared_stats.lock() {
            *shared = self.stats.clone();
        }

        // Sync event to shared buffer for HTTP API access (raw ProxyEvent for API compatibility)
        if let Ok(mut shared) = self.shared_events.lock() {
            shared.push(event.clone());
        }

        // Store the full TrackedEvent (includes user_id, session_id for filtering)
        self.events.push(tracked_event);
        // In auto-follow mode (None), we don't need to track selection
        // The view will always show the latest events
    }

    /// Check and log milestone events to the system log panel
    /// This adds personality and useful info as events flow through
    fn check_milestones(&self, event: &ProxyEvent) {
        // First request - connection established!
        if self.stats.total_requests == 1 && matches!(event, ProxyEvent::Request { .. }) {
            tracing::info!("🎯 First contact! Claude Code connected.");
        }

        // First tool call
        if self.stats.total_tool_calls == 1 && matches!(event, ProxyEvent::ToolCall { .. }) {
            tracing::info!("🔧 First tool call intercepted.");
        }

        // Tool call milestones (10, 25, 50, 100, ...)
        if matches!(event, ProxyEvent::ToolCall { .. }) {
            match self.stats.total_tool_calls {
                10 => tracing::info!("📊 Milestone: 10 tool calls"),
                25 => tracing::info!("📊 Milestone: 25 tool calls"),
                50 => tracing::info!("📊 Milestone: 50 tool calls"),
                100 => tracing::info!("🎉 Milestone: 100 tool calls!"),
                250 => tracing::info!("🔥 Milestone: 250 tool calls!"),
                500 => tracing::info!("🚀 Milestone: 500 tool calls!"),
                _ => {}
            }
        }

        // First thinking block - extended thinking active
        if self.stats.thinking_blocks == 1 && matches!(event, ProxyEvent::Thinking { .. }) {
            tracing::info!("💭 Extended thinking detected.");
        }

        // Model detection and cache tips on ApiUsage
        if let ProxyEvent::ApiUsage { model, .. } = event {
            // First API usage - show which model is active
            if self.stats.model_calls.is_empty()
                || self.stats.model_calls.values().sum::<u32>() == 1
            {
                let model_short = if model.contains("opus") {
                    "Opus"
                } else if model.contains("sonnet") {
                    "Sonnet"
                } else if model.contains("haiku") {
                    "Haiku"
                } else {
                    model.as_str()
                };
                tracing::info!("🤖 Model detected: {}", model_short);
            }

            // Cache efficiency tips (after some data)
            let cache_rate = self.stats.cache_hit_rate();
            if self.stats.total_requests == 5 {
                if cache_rate >= 90.0 {
                    tracing::info!("✨ Cache efficiency: {:.0}% - excellent!", cache_rate);
                } else if cache_rate < 50.0 && self.stats.total_cache_read_tokens > 0 {
                    tracing::info!("💡 Cache efficiency: {:.0}% - could improve", cache_rate);
                }
            }
        }

        // Context compaction detected
        if matches!(event, ProxyEvent::ContextCompact { .. }) {
            tracing::info!("📦 Context compaction triggered.");
        }

        // Cost milestones
        if let ProxyEvent::ApiUsage { .. } = event {
            let cost = self.stats.total_cost();
            // Round to nearest cent for comparison
            let cost_cents = (cost * 100.0).round() as u32;
            match cost_cents {
                100 => tracing::info!("💰 Cost milestone: $1.00"),
                500 => tracing::info!("💰 Cost milestone: $5.00"),
                1000 => tracing::info!("💰 Cost milestone: $10.00"),
                _ => {}
            }
        }
    }

    // ─────────────────────────────────────────────────────────────
    // Selection & Scrolling (Trait-based dispatch)
    // ─────────────────────────────────────────────────────────────

    /// Dispatch a key event to the currently focused panel via the Interactive trait
    ///
    /// This is the primary dispatch mechanism for keyboard events.
    /// Each panel implements Interactive::handle_key() with its own behavior.
    ///
    /// Returns Handled::Yes if the panel consumed the event, Handled::No if not.
    pub fn dispatch_to_focused(&mut self, key: KeyEvent) -> Handled {
        // Settings view has its own panel structure
        if self.view == View::Settings {
            return self.dispatch_to_settings(key);
        }

        // Events/Stats view: dispatch based on focused panel
        match self.focused {
            FocusablePanel::Events => {
                self.events_panel.sync_events(self.events.len());
                self.events_panel.handle_key(key)
            }
            FocusablePanel::Thinking => self.thinking_panel.handle_key(key),
            FocusablePanel::Logs => {
                self.logs_panel.entry_count = self.log_buffer.get_all().len();
                self.logs_panel.handle_key(key)
            }
        }
    }

    /// Dispatch key events within Settings view
    /// Now fully delegated to SettingsPanel component
    fn dispatch_to_settings(&mut self, key: KeyEvent) -> Handled {
        // Sync theme count before handling (for proper bounds)
        let themes = Theme::list_available();
        self.settings_panel.sync_themes(themes.len(), 20); // viewport hint

        // Delegate all key handling to the component
        self.settings_panel.handle_key(key)
    }

    // ─────────────────────────────────────────────────────────────
    // Toast Notifications
    // ─────────────────────────────────────────────────────────────

    /// Show a toast notification (auto-dismisses after 2 seconds)
    pub fn show_toast(&mut self, message: impl Into<String>) {
        self.toast = Some(Toast::new(message));
    }

    /// Clear the toast if it has expired
    pub fn clear_expired_toast(&mut self) {
        if let Some(ref toast) = self.toast {
            if toast.is_expired() {
                self.toast = None;
            }
        }
    }

    // ─────────────────────────────────────────────────────────────
    // Copy Operations
    // ─────────────────────────────────────────────────────────────

    /// Get human-readable text to copy based on current focus
    ///
    /// Returns formatted text appropriate for the currently focused panel:
    /// - Detail panel: Full event details
    /// - Events list: Selected event summary
    /// - Thinking panel: Current thinking content
    /// - Logs panel: Selected log entry
    pub fn copy_current_readable(&self) -> Option<String> {
        // Note: Detail modal handles its own copy via modal input
        match self.focused {
            FocusablePanel::Events => {
                // Delegate to component
                self.events_panel.copy_text_with_events(&self.events)
            }
            FocusablePanel::Thinking => {
                // Copy current thinking content
                self.current_thinking_content()
            }
            FocusablePanel::Logs => {
                // Delegate to component
                let entries = self.log_buffer.get_all();
                self.logs_panel.selected_entry_text(&entries)
            }
        }
    }

    /// Get JSONL representation of current event for copying
    ///
    /// Returns serialized JSON for the selected event.
    /// Only applicable when Events panel is focused.
    pub fn copy_current_jsonl(&self) -> Option<String> {
        // JSONL only makes sense for events (modal handles its own copy)
        if self.focused == FocusablePanel::Events {
            self.events_panel.copy_data_with_events(&self.events)
        } else {
            None
        }
    }

    // ─────────────────────────────────────────────────────────────
    // Utilities
    // ─────────────────────────────────────────────────────────────

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
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
