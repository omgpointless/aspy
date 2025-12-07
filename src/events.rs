// Events that flow from the proxy to the TUI and storage components
//
// These events represent the parsed tool calls and responses that we extract
// from the Anthropic API traffic. Using an enum allows pattern matching and
// ensures type-safe communication between async tasks.

use crate::parser::models::CapturedHeaders;
use crate::tokens::{AugmentStats, TransformStats};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Main event type that flows through the application
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")] // Creates JSON like {"type": "tool_call", ...}
pub enum ProxyEvent {
    /// A tool call was requested by Claude
    ToolCall {
        id: String,
        timestamp: DateTime<Utc>,
        tool_name: String,
        input: serde_json::Value,
    },

    /// A tool call returned a result
    ToolResult {
        id: String,
        timestamp: DateTime<Utc>,
        tool_name: String,
        output: serde_json::Value,
        duration: Duration,
        success: bool,
    },

    /// An HTTP request passed through the proxy
    Request {
        id: String,
        timestamp: DateTime<Utc>,
        method: String,
        path: String,
        body_size: usize,
        body: Option<serde_json::Value>, // Parsed request body
    },

    /// An HTTP response passed through the proxy
    Response {
        request_id: String,
        timestamp: DateTime<Utc>,
        status: u16,
        body_size: usize,
        /// Time to first byte - how long until the API started responding
        ttfb: Duration,
        /// Total duration - full request-to-response-complete time
        duration: Duration,
        body: Option<serde_json::Value>, // Parsed response body (or assembled from SSE)
    },

    /// An error occurred during proxying or parsing
    Error {
        timestamp: DateTime<Utc>,
        message: String,
        context: Option<String>,
    },

    /// HTTP headers captured from request/response
    HeadersCaptured {
        request_id: String,
        timestamp: DateTime<Utc>,
        headers: CapturedHeaders,
    },

    /// Rate limit information updated
    RateLimitUpdate {
        timestamp: DateTime<Utc>,
        requests_remaining: Option<u32>,
        requests_limit: Option<u32>,
        tokens_remaining: Option<u32>,
        tokens_limit: Option<u32>,
        reset_time: Option<String>,
    },

    /// API usage information captured from response
    ApiUsage {
        timestamp: DateTime<Utc>,
        model: String,
        input_tokens: u32,
        output_tokens: u32,
        cache_creation_tokens: u32,
        cache_read_tokens: u32,
    },

    /// Claude's thinking/reasoning block (extended thinking feature)
    Thinking {
        timestamp: DateTime<Utc>,
        content: String,
        /// Approximate token count (content.len() / 4)
        token_estimate: u32,
    },

    /// Context window was compacted (detected by cache dropping to 0)
    ContextCompact {
        timestamp: DateTime<Utc>,
        /// Context size before compact
        previous_context: u64,
        /// Context size after compact (from the triggering call)
        new_context: u64,
        /// Breakdown of what changed (if snapshots available)
        breakdown: Option<crate::parser::models::ContextSnapshotDiff>,
    },

    /// Thinking block started (emitted immediately for real-time feedback)
    ThinkingStarted { timestamp: DateTime<Utc> },

    /// User's prompt extracted from request
    UserPrompt {
        timestamp: DateTime<Utc>,
        content: String,
    },

    /// Assistant's (Claude's) text response
    AssistantResponse {
        timestamp: DateTime<Utc>,
        content: String,
    },

    /// Request was transformed (tokens added/removed)
    RequestTransformed {
        timestamp: DateTime<Utc>,
        /// Name of the transformer
        transformer: String,
        /// Tokens before transformation
        tokens_before: u32,
        /// Tokens after transformation
        tokens_after: u32,
        /// Human-readable descriptions of modifications made
        modifications: Vec<String>,
    },

    /// Response was augmented (tokens injected)
    ResponseAugmented {
        timestamp: DateTime<Utc>,
        /// Name of the augmenter
        augmenter: String,
        /// Tokens injected
        tokens_injected: u32,
    },

    /// PreCompact hook was triggered (before context compaction)
    ///
    /// Fired by Claude Code's PreCompact hook before /compact runs.
    /// Useful for tracking compact timeline and detecting "ghost compacts".
    PreCompactHook {
        timestamp: DateTime<Utc>,
        /// "manual" (user ran /compact) or "auto" (context window full)
        trigger: String,
    },

    /// Context recovery detected (Claude Code crunched tool_results)
    ///
    /// Fired when we detect a significant drop in context size between requests,
    /// indicating Claude Code trimmed tool_result content to free up space.
    /// This is different from /compact - it's automatic context management.
    ContextRecovery {
        timestamp: DateTime<Utc>,
        /// Estimated tokens before recovery
        tokens_before: u32,
        /// Estimated tokens after recovery
        tokens_after: u32,
        /// Percentage of context recovered
        percent_recovered: f32,
    },

    /// Todo list snapshot from TodoWrite tool call
    ///
    /// Captured when Claude calls TodoWrite to update its task list.
    /// Stored in cortex for cross-session recall and context recovery.
    TodoSnapshot {
        timestamp: DateTime<Utc>,
        /// The todo list as JSON string (serialized Vec<TodoItem>)
        todos_json: String,
        /// Count of todos by status for quick queries
        pending_count: u32,
        in_progress_count: u32,
        completed_count: u32,
    },

    /// Context window estimate from historical data
    ///
    /// Emitted when a session resumes and we have historical context data.
    /// This allows the TUI to show the estimated context immediately instead
    /// of "waiting for API call".
    ContextEstimate {
        timestamp: DateTime<Utc>,
        /// Estimated context tokens from last known API usage
        estimated_tokens: u64,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Tracked Event (Envelope for user/session context)
// ─────────────────────────────────────────────────────────────────────────────

/// An event wrapped with user and session context for filtering and tracking.
///
/// This envelope pattern allows us to:
/// - Filter events by user_id (client identity like "foundry", "dev-1")
/// - Track session_id for display/resume purposes
/// - Extend later with OpenTelemetry trace context
///
/// The inner `event` is the actual ProxyEvent (ToolCall, Response, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedEvent {
    /// User identity (client ID from URL path, or API key hash fallback)
    /// Examples: "foundry", "dev-1", "a3f2c91b"
    pub user_id: Option<String>,

    /// Claude Code's session ID (transient, changes on /compact or restart)
    /// Used for display purposes (copy to /resume), not for filtering
    pub session_id: Option<String>,

    /// When the event was tracked (may differ slightly from inner event timestamp)
    pub tracked_at: DateTime<Utc>,

    /// The actual proxy event
    #[serde(flatten)]
    pub event: ProxyEvent,
}

impl TrackedEvent {
    /// Create a new tracked event with user context
    pub fn new(event: ProxyEvent, user_id: Option<String>, session_id: Option<String>) -> Self {
        Self {
            user_id,
            session_id,
            tracked_at: Utc::now(),
            event,
        }
    }

    /// Create a tracked event with no user context (anonymous/unknown)
    ///
    /// Used for events where user routing context isn't available:
    /// - Demo mode synthetic events
    /// - Test fixtures
    /// - Error events before client identification
    #[allow(dead_code)]
    pub fn anonymous(event: ProxyEvent) -> Self {
        Self::new(event, None, None)
    }

    /// Get the inner event's timestamp (for display/sorting)
    ///
    /// Extracts the timestamp from the wrapped ProxyEvent variant.
    /// Used for:
    /// - Event list sorting when filtered by user_id
    /// - Time-based grouping in session views
    #[allow(dead_code)]
    pub fn event_timestamp(&self) -> DateTime<Utc> {
        match &self.event {
            ProxyEvent::ToolCall { timestamp, .. }
            | ProxyEvent::ToolResult { timestamp, .. }
            | ProxyEvent::Request { timestamp, .. }
            | ProxyEvent::Response { timestamp, .. }
            | ProxyEvent::Error { timestamp, .. }
            | ProxyEvent::HeadersCaptured { timestamp, .. }
            | ProxyEvent::RateLimitUpdate { timestamp, .. }
            | ProxyEvent::ApiUsage { timestamp, .. }
            | ProxyEvent::Thinking { timestamp, .. }
            | ProxyEvent::ContextCompact { timestamp, .. }
            | ProxyEvent::ThinkingStarted { timestamp }
            | ProxyEvent::UserPrompt { timestamp, .. }
            | ProxyEvent::AssistantResponse { timestamp, .. }
            | ProxyEvent::RequestTransformed { timestamp, .. }
            | ProxyEvent::ResponseAugmented { timestamp, .. }
            | ProxyEvent::PreCompactHook { timestamp, .. }
            | ProxyEvent::ContextRecovery { timestamp, .. }
            | ProxyEvent::TodoSnapshot { timestamp, .. }
            | ProxyEvent::ContextEstimate { timestamp, .. } => *timestamp,
        }
    }
}

/// Summary statistics for the status bar
#[derive(Debug, Clone)]
pub struct Stats {
    pub total_requests: usize,
    pub failed_requests: usize,
    pub total_tool_calls: usize,
    pub failed_tool_calls: usize,
    /// Accumulated TTFB (time to first byte) for averaging
    pub total_ttfb: Duration,
    /// Count of responses (for TTFB averaging)
    pub response_count: usize,

    // Token usage tracking
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,

    /// Number of context compacts detected this session
    pub compact_count: usize,

    // Thinking block tracking
    pub thinking_blocks: usize,
    pub thinking_tokens: u64,

    /// Session-level turn count (increments on fresh user prompts, persists across compaction)
    /// A "fresh" turn = user prompt with no tool_result blocks (not a tool continuation)
    pub turn_count: u64,

    // === Distribution tracking for Statistics view ===
    /// API calls per model: "claude-opus-4-5-20251101" -> 65
    pub model_calls: HashMap<String, u32>,

    /// Token usage per model for detailed breakdown
    pub model_tokens: HashMap<String, ModelTokens>,

    /// Tool calls by name: "Read" -> 18, "Edit" -> 10
    pub tool_calls_by_name: HashMap<String, u32>,

    /// Tool execution durations for timing analysis
    /// Stores durations in milliseconds to avoid Duration in HashMap
    pub tool_durations_ms: HashMap<String, Vec<u64>>,

    // === Historical data for trend visualization (Sparklines) ===
    /// Token usage snapshots (last 30 data points)
    pub token_history: VecDeque<TokenSnapshot>,

    /// Tool call frequency over time (last 30 data points)
    pub tool_call_history: VecDeque<u32>,

    /// Cache hit rate history (last 30 data points)
    pub cache_rate_history: VecDeque<f64>,

    /// Thinking token progression (last 30 data points)
    pub thinking_token_history: VecDeque<u64>,

    // === Aspy token modification tracking ===
    /// Statistics for request transformations (tokens removed/added)
    pub transform_stats: TransformStats,
    /// Statistics for response augmentations (tokens injected)
    pub augment_stats: AugmentStats,
}

/// Per-model token tracking for Statistics view
#[derive(Debug, Clone, Default)]
pub struct ModelTokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub calls: u32,
}

/// Snapshot of token usage at a point in time for sparkline trends
#[derive(Debug, Clone)]
pub struct TokenSnapshot {
    #[allow(dead_code)] // Used for sparkline x-axis when trends feature lands
    pub timestamp: Instant,
    pub input: u64,
    pub output: u64,
    pub cached: u64,
}

impl Stats {
    /// Returns the percentage of HTTP requests that succeeded (non-error status)
    /// Calculated as (total - failed) / total to avoid false dips during pending requests
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            100.0 // No requests yet = nothing has failed
        } else {
            ((self.total_requests - self.failed_requests) as f64 / self.total_requests as f64)
                * 100.0
        }
    }

    /// Average time to first byte across all API responses
    pub fn avg_ttfb(&self) -> Duration {
        if self.response_count == 0 {
            Duration::default()
        } else {
            self.total_ttfb / self.response_count as u32
        }
    }

    /// Get total tokens used (all types combined)
    pub fn total_tokens(&self) -> u64 {
        self.total_input_tokens
            + self.total_output_tokens
            + self.total_cache_creation_tokens
            + self.total_cache_read_tokens
    }

    /// Calculate total cost across all models used
    pub fn total_cost(&self) -> f64 {
        self.model_tokens
            .iter()
            .map(|(model, tokens)| {
                crate::pricing::calculate_cost(
                    model,
                    tokens.input as u32,
                    tokens.output as u32,
                    tokens.cache_creation as u32,
                    tokens.cache_read as u32,
                )
            })
            .sum()
    }

    /// Calculate cache savings across all models used
    pub fn cache_savings(&self) -> f64 {
        self.model_tokens
            .iter()
            .map(|(model, tokens)| {
                crate::pricing::calculate_cache_savings(model, tokens.cache_read as u32)
            })
            .sum()
    }

    /// Calculate cache hit percentage (cached / (cached + input))
    pub fn cache_hit_rate(&self) -> f64 {
        let total_input = self.total_input_tokens + self.total_cache_read_tokens;
        if total_input == 0 {
            0.0
        } else {
            (self.total_cache_read_tokens as f64 / total_input as f64) * 100.0
        }
    }

    /// Update ONLY historical ring buffers (for TUI use)
    /// The TUI handles aggregate stats manually for TUI-specific logic
    pub fn update_history(&mut self, event: &ProxyEvent) {
        match event {
            ProxyEvent::ToolCall { .. } => {
                // === Historical tracking for sparklines ===
                self.tool_call_history
                    .push_back(self.total_tool_calls as u32);
                if self.tool_call_history.len() > 30 {
                    self.tool_call_history.pop_front();
                }
            }
            ProxyEvent::ApiUsage {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                ..
            } => {
                // === Historical tracking for sparklines ===
                // Add token snapshot
                self.token_history.push_back(TokenSnapshot {
                    timestamp: Instant::now(),
                    input: *input_tokens as u64,
                    output: *output_tokens as u64,
                    cached: *cache_read_tokens as u64,
                });
                if self.token_history.len() > 30 {
                    self.token_history.pop_front();
                }

                // Add cache hit rate snapshot
                let cache_rate = self.cache_hit_rate();
                self.cache_rate_history.push_back(cache_rate);
                if self.cache_rate_history.len() > 30 {
                    self.cache_rate_history.pop_front();
                }
            }
            ProxyEvent::Thinking { token_estimate, .. } => {
                // === Historical tracking for sparklines ===
                self.thinking_token_history
                    .push_back(*token_estimate as u64);
                if self.thinking_token_history.len() > 30 {
                    self.thinking_token_history.pop_front();
                }
            }
            _ => {}
        }
    }

    /// Update stats based on a proxy event
    pub fn update(&mut self, event: &ProxyEvent) {
        match event {
            ProxyEvent::Request { .. } => {
                self.total_requests += 1;
            }
            ProxyEvent::Response { status, ttfb, .. } => {
                if *status >= 400 {
                    self.failed_requests += 1;
                }
                self.total_ttfb += *ttfb;
                self.response_count += 1;
            }
            ProxyEvent::ToolCall { .. } => {
                self.total_tool_calls += 1;

                // === Historical tracking for sparklines ===
                // Track cumulative tool calls over time
                self.tool_call_history
                    .push_back(self.total_tool_calls as u32);
                if self.tool_call_history.len() > 30 {
                    self.tool_call_history.pop_front();
                }
            }
            ProxyEvent::ToolResult {
                tool_name,
                success,
                duration,
                ..
            } => {
                if !success {
                    self.failed_tool_calls += 1;
                }
                // Track tool durations
                self.tool_durations_ms
                    .entry(tool_name.clone())
                    .or_default()
                    .push(duration.as_millis() as u64);
                // Track tool call counts
                *self
                    .tool_calls_by_name
                    .entry(tool_name.clone())
                    .or_default() += 1;
            }
            ProxyEvent::ApiUsage {
                model,
                input_tokens,
                output_tokens,
                cache_creation_tokens,
                cache_read_tokens,
                ..
            } => {
                self.total_input_tokens += *input_tokens as u64;
                self.total_output_tokens += *output_tokens as u64;
                self.total_cache_creation_tokens += *cache_creation_tokens as u64;
                self.total_cache_read_tokens += *cache_read_tokens as u64;

                // Track per-model stats
                let model_stats = self.model_tokens.entry(model.clone()).or_default();
                model_stats.input += *input_tokens as u64;
                model_stats.output += *output_tokens as u64;
                model_stats.cache_read += *cache_read_tokens as u64;
                model_stats.cache_creation += *cache_creation_tokens as u64;
                model_stats.calls += 1;

                *self.model_calls.entry(model.clone()).or_default() += 1;

                // === Historical tracking for sparklines ===
                // Add token snapshot
                self.token_history.push_back(TokenSnapshot {
                    timestamp: Instant::now(),
                    input: *input_tokens as u64,
                    output: *output_tokens as u64,
                    cached: *cache_read_tokens as u64,
                });
                if self.token_history.len() > 30 {
                    self.token_history.pop_front();
                }

                // Add cache hit rate snapshot
                let cache_rate = self.cache_hit_rate();
                self.cache_rate_history.push_back(cache_rate);
                if self.cache_rate_history.len() > 30 {
                    self.cache_rate_history.pop_front();
                }
            }
            ProxyEvent::Thinking { token_estimate, .. } => {
                self.thinking_blocks += 1;
                self.thinking_tokens += *token_estimate as u64;

                // === Historical tracking for sparklines ===
                self.thinking_token_history
                    .push_back(*token_estimate as u64);
                if self.thinking_token_history.len() > 30 {
                    self.thinking_token_history.pop_front();
                }
            }
            ProxyEvent::ContextCompact { .. } => {
                self.compact_count += 1;
            }
            ProxyEvent::RequestTransformed {
                transformer,
                tokens_before,
                tokens_after,
                ..
            } => {
                use crate::tokens::TokenDelta;
                let delta = TokenDelta::new(*tokens_before, *tokens_after);
                self.transform_stats.record(transformer, &delta);
            }
            ProxyEvent::ResponseAugmented {
                augmenter,
                tokens_injected,
                ..
            } => {
                self.augment_stats.record(augmenter, *tokens_injected);
            }
            _ => {}
        }
    }

    /// Merge another Stats into this one (for aggregation)
    #[allow(dead_code)] // Used by SessionManager::aggregate_stats (pending integration)
    pub fn merge(&mut self, other: &Stats) {
        self.total_requests += other.total_requests;
        self.failed_requests += other.failed_requests;
        self.total_tool_calls += other.total_tool_calls;
        self.failed_tool_calls += other.failed_tool_calls;
        self.total_ttfb += other.total_ttfb;
        self.response_count += other.response_count;

        self.total_input_tokens += other.total_input_tokens;
        self.total_output_tokens += other.total_output_tokens;
        self.total_cache_creation_tokens += other.total_cache_creation_tokens;
        self.total_cache_read_tokens += other.total_cache_read_tokens;

        self.thinking_blocks += other.thinking_blocks;
        self.thinking_tokens += other.thinking_tokens;
        self.compact_count += other.compact_count;
        self.turn_count += other.turn_count;

        // Merge per-model stats
        for (model, tokens) in &other.model_tokens {
            let entry = self.model_tokens.entry(model.clone()).or_default();
            entry.input += tokens.input;
            entry.output += tokens.output;
            entry.cache_read += tokens.cache_read;
            entry.cache_creation += tokens.cache_creation;
            entry.calls += tokens.calls;
        }

        for (model, count) in &other.model_calls {
            *self.model_calls.entry(model.clone()).or_default() += count;
        }

        for (tool, count) in &other.tool_calls_by_name {
            *self.tool_calls_by_name.entry(tool.clone()).or_default() += count;
        }

        // Note: tool_durations_ms not merged (timing data not aggregatable)

        // Merge Aspy modification stats
        self.transform_stats.tokens_injected += other.transform_stats.tokens_injected;
        self.transform_stats.tokens_removed += other.transform_stats.tokens_removed;
        for (name, (inj, rem)) in &other.transform_stats.by_transformer {
            let entry = self
                .transform_stats
                .by_transformer
                .entry(name.clone())
                .or_insert((0, 0));
            entry.0 += inj;
            entry.1 += rem;
        }

        self.augment_stats.tokens_injected += other.augment_stats.tokens_injected;
        for (name, count) in &other.augment_stats.by_augmenter {
            *self
                .augment_stats
                .by_augmenter
                .entry(name.clone())
                .or_insert(0) += count;
        }
    }
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            total_requests: 0,
            failed_requests: 0,
            total_tool_calls: 0,
            failed_tool_calls: 0,
            total_ttfb: Duration::default(),
            response_count: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cache_creation_tokens: 0,
            total_cache_read_tokens: 0,
            compact_count: 0,
            thinking_blocks: 0,
            thinking_tokens: 0,
            turn_count: 0,
            model_calls: HashMap::new(),
            model_tokens: HashMap::new(),
            tool_calls_by_name: HashMap::new(),
            tool_durations_ms: HashMap::new(),
            // Initialize ring buffers with capacity 30
            token_history: VecDeque::with_capacity(30),
            tool_call_history: VecDeque::with_capacity(30),
            cache_rate_history: VecDeque::with_capacity(30),
            thinking_token_history: VecDeque::with_capacity(30),
            // Aspy modification tracking
            transform_stats: TransformStats::default(),
            augment_stats: AugmentStats::default(),
        }
    }
}

/// Helper to generate unique IDs for correlating requests/responses
pub fn generate_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{}-{}", Utc::now().timestamp_millis(), count)
}
