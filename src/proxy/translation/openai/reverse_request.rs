//! Anthropic → OpenAI request translation
//!
//! Converts Anthropic Messages API requests to OpenAI Chat Completions format.
//! Use case: Routing Claude Code requests through OpenRouter/GPT backends.
//!
//! # Key Differences (Reverse of request.rs)
//!
//! | Anthropic                       | OpenAI                           |
//! |---------------------------------|----------------------------------|
//! | Top-level `system` field        | `messages[].role: "system"`      |
//! | `max_tokens` (required)         | `max_tokens` (optional)          |
//! | `temperature` (0-1)             | `temperature` (0-2)              |
//! | `stop_sequences` (array)        | `stop` (string/array)            |
//! | `tools`                         | `tools` (similar structure)      |
//! | `thinking.budget_tokens`        | `reasoning.max_tokens` (passthrough) |

use crate::proxy::translation::{
    context::{ModelMapping, TranslationContext},
    ApiFormat, RequestTranslator,
};
use anyhow::{Context, Result};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Translates Anthropic Messages requests to OpenAI Chat Completions format
pub struct AnthropicToOpenAiRequest {
    model_mapping: Arc<ModelMapping>,
}

impl AnthropicToOpenAiRequest {
    pub fn new(model_mapping: ModelMapping) -> Self {
        Self {
            model_mapping: Arc::new(model_mapping),
        }
    }

    /// Internal translation logic that accepts a ModelMapping reference
    fn translate_internal(
        &self,
        body: &[u8],
        effective_mapping: &ModelMapping,
    ) -> Result<(Vec<u8>, TranslationContext)> {
        let anthropic_request: AnthropicRequest = serde_json::from_slice(body).map_err(|e| {
            // Log a sample of the body to help debug parsing failures
            let body_preview = String::from_utf8_lossy(&body[..body.len().min(500)]);
            tracing::error!(
                "Failed to parse Anthropic request: {} | Body preview: {}...",
                e,
                body_preview
            );
            anyhow::anyhow!("Failed to parse Anthropic request: {}", e)
        })?;

        // Build OpenAI messages array
        let mut openai_messages: Vec<OpenAiMessage> = Vec::new();

        // Prepend system message if present
        if let Some(system) = &anthropic_request.system {
            // Handle both string and array system prompts
            let system_text = match system {
                SystemPrompt::Text(text) => text.clone(),
                SystemPrompt::Blocks(blocks) => blocks
                    .iter()
                    .map(|b| match b {
                        SystemBlock::Text { text } => text.as_str(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
            if !system_text.is_empty() {
                openai_messages.push(OpenAiMessage {
                    role: "system".to_string(),
                    content: Some(OpenAiContent::Text(system_text)),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
        }

        // Convert messages
        for msg in &anthropic_request.messages {
            openai_messages.extend(convert_message(msg));
        }

        // Map model name using effective mapping (may be provider-specific override)
        let openai_model = effective_mapping.to_openai(&anthropic_request.model);

        // Convert thinking to reasoning for OpenRouter passthrough
        let reasoning = anthropic_request
            .thinking
            .as_ref()
            .map(|t| ReasoningConfig {
                max_tokens: t.budget_tokens,
            });

        // Build OpenAI request
        // Note: max_tokens is optional in Anthropic API, passthrough as-is to OpenAI
        let openai_request = OpenAiChatRequest {
            model: openai_model,
            messages: openai_messages,
            max_tokens: anthropic_request.max_tokens,
            // Pass temperature through unchanged - most providers use 0-1 range
            temperature: anthropic_request.temperature,
            top_p: anthropic_request.top_p,
            stop: anthropic_request.stop_sequences,
            stream: anthropic_request.stream,
            tools: anthropic_request
                .tools
                .map(|tools| tools.into_iter().map(convert_tool).collect()),
            tool_choice: anthropic_request.tool_choice.map(convert_tool_choice),
            reasoning,
        };

        let translated_body =
            serde_json::to_vec(&openai_request).context("Failed to serialize OpenAI request")?;

        // Create translation context with the effective mapping
        let ctx = TranslationContext::new(
            ApiFormat::Anthropic,
            ApiFormat::OpenAI,
            Arc::new(effective_mapping.clone()),
            anthropic_request.stream.unwrap_or(false),
        )
        .with_original_model(anthropic_request.model.clone());

        tracing::debug!(
            "Translated Anthropic request: model={} -> {}, messages={}",
            anthropic_request.model,
            openai_request.model,
            openai_request.messages.len()
        );

        Ok((translated_body, ctx))
    }
}

impl RequestTranslator for AnthropicToOpenAiRequest {
    fn name(&self) -> &'static str {
        "anthropic-to-openai-request"
    }

    fn source_format(&self) -> ApiFormat {
        ApiFormat::Anthropic
    }

    fn target_format(&self) -> ApiFormat {
        ApiFormat::OpenAI
    }

    fn translate(
        &self,
        body: &[u8],
        _headers: &HeaderMap,
    ) -> Result<(Vec<u8>, TranslationContext)> {
        self.translate_internal(body, &self.model_mapping)
    }

    fn translate_with_mapping(
        &self,
        body: &[u8],
        _headers: &HeaderMap,
        mapping_override: Option<&ModelMapping>,
    ) -> Result<(Vec<u8>, TranslationContext)> {
        match mapping_override {
            Some(override_mapping) => self.translate_internal(body, override_mapping),
            None => self.translate_internal(body, &self.model_mapping),
        }
    }
}

// ============================================================================
// Anthropic Request Types (Input - Deserialize)
// ============================================================================

#[derive(Debug, Deserialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(default)]
    system: Option<SystemPrompt>,
    #[serde(default)]
    max_tokens: Option<u32>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    top_p: Option<f32>,
    #[serde(default)]
    stop_sequences: Option<Vec<String>>,
    #[serde(default)]
    stream: Option<bool>,
    #[serde(default)]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(default)]
    tool_choice: Option<AnthropicToolChoice>,
    #[serde(default)]
    thinking: Option<ThinkingConfig>,
    // Ignored fields
    #[serde(default)]
    #[allow(dead_code)]
    metadata: Option<serde_json::Value>,
    #[serde(default)]
    #[allow(dead_code)]
    top_k: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SystemPrompt {
    Text(String),
    Blocks(Vec<SystemBlock>),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum SystemBlock {
    #[serde(rename = "text")]
    Text { text: String },
}

#[derive(Debug, Deserialize)]
struct ThinkingConfig {
    #[serde(default)]
    #[allow(dead_code)]
    r#type: Option<String>, // "enabled"
    #[serde(default)]
    budget_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<AnthropicContentBlock>),
}

#[derive(Debug, Deserialize)]
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
        #[serde(default)]
        content: Option<ToolResultContent>,
    },
    // Extended thinking blocks (filtered out - not sent to OpenAI)
    #[serde(rename = "thinking")]
    Thinking {
        #[allow(dead_code)]
        thinking: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ToolResultContent {
    Text(String),
    Blocks(Vec<ToolResultBlock>),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ToolResultBlock {
    #[serde(rename = "text")]
    Text { text: String },
}

#[derive(Debug, Deserialize)]
struct ImageSource {
    /// Image source type (e.g., "base64") - captured for validation, not used directly
    #[serde(rename = "type")]
    _source_type: String,
    media_type: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicTool {
    name: String,
    #[serde(default)]
    description: Option<String>,
    input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
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
// OpenAI Request Types (Output - Serialize)
// ============================================================================

#[derive(Debug, Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<OpenAiToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<ReasoningConfig>,
}

#[derive(Debug, Serialize)]
struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<OpenAiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OpenAiContent {
    Text(String),
    Parts(Vec<OpenAiContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum OpenAiContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Serialize)]
struct ImageUrl {
    url: String,
}

#[derive(Debug, Serialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiFunctionCall,
}

#[derive(Debug, Serialize)]
struct OpenAiFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiFunction,
}

#[derive(Debug, Serialize)]
struct OpenAiFunction {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OpenAiToolChoice {
    Mode(String), // "auto", "none", "required"
    Specific {
        #[serde(rename = "type")]
        choice_type: String,
        function: ToolChoiceFunction,
    },
}

#[derive(Debug, Serialize)]
struct ToolChoiceFunction {
    name: String,
}

// ============================================================================
// Conversion Functions
// ============================================================================

/// Convert Anthropic message to OpenAI format
///
/// Note: A single Anthropic message may produce multiple OpenAI messages
/// (e.g., tool_result blocks become separate role: "tool" messages)
fn convert_message(msg: &AnthropicMessage) -> Vec<OpenAiMessage> {
    match &msg.content {
        AnthropicContent::Text(text) => {
            vec![OpenAiMessage {
                role: msg.role.clone(),
                content: Some(OpenAiContent::Text(text.clone())),
                tool_calls: None,
                tool_call_id: None,
            }]
        }
        AnthropicContent::Blocks(blocks) => {
            let mut messages: Vec<OpenAiMessage> = Vec::new();
            let mut text_parts: Vec<String> = Vec::new();
            let mut image_parts: Vec<OpenAiContentPart> = Vec::new();
            let mut tool_calls: Vec<OpenAiToolCall> = Vec::new();

            for block in blocks {
                match block {
                    AnthropicContentBlock::Text { text } => {
                        text_parts.push(text.clone());
                    }
                    AnthropicContentBlock::Image { source } => {
                        // Convert Anthropic base64 image to data URL
                        let data_url = format!("data:{};base64,{}", source.media_type, source.data);
                        image_parts.push(OpenAiContentPart::ImageUrl {
                            image_url: ImageUrl { url: data_url },
                        });
                    }
                    AnthropicContentBlock::ToolUse { id, name, input } => {
                        tool_calls.push(OpenAiToolCall {
                            id: id.clone(),
                            call_type: "function".to_string(),
                            function: OpenAiFunctionCall {
                                name: name.clone(),
                                arguments: serde_json::to_string(input).unwrap_or_default(),
                            },
                        });
                    }
                    AnthropicContentBlock::ToolResult {
                        tool_use_id,
                        content,
                    } => {
                        // Tool results become separate messages with role: "tool"
                        let result_text = content
                            .as_ref()
                            .map(|c| match c {
                                ToolResultContent::Text(text) => text.clone(),
                                ToolResultContent::Blocks(blocks) => blocks
                                    .iter()
                                    .map(|b| match b {
                                        ToolResultBlock::Text { text } => text.as_str(),
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                            })
                            .unwrap_or_default();

                        messages.push(OpenAiMessage {
                            role: "tool".to_string(),
                            content: Some(OpenAiContent::Text(result_text)),
                            tool_calls: None,
                            tool_call_id: Some(tool_use_id.clone()),
                        });
                    }
                    AnthropicContentBlock::Thinking { .. } => {
                        // Skip thinking blocks - not part of OpenAI format
                    }
                }
            }

            // Build the main message with text/images and/or tool calls
            if !text_parts.is_empty() || !image_parts.is_empty() || !tool_calls.is_empty() {
                let content = if !image_parts.is_empty() {
                    // Mixed content with images
                    let mut parts: Vec<OpenAiContentPart> = text_parts
                        .into_iter()
                        .map(|text| OpenAiContentPart::Text { text })
                        .collect();
                    parts.extend(image_parts);
                    Some(OpenAiContent::Parts(parts))
                } else if !text_parts.is_empty() {
                    Some(OpenAiContent::Text(text_parts.join("")))
                } else {
                    None
                };

                messages.insert(
                    0,
                    OpenAiMessage {
                        role: msg.role.clone(),
                        content,
                        tool_calls: if tool_calls.is_empty() {
                            None
                        } else {
                            Some(tool_calls)
                        },
                        tool_call_id: None,
                    },
                );
            }

            // If no messages were created but we had blocks, create empty message
            if messages.is_empty() {
                messages.push(OpenAiMessage {
                    role: msg.role.clone(),
                    content: Some(OpenAiContent::Text(String::new())),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }

            messages
        }
    }
}

/// Convert Anthropic tool to OpenAI format
fn convert_tool(tool: AnthropicTool) -> OpenAiTool {
    OpenAiTool {
        tool_type: "function".to_string(),
        function: OpenAiFunction {
            name: tool.name,
            description: tool.description,
            parameters: Some(tool.input_schema),
        },
    }
}

/// Convert Anthropic tool_choice to OpenAI format
fn convert_tool_choice(choice: AnthropicToolChoice) -> OpenAiToolChoice {
    match choice {
        AnthropicToolChoice::Auto => OpenAiToolChoice::Mode("auto".to_string()),
        AnthropicToolChoice::Any => OpenAiToolChoice::Mode("required".to_string()),
        AnthropicToolChoice::Tool { name } => OpenAiToolChoice::Specific {
            choice_type: "function".to_string(),
            function: ToolChoiceFunction { name },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_translator() -> AnthropicToOpenAiRequest {
        AnthropicToOpenAiRequest::new(ModelMapping::new())
    }

    #[test]
    fn test_simple_request_translation() {
        let translator = make_translator();
        let headers = HeaderMap::new();

        let anthropic_body = r#"{
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 1024,
            "messages": [
                {"role": "user", "content": "Hello"}
            ]
        }"#;

        let (translated, ctx) = translator
            .translate(anthropic_body.as_bytes(), &headers)
            .unwrap();
        let openai: serde_json::Value = serde_json::from_slice(&translated).unwrap();

        // With empty model mapping, model passes through unchanged
        assert_eq!(openai["model"], "claude-sonnet-4-20250514");
        assert_eq!(openai["messages"][0]["role"], "user");
        assert_eq!(openai["messages"][0]["content"], "Hello");
        assert_eq!(openai["max_tokens"], 1024);
        assert_eq!(
            ctx.original_model,
            Some("claude-sonnet-4-20250514".to_string())
        );
    }

    #[test]
    fn test_system_prompt_becomes_message() {
        let translator = make_translator();
        let headers = HeaderMap::new();

        let anthropic_body = r#"{
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 1024,
            "system": "You are helpful",
            "messages": [
                {"role": "user", "content": "Hello"}
            ]
        }"#;

        let (translated, _) = translator
            .translate(anthropic_body.as_bytes(), &headers)
            .unwrap();
        let openai: serde_json::Value = serde_json::from_slice(&translated).unwrap();

        assert_eq!(openai["messages"][0]["role"], "system");
        assert_eq!(openai["messages"][0]["content"], "You are helpful");
        assert_eq!(openai["messages"][1]["role"], "user");
        assert_eq!(openai["messages"][1]["content"], "Hello");
    }

    #[test]
    fn test_temperature_passthrough_reverse() {
        let translator = make_translator();
        let headers = HeaderMap::new();

        // Temperature passes through unchanged (most providers use 0-1 range)
        let anthropic_body = r#"{
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 1024,
            "temperature": 0.5,
            "messages": [{"role": "user", "content": "Hi"}]
        }"#;

        let (translated, _) = translator
            .translate(anthropic_body.as_bytes(), &headers)
            .unwrap();
        let openai: serde_json::Value = serde_json::from_slice(&translated).unwrap();

        // Temperature passes through unchanged
        assert_eq!(openai["temperature"], 0.5);
    }

    #[test]
    fn test_tool_use_conversion() {
        let translator = make_translator();
        let headers = HeaderMap::new();

        let anthropic_body = r#"{
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 1024,
            "messages": [
                {"role": "user", "content": "What's the weather?"},
                {
                    "role": "assistant",
                    "content": [{
                        "type": "tool_use",
                        "id": "tool_123",
                        "name": "get_weather",
                        "input": {"city": "London"}
                    }]
                },
                {
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": "tool_123",
                        "content": "Sunny, 22°C"
                    }]
                }
            ]
        }"#;

        let (translated, _) = translator
            .translate(anthropic_body.as_bytes(), &headers)
            .unwrap();
        let openai: serde_json::Value = serde_json::from_slice(&translated).unwrap();

        // Assistant message has tool_calls
        let assistant_msg = &openai["messages"][1];
        assert_eq!(assistant_msg["role"], "assistant");
        let tool_calls = assistant_msg["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls[0]["id"], "tool_123");
        assert_eq!(tool_calls[0]["function"]["name"], "get_weather");

        // Tool result becomes role: "tool"
        let tool_msg = &openai["messages"][2];
        assert_eq!(tool_msg["role"], "tool");
        assert_eq!(tool_msg["tool_call_id"], "tool_123");
        assert_eq!(tool_msg["content"], "Sunny, 22°C");
    }

    #[test]
    fn test_thinking_to_reasoning_passthrough() {
        let translator = make_translator();
        let headers = HeaderMap::new();

        let anthropic_body = r#"{
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 16000,
            "thinking": {
                "type": "enabled",
                "budget_tokens": 8000
            },
            "messages": [{"role": "user", "content": "Solve this"}]
        }"#;

        let (translated, _) = translator
            .translate(anthropic_body.as_bytes(), &headers)
            .unwrap();
        let openai: serde_json::Value = serde_json::from_slice(&translated).unwrap();

        // Should have reasoning.max_tokens
        assert_eq!(openai["reasoning"]["max_tokens"], 8000);
    }

    #[test]
    fn test_streaming_flag() {
        let translator = make_translator();
        let headers = HeaderMap::new();

        let anthropic_body = r#"{
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 1024,
            "stream": true,
            "messages": [{"role": "user", "content": "Hi"}]
        }"#;

        let (_, ctx) = translator
            .translate(anthropic_body.as_bytes(), &headers)
            .unwrap();
        assert!(ctx.streaming);
    }

    #[test]
    fn test_tool_choice_conversion() {
        let translator = make_translator();
        let headers = HeaderMap::new();

        // Test "any" -> "required"
        let anthropic_body = r#"{
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 1024,
            "tool_choice": {"type": "any"},
            "tools": [{"name": "test", "input_schema": {"type": "object"}}],
            "messages": [{"role": "user", "content": "Hi"}]
        }"#;

        let (translated, _) = translator
            .translate(anthropic_body.as_bytes(), &headers)
            .unwrap();
        let openai: serde_json::Value = serde_json::from_slice(&translated).unwrap();

        assert_eq!(openai["tool_choice"], "required");
    }
}
