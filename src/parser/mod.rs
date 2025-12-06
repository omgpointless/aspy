// Parser module - extracts tool calls and results from API traffic
//
// This module is responsible for parsing the Anthropic API request/response
// bodies and converting them into our internal ProxyEvent types.

pub mod models;

use crate::events::ProxyEvent;
use anyhow::{Context, Result};
use chrono::Utc;
use models::{ApiRequest, ApiResponse, ContextSnapshot};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Type alias for pending tool calls map: tool_use_id -> (tool_name, start_time)
type PendingCallsMap = HashMap<String, (String, chrono::DateTime<Utc>)>;

/// State for context compact detection (per-user)
/// Tracks total cache tokens (read + creation) from non-Haiku models
#[derive(Default)]
struct CompactDetectionState {
    /// Last seen total cache (cache_read + cache_creation) for compact detection
    last_cached_tokens: u64,
    /// Last known context size before potential compact
    last_context_tokens: u64,
    /// Snapshot of previous request's content breakdown (for comparison on compact)
    last_snapshot: Option<ContextSnapshot>,
    /// Snapshot of current request (set during parse_request, used by check_for_compact)
    pending_snapshot: Option<ContextSnapshot>,
}

/// Type alias for per-user compact detection state map
type CompactStateMap = HashMap<String, CompactDetectionState>;

/// Tracks tool calls and their timing to correlate calls with results
///
/// This struct maintains state across multiple API calls to match up
/// tool_use blocks (requests) with tool_result blocks (responses).
#[derive(Clone)]
pub struct Parser {
    /// Maps tool_use_id -> (tool_name, start_time)
    /// Arc<Mutex<>> allows sharing mutable state across async tasks
    pending_calls: Arc<Mutex<PendingCallsMap>>,
    /// Per-user state for detecting context compaction events
    /// Keyed by user_id (api_key_hash) to isolate state between users
    compact_state: Arc<Mutex<CompactStateMap>>,
}

impl Parser {
    pub fn new() -> Self {
        Self {
            pending_calls: Arc::new(Mutex::new(HashMap::new())),
            compact_state: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Check for context compaction and return a ContextCompact event if detected
    ///
    /// Compaction is detected when:
    /// 1. This is a non-Haiku model (Haiku is dispatcher, never has cache)
    /// 2. Total cache (read + creation) dropped significantly (>30% decrease OR >30K drop)
    /// 3. Previous total cache was >10K tokens (avoids false positives on first call)
    ///
    /// Note: We use cache_read + cache_creation because when Anthropic's cache
    /// expires, tokens move from cache_read to cache_creation - but the total
    /// context size is unchanged. A real compact drops the TOTAL, not just the
    /// read portion.
    ///
    /// State is tracked per-user (by api_key_hash) to isolate sessions and prevent
    /// cross-user false positives when proxy restarts or multiple users are active.
    ///
    /// Returns Some(ContextCompact) if compact detected, None otherwise
    async fn check_for_compact(
        &self,
        user_id: Option<&str>,
        model: &str,
        input_tokens: u32,
        cache_read_tokens: u32,
        cache_creation_tokens: u32,
    ) -> Option<ProxyEvent> {
        // Skip Haiku - it's the dispatcher model and never maintains cache
        if model.contains("haiku") {
            return None;
        }

        // Use user_id as key, or "unknown" for anonymous requests
        let user_key = user_id.unwrap_or("unknown").to_string();

        let mut state_map = self.compact_state.lock().await;
        let state = state_map.entry(user_key).or_default();

        // Total cache = read + creation (cache expiry moves tokens between these)
        let total_cache = cache_read_tokens as u64 + cache_creation_tokens as u64;
        let current_context = input_tokens as u64 + total_cache;
        let prev_cache = state.last_cached_tokens;

        // Detect significant cache drop (compact doesn't always go to zero)
        // Triggers on: >30% drop OR >30K absolute drop
        // IMPORTANT: total_cache must be > 0 to distinguish from new sessions.
        // A real compact creates new cache (the summary), while a fresh session
        // starts with zero cache. This prevents false positives when a user
        // starts a new Claude Code session while Aspy is still running.
        let significant_drop = prev_cache > 10_000
            && total_cache > 0
            && (total_cache < prev_cache.saturating_sub(30_000)
                || total_cache < prev_cache * 70 / 100);

        let compact_event = if significant_drop {
            // Calculate breakdown if we have both snapshots
            // pending_snapshot = current request, last_snapshot = previous request
            let breakdown = match (&state.last_snapshot, &state.pending_snapshot) {
                (Some(prev), Some(curr)) => {
                    let diff = curr.diff(prev);
                    tracing::info!("📦 Compact breakdown: {}", diff.summary());
                    if let Some((category, chars)) = diff.primary_reduction() {
                        tracing::info!("   Primary reduction: {} (-{} chars)", category, chars);
                    }
                    Some(diff)
                }
                _ => None,
            };

            Some(ProxyEvent::ContextCompact {
                timestamp: Utc::now(),
                previous_context: state.last_context_tokens,
                new_context: current_context,
                breakdown,
            })
        } else {
            None
        };

        // Update state for next check (only for non-Haiku)
        // Rotate: pending_snapshot → last_snapshot
        if total_cache > 0 {
            state.last_cached_tokens = total_cache;
            state.last_context_tokens = current_context;
            // Rotate snapshot: pending becomes last for next comparison
            if state.pending_snapshot.is_some() {
                state.last_snapshot = state.pending_snapshot.take();
            }
        } else if compact_event.is_some() {
            // Reset after compact
            state.last_cached_tokens = 0;
            state.last_context_tokens = current_context;
            state.last_snapshot = state.pending_snapshot.take();
        }

        compact_event
    }

    /// Store a context snapshot for the current request (called during parse_request)
    /// This snapshot will be used by check_for_compact to calculate breakdown
    pub async fn store_request_snapshot(&self, user_id: Option<&str>, snapshot: ContextSnapshot) {
        let user_key = user_id.unwrap_or("unknown").to_string();
        let mut state_map = self.compact_state.lock().await;
        let state = state_map.entry(user_key).or_default();
        state.pending_snapshot = Some(snapshot);
    }

    /// Get the current context snapshot for a user
    ///
    /// Returns the most recent snapshot (pending if available, else last).
    /// Used by MCP tool to answer "Why is my context so high?"
    pub async fn get_context_snapshot(&self, user_id: &str) -> Option<ContextSnapshot> {
        let state_map = self.compact_state.lock().await;
        state_map.get(user_id).and_then(|state| {
            // Prefer pending (current request) over last (previous request)
            state
                .pending_snapshot
                .clone()
                .or_else(|| state.last_snapshot.clone())
        })
    }

    /// Register a tool_use ID for correlation with future tool_results
    ///
    /// This is called during SSE streaming when we see a content_block_start
    /// with type "tool_use". We register immediately so the pending_calls map
    /// is populated before the next request arrives with the tool_result.
    ///
    /// This fixes a race condition where the streaming response task hadn't
    /// finished parsing before Claude Code sent the next request.
    pub async fn register_pending_tool(&self, id: String, name: String) {
        tracing::trace!("REGISTERING pending tool: {} ({})", &id, &name);
        let mut pending = self.pending_calls.lock().await;
        pending.insert(id, (name, Utc::now()));
        tracing::trace!("pending_calls now has {} entries", pending.len());
    }

    /// Parse an API request looking for tool results
    ///
    /// Tool results represent Claude Code's responses to previous tool calls.
    /// We correlate them with the original call to calculate duration.
    ///
    /// Also calculates and stores a ContextSnapshot for compact breakdown analysis.
    pub async fn parse_request(
        &self,
        body: &[u8],
        user_id: Option<&str>,
    ) -> Result<Vec<ProxyEvent>> {
        let request: ApiRequest = match serde_json::from_slice(body) {
            Ok(req) => req,
            Err(e) => {
                // Log the actual error for debugging
                tracing::error!("Serde error: {} at line {} col {}", e, e.line(), e.column());
                return Err(anyhow::anyhow!("Failed to parse API request: {}", e));
            }
        };

        // Calculate and store context snapshot for compact breakdown analysis
        let snapshot = ContextSnapshot::from_request(&request);
        self.store_request_snapshot(user_id, snapshot).await;

        let mut events = Vec::new();
        let tool_results = request.tool_results();

        let mut pending = self.pending_calls.lock().await;

        tracing::trace!(
            "parse_request: found {} tool_results, pending_calls has {} entries",
            tool_results.len(),
            pending.len()
        );

        for (tool_use_id, content, is_error) in tool_results {
            tracing::trace!("Looking for tool_use_id {} in pending_calls", &tool_use_id);
            // Look up the original tool call to get its name and start time
            if let Some((tool_name, start_time)) = pending.remove(&tool_use_id) {
                let duration = Utc::now()
                    .signed_duration_since(start_time)
                    .to_std()
                    .unwrap_or_default();

                tracing::trace!(
                    "MATCH! Emitting ToolResult for {} ({})",
                    &tool_use_id,
                    &tool_name
                );

                events.push(ProxyEvent::ToolResult {
                    id: tool_use_id,
                    timestamp: Utc::now(),
                    tool_name,
                    output: content,
                    duration,
                    success: !is_error,
                });
            } else {
                tracing::trace!(
                    "NO MATCH for tool_use_id {} - not in pending_calls",
                    &tool_use_id
                );
            }
        }

        Ok(events)
    }

    /// Parse an API response looking for tool calls
    ///
    /// Tool calls (tool_use blocks) represent Claude requesting to use a tool.
    /// We store them in pending_calls so we can correlate with results later.
    ///
    /// This handles both regular JSON responses and Server-Sent Events (SSE) streaming.
    ///
    /// The user_id parameter is used for per-user compact detection state.
    pub async fn parse_response(
        &self,
        body: &[u8],
        user_id: Option<&str>,
    ) -> Result<Vec<ProxyEvent>> {
        // Try to detect if this is SSE format
        let body_str = std::str::from_utf8(body).unwrap_or("");

        if body_str.starts_with("event:") || body_str.contains("\nevent:") {
            // This is a streaming SSE response
            tracing::trace!("Detected SSE streaming response");
            return self.parse_sse_response(body_str, user_id).await;
        }

        // Regular JSON response
        let response: ApiResponse =
            serde_json::from_slice(body).context("Failed to parse API response")?;

        let mut events = Vec::new();
        let tool_uses = response.tool_uses();

        let mut pending = self.pending_calls.lock().await;

        for (id, name, input) in tool_uses {
            let timestamp = Utc::now();

            // Store this tool call so we can correlate it with the result later
            pending.insert(id.clone(), (name.clone(), timestamp));

            events.push(ProxyEvent::ToolCall {
                id,
                timestamp,
                tool_name: name,
                input,
            });
        }

        // Extract assistant text content from response
        let text_blocks: Vec<String> = response
            .content
            .iter()
            .filter_map(|block| {
                if let models::ContentBlock::Text { text } = block {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .collect();

        // Emit a single AssistantResponse event with all text combined
        if !text_blocks.is_empty() {
            let combined_text = text_blocks.join("\n\n");
            events.push(ProxyEvent::AssistantResponse {
                timestamp: Utc::now(),
                content: combined_text,
            });
        }

        // Extract usage information if present
        if let Some(usage) = response.usage {
            let cache_read = usage.cache_read_input_tokens.unwrap_or(0);
            let cache_creation = usage.cache_creation_input_tokens.unwrap_or(0);

            // Check for context compaction before emitting ApiUsage
            if let Some(compact_event) = self
                .check_for_compact(
                    user_id,
                    &response.model,
                    usage.input_tokens,
                    cache_read,
                    cache_creation,
                )
                .await
            {
                events.push(compact_event);
            }

            events.push(ProxyEvent::ApiUsage {
                timestamp: Utc::now(),
                model: response.model.clone(),
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_creation_tokens: cache_creation,
                cache_read_tokens: cache_read,
            });
        }

        Ok(events)
    }

    /// Parse Server-Sent Events (SSE) streaming response
    ///
    /// SSE responses contain multiple event types that we need to handle:
    /// - message_start: Contains model and initial usage (input tokens)
    /// - content_block_start: Begins a content block (tool_use, thinking, text)
    /// - content_block_delta: Incremental data for the block (input_json_delta, thinking_delta)
    /// - content_block_stop: Block complete, emit the event
    /// - message_delta: Final usage data (output tokens)
    ///
    /// Key insight: We must ACCUMULATE deltas before emitting events!
    ///
    /// The user_id parameter is used for per-user compact detection state.
    async fn parse_sse_response(
        &self,
        body: &str,
        user_id: Option<&str>,
    ) -> Result<Vec<ProxyEvent>> {
        let mut events = Vec::new();
        let mut pending = self.pending_calls.lock().await;

        // Message-level tracking
        let mut model: Option<String> = None;
        let mut input_tokens: u32 = 0;
        let mut output_tokens: u32 = 0;
        let mut cache_creation_tokens: u32 = 0;
        let mut cache_read_tokens: u32 = 0;

        // Partial content blocks being accumulated (index -> block data)
        let mut partial_blocks: HashMap<u32, PartialContentBlock> = HashMap::new();

        // Parse SSE format line by line
        for line in body.lines() {
            let line = line.trim();

            // Look for "data:" lines which contain JSON
            if !line.starts_with("data:") {
                continue;
            }

            let json_str = line.strip_prefix("data:").unwrap_or("").trim();
            if json_str.is_empty() || json_str == "[DONE]" {
                continue;
            }

            // Try to parse the JSON data
            let data: serde_json::Value = match serde_json::from_str(json_str) {
                Ok(d) => d,
                Err(_) => continue,
            };

            let event_type = data.get("type").and_then(|v| v.as_str()).unwrap_or("");

            match event_type {
                "message_start" => {
                    // Extract model and initial usage from message_start
                    if let Some(message) = data.get("message") {
                        model = message
                            .get("model")
                            .and_then(|v| v.as_str())
                            .map(String::from);

                        if let Some(usage) = message.get("usage") {
                            input_tokens = usage
                                .get("input_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as u32;
                            cache_creation_tokens = usage
                                .get("cache_creation_input_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0)
                                as u32;
                            cache_read_tokens = usage
                                .get("cache_read_input_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0)
                                as u32;
                        }
                    }
                }

                "content_block_start" => {
                    // Start tracking a new content block - DON'T emit yet!
                    let index = data.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                    if let Some(content_block) = data.get("content_block") {
                        let block_type = content_block
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        let partial = match block_type {
                            "tool_use" => {
                                let id = content_block
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let name = content_block
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();

                                PartialContentBlock::ToolUse {
                                    id,
                                    name,
                                    input_json: String::new(),
                                    timestamp: Utc::now(),
                                }
                            }
                            "thinking" => PartialContentBlock::Thinking {
                                content: String::new(),
                                timestamp: Utc::now(),
                            },
                            "text" => PartialContentBlock::Text {
                                content: String::new(),
                                timestamp: Utc::now(),
                            },
                            _ => PartialContentBlock::Other,
                        };

                        partial_blocks.insert(index, partial);
                    }
                }

                "content_block_delta" => {
                    // Accumulate delta into the partial block
                    let index = data.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                    if let Some(delta) = data.get("delta") {
                        let delta_type = delta.get("type").and_then(|v| v.as_str()).unwrap_or("");

                        if let Some(partial) = partial_blocks.get_mut(&index) {
                            match (partial, delta_type) {
                                (
                                    PartialContentBlock::ToolUse { input_json, .. },
                                    "input_json_delta",
                                ) => {
                                    // Accumulate JSON string fragments
                                    if let Some(partial_json) =
                                        delta.get("partial_json").and_then(|v| v.as_str())
                                    {
                                        input_json.push_str(partial_json);
                                    }
                                }
                                (
                                    PartialContentBlock::Thinking { content, .. },
                                    "thinking_delta",
                                ) => {
                                    // Accumulate thinking text
                                    if let Some(thinking) =
                                        delta.get("thinking").and_then(|v| v.as_str())
                                    {
                                        content.push_str(thinking);
                                    }
                                }
                                (PartialContentBlock::Text { content, .. }, "text_delta") => {
                                    // Accumulate assistant response text
                                    if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                        content.push_str(text);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }

                "content_block_stop" => {
                    // Block complete - NOW emit the event
                    let index = data.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                    if let Some(partial) = partial_blocks.remove(&index) {
                        match partial {
                            PartialContentBlock::ToolUse {
                                id,
                                name,
                                input_json,
                                timestamp,
                            } => {
                                // Parse the accumulated JSON string into a Value
                                let input: serde_json::Value = if input_json.is_empty() {
                                    serde_json::Value::Object(serde_json::Map::new())
                                } else {
                                    serde_json::from_str(&input_json).unwrap_or({
                                        // If parsing fails, store as raw string
                                        serde_json::Value::String(input_json)
                                    })
                                };

                                // Register in pending_calls for correlation with results
                                pending.insert(id.clone(), (name.clone(), timestamp));

                                events.push(ProxyEvent::ToolCall {
                                    id,
                                    timestamp,
                                    tool_name: name,
                                    input,
                                });
                            }
                            PartialContentBlock::Thinking { content, timestamp } => {
                                if !content.is_empty() {
                                    let token_estimate = (content.len() / 4) as u32;
                                    events.push(ProxyEvent::Thinking {
                                        timestamp,
                                        content,
                                        token_estimate,
                                    });
                                }
                            }
                            PartialContentBlock::Text { content, timestamp } => {
                                if !content.is_empty() {
                                    events
                                        .push(ProxyEvent::AssistantResponse { timestamp, content });
                                }
                            }
                            PartialContentBlock::Other => {}
                        }
                    }
                }

                "message_delta" => {
                    // Extract output tokens from message_delta
                    if let Some(usage) = data.get("usage") {
                        output_tokens = usage
                            .get("output_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32;
                    }
                }

                _ => {}
            }
        }

        // Emit any remaining partial blocks (shouldn't happen with well-formed SSE)
        for (_, partial) in partial_blocks {
            match partial {
                PartialContentBlock::ToolUse {
                    id,
                    name,
                    input_json,
                    timestamp,
                } => {
                    let input: serde_json::Value = if input_json.is_empty() {
                        serde_json::Value::Object(serde_json::Map::new())
                    } else {
                        serde_json::from_str(&input_json)
                            .unwrap_or(serde_json::Value::String(input_json))
                    };

                    pending.insert(id.clone(), (name.clone(), timestamp));
                    events.push(ProxyEvent::ToolCall {
                        id,
                        timestamp,
                        tool_name: name,
                        input,
                    });
                }
                PartialContentBlock::Thinking { content, timestamp } => {
                    if !content.is_empty() {
                        let token_estimate = (content.len() / 4) as u32;
                        events.push(ProxyEvent::Thinking {
                            timestamp,
                            content,
                            token_estimate,
                        });
                    }
                }
                PartialContentBlock::Text { content, timestamp } => {
                    if !content.is_empty() {
                        events.push(ProxyEvent::AssistantResponse { timestamp, content });
                    }
                }
                PartialContentBlock::Other => {}
            }
        }

        // Drop pending lock before compact check to avoid holding two locks
        drop(pending);

        // Emit usage event if we collected data
        if let Some(model_name) = model {
            if input_tokens > 0 || output_tokens > 0 {
                // Check for context compaction before emitting ApiUsage
                if let Some(compact_event) = self
                    .check_for_compact(
                        user_id,
                        &model_name,
                        input_tokens,
                        cache_read_tokens,
                        cache_creation_tokens,
                    )
                    .await
                {
                    events.push(compact_event);
                }

                events.push(ProxyEvent::ApiUsage {
                    timestamp: Utc::now(),
                    model: model_name,
                    input_tokens,
                    output_tokens,
                    cache_creation_tokens,
                    cache_read_tokens,
                });
            }
        }

        Ok(events)
    }
}

/// Partial content block being accumulated during SSE parsing
enum PartialContentBlock {
    /// Tool use block: accumulate input JSON string
    ToolUse {
        id: String,
        name: String,
        input_json: String,
        timestamp: chrono::DateTime<Utc>,
    },
    /// Thinking block: accumulate thinking text
    Thinking {
        content: String,
        timestamp: chrono::DateTime<Utc>,
    },
    /// Text block: accumulate assistant response text
    Text {
        content: String,
        timestamp: chrono::DateTime<Utc>,
    },
    /// Other block types we don't track
    Other,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_tool_use() {
        let parser = Parser::new();

        let response_json = r#"{
            "id": "msg_123",
            "model": "claude-3-opus-20240229",
            "content": [
                {
                    "type": "tool_use",
                    "id": "tool_abc",
                    "name": "Read",
                    "input": {"file_path": "/test/file.txt"}
                }
            ]
        }"#;

        let events = parser
            .parse_response(response_json.as_bytes(), None)
            .await
            .unwrap();

        assert_eq!(events.len(), 1);
        match &events[0] {
            ProxyEvent::ToolCall { tool_name, .. } => {
                assert_eq!(tool_name, "Read");
            }
            _ => panic!("Expected ToolCall event"),
        }
    }

    #[tokio::test]
    async fn test_parse_tool_result_with_cache_control() {
        // Test that tool_result blocks with cache_control field parse correctly
        let parser = Parser::new();

        // First, register a pending tool call (simulating what happens during streaming)
        parser
            .register_pending_tool(
                "toolu_01ASGBmB2GxUaBj6UsJ9fhZE".to_string(),
                "Glob".to_string(),
            )
            .await;

        // This is the actual structure from Claude Code logs
        let request_json = r#"{
            "model": "claude-3-opus-20240229",
            "max_tokens": 8096,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "tool_result",
                            "tool_use_id": "toolu_01ASGBmB2GxUaBj6UsJ9fhZE",
                            "cache_control": {"type": "ephemeral"},
                            "content": "file1.txt\nfile2.txt"
                        }
                    ]
                }
            ]
        }"#;

        let events = parser
            .parse_request(request_json.as_bytes(), None)
            .await
            .unwrap();

        assert_eq!(events.len(), 1, "Expected 1 ToolResult event");
        match &events[0] {
            ProxyEvent::ToolResult {
                id,
                tool_name,
                success,
                ..
            } => {
                assert_eq!(id, "toolu_01ASGBmB2GxUaBj6UsJ9fhZE");
                assert_eq!(tool_name, "Glob");
                assert!(*success);
            }
            other => panic!("Expected ToolResult event, got {:?}", other),
        }
    }
}
