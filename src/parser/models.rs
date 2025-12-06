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
    // Note: system can be a string OR an array of content blocks
    #[serde(default)]
    pub system: Option<serde_json::Value>,
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

    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(default)]
        signature: Option<String>,
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

// ═══════════════════════════════════════════════════════════════════════════
// Context Snapshot - breakdown of request content for compact analysis
// ═══════════════════════════════════════════════════════════════════════════

/// Snapshot of context composition for a request
///
/// Used to track what's in the context window and detect what changed
/// during compaction events. Lightweight struct (~64 bytes) stored per-user.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextSnapshot {
    /// Total message count
    pub message_count: u32,
    /// Number of tool_result blocks
    pub tool_result_count: u32,
    /// Total chars in tool_result content
    pub tool_result_chars: u64,
    /// Number of tool_use blocks
    pub tool_use_count: u32,
    /// Total chars in tool_use input
    pub tool_use_chars: u64,
    /// Total chars in thinking blocks
    pub thinking_chars: u64,
    /// Total chars in text blocks
    pub text_chars: u64,
    /// Total chars in system prompt
    pub system_chars: u64,
}

impl ContextSnapshot {
    /// Calculate snapshot from an API request
    pub fn from_request(req: &ApiRequest) -> Self {
        let system_chars = req
            .system
            .as_ref()
            .map(|s| s.to_string().len() as u64)
            .unwrap_or(0);

        let mut snap = Self {
            message_count: req.messages.len() as u32,
            system_chars,
            ..Default::default()
        };

        // Walk all messages and content blocks
        for msg in &req.messages {
            match &msg.content {
                MessageContent::Text(text) => {
                    snap.text_chars += text.len() as u64;
                }
                MessageContent::Blocks(blocks) => {
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => {
                                snap.text_chars += text.len() as u64;
                            }
                            ContentBlock::ToolUse { input, .. } => {
                                snap.tool_use_count += 1;
                                snap.tool_use_chars += input.to_string().len() as u64;
                            }
                            ContentBlock::ToolResult { content, .. } => {
                                snap.tool_result_count += 1;
                                snap.tool_result_chars += content.to_string().len() as u64;
                            }
                            ContentBlock::Thinking { thinking, .. } => {
                                snap.thinking_chars += thinking.len() as u64;
                            }
                            ContentBlock::Other => {}
                        }
                    }
                }
            }
        }

        snap
    }

    /// Calculate diff between two snapshots (self - other)
    /// Returns a new snapshot with the differences (positive = increased, negative would underflow so we use saturating)
    pub fn diff(&self, previous: &Self) -> ContextSnapshotDiff {
        ContextSnapshotDiff {
            message_count: self.message_count as i32 - previous.message_count as i32,
            tool_result_count: self.tool_result_count as i32 - previous.tool_result_count as i32,
            tool_result_chars: self.tool_result_chars as i64 - previous.tool_result_chars as i64,
            tool_use_count: self.tool_use_count as i32 - previous.tool_use_count as i32,
            tool_use_chars: self.tool_use_chars as i64 - previous.tool_use_chars as i64,
            thinking_chars: self.thinking_chars as i64 - previous.thinking_chars as i64,
            text_chars: self.text_chars as i64 - previous.text_chars as i64,
            system_chars: self.system_chars as i64 - previous.system_chars as i64,
        }
    }

    /// Total content size in chars (rough proxy for tokens)
    /// Future use: MCP tool for context analysis
    #[allow(dead_code)]
    pub fn total_chars(&self) -> u64 {
        self.tool_result_chars
            + self.tool_use_chars
            + self.thinking_chars
            + self.text_chars
            + self.system_chars
    }
}

/// Diff between two context snapshots (signed values)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextSnapshotDiff {
    pub message_count: i32,
    pub tool_result_count: i32,
    pub tool_result_chars: i64,
    pub tool_use_count: i32,
    pub tool_use_chars: i64,
    pub thinking_chars: i64,
    pub text_chars: i64,
    pub system_chars: i64,
}

impl ContextSnapshotDiff {
    /// Get the primary reduction category (what was trimmed most)
    pub fn primary_reduction(&self) -> Option<(&'static str, i64)> {
        let reductions = [
            ("tool_results", -self.tool_result_chars),
            ("tool_inputs", -self.tool_use_chars),
            ("thinking", -self.thinking_chars),
            ("text", -self.text_chars),
            ("system", -self.system_chars),
        ];

        reductions
            .into_iter()
            .filter(|(_, v)| *v > 0) // Only reductions (negative diffs become positive here)
            .max_by_key(|(_, v)| *v)
    }

    /// Format as human-readable summary
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if self.tool_result_chars != 0 {
            parts.push(format!("tool_results: {:+} chars", self.tool_result_chars));
        }
        if self.thinking_chars != 0 {
            parts.push(format!("thinking: {:+} chars", self.thinking_chars));
        }
        if self.text_chars != 0 {
            parts.push(format!("text: {:+} chars", self.text_chars));
        }
        if self.tool_use_chars != 0 {
            parts.push(format!("tool_inputs: {:+} chars", self.tool_use_chars));
        }

        if parts.is_empty() {
            "no significant changes".to_string()
        } else {
            parts.join(", ")
        }
    }
}
