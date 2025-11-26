// Data models for parsing Anthropic API requests and responses
//
// These structs map to the Anthropic Messages API format.
// We use Serde's derive macros to automatically generate
// serialization/deserialization code.
//
// Note: We only parse the fields we care about for observability.
// Serde will ignore extra fields, making this robust to API changes.

use serde::{Deserialize, Serialize};

/// Represents an Anthropic API request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest {
    pub model: String,
    pub messages: Vec<Message>,

    // Optional fields that affect behavior
    #[serde(default)]
    pub system: Option<String>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub top_k: Option<u32>,
    #[serde(default)]
    pub stop_sequences: Vec<String>,

    // Tools and token limits
    #[serde(default)]
    pub tools: Vec<Tool>,
    #[serde(default)]
    pub max_tokens: u32,

    // CRITICAL: Streaming flag - determines if response is SSE stream
    #[serde(default)]
    pub stream: Option<bool>,

    // User-provided metadata
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Represents an Anthropic API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub id: String,
    pub model: String,
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub stop_sequence: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: MessageContent,
}

/// Message content can be a string or an array of content blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// A content block in a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: serde_json::Value,
        #[serde(default)]
        is_error: bool,
    },

    /// Catch-all for other content types we don't care about
    #[serde(other)]
    Other,
}

/// Tool definition in the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

/// API usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,

    // Prompt caching fields (may not be present in all responses)
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u32>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u32>,
}

impl ApiResponse {
    /// Extract all tool use blocks from the response
    pub fn tool_uses(&self) -> Vec<(String, String, serde_json::Value)> {
        self.content
            .iter()
            .filter_map(|block| {
                if let ContentBlock::ToolUse { id, name, input } = block {
                    Some((id.clone(), name.clone(), input.clone()))
                } else {
                    None
                }
            })
            .collect()
    }
}

impl ApiRequest {
    /// Extract all tool results from the request (these are Claude Code's responses to tool calls)
    pub fn tool_results(&self) -> Vec<(String, serde_json::Value, bool)> {
        self.messages
            .iter()
            .flat_map(|msg| {
                if let MessageContent::Blocks(blocks) = &msg.content {
                    blocks
                        .iter()
                        .filter_map(|block| {
                            if let ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                                is_error,
                            } = block
                            {
                                Some((tool_use_id.clone(), content.clone(), *is_error))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                }
            })
            .collect()
    }

    /// Check if this is a streaming request (for future SSE streaming support)
    #[allow(dead_code)]
    pub fn is_streaming(&self) -> bool {
        self.stream.unwrap_or(false)
    }
}

/// Captured HTTP headers from request and response
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapturedHeaders {
    // Request headers
    pub anthropic_version: Option<String>,
    pub anthropic_beta: Vec<String>,
    pub api_key_hash: Option<String>, // SHA-256 hash for tracking

    // Response headers
    pub request_id: Option<String>,
    pub organization_id: Option<String>,

    // Rate limit headers
    pub requests_limit: Option<u32>,
    pub requests_remaining: Option<u32>,
    pub requests_reset: Option<String>,
    pub tokens_limit: Option<u32>,
    pub tokens_remaining: Option<u32>,
    pub tokens_reset: Option<String>,
}

impl CapturedHeaders {
    /// Create empty headers
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if rate limit information is available
    pub fn has_rate_limits(&self) -> bool {
        self.requests_limit.is_some() || self.tokens_limit.is_some()
    }

    /// Get requests usage percentage (0.0 to 1.0)
    pub fn requests_usage_pct(&self) -> Option<f32> {
        match (self.requests_remaining, self.requests_limit) {
            (Some(remaining), Some(limit)) if limit > 0 => {
                Some((limit - remaining) as f32 / limit as f32)
            }
            _ => None,
        }
    }

    /// Get tokens usage percentage (0.0 to 1.0)
    pub fn tokens_usage_pct(&self) -> Option<f32> {
        match (self.tokens_remaining, self.tokens_limit) {
            (Some(remaining), Some(limit)) if limit > 0 => {
                Some((limit - remaining) as f32 / limit as f32)
            }
            _ => None,
        }
    }
}
