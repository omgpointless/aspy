//! Translation context - carries state from request to response translation
//!
//! The `TranslationContext` is created during request translation and passed
//! through to response translation. It contains metadata needed to correctly
//! convert responses back to the client's expected format.
//!
//! # Implementation Status
//!
//! This context supports both buffered and streaming response translation:
//!
//! - **Buffered (Integrated)**: Fields like `client_format`, `backend_format`,
//!   `original_model`, and `model_mapping` are used by `translate_buffered()`.
//!
//! - **Streaming (Infrastructure Ready)**: Fields like `line_buffer`, `chunk_index`,
//!   `completion_id`, and `finish_reason` are used by `translate_chunk()` in
//!   `openai/response.rs`. These are fully implemented but not yet called from
//!   `handle_streaming_response()` in `proxy/mod.rs`.
//!
//! The streaming fields are marked with `#[allow(dead_code)]` until proxy
//! integration is complete.

use super::ApiFormat;
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// Model Mapping
// ============================================================================

/// Model name mapping for Claude Code → OpenAI-compatible endpoints
///
/// Primary use case: Claude Code sends Anthropic model names, we map them
/// to whatever backend you're targeting (OpenRouter, Azure, local Ollama, etc.)
///
/// Config format (Claude Code perspective):
/// ```toml
/// [translation.model_mapping]
/// "haiku" = "xai/grok-code-fast"
/// "sonnet" = "openai/gpt-5.1"
/// "opus" = "amazon/nova-2-lite-v1:free"
/// ```
///
/// Supports partial matching: "haiku" matches "claude-haiku-4-5-20251001"
#[derive(Debug, Clone, Default)]
pub struct ModelMapping {
    /// Anthropic pattern → target model (primary direction: Claude Code → backend)
    anthropic_to_target: HashMap<String, String>,
    /// Target model → Anthropic pattern (reverse direction, for completeness)
    target_to_anthropic: HashMap<String, String>,
}

impl ModelMapping {
    /// Create empty mapping
    pub fn new() -> Self {
        Self::default()
    }

    /// Create mapping from config HashMap
    ///
    /// Config keys are Anthropic patterns (what Claude Code sends),
    /// values are target models (where requests go).
    pub fn from_config(config: &HashMap<String, String>) -> Self {
        let mut mapping = Self::new();
        for (anthropic_pattern, target_model) in config {
            mapping.add(anthropic_pattern.clone(), target_model.clone());
        }
        mapping
    }

    /// Add a mapping (anthropic_pattern → target)
    pub fn add(&mut self, anthropic_pattern: String, target_model: String) {
        self.target_to_anthropic
            .insert(target_model.clone(), anthropic_pattern.clone());
        self.anthropic_to_target
            .insert(anthropic_pattern, target_model);
    }

    /// Map Anthropic model to target (Claude Code → backend)
    ///
    /// Supports partial matching: "haiku" in config matches "claude-haiku-4-5-20251001"
    pub fn to_target(&self, anthropic_model: &str) -> String {
        // Try exact match first
        if let Some(target) = self.anthropic_to_target.get(anthropic_model) {
            return target.clone();
        }

        // Try partial match (config key contained in model name)
        let model_lower = anthropic_model.to_lowercase();
        for (pattern, target) in &self.anthropic_to_target {
            if model_lower.contains(&pattern.to_lowercase()) {
                return target.clone();
            }
        }

        // No match - pass through unchanged
        anthropic_model.to_string()
    }

    /// Map target model back to Anthropic (reverse direction)
    ///
    /// Used when OpenAI clients talk to Claude backend (secondary use case).
    pub fn to_anthropic(&self, target_model: &str) -> String {
        self.target_to_anthropic
            .get(target_model)
            .cloned()
            .unwrap_or_else(|| target_model.to_string())
    }

    /// Alias for to_target (backwards compatibility with existing code)
    pub fn to_openai(&self, anthropic_model: &str) -> String {
        self.to_target(anthropic_model)
    }
}

// ============================================================================
// Translation Context
// ============================================================================

/// Context carried from request translation to response translation
///
/// This struct maintains state needed to correctly translate responses back
/// to the client's expected format. It's created during request translation
/// and passed through the proxy to response translation.
///
/// # Field Categories
///
/// ## Core Fields (Used by Buffered Translation - Integrated)
/// - `client_format`, `backend_format`: Determine if/how translation occurs
/// - `model_mapping`: Bidirectional model name conversion
/// - `original_model`: Preserves client's model name for response
/// - `streaming`: Indicates if client requested SSE streaming
///
/// ## Streaming State Fields (Used by Streaming Translation - Not Yet Integrated)
/// These fields are fully implemented and used by `translate_chunk()` in
/// `openai/response.rs`, but the integration into `handle_streaming_response()`
/// is pending. They track mutable state across SSE chunks:
///
/// - `line_buffer`: Handles SSE events split across TCP chunks
/// - `completion_id`: OpenAI's `chatcmpl-xxx` ID (generated once per request)
/// - `chunk_index`: Tracks content block index for tool calls
/// - `accumulated_content`: For potential usage calculation
/// - `sent_initial`: Ensures role is sent only in first chunk
/// - `finish_reason`: Captured from `message_delta` for final chunk
/// - `response_model`: Model name from Anthropic response (for mapping back)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TranslationContext {
    // ─────────────────────────────────────────────────────────────────────────
    // Core fields (used by buffered translation - INTEGRATED)
    // ─────────────────────────────────────────────────────────────────────────
    /// Original format the client spoke (e.g., OpenAI)
    pub client_format: ApiFormat,

    /// Format used with the backend (typically Anthropic)
    pub backend_format: ApiFormat,

    /// Model mapping for bidirectional name translation
    pub model_mapping: Arc<ModelMapping>,

    /// Original model name from client request (preserved for response mapping)
    pub original_model: Option<String>,

    /// Whether the client requested streaming (stream: true in request)
    pub streaming: bool,

    /// Unique request ID for correlation (optional, for logging)
    pub request_id: Option<String>,

    // ─────────────────────────────────────────────────────────────────────────
    // Streaming state fields (used by translate_chunk() - NOT YET INTEGRATED)
    //
    // These fields support the streaming translation logic in openai/response.rs.
    // The implementation is complete, but handle_streaming_response() in
    // proxy/mod.rs does not yet call translate_chunk(). Until then, these
    // fields are unused at runtime (hence #[allow(dead_code)] on the struct).
    // ─────────────────────────────────────────────────────────────────────────
    /// Buffer for incomplete SSE lines that span chunk boundaries
    ///
    /// SSE events may be split across TCP chunks. This buffer accumulates
    /// partial lines until a complete `data: {...}\n\n` is received.
    pub line_buffer: String,

    /// Generated completion ID for OpenAI format (e.g., "chatcmpl-abc123")
    ///
    /// Created once per request and reused across all streaming chunks.
    /// OpenAI clients expect the same ID throughout the stream.
    pub completion_id: String,

    /// Current content block index for tool call streaming
    ///
    /// OpenAI's streaming format requires an `index` field for tool calls.
    /// This increments when Anthropic sends `content_block_stop` events.
    pub chunk_index: u32,

    /// Accumulated content for potential usage calculation
    ///
    /// Some OpenAI clients expect usage stats even in streaming mode.
    /// Content is accumulated here for token counting if needed.
    pub accumulated_content: String,

    /// Whether the initial chunk with role has been sent
    ///
    /// OpenAI streaming sends `{"role": "assistant"}` only in the first chunk.
    /// This flag ensures we don't repeat it.
    pub sent_initial: bool,

    /// Finish reason captured from Anthropic's `message_delta` event
    ///
    /// Anthropic sends `stop_reason` in `message_delta`, which maps to
    /// OpenAI's `finish_reason` (e.g., "end_turn" → "stop").
    pub finish_reason: Option<String>,

    /// Model name from Anthropic response (may differ from request)
    ///
    /// Used for reverse mapping if `original_model` wasn't captured.
    pub response_model: Option<String>,

    /// Whether we're currently inside a content block (text or tool_use)
    ///
    /// Used for OpenAI→Anthropic translation to know when to emit
    /// `content_block_stop` before starting a new block.
    pub in_content_block: bool,
}

impl TranslationContext {
    /// Create a new translation context
    pub fn new(
        client_format: ApiFormat,
        backend_format: ApiFormat,
        model_mapping: Arc<ModelMapping>,
        streaming: bool,
    ) -> Self {
        Self {
            client_format,
            backend_format,
            model_mapping,
            original_model: None,
            streaming,
            request_id: None,
            line_buffer: String::new(),
            completion_id: generate_completion_id(),
            chunk_index: 0,
            accumulated_content: String::new(),
            sent_initial: false,
            finish_reason: None,
            response_model: None,
            in_content_block: false,
        }
    }

    /// Create a passthrough context (no translation needed)
    pub fn passthrough() -> Self {
        Self {
            client_format: ApiFormat::Anthropic,
            backend_format: ApiFormat::Anthropic,
            model_mapping: Arc::new(ModelMapping::new()),
            original_model: None,
            streaming: false,
            request_id: None,
            line_buffer: String::new(),
            completion_id: String::new(),
            chunk_index: 0,
            accumulated_content: String::new(),
            sent_initial: false,
            finish_reason: None,
            response_model: None,
            in_content_block: false,
        }
    }

    /// Check if response translation is needed
    pub fn needs_response_translation(&self) -> bool {
        self.client_format != self.backend_format
    }

    /// Set the original model name from the client request
    pub fn with_original_model(mut self, model: String) -> Self {
        self.original_model = Some(model);
        self
    }

    /// Set the request ID for correlation
    #[allow(dead_code)]
    pub fn with_request_id(mut self, id: String) -> Self {
        self.request_id = Some(id);
        self
    }

    /// Get the model name to use in responses
    ///
    /// Prefers the original model name from the request, falls back to
    /// mapping the response model, or passes through the response model as-is.
    #[allow(dead_code)]
    pub fn response_model_name(&self) -> String {
        if let Some(ref original) = self.original_model {
            return original.clone();
        }
        if let Some(ref response) = self.response_model {
            return self.model_mapping.to_openai(response);
        }
        // No model information available - this shouldn't happen in practice
        "unknown".to_string()
    }
}

impl Default for TranslationContext {
    fn default() -> Self {
        Self::passthrough()
    }
}

/// Generate a unique completion ID in OpenAI format
fn generate_completion_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    // Simple pseudo-random suffix using timestamp
    let suffix: u32 = (timestamp % 1_000_000) as u32;

    format!("chatcmpl-{:x}{:06x}", timestamp, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_mapping_passthrough() {
        let mapping = ModelMapping::new();

        // No mappings - everything passes through unchanged
        assert_eq!(
            mapping.to_target("claude-haiku-4-5-20251001"),
            "claude-haiku-4-5-20251001"
        );
        assert_eq!(mapping.to_target("unknown-model"), "unknown-model");
        assert_eq!(mapping.to_anthropic("gpt-4"), "gpt-4");
    }

    #[test]
    fn test_model_mapping_claude_code_perspective() {
        let mut config = HashMap::new();
        // Config: what Claude Code sends = where it goes
        config.insert("haiku".to_string(), "xai/grok-code-fast".to_string());
        config.insert("sonnet".to_string(), "openai/gpt-5.1".to_string());
        config.insert("opus".to_string(), "amazon/nova-2-lite-v1:free".to_string());

        let mapping = ModelMapping::from_config(&config);

        // Partial matching: "haiku" matches full model name
        assert_eq!(
            mapping.to_target("claude-haiku-4-5-20251001"),
            "xai/grok-code-fast"
        );
        assert_eq!(
            mapping.to_target("claude-sonnet-4-20250514"),
            "openai/gpt-5.1"
        );
        assert_eq!(
            mapping.to_target("claude-opus-4-20250514"),
            "amazon/nova-2-lite-v1:free"
        );

        // Unmapped passes through
        assert_eq!(mapping.to_target("some-random-model"), "some-random-model");
    }

    #[test]
    fn test_model_mapping_exact_match_priority() {
        let mut config = HashMap::new();
        // Exact match should take priority over partial
        config.insert(
            "claude-haiku-4-5-20251001".to_string(),
            "exact-target".to_string(),
        );
        config.insert("haiku".to_string(), "partial-target".to_string());

        let mapping = ModelMapping::from_config(&config);

        // Exact match wins
        assert_eq!(
            mapping.to_target("claude-haiku-4-5-20251001"),
            "exact-target"
        );
    }

    #[test]
    fn test_translation_context_passthrough() {
        let ctx = TranslationContext::passthrough();

        assert_eq!(ctx.client_format, ApiFormat::Anthropic);
        assert_eq!(ctx.backend_format, ApiFormat::Anthropic);
        assert!(!ctx.needs_response_translation());
    }

    #[test]
    fn test_translation_context_needs_translation() {
        let ctx = TranslationContext::new(
            ApiFormat::OpenAI,
            ApiFormat::Anthropic,
            Arc::new(ModelMapping::new()),
            true,
        );

        assert!(ctx.needs_response_translation());
    }

    #[test]
    fn test_completion_id_format() {
        let id = generate_completion_id();

        assert!(id.starts_with("chatcmpl-"));
        assert!(id.len() > 15); // Reasonable length
    }
}
