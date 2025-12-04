//! OpenAI → Anthropic request translation
//!
//! Converts OpenAI Chat Completions API requests to Anthropic Messages API format.
//!
//! # Key Differences
//!
//! | OpenAI                          | Anthropic                        |
//! |---------------------------------|----------------------------------|
//! | `messages[].role: "system"`     | Top-level `system` field         |
//! | `max_tokens` (optional)         | `max_tokens` (required)          |
//! | `temperature` (0-2)             | `temperature` (0-1)              |
//! | `top_p`                         | `top_p`                          |
//! | `stop` (string/array)           | `stop_sequences` (array)         |
//! | `stream`                        | `stream`                         |
//! | `tools`                         | `tools` (similar structure)      |
//! | `tool_choice`                   | `tool_choice`                    |

use crate::proxy::translation::{
    context::{ModelMapping, TranslationContext},
    ApiFormat, RequestTranslator,
};
use anyhow::{Context, Result};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Translates OpenAI Chat Completions requests to Anthropic Messages format
pub struct OpenAiToAnthropicRequest {
    model_mapping: Arc<ModelMapping>,
}

impl OpenAiToAnthropicRequest {
    pub fn new(model_mapping: ModelMapping) -> Self {
        Self {
            model_mapping: Arc::new(model_mapping),
        }
    }
}

impl RequestTranslator for OpenAiToAnthropicRequest {
    fn name(&self) -> &'static str {
        "openai-to-anthropic-request"
    }

    fn source_format(&self) -> ApiFormat {
        ApiFormat::OpenAI
    }

    fn target_format(&self) -> ApiFormat {
        ApiFormat::Anthropic
    }

    fn translate(
        &self,
        body: &[u8],
        _headers: &HeaderMap,
    ) -> Result<(Vec<u8>, TranslationContext)> {
        let openai_request: OpenAiChatRequest =
            serde_json::from_slice(body).context("Failed to parse OpenAI request")?;

        // Extract system message if present
        let (system, messages) = extract_system_message(&openai_request.messages);

        // Map model name
        let anthropic_model = self.model_mapping.to_anthropic(&openai_request.model);

        // Convert messages
        let anthropic_messages: Vec<AnthropicMessage> =
            messages.into_iter().map(convert_message).collect();

        // Build Anthropic request
        let anthropic_request = AnthropicRequest {
            model: anthropic_model,
            messages: anthropic_messages,
            system,
            max_tokens: openai_request.max_tokens.unwrap_or(4096),
            // Pass temperature through unchanged - most providers use 0-1 range
            temperature: openai_request.temperature,
            top_p: openai_request.top_p,
            top_k: None,
            stop_sequences: convert_stop_sequences(openai_request.stop),
            stream: openai_request.stream,
            tools: openai_request
                .tools
                .map(|tools| tools.into_iter().filter_map(convert_tool).collect()),
            tool_choice: convert_tool_choice(openai_request.tool_choice),
            metadata: None,
        };

        let translated_body = serde_json::to_vec(&anthropic_request)
            .context("Failed to serialize Anthropic request")?;

        // Create translation context
        let ctx = TranslationContext::new(
            ApiFormat::OpenAI,
            ApiFormat::Anthropic,
            self.model_mapping.clone(),
            openai_request.stream.unwrap_or(false),
        )
        .with_original_model(openai_request.model);

        tracing::debug!(
            "Translated OpenAI request: model={} -> {}, messages={}",
            ctx.original_model.as_deref().unwrap_or("unknown"),
            anthropic_request.model,
            anthropic_request.messages.len()
        );

        Ok((translated_body, ctx))
    }
}

// ============================================================================
// OpenAI Request Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(default)]
    max_tokens: Option<u32>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    top_p: Option<f32>,
    #[serde(default)]
    stop: Option<StopSequence>,
    #[serde(default)]
    stream: Option<bool>,
    #[serde(default)]
    tools: Option<Vec<OpenAiTool>>,
    #[serde(default)]
    tool_choice: Option<OpenAiToolChoice>,
    // Ignored fields (not supported by Anthropic)
    #[serde(default)]
    #[allow(dead_code)]
    frequency_penalty: Option<f32>,
    #[serde(default)]
    #[allow(dead_code)]
    presence_penalty: Option<f32>,
    #[serde(default)]
    #[allow(dead_code)]
    logprobs: Option<bool>,
    #[serde(default)]
    #[allow(dead_code)]
    n: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    role: String,
    #[serde(default)]
    content: Option<OpenAiContent>,
    #[serde(default)]
    #[allow(dead_code)]
    name: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(default)]
    tool_call_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OpenAiContent {
    Text(String),
    Parts(Vec<OpenAiContentPart>),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum OpenAiContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Deserialize)]
struct ImageUrl {
    url: String,
    #[serde(default)]
    #[allow(dead_code)]
    detail: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    call_type: String,
    function: OpenAiFunctionCall,
}

#[derive(Debug, Deserialize)]
struct OpenAiFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StopSequence {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Deserialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiFunction,
}

#[derive(Debug, Deserialize)]
struct OpenAiFunction {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    parameters: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OpenAiToolChoice {
    Mode(String), // "auto", "none", "required"
    Specific {
        #[serde(rename = "type")]
        #[allow(dead_code)]
        choice_type: String,
        function: ToolChoiceFunction,
    },
}

#[derive(Debug, Deserialize)]
struct ToolChoiceFunction {
    name: String,
}

// ============================================================================
// Anthropic Request Types
// ============================================================================

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<AnthropicToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<AnthropicContentBlock>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: ImageSource },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Serialize)]
struct ImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    input_schema: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum AnthropicToolChoice {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "any")]
    Any,
    #[serde(rename = "tool")]
    Tool { name: String },
}

// ============================================================================
// Conversion Functions
// ============================================================================

/// Extract system message from OpenAI messages array
fn extract_system_message(messages: &[OpenAiMessage]) -> (Option<String>, Vec<&OpenAiMessage>) {
    let mut system_parts: Vec<String> = Vec::new();
    let mut other_messages: Vec<&OpenAiMessage> = Vec::new();

    for msg in messages {
        if msg.role == "system" {
            if let Some(content) = &msg.content {
                match content {
                    OpenAiContent::Text(text) => system_parts.push(text.clone()),
                    OpenAiContent::Parts(parts) => {
                        for part in parts {
                            if let OpenAiContentPart::Text { text } = part {
                                system_parts.push(text.clone());
                            }
                        }
                    }
                }
            }
        } else {
            other_messages.push(msg);
        }
    }

    let system = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n\n"))
    };

    (system, other_messages)
}

/// Convert a single OpenAI message to Anthropic format
fn convert_message(msg: &OpenAiMessage) -> AnthropicMessage {
    let role = match msg.role.as_str() {
        "assistant" => "assistant",
        "tool" => "user", // Tool results come from "user" in Anthropic
        _ => "user",
    };

    let content = if msg.role == "tool" {
        // Tool result message
        if let (Some(tool_call_id), Some(content)) = (&msg.tool_call_id, &msg.content) {
            let result_text = match content {
                OpenAiContent::Text(text) => text.clone(),
                OpenAiContent::Parts(parts) => parts
                    .iter()
                    .filter_map(|p| match p {
                        OpenAiContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
            AnthropicContent::Blocks(vec![AnthropicContentBlock::ToolResult {
                tool_use_id: tool_call_id.clone(),
                content: result_text,
            }])
        } else {
            AnthropicContent::Text(String::new())
        }
    } else if let Some(tool_calls) = &msg.tool_calls {
        // Assistant message with tool calls
        let mut blocks: Vec<AnthropicContentBlock> = Vec::new();

        // Add text content if present
        if let Some(content) = &msg.content {
            match content {
                OpenAiContent::Text(text) if !text.is_empty() => {
                    blocks.push(AnthropicContentBlock::Text { text: text.clone() });
                }
                OpenAiContent::Parts(parts) => {
                    for part in parts {
                        if let OpenAiContentPart::Text { text } = part {
                            if !text.is_empty() {
                                blocks.push(AnthropicContentBlock::Text { text: text.clone() });
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Add tool use blocks
        for tool_call in tool_calls {
            let input: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)
                .unwrap_or(serde_json::json!({}));
            blocks.push(AnthropicContentBlock::ToolUse {
                id: tool_call.id.clone(),
                name: tool_call.function.name.clone(),
                input,
            });
        }

        AnthropicContent::Blocks(blocks)
    } else {
        // Regular message
        match &msg.content {
            Some(OpenAiContent::Text(text)) => AnthropicContent::Text(text.clone()),
            Some(OpenAiContent::Parts(parts)) => {
                let blocks: Vec<AnthropicContentBlock> = parts
                    .iter()
                    .filter_map(|part| match part {
                        OpenAiContentPart::Text { text } => {
                            Some(AnthropicContentBlock::Text { text: text.clone() })
                        }
                        OpenAiContentPart::ImageUrl { image_url } => {
                            convert_image_url(&image_url.url)
                        }
                    })
                    .collect();
                if blocks.len() == 1 {
                    if let AnthropicContentBlock::Text { text } = &blocks[0] {
                        return AnthropicMessage {
                            role: role.to_string(),
                            content: AnthropicContent::Text(text.clone()),
                        };
                    }
                }
                AnthropicContent::Blocks(blocks)
            }
            None => AnthropicContent::Text(String::new()),
        }
    };

    AnthropicMessage {
        role: role.to_string(),
        content,
    }
}

/// Convert OpenAI image URL to Anthropic image block
fn convert_image_url(url: &str) -> Option<AnthropicContentBlock> {
    // Handle data URLs
    if url.starts_with("data:") {
        // Parse data URL: data:image/png;base64,xxxxx
        let parts: Vec<&str> = url.splitn(2, ',').collect();
        if parts.len() == 2 {
            let header = parts[0];
            let data = parts[1];

            // Extract media type
            let media_type = header
                .trim_start_matches("data:")
                .split(';')
                .next()
                .unwrap_or("image/png")
                .to_string();

            return Some(AnthropicContentBlock::Image {
                source: ImageSource {
                    source_type: "base64".to_string(),
                    media_type,
                    data: data.to_string(),
                },
            });
        }
    }

    // For regular URLs, Anthropic requires base64 data
    // We can't fetch the URL here, so log a warning
    tracing::warn!(
        "Cannot convert external image URL to Anthropic format (requires base64): {}",
        url
    );
    None
}

/// Convert stop sequences
fn convert_stop_sequences(stop: Option<StopSequence>) -> Option<Vec<String>> {
    stop.map(|s| match s {
        StopSequence::Single(s) => vec![s],
        StopSequence::Multiple(v) => v,
    })
}

/// Convert OpenAI tool to Anthropic format
fn convert_tool(tool: OpenAiTool) -> Option<AnthropicTool> {
    if tool.tool_type != "function" {
        return None;
    }

    Some(AnthropicTool {
        name: tool.function.name,
        description: tool.function.description,
        input_schema: tool
            .function
            .parameters
            .unwrap_or(serde_json::json!({"type": "object", "properties": {}})),
    })
}

/// Convert tool choice
fn convert_tool_choice(choice: Option<OpenAiToolChoice>) -> Option<AnthropicToolChoice> {
    choice.map(|c| match c {
        OpenAiToolChoice::Mode(mode) => match mode.as_str() {
            "none" => AnthropicToolChoice::Auto, // No direct equivalent, use auto
            "required" => AnthropicToolChoice::Any,
            _ => AnthropicToolChoice::Auto,
        },
        OpenAiToolChoice::Specific { function, .. } => AnthropicToolChoice::Tool {
            name: function.name,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_translator() -> OpenAiToAnthropicRequest {
        OpenAiToAnthropicRequest::new(ModelMapping::new())
    }

    #[test]
    fn test_simple_request_translation() {
        let translator = make_translator();
        let headers = HeaderMap::new();

        let openai_body = r#"{
            "model": "gpt-4",
            "messages": [
                {"role": "user", "content": "Hello"}
            ]
        }"#;

        let (translated, ctx) = translator
            .translate(openai_body.as_bytes(), &headers)
            .unwrap();
        let anthropic: serde_json::Value = serde_json::from_slice(&translated).unwrap();

        // With empty model mapping, model passes through unchanged
        assert_eq!(anthropic["model"], "gpt-4");
        assert_eq!(anthropic["messages"][0]["role"], "user");
        assert_eq!(anthropic["messages"][0]["content"], "Hello");
        assert_eq!(ctx.original_model, Some("gpt-4".to_string()));
    }

    #[test]
    fn test_system_message_extraction() {
        let translator = make_translator();
        let headers = HeaderMap::new();

        let openai_body = r#"{
            "model": "gpt-4",
            "messages": [
                {"role": "system", "content": "You are helpful"},
                {"role": "user", "content": "Hello"}
            ]
        }"#;

        let (translated, _) = translator
            .translate(openai_body.as_bytes(), &headers)
            .unwrap();
        let anthropic: serde_json::Value = serde_json::from_slice(&translated).unwrap();

        assert_eq!(anthropic["system"], "You are helpful");
        assert_eq!(anthropic["messages"].as_array().unwrap().len(), 1);
        assert_eq!(anthropic["messages"][0]["role"], "user");
    }

    #[test]
    fn test_temperature_passthrough() {
        let translator = make_translator();
        let headers = HeaderMap::new();

        // Temperature passes through unchanged (most providers use 0-1 range)
        let openai_body = r#"{
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "Hi"}],
            "temperature": 0.7
        }"#;

        let (translated, _) = translator
            .translate(openai_body.as_bytes(), &headers)
            .unwrap();
        let anthropic: serde_json::Value = serde_json::from_slice(&translated).unwrap();

        // Temperature passes through unchanged
        assert_eq!(anthropic["temperature"], 0.7);
    }

    #[test]
    fn test_tool_calls_translation() {
        let translator = make_translator();
        let headers = HeaderMap::new();

        let openai_body = r#"{
            "model": "gpt-4",
            "messages": [
                {"role": "user", "content": "What's the weather?"},
                {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"city\": \"London\"}"
                        }
                    }]
                },
                {
                    "role": "tool",
                    "tool_call_id": "call_123",
                    "content": "Sunny, 22°C"
                }
            ]
        }"#;

        let (translated, _) = translator
            .translate(openai_body.as_bytes(), &headers)
            .unwrap();
        let anthropic: serde_json::Value = serde_json::from_slice(&translated).unwrap();

        // Check assistant message has tool_use block
        let assistant_msg = &anthropic["messages"][1];
        assert_eq!(assistant_msg["role"], "assistant");
        let content = assistant_msg["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "tool_use");
        assert_eq!(content[0]["id"], "call_123");
        assert_eq!(content[0]["name"], "get_weather");

        // Check tool result
        let tool_msg = &anthropic["messages"][2];
        assert_eq!(tool_msg["role"], "user");
        let content = tool_msg["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "tool_result");
        assert_eq!(content[0]["tool_use_id"], "call_123");
    }

    #[test]
    fn test_streaming_flag() {
        let translator = make_translator();
        let headers = HeaderMap::new();

        let openai_body = r#"{
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "Hi"}],
            "stream": true
        }"#;

        let (_, ctx) = translator
            .translate(openai_body.as_bytes(), &headers)
            .unwrap();
        assert!(ctx.streaming);
    }
}
