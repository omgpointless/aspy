// Events that flow from the proxy to the TUI and storage components
//
// These events represent the parsed tool calls and responses that we extract
// from the Anthropic API traffic. Using an enum allows pattern matching and
// ensures type-safe communication between async tasks.

use crate::parser::models::CapturedHeaders;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
}

/// Summary statistics for the status bar
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub total_requests: usize,
    pub total_tool_calls: usize,
    pub successful_calls: usize,
    pub failed_calls: usize,
    pub total_duration: Duration,

    // Token usage tracking
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,

    // Thinking block tracking
    pub thinking_blocks: usize,
    pub thinking_tokens: u64,

    // Current model being used (for cost calculation)
    pub current_model: Option<String>,

    // Most recent thinking content (for display panel)
    pub current_thinking: Option<String>,
}

impl Stats {
    pub fn success_rate(&self) -> f64 {
        if self.total_tool_calls == 0 {
            0.0
        } else {
            (self.successful_calls as f64 / self.total_tool_calls as f64) * 100.0
        }
    }

    pub fn avg_duration(&self) -> Duration {
        if self.total_tool_calls == 0 {
            Duration::default()
        } else {
            self.total_duration / self.total_tool_calls as u32
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
}

/// Helper to generate unique IDs for correlating requests/responses
pub fn generate_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{}-{}", Utc::now().timestamp_millis(), count)
}
