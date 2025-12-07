// SSE (Server-Sent Events) parsing module
//
// This module handles parsing of SSE streams from the Anthropic API.
// It provides two levels of parsing:
//
// 1. **Line-level extraction** (real-time): Parse individual SSE lines as they
//    arrive to extract metadata like tool_use IDs, thinking blocks, model info.
//    This enables immediate event emission without waiting for stream completion.
//
// 2. **Stream assembly** (post-stream): Assemble accumulated SSE data into a
//    structured JSON representation for logging and display.
//
// # Architecture
//
// The Anthropic API uses SSE for streaming responses. Each line has format:
// ```
// event: <event_type>
// data: <json_payload>
// ```
//
// Key event types:
// - `message_start`: Contains model info
// - `content_block_start`: Starts a text/tool_use/thinking block
// - `content_block_delta`: Incremental content (text_delta, thinking_delta, input_json_delta)
// - `content_block_stop`: Block complete
// - `message_delta`: Final message metadata (stop_reason, usage)
// - `message_stop`: Stream complete

use serde_json::json;

// ============================================================================
// SSE Detection
// ============================================================================

/// Check if a response is SSE based on content-type header
pub fn is_sse_response(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.contains("text/event-stream"))
        .unwrap_or(false)
}

// ============================================================================
// Line-Level Extractors
// Used during streaming for real-time metadata extraction
// ============================================================================

/// Extract tool_use ID and name from an SSE data line
///
/// Called during streaming to register tool_use IDs immediately. This prevents
/// a race condition where the next request (with tool_result) arrives before
/// we've finished parsing the response.
///
/// Returns `Some((id, name))` if this line starts a tool_use block.
pub fn extract_tool_use(line: &str) -> Option<(String, String)> {
    let data = parse_sse_data_line(line)?;

    // Check for content_block_start event
    if data.get("type")?.as_str()? != "content_block_start" {
        return None;
    }

    // Check if the content_block is a tool_use
    let content_block = data.get("content_block")?;
    if content_block.get("type")?.as_str()? != "tool_use" {
        return None;
    }

    // Extract ID and name
    let id = content_block.get("id")?.as_str()?.to_string();
    let name = content_block.get("name")?.as_str()?.to_string();

    Some((id, name))
}

/// Check if an SSE line indicates the start of a thinking block
///
/// Used for real-time "Thinking..." feedback before the full block arrives.
pub fn is_thinking_block_start(line: &str) -> bool {
    let Some(data) = parse_sse_data_line(line) else {
        return false;
    };

    // Check for content_block_start event
    if data.get("type").and_then(|v| v.as_str()) != Some("content_block_start") {
        return false;
    }

    // Check if block type is "thinking"
    data.get("content_block")
        .and_then(|b| b.get("type"))
        .and_then(|t| t.as_str())
        == Some("thinking")
}

/// Extract thinking text from a thinking_delta SSE event
///
/// Returns the incremental thinking text if this is a thinking delta.
pub fn extract_thinking_delta(line: &str) -> Option<String> {
    let data = parse_sse_data_line(line)?;

    // Check for content_block_delta event
    if data.get("type")?.as_str()? != "content_block_delta" {
        return None;
    }

    // Check if delta type is thinking_delta
    let delta = data.get("delta")?;
    if delta.get("type")?.as_str()? != "thinking_delta" {
        return None;
    }

    // Extract the thinking text
    delta.get("thinking")?.as_str().map(String::from)
}

/// Extract content block index from SSE content_block_start event
///
/// Used to track the highest block index for annotation injection.
pub fn extract_content_block_index(line: &str) -> Option<u32> {
    let data = parse_sse_data_line(line)?;

    // Check for content_block_start event
    if data.get("type")?.as_str()? != "content_block_start" {
        return None;
    }

    // Extract the index
    data.get("index")?.as_u64().map(|i| i as u32)
}

/// Extract model name from SSE message_start event
///
/// Returns the model string (e.g., "claude-3-opus-20240229").
pub fn extract_model(line: &str) -> Option<String> {
    let data = parse_sse_data_line(line)?;

    // Check for message_start event
    if data.get("type")?.as_str()? != "message_start" {
        return None;
    }

    // Extract model from message_start.message.model
    data.get("message")?
        .get("model")?
        .as_str()
        .map(String::from)
}

// ============================================================================
// Stream Assembly
// Assembles accumulated SSE data into structured JSON
// ============================================================================

/// Parse accumulated SSE response into a JSON representation for display
///
/// Reconstructs the message by:
/// 1. Extracting model from `message_start`
/// 2. Collecting content blocks from `content_block_start`
/// 3. Accumulating text deltas from `content_block_delta`
/// 4. Capturing stop_reason and usage from `message_delta`
pub fn assemble_to_json(body: &str) -> Option<serde_json::Value> {
    let mut content_blocks = Vec::new();
    let mut model = String::new();
    let mut stop_reason: Option<String> = None;
    let mut usage_data: Option<serde_json::Value> = None;

    for line in body.lines() {
        let Some(data) = parse_sse_data_line(line.trim()) else {
            continue;
        };

        let event_type = data.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match event_type {
            "message_start" => {
                if let Some(message) = data.get("message") {
                    model = message
                        .get("model")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                }
            }
            "content_block_start" => {
                if let Some(block) = data.get("content_block") {
                    content_blocks.push(block.clone());
                }
            }
            "content_block_delta" => {
                if let Some(delta) = data.get("delta") {
                    if let Some(last_block) = content_blocks.last_mut() {
                        // Accumulate text delta
                        if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                            if let Some(existing_text) = last_block.get_mut("text") {
                                if let Some(s) = existing_text.as_str() {
                                    *existing_text = json!(format!("{}{}", s, text));
                                }
                            } else if let Some(obj) = last_block.as_object_mut() {
                                obj.insert("text".to_string(), json!(text));
                            }
                        }
                    }
                }
            }
            "message_delta" => {
                if let Some(delta) = data.get("delta") {
                    stop_reason = delta
                        .get("stop_reason")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                }
                if let Some(usage) = data.get("usage") {
                    usage_data = Some(usage.clone());
                }
            }
            _ => {}
        }
    }

    if !content_blocks.is_empty() || !model.is_empty() {
        Some(json!({
            "model": model,
            "content": content_blocks,
            "stop_reason": stop_reason,
            "usage": usage_data,
            "_note": "Assembled from SSE stream"
        }))
    } else {
        None
    }
}

/// Assemble OpenAI-format SSE stream into a JSON object for debugging/display
///
/// OpenAI SSE format uses:
/// - `choices[].delta.content` for text
/// - `choices[].delta.tool_calls[]` for tool calls
/// - `choices[].finish_reason` for stop reason
/// - `model` at the top level
/// - `usage` for token counts (on final chunk)
///
/// Returns a JSON object with model, content, stop_reason, usage, and a note
/// that this was assembled from an OpenAI-format stream.
pub fn assemble_openai_to_json(body: &str) -> Option<serde_json::Value> {
    let mut content = String::new();
    let mut model = String::new();
    let mut finish_reason: Option<String> = None;
    let mut usage_data: Option<serde_json::Value> = None;
    let mut tool_calls: Vec<serde_json::Value> = Vec::new();
    // Track tool call arguments by index
    let mut tool_args: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    let mut tool_names: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    let mut tool_ids: std::collections::HashMap<usize, String> = std::collections::HashMap::new();

    for line in body.lines() {
        let Some(data) = parse_sse_data_line(line.trim()) else {
            continue;
        };

        // Extract model (available on every chunk)
        if model.is_empty() {
            if let Some(m) = data.get("model").and_then(|v| v.as_str()) {
                model = m.to_string();
            }
        }

        // Extract content from choices[].delta
        if let Some(choices) = data.get("choices").and_then(|v| v.as_array()) {
            for choice in choices {
                // Accumulate text content
                if let Some(text) = choice
                    .get("delta")
                    .and_then(|d| d.get("content"))
                    .and_then(|c| c.as_str())
                {
                    content.push_str(text);
                }

                // Accumulate tool calls
                if let Some(tool_call_deltas) = choice
                    .get("delta")
                    .and_then(|d| d.get("tool_calls"))
                    .and_then(|t| t.as_array())
                {
                    for tc in tool_call_deltas {
                        let index = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

                        // Capture tool ID (only if non-empty)
                        if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                            if !id.is_empty() {
                                tool_ids.insert(index, id.to_string());
                            }
                        }

                        // Capture function name (only if non-empty)
                        if let Some(name) = tc
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                        {
                            if !name.is_empty() {
                                tool_names.insert(index, name.to_string());
                            }
                        }

                        // Accumulate function arguments
                        if let Some(args) = tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|a| a.as_str())
                        {
                            tool_args.entry(index).or_default().push_str(args);
                        }
                    }
                }

                // Extract finish_reason
                if let Some(reason) = choice.get("finish_reason").and_then(|r| r.as_str()) {
                    finish_reason = Some(reason.to_string());
                }
            }
        }

        // Extract usage (usually on final chunk)
        if let Some(usage) = data.get("usage") {
            if !usage.is_null() {
                usage_data = Some(usage.clone());
            }
        }
    }

    // Build tool_calls array from accumulated data
    let max_index = tool_ids
        .keys()
        .chain(tool_names.keys())
        .chain(tool_args.keys())
        .max()
        .copied();
    if let Some(max) = max_index {
        for i in 0..=max {
            if tool_ids.contains_key(&i) || tool_names.contains_key(&i) {
                tool_calls.push(json!({
                    "id": tool_ids.get(&i).cloned().unwrap_or_default(),
                    "type": "function",
                    "function": {
                        "name": tool_names.get(&i).cloned().unwrap_or_default(),
                        "arguments": tool_args.get(&i).cloned().unwrap_or_default()
                    }
                }));
            }
        }
    }

    if !content.is_empty() || !model.is_empty() || !tool_calls.is_empty() {
        Some(json!({
            "model": model,
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": if content.is_empty() { serde_json::Value::Null } else { json!(content) },
                    "tool_calls": if tool_calls.is_empty() { serde_json::Value::Null } else { json!(tool_calls) }
                },
                "finish_reason": finish_reason
            }],
            "usage": usage_data,
            "_note": "Assembled from OpenAI-format SSE stream"
        }))
    } else {
        None
    }
}

// ============================================================================
// Internal Helpers
// ============================================================================

/// Parse an SSE "data:" line into JSON
///
/// Returns None if:
/// - Line doesn't start with "data:"
/// - Data is empty or "[DONE]"
/// - JSON parsing fails
fn parse_sse_data_line(line: &str) -> Option<serde_json::Value> {
    let json_str = line.strip_prefix("data:")?.trim();
    if json_str.is_empty() || json_str == "[DONE]" {
        return None;
    }
    serde_json::from_str(json_str).ok()
}
