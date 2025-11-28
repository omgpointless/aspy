// Events that flow from the proxy to the TUI and storage components
//
// These events represent the parsed tool calls and responses that we extract
// from the Anthropic API traffic. Using an enum allows pattern matching and
// ensures type-safe communication between async tasks.

use crate::parser::models::CapturedHeaders;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

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
    },

    /// Thinking block started (emitted immediately for real-time feedback)
    ThinkingStarted { timestamp: DateTime<Utc> },
}

/// Summary statistics for the status bar
#[derive(Debug, Clone, Default)]
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

    // Context window tracking (most recent API call's context size)
    /// Current context size = input_tokens + cache_read_tokens from last ApiUsage
    pub current_context_tokens: u64,
    /// Last seen cache_read_tokens (for compact detection)
    pub last_cached_tokens: u64,
    /// Number of context compacts detected this session
    pub compact_count: usize,
    /// Configured context limit (from config file, default 150K)
    pub configured_context_limit: u64,

    // Thinking block tracking
    pub thinking_blocks: usize,
    pub thinking_tokens: u64,

    // Current model being used (for cost calculation)
    pub current_model: Option<String>,

    // Most recent thinking content (for display panel)
    pub current_thinking: Option<String>,

    // === Session metadata ===
    /// When the session started (absolute time for reports)
    pub session_started: Option<DateTime<Utc>>,

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

    /// Calculate total cost based on current model
    pub fn total_cost(&self) -> f64 {
        if let Some(ref model) = self.current_model {
            crate::pricing::calculate_cost(
                model,
                self.total_input_tokens as u32,
                self.total_output_tokens as u32,
                self.total_cache_creation_tokens as u32,
                self.total_cache_read_tokens as u32,
            )
        } else {
            0.0
        }
    }

    /// Calculate cache savings
    pub fn cache_savings(&self) -> f64 {
        if let Some(ref model) = self.current_model {
            crate::pricing::calculate_cache_savings(model, self.total_cache_read_tokens as u32)
        } else {
            0.0
        }
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

    /// Get average tool execution time in milliseconds for a specific tool
    pub fn avg_tool_duration_ms(&self, tool_name: &str) -> Option<u64> {
        self.tool_durations_ms.get(tool_name).and_then(|durations| {
            if durations.is_empty() {
                None
            } else {
                Some(durations.iter().sum::<u64>() / durations.len() as u64)
            }
        })
    }

    /// Get context window usage as percentage (0-100)
    /// Returns None if no context data available yet
    pub fn context_usage_percent(&self) -> Option<f64> {
        if self.current_context_tokens == 0 {
            return None;
        }
        let limit = self.context_limit();
        Some((self.current_context_tokens as f64 / limit as f64) * 100.0)
    }

    /// Get the configured context limit
    pub fn context_limit(&self) -> u64 {
        if self.configured_context_limit > 0 {
            self.configured_context_limit
        } else {
            150_000 // Default fallback
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
