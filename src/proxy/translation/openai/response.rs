//! Anthropic → OpenAI response translation
//!
//! Converts Anthropic Messages API responses to OpenAI Chat Completions format.
//!
//! # Implementation Status
//!
//! - **Buffered Translation**: `translate_buffered()` handles non-streaming responses
//! - **Streaming Translation**: `translate_chunk()` and `finalize()` handle SSE streams
//!
//! Both modes are fully integrated in `proxy/mod.rs`.
//!
//! # Streaming (SSE) Event Mapping
//!
//! | Anthropic Event          | OpenAI Event                              |
//! |--------------------------|-------------------------------------------|
//! | `message_start`          | Initial chunk with `role: "assistant"`    |
//! | `content_block_start`    | Tool call header (for tool_use blocks)    |
//! | `content_block_delta`    | `choices[].delta.content` or tool args    |
//! | `content_block_stop`     | Increment chunk_index (for tool indexing) |
//! | `message_delta`          | `choices[].finish_reason`                 |
//! | `message_stop`           | (triggers `finalize()` → `data: [DONE]`)  |
//!
//! # Buffered (JSON) Translation
//!
//! The full response is translated at once, mapping Anthropic's structure
//! to OpenAI's `ChatCompletion` object. Thinking blocks are filtered out.

use crate::proxy::translation::{
    context::{ModelMapping, TranslationContext},
    ApiFormat, ResponseTranslator,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Translates Anthropic Messages responses to OpenAI Chat Completions format
pub struct AnthropicToOpenAiResponse {
    model_mapping: Arc<ModelMapping>,
}

impl AnthropicToOpenAiResponse {
    pub fn new(model_mapping: ModelMapping) -> Self {
        Self {
            model_mapping: Arc::new(model_mapping),
        }
    }
}

impl ResponseTranslator for AnthropicToOpenAiResponse {
    fn name(&self) -> &'static str {
        "anthropic-to-openai-response"
    }

    fn source_format(&self) -> ApiFormat {
        ApiFormat::Anthropic
    }

    fn target_format(&self) -> ApiFormat {
        ApiFormat::OpenAI
    }

    fn translate_buffered(&self, body: &[u8], ctx: &TranslationContext) -> Result<Vec<u8>> {
        let anthropic_response: AnthropicResponse =
            serde_json::from_slice(body).context("Failed to parse Anthropic response")?;

        let openai_response =
            convert_buffered_response(&anthropic_response, ctx, &self.model_mapping);

        serde_json::to_vec(&openai_response).context("Failed to serialize OpenAI response")
    }

    fn translate_chunk(&self, chunk: &[u8], ctx: &mut TranslationContext) -> Result<Vec<u8>> {
        let chunk_str = std::str::from_utf8(chunk).context("Invalid UTF-8 in chunk")?;

        // Append to line buffer for handling partial lines
        ctx.line_buffer.push_str(chunk_str);

        let mut output = Vec::new();

        // Process complete lines
        while let Some(newline_pos) = ctx.line_buffer.find('\n') {
            let line = ctx.line_buffer[..newline_pos].trim().to_string();
            ctx.line_buffer = ctx.line_buffer[newline_pos + 1..].to_string();

            if line.is_empty() {
                continue;
            }

            // Parse SSE line (strip "data:" prefix then trim whitespace, like assembler)
            if let Some(data) = line.strip_prefix("data:").map(|s| s.trim()) {
                if let Some(translated) = self.translate_sse_data(data, ctx)? {
                    output.extend(translated);
                }
            }
            // Skip "event:" lines - we only care about data
        }

        Ok(output)
    }

    fn finalize(&self, ctx: &TranslationContext) -> Option<Vec<u8>> {
        if ctx.needs_response_translation() {
            // OpenAI streams end with "data: [DONE]"
            Some(b"data: [DONE]\n\n".to_vec())
        } else {
            None
        }
    }
}

impl AnthropicToOpenAiResponse {
    /// Translate a single SSE data payload from Anthropic to OpenAI format.
    ///
    /// This method handles the actual event-by-event translation from Anthropic's
    /// SSE format to OpenAI's format. Called by `translate_chunk()` for each
    /// complete `data: {...}` line.
    ///
    /// # Event Handling
    ///
    /// - `message_start`: Extracts model, sends initial chunk with role
    /// - `content_block_start`: Sends tool call header for tool_use blocks
    /// - `content_block_delta`: Sends text or tool argument increments
    /// - `content_block_stop`: Increments chunk_index for tool ordering
    /// - `message_delta`: Captures finish_reason, sends final content chunk
    /// - `message_stop`: No-op (finalize() sends [DONE])
    fn translate_sse_data(
        &self,
        data: &str,
        ctx: &mut TranslationContext,
    ) -> Result<Option<Vec<u8>>> {
        // Parse the JSON data
        let event: serde_json::Value =
            serde_json::from_str(data).context("Failed to parse SSE data")?;

        let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match event_type {
            "message_start" => {
                // Extract model and message ID
                if let Some(message) = event.get("message") {
                    if let Some(model) = message.get("model").and_then(|m| m.as_str()) {
                        ctx.response_model = Some(model.to_string());
                    }
                }

                // Send initial chunk with role
                let chunk = OpenAiStreamChunk {
                    id: ctx.completion_id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    created: current_timestamp(),
                    model: ctx.response_model_name(),
                    choices: vec![OpenAiStreamChoice {
                        index: 0,
                        delta: OpenAiDelta {
                            role: Some("assistant".to_string()),
                            content: None,
                            tool_calls: None,
                        },
                        finish_reason: None,
                    }],
                    usage: None,
                };

                ctx.sent_initial = true;
                Ok(Some(format_sse_chunk(&chunk)?))
            }

            "content_block_start" => {
                // Check if this is a tool_use block
                if let Some(content_block) = event.get("content_block") {
                    if content_block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                        let tool_call = OpenAiToolCallDelta {
                            index: ctx.chunk_index as usize,
                            id: content_block
                                .get("id")
                                .and_then(|i| i.as_str())
                                .map(String::from),
                            call_type: Some("function".to_string()),
                            function: Some(OpenAiFunctionDelta {
                                name: content_block
                                    .get("name")
                                    .and_then(|n| n.as_str())
                                    .map(String::from),
                                arguments: None,
                            }),
                        };

                        let chunk = OpenAiStreamChunk {
                            id: ctx.completion_id.clone(),
                            object: "chat.completion.chunk".to_string(),
                            created: current_timestamp(),
                            model: ctx.response_model_name(),
                            choices: vec![OpenAiStreamChoice {
                                index: 0,
                                delta: OpenAiDelta {
                                    role: None,
                                    content: None,
                                    tool_calls: Some(vec![tool_call]),
                                },
                                finish_reason: None,
                            }],
                            usage: None,
                        };

                        return Ok(Some(format_sse_chunk(&chunk)?));
                    }
                }
                Ok(None)
            }

            "content_block_delta" => {
                if let Some(delta) = event.get("delta") {
                    let delta_type = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");

                    match delta_type {
                        "text_delta" => {
                            let text = delta.get("text").and_then(|t| t.as_str()).unwrap_or("");

                            if !text.is_empty() {
                                ctx.accumulated_content.push_str(text);

                                let chunk = OpenAiStreamChunk {
                                    id: ctx.completion_id.clone(),
                                    object: "chat.completion.chunk".to_string(),
                                    created: current_timestamp(),
                                    model: ctx.response_model_name(),
                                    choices: vec![OpenAiStreamChoice {
                                        index: 0,
                                        delta: OpenAiDelta {
                                            role: None,
                                            content: Some(text.to_string()),
                                            tool_calls: None,
                                        },
                                        finish_reason: None,
                                    }],
                                    usage: None,
                                };

                                return Ok(Some(format_sse_chunk(&chunk)?));
                            }
                        }
                        "input_json_delta" => {
                            // Tool use argument streaming
                            let partial_json = delta
                                .get("partial_json")
                                .and_then(|j| j.as_str())
                                .unwrap_or("");

                            if !partial_json.is_empty() {
                                let tool_call = OpenAiToolCallDelta {
                                    index: ctx.chunk_index as usize,
                                    id: None,
                                    call_type: None,
                                    function: Some(OpenAiFunctionDelta {
                                        name: None,
                                        arguments: Some(partial_json.to_string()),
                                    }),
                                };

                                let chunk = OpenAiStreamChunk {
                                    id: ctx.completion_id.clone(),
                                    object: "chat.completion.chunk".to_string(),
                                    created: current_timestamp(),
                                    model: ctx.response_model_name(),
                                    choices: vec![OpenAiStreamChoice {
                                        index: 0,
                                        delta: OpenAiDelta {
                                            role: None,
                                            content: None,
                                            tool_calls: Some(vec![tool_call]),
                                        },
                                        finish_reason: None,
                                    }],
                                    usage: None,
                                };

                                return Ok(Some(format_sse_chunk(&chunk)?));
                            }
                        }
                        "thinking_delta" => {
                            // Skip thinking blocks - not part of OpenAI format
                        }
                        _ => {}
                    }
                }
                Ok(None)
            }

            "content_block_stop" => {
                ctx.chunk_index += 1;
                Ok(None)
            }

            "message_delta" => {
                // Extract stop_reason
                if let Some(delta) = event.get("delta") {
                    if let Some(stop_reason) = delta.get("stop_reason").and_then(|s| s.as_str()) {
                        ctx.finish_reason = Some(convert_stop_reason(stop_reason));
                    }
                }

                // Send chunk with finish_reason
                if let Some(ref finish_reason) = ctx.finish_reason {
                    let chunk = OpenAiStreamChunk {
                        id: ctx.completion_id.clone(),
                        object: "chat.completion.chunk".to_string(),
                        created: current_timestamp(),
                        model: ctx.response_model_name(),
                        choices: vec![OpenAiStreamChoice {
                            index: 0,
                            delta: OpenAiDelta {
                                role: None,
                                content: None,
                                tool_calls: None,
                            },
                            finish_reason: Some(finish_reason.clone()),
                        }],
                        usage: None,
                    };

                    return Ok(Some(format_sse_chunk(&chunk)?));
                }
                Ok(None)
            }

            "message_stop" => {
                // Final event - we'll send [DONE] in finalize()
                Ok(None)
            }

            "ping" | "error" => {
                // Skip ping events, pass through errors
                if event_type == "error" {
                    tracing::warn!("Anthropic SSE error: {:?}", event);
                }
                Ok(None)
            }

            _ => {
                tracing::trace!("Ignoring unknown Anthropic SSE event type: {}", event_type);
                Ok(None)
            }
        }
    }
}

// ============================================================================
// Anthropic Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    response_type: String,
    role: String,
    content: Vec<AnthropicContentBlock>,
    model: String,
    stop_reason: Option<String>,
    #[allow(dead_code)]
    stop_sequence: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "thinking")]
    Thinking {
        #[allow(dead_code)]
        thinking: String,
    },
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
    #[serde(default)]
    #[allow(dead_code)]
    cache_creation_input_tokens: Option<u32>,
    #[serde(default)]
    #[allow(dead_code)]
    cache_read_input_tokens: Option<u32>,
}

// ============================================================================
// OpenAI Response Types
// ============================================================================

#[derive(Debug, Serialize)]
struct OpenAiChatCompletion {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<OpenAiChoice>,
    usage: OpenAiUsage,
    system_fingerprint: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenAiChoice {
    index: u32,
    message: OpenAiMessage,
    finish_reason: String,
}

#[derive(Debug, Serialize)]
struct OpenAiMessage {
    role: String,
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Serialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiFunction,
}

#[derive(Debug, Serialize)]
struct OpenAiFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// ============================================================================
// Streaming Types
// ============================================================================
//
// These types support the streaming translation logic in `translate_chunk()`.
// They mirror OpenAI's streaming format:
// - Each SSE chunk contains a `ChatCompletionChunk` object
// - The `delta` field contains incremental content (vs `message` in buffered)
// - Tool calls stream incrementally with `index` for ordering
// ============================================================================

/// OpenAI streaming chunk format (`chat.completion.chunk`)
///
/// Sent as `data: {...}\n\n` in SSE stream. Each chunk contains partial content.
#[derive(Debug, Serialize)]
struct OpenAiStreamChunk {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<OpenAiStreamChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Serialize)]
struct OpenAiStreamChoice {
    index: u32,
    delta: OpenAiDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenAiDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCallDelta>>,
}

#[derive(Debug, Serialize)]
struct OpenAiToolCallDelta {
    index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    call_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function: Option<OpenAiFunctionDelta>,
}

#[derive(Debug, Serialize)]
struct OpenAiFunctionDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
}

// ============================================================================
// Conversion Functions
// ============================================================================

/// Convert a complete buffered Anthropic response to OpenAI format
fn convert_buffered_response(
    response: &AnthropicResponse,
    ctx: &TranslationContext,
    model_mapping: &ModelMapping,
) -> OpenAiChatCompletion {
    // Collect text content and tool calls
    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<OpenAiToolCall> = Vec::new();

    for block in &response.content {
        match block {
            AnthropicContentBlock::Text { text } => {
                text_parts.push(text.clone());
            }
            AnthropicContentBlock::ToolUse { id, name, input } => {
                tool_calls.push(OpenAiToolCall {
                    id: id.clone(),
                    call_type: "function".to_string(),
                    function: OpenAiFunction {
                        name: name.clone(),
                        arguments: serde_json::to_string(input).unwrap_or_default(),
                    },
                });
            }
            AnthropicContentBlock::Thinking { .. } => {
                // Skip thinking blocks in OpenAI output
            }
        }
    }

    let content = if text_parts.is_empty() {
        None
    } else {
        Some(text_parts.join(""))
    };

    let message = OpenAiMessage {
        role: response.role.clone(),
        content,
        tool_calls: if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        },
    };

    // Determine model name
    let model = ctx
        .original_model
        .clone()
        .unwrap_or_else(|| model_mapping.to_openai(&response.model));

    OpenAiChatCompletion {
        id: format!("chatcmpl-{}", response.id.replace("msg_", "")),
        object: "chat.completion".to_string(),
        created: current_timestamp(),
        model,
        choices: vec![OpenAiChoice {
            index: 0,
            message,
            finish_reason: convert_stop_reason(response.stop_reason.as_deref().unwrap_or("stop")),
        }],
        usage: OpenAiUsage {
            prompt_tokens: response.usage.input_tokens,
            completion_tokens: response.usage.output_tokens,
            total_tokens: response.usage.input_tokens + response.usage.output_tokens,
        },
        system_fingerprint: None,
    }
}

/// Convert Anthropic stop_reason to OpenAI finish_reason
fn convert_stop_reason(stop_reason: &str) -> String {
    match stop_reason {
        "end_turn" => "stop".to_string(),
        "max_tokens" => "length".to_string(),
        "stop_sequence" => "stop".to_string(),
        "tool_use" => "tool_calls".to_string(),
        _ => "stop".to_string(),
    }
}

/// Get current Unix timestamp
fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Format a streaming chunk as SSE data line
///
/// Produces `data: {...}\n\n` format expected by OpenAI-compatible clients.
fn format_sse_chunk(chunk: &OpenAiStreamChunk) -> Result<Vec<u8>> {
    let json = serde_json::to_string(chunk)?;
    Ok(format!("data: {}\n\n", json).into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_translator() -> AnthropicToOpenAiResponse {
        AnthropicToOpenAiResponse::new(ModelMapping::new())
    }

    #[test]
    fn test_buffered_response_translation() {
        let translator = make_translator();
        let ctx = TranslationContext::new(
            ApiFormat::OpenAI,
            ApiFormat::Anthropic,
            Arc::new(ModelMapping::new()),
            false,
        )
        .with_original_model("gpt-4".to_string());

        let anthropic_body = r#"{
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello!"}],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        }"#;

        let translated = translator
            .translate_buffered(anthropic_body.as_bytes(), &ctx)
            .unwrap();
        let openai: serde_json::Value = serde_json::from_slice(&translated).unwrap();

        assert_eq!(openai["object"], "chat.completion");
        assert_eq!(openai["model"], "gpt-4");
        assert_eq!(openai["choices"][0]["message"]["content"], "Hello!");
        assert_eq!(openai["choices"][0]["finish_reason"], "stop");
        assert_eq!(openai["usage"]["prompt_tokens"], 10);
        assert_eq!(openai["usage"]["completion_tokens"], 5);
    }

    #[test]
    fn test_tool_use_response_translation() {
        let translator = make_translator();
        let ctx = TranslationContext::new(
            ApiFormat::OpenAI,
            ApiFormat::Anthropic,
            Arc::new(ModelMapping::new()),
            false,
        );

        let anthropic_body = r#"{
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": "tool_123",
                "name": "get_weather",
                "input": {"city": "London"}
            }],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 10, "output_tokens": 20}
        }"#;

        let translated = translator
            .translate_buffered(anthropic_body.as_bytes(), &ctx)
            .unwrap();
        let openai: serde_json::Value = serde_json::from_slice(&translated).unwrap();

        assert_eq!(openai["choices"][0]["finish_reason"], "tool_calls");
        let tool_call = &openai["choices"][0]["message"]["tool_calls"][0];
        assert_eq!(tool_call["id"], "tool_123");
        assert_eq!(tool_call["type"], "function");
        assert_eq!(tool_call["function"]["name"], "get_weather");
    }

    #[test]
    fn test_stop_reason_conversion() {
        assert_eq!(convert_stop_reason("end_turn"), "stop");
        assert_eq!(convert_stop_reason("max_tokens"), "length");
        assert_eq!(convert_stop_reason("tool_use"), "tool_calls");
        assert_eq!(convert_stop_reason("stop_sequence"), "stop");
    }

    #[test]
    fn test_streaming_text_delta() {
        let translator = make_translator();
        let mut ctx = TranslationContext::new(
            ApiFormat::OpenAI,
            ApiFormat::Anthropic,
            Arc::new(ModelMapping::new()),
            true,
        );

        // Simulate message_start
        let chunk1 = b"data: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-sonnet-4-20250514\"}}\n\n";
        let result1 = translator.translate_chunk(chunk1, &mut ctx).unwrap();
        assert!(!result1.is_empty());

        // Simulate text delta
        let chunk2 = b"data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n";
        let result2 = translator.translate_chunk(chunk2, &mut ctx).unwrap();

        let result_str = String::from_utf8(result2).unwrap();
        assert!(result_str.contains("\"content\":\"Hello\""));
    }

    #[test]
    fn test_finalize_adds_done() {
        let translator = make_translator();
        let ctx = TranslationContext::new(
            ApiFormat::OpenAI,
            ApiFormat::Anthropic,
            Arc::new(ModelMapping::new()),
            true,
        );

        let done = translator.finalize(&ctx).unwrap();
        assert_eq!(done, b"data: [DONE]\n\n");
    }
}
