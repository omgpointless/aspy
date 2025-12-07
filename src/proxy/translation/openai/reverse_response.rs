//! OpenAI → Anthropic response translation
//!
//! Converts OpenAI Chat Completions responses to Anthropic Messages format.
//! Use case: Translating responses from OpenRouter/GPT backends back to Claude Code.
//!
//! # Streaming (SSE) Event Mapping (Reverse of response.rs)
//!
//! | OpenAI Event                    | Anthropic Event                      |
//! |---------------------------------|--------------------------------------|
//! | First chunk with `role`         | `message_start`                      |
//! | `delta.content`                 | `content_block_delta` (text_delta)   |
//! | `delta.tool_calls[].id+name`    | `content_block_start` (tool_use)     |
//! | `delta.tool_calls[].arguments`  | `content_block_delta` (input_json)   |
//! | `finish_reason`                 | `message_delta` + `stop_reason`      |
//! | `data: [DONE]`                  | `message_stop`                       |
//!
//! # Buffered (JSON) Translation
//!
//! The full response is translated at once, mapping OpenAI's `ChatCompletion`
//! structure to Anthropic's `Message` object.

use crate::proxy::translation::{
    context::{ModelMapping, TranslationContext},
    ApiFormat, ResponseTranslator,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Translates OpenAI Chat Completions responses to Anthropic Messages format
pub struct OpenAiToAnthropicResponse {
    model_mapping: Arc<ModelMapping>,
}

impl OpenAiToAnthropicResponse {
    pub fn new(model_mapping: ModelMapping) -> Self {
        Self {
            model_mapping: Arc::new(model_mapping),
        }
    }
}

impl ResponseTranslator for OpenAiToAnthropicResponse {
    fn name(&self) -> &'static str {
        "openai-to-anthropic-response"
    }

    fn source_format(&self) -> ApiFormat {
        ApiFormat::OpenAI
    }

    fn target_format(&self) -> ApiFormat {
        ApiFormat::Anthropic
    }

    fn translate_buffered(&self, body: &[u8], ctx: &TranslationContext) -> Result<Vec<u8>> {
        let openai_response: OpenAiChatCompletion =
            serde_json::from_slice(body).context("Failed to parse OpenAI response")?;

        let anthropic_response =
            convert_buffered_response(&openai_response, ctx, &self.model_mapping);

        serde_json::to_vec(&anthropic_response).context("Failed to serialize Anthropic response")
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

            // Parse SSE line
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    // OpenAI stream end - emit message_stop
                    output.extend(format_sse_event(
                        "message_stop",
                        &MessageStopEvent {
                            event_type: "message_stop".to_string(),
                        },
                    )?);
                } else if let Some(translated) = self.translate_sse_data(data, ctx)? {
                    output.extend(translated);
                }
            }
            // Skip other SSE lines (event:, etc.)
        }

        Ok(output)
    }

    fn finalize(&self, ctx: &TranslationContext) -> Option<Vec<u8>> {
        if ctx.needs_response_translation() {
            // Anthropic doesn't have an explicit terminator like OpenAI's [DONE]
            // message_stop is sent when we see [DONE] in translate_chunk
            None
        } else {
            None
        }
    }
}

impl OpenAiToAnthropicResponse {
    /// Translate a single SSE data payload from OpenAI to Anthropic format
    fn translate_sse_data(
        &self,
        data: &str,
        ctx: &mut TranslationContext,
    ) -> Result<Option<Vec<u8>>> {
        let chunk: OpenAiStreamChunk =
            serde_json::from_str(data).context("Failed to parse OpenAI SSE data")?;

        let mut output = Vec::new();

        // Handle first chunk - emit message_start
        if !ctx.sent_initial {
            let model = ctx
                .original_model
                .clone()
                .unwrap_or_else(|| self.model_mapping.to_anthropic(&chunk.model));

            let message_start = MessageStartEvent {
                event_type: "message_start".to_string(),
                message: MessageStartPayload {
                    id: format!("msg_{}", chunk.id.replace("chatcmpl-", "")),
                    msg_type: "message".to_string(),
                    role: "assistant".to_string(),
                    content: vec![],
                    model,
                    stop_reason: None,
                    stop_sequence: None,
                    usage: AnthropicUsage {
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                },
            };

            output.extend(format_sse_event("message_start", &message_start)?);
            ctx.sent_initial = true;
        }

        // Process choices
        for choice in &chunk.choices {
            // Handle role (first chunk indicator)
            if choice.delta.role.is_some() {
                // Role already handled in message_start
            }

            // Handle text content
            if let Some(content) = &choice.delta.content {
                if !content.is_empty() {
                    // If we haven't started a text block yet, start one
                    if ctx.chunk_index == 0 {
                        // Start text block if this is first content
                        if ctx.accumulated_content.is_empty() {
                            let block_start = ContentBlockStartEvent {
                                event_type: "content_block_start".to_string(),
                                index: ctx.chunk_index,
                                content_block: ContentBlockPayload::Text {
                                    text: String::new(),
                                },
                            };
                            output.extend(format_sse_event("content_block_start", &block_start)?);
                        }
                    }

                    ctx.accumulated_content.push_str(content);

                    let delta_event = ContentBlockDeltaEvent {
                        event_type: "content_block_delta".to_string(),
                        index: ctx.chunk_index,
                        delta: ContentDelta::TextDelta {
                            text: content.clone(),
                        },
                    };
                    output.extend(format_sse_event("content_block_delta", &delta_event)?);
                }
            }

            // Handle tool calls
            if let Some(tool_calls) = &choice.delta.tool_calls {
                for tc in tool_calls {
                    // New tool call starting (has id and name)
                    if tc.id.is_some()
                        || tc
                            .function
                            .as_ref()
                            .map(|f| f.name.is_some())
                            .unwrap_or(false)
                    {
                        // Close previous block if we were in text
                        if !ctx.accumulated_content.is_empty() {
                            let block_stop = ContentBlockStopEvent {
                                event_type: "content_block_stop".to_string(),
                                index: ctx.chunk_index,
                            };
                            output.extend(format_sse_event("content_block_stop", &block_stop)?);
                            ctx.chunk_index += 1;
                            ctx.accumulated_content.clear();
                        }

                        // Start tool_use block
                        let block_start = ContentBlockStartEvent {
                            event_type: "content_block_start".to_string(),
                            index: ctx.chunk_index,
                            content_block: ContentBlockPayload::ToolUse {
                                id: tc.id.clone().unwrap_or_default(),
                                name: tc
                                    .function
                                    .as_ref()
                                    .and_then(|f| f.name.clone())
                                    .unwrap_or_default(),
                                input: serde_json::json!({}),
                            },
                        };
                        output.extend(format_sse_event("content_block_start", &block_start)?);
                    }

                    // Streaming arguments
                    if let Some(func) = &tc.function {
                        if let Some(args) = &func.arguments {
                            if !args.is_empty() {
                                let delta_event = ContentBlockDeltaEvent {
                                    event_type: "content_block_delta".to_string(),
                                    index: ctx.chunk_index,
                                    delta: ContentDelta::InputJsonDelta {
                                        partial_json: args.clone(),
                                    },
                                };
                                output
                                    .extend(format_sse_event("content_block_delta", &delta_event)?);
                            }
                        }
                    }
                }
            }

            // Handle finish reason
            if let Some(finish_reason) = &choice.finish_reason {
                // Close any open content block
                let block_stop = ContentBlockStopEvent {
                    event_type: "content_block_stop".to_string(),
                    index: ctx.chunk_index,
                };
                output.extend(format_sse_event("content_block_stop", &block_stop)?);

                // Send message_delta with stop_reason
                let stop_reason = convert_finish_reason(finish_reason);
                ctx.finish_reason = Some(stop_reason.clone());

                let message_delta = MessageDeltaEvent {
                    event_type: "message_delta".to_string(),
                    delta: MessageDelta {
                        stop_reason,
                        stop_sequence: None,
                    },
                    usage: DeltaUsage { output_tokens: 0 },
                };
                output.extend(format_sse_event("message_delta", &message_delta)?);
            }
        }

        if output.is_empty() {
            Ok(None)
        } else {
            Ok(Some(output))
        }
    }
}

// ============================================================================
// OpenAI Response Types (Input - Deserialize)
// ============================================================================

#[derive(Debug, Deserialize)]
struct OpenAiChatCompletion {
    id: String,
    /// Some providers (e.g., ZAI/GLM) omit this field
    #[serde(default)]
    #[allow(dead_code)]
    object: Option<String>,
    #[allow(dead_code)]
    created: u64,
    model: String,
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    #[allow(dead_code)]
    index: u32,
    message: OpenAiMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    #[allow(dead_code)]
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    call_type: String,
    function: OpenAiFunction,
}

#[derive(Debug, Deserialize)]
struct OpenAiFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
}

// Streaming types
#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    id: String,
    /// Some providers (e.g., ZAI/GLM) omit this field
    #[serde(default)]
    #[allow(dead_code)]
    object: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    created: Option<u64>,
    model: String,
    choices: Vec<OpenAiStreamChoice>,
    #[serde(default)]
    #[allow(dead_code)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    #[allow(dead_code)]
    index: u32,
    delta: OpenAiDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiDelta {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCallDelta {
    #[allow(dead_code)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "type")]
    #[serde(default)]
    #[allow(dead_code)]
    call_type: Option<String>,
    #[serde(default)]
    function: Option<OpenAiFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenAiFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

// ============================================================================
// Anthropic Response Types (Output - Serialize)
// ============================================================================

#[derive(Debug, Serialize)]
struct AnthropicResponse {
    id: String,
    #[serde(rename = "type")]
    response_type: String,
    role: String,
    content: Vec<AnthropicContentBlock>,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequence: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Serialize)]
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
}

#[derive(Debug, Serialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

// Streaming event types
#[derive(Debug, Serialize)]
struct MessageStartEvent {
    #[serde(rename = "type")]
    event_type: String,
    message: MessageStartPayload,
}

#[derive(Debug, Serialize)]
struct MessageStartPayload {
    id: String,
    #[serde(rename = "type")]
    msg_type: String,
    role: String,
    content: Vec<serde_json::Value>,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequence: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Serialize)]
struct ContentBlockStartEvent {
    #[serde(rename = "type")]
    event_type: String,
    index: u32,
    content_block: ContentBlockPayload,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ContentBlockPayload {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Serialize)]
struct ContentBlockDeltaEvent {
    #[serde(rename = "type")]
    event_type: String,
    index: u32,
    delta: ContentDelta,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ContentDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Serialize)]
struct ContentBlockStopEvent {
    #[serde(rename = "type")]
    event_type: String,
    index: u32,
}

#[derive(Debug, Serialize)]
struct MessageDeltaEvent {
    #[serde(rename = "type")]
    event_type: String,
    delta: MessageDelta,
    usage: DeltaUsage,
}

#[derive(Debug, Serialize)]
struct MessageDelta {
    stop_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequence: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeltaUsage {
    output_tokens: u32,
}

#[derive(Debug, Serialize)]
struct MessageStopEvent {
    #[serde(rename = "type")]
    event_type: String,
}

// ============================================================================
// Conversion Functions
// ============================================================================

/// Convert a complete buffered OpenAI response to Anthropic format
fn convert_buffered_response(
    response: &OpenAiChatCompletion,
    ctx: &TranslationContext,
    model_mapping: &ModelMapping,
) -> AnthropicResponse {
    let choice = response.choices.first();

    let mut content: Vec<AnthropicContentBlock> = Vec::new();

    if let Some(choice) = choice {
        // Add text content if present
        if let Some(text) = &choice.message.content {
            if !text.is_empty() {
                content.push(AnthropicContentBlock::Text { text: text.clone() });
            }
        }

        // Add tool calls if present
        if let Some(tool_calls) = &choice.message.tool_calls {
            for tc in tool_calls {
                let input: serde_json::Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));

                content.push(AnthropicContentBlock::ToolUse {
                    id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    input,
                });
            }
        }
    }

    let stop_reason = choice
        .and_then(|c| c.finish_reason.as_ref())
        .map(|r| convert_finish_reason(r));

    let usage = response
        .usage
        .as_ref()
        .map(|u| AnthropicUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
        })
        .unwrap_or(AnthropicUsage {
            input_tokens: 0,
            output_tokens: 0,
        });

    // Determine model name
    let model = ctx
        .original_model
        .clone()
        .unwrap_or_else(|| model_mapping.to_anthropic(&response.model));

    AnthropicResponse {
        id: format!("msg_{}", response.id.replace("chatcmpl-", "")),
        response_type: "message".to_string(),
        role: "assistant".to_string(),
        content,
        model,
        stop_reason,
        stop_sequence: None,
        usage,
    }
}

/// Convert OpenAI finish_reason to Anthropic stop_reason
fn convert_finish_reason(finish_reason: &str) -> String {
    match finish_reason {
        "stop" => "end_turn".to_string(),
        "length" => "max_tokens".to_string(),
        "tool_calls" => "tool_use".to_string(),
        "content_filter" => "end_turn".to_string(),
        _ => "end_turn".to_string(),
    }
}

/// Format an Anthropic SSE event
fn format_sse_event<T: Serialize>(event_type: &str, data: &T) -> Result<Vec<u8>> {
    let json = serde_json::to_string(data)?;
    Ok(format!("event: {}\ndata: {}\n\n", event_type, json).into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_translator() -> OpenAiToAnthropicResponse {
        OpenAiToAnthropicResponse::new(ModelMapping::new())
    }

    #[test]
    fn test_buffered_response_translation() {
        let translator = make_translator();
        let ctx = TranslationContext::new(
            ApiFormat::Anthropic,
            ApiFormat::OpenAI,
            Arc::new(ModelMapping::new()),
            false,
        )
        .with_original_model("claude-sonnet-4-20250514".to_string());

        let openai_body = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "gpt-4-turbo",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello!"
                },
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        }"#;

        let translated = translator
            .translate_buffered(openai_body.as_bytes(), &ctx)
            .unwrap();
        let anthropic: serde_json::Value = serde_json::from_slice(&translated).unwrap();

        assert_eq!(anthropic["type"], "message");
        assert_eq!(anthropic["role"], "assistant");
        assert_eq!(anthropic["model"], "claude-sonnet-4-20250514");
        assert_eq!(anthropic["content"][0]["type"], "text");
        assert_eq!(anthropic["content"][0]["text"], "Hello!");
        assert_eq!(anthropic["stop_reason"], "end_turn");
        assert_eq!(anthropic["usage"]["input_tokens"], 10);
        assert_eq!(anthropic["usage"]["output_tokens"], 5);
    }

    #[test]
    fn test_tool_calls_response_translation() {
        let translator = make_translator();
        let ctx = TranslationContext::new(
            ApiFormat::Anthropic,
            ApiFormat::OpenAI,
            Arc::new(ModelMapping::new()),
            false,
        );

        let openai_body = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "gpt-4-turbo",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"city\":\"London\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 20, "total_tokens": 30}
        }"#;

        let translated = translator
            .translate_buffered(openai_body.as_bytes(), &ctx)
            .unwrap();
        let anthropic: serde_json::Value = serde_json::from_slice(&translated).unwrap();

        assert_eq!(anthropic["stop_reason"], "tool_use");
        assert_eq!(anthropic["content"][0]["type"], "tool_use");
        assert_eq!(anthropic["content"][0]["id"], "call_abc");
        assert_eq!(anthropic["content"][0]["name"], "get_weather");
    }

    #[test]
    fn test_finish_reason_conversion() {
        assert_eq!(convert_finish_reason("stop"), "end_turn");
        assert_eq!(convert_finish_reason("length"), "max_tokens");
        assert_eq!(convert_finish_reason("tool_calls"), "tool_use");
    }

    #[test]
    fn test_streaming_text_delta() {
        let translator = make_translator();
        let mut ctx = TranslationContext::new(
            ApiFormat::Anthropic,
            ApiFormat::OpenAI,
            Arc::new(ModelMapping::new()),
            true,
        );

        // Simulate first chunk with role
        let chunk1 = b"data: {\"id\":\"chatcmpl-123\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\n";
        let result1 = translator.translate_chunk(chunk1, &mut ctx).unwrap();
        assert!(!result1.is_empty());

        let result_str1 = String::from_utf8(result1).unwrap();
        assert!(result_str1.contains("message_start"));

        // Simulate text delta
        let chunk2 = b"data: {\"id\":\"chatcmpl-123\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n";
        let result2 = translator.translate_chunk(chunk2, &mut ctx).unwrap();

        let result_str2 = String::from_utf8(result2).unwrap();
        assert!(result_str2.contains("text_delta"));
        assert!(result_str2.contains("Hello"));
    }

    #[test]
    fn test_streaming_done() {
        let translator = make_translator();
        let mut ctx = TranslationContext::new(
            ApiFormat::Anthropic,
            ApiFormat::OpenAI,
            Arc::new(ModelMapping::new()),
            true,
        );
        ctx.sent_initial = true;

        // Simulate [DONE]
        let chunk = b"data: [DONE]\n\n";
        let result = translator.translate_chunk(chunk, &mut ctx).unwrap();

        let result_str = String::from_utf8(result).unwrap();
        assert!(result_str.contains("message_stop"));
    }
}
