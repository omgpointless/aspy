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

    // Context window tracking
    pub context_window_size: u64,    // Max tokens for current model
    pub current_context_tokens: u64, // Tokens used in current conversation turn
    pub peak_context_tokens: u64,    // Highest context usage seen

    // Per-tool statistics
    pub tool_call_counts: std::collections::HashMap<String, usize>,
    pub tool_durations: std::collections::HashMap<String, Duration>,
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

    /// Get context window usage as a percentage (0.0 - 1.0)
    pub fn context_usage_pct(&self) -> f64 {
        if self.context_window_size == 0 {
            0.0
        } else {
            self.current_context_tokens as f64 / self.context_window_size as f64
        }
    }

    /// Get context window size for a model
    pub fn model_context_window(model: &str) -> u64 {
        // Claude 2.x models have 100k context
        if model.contains("claude-2") {
            100_000
        } else {
            // All Claude 3+ models have 200k context
            200_000
        }
    }

    /// Update context window tracking from API usage
    pub fn update_context(&mut self, input_tokens: u64, output_tokens: u64) {
        // Current context is approximated as input + output tokens
        self.current_context_tokens = input_tokens + output_tokens;

        // Update peak if this is higher
        if self.current_context_tokens > self.peak_context_tokens {
            self.peak_context_tokens = self.current_context_tokens;
        }

        // Update context window size based on current model
        if let Some(ref model) = self.current_model {
            self.context_window_size = Self::model_context_window(model);
        }
    }

    /// Get average duration for a specific tool
    pub fn avg_tool_duration(&self, tool_name: &str) -> Option<Duration> {
        let count = self.tool_call_counts.get(tool_name)?;
        let total = self.tool_durations.get(tool_name)?;
        if *count > 0 {
            Some(*total / *count as u32)
        } else {
            None
        }
    }

    /// Get top N tools by call count
    pub fn top_tools(&self, n: usize) -> Vec<(&String, usize)> {
        let mut tools: Vec<_> = self.tool_call_counts.iter().map(|(k, v)| (k, *v)).collect();
        tools.sort_by(|a, b| b.1.cmp(&a.1));
        tools.truncate(n);
        tools
    }
}

/// Helper to generate unique IDs for correlating requests/responses
pub fn generate_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{}-{}", Utc::now().timestamp_millis(), count)
}
