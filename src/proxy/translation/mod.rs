//! API Translation module - bidirectional format conversion between API protocols
//!
//! This module provides a trait-based system for translating HTTP requests and
//! responses between different API formats (e.g., OpenAI ↔ Anthropic). Translators
//! operate at the HTTP body level, before/after the core proxy logic.
//!
//! # Architecture
//!
//! ```text
//! Client Request (OpenAI format)
//!     ↓
//! FormatDetector → detect format from path/headers/body
//!     ↓
//! RequestTranslator → OpenAI → Anthropic
//!     ↓
//! [Core Proxy Logic - forwards to Anthropic API]
//!     ↓
//! ResponseTranslator → Anthropic → OpenAI
//!     ↓
//! Client Response (OpenAI format)
//! ```
//!
//! # Layer Distinction
//!
//! - **Augmentor**: Injects content into SSE streams (response modification)
//! - **EventProcessor**: Transforms ProxyEvents after parsing
//! - **Translator**: Converts entire HTTP bodies between API formats (this module)
//!
//! # Implementation Status
//!
//! ## Fully Integrated
//! - **Request translation**: OpenAI → Anthropic (via `proxy_handler`)
//! - **Buffered response translation**: Anthropic → OpenAI (via `handle_buffered_response`)
//!
//! ## Infrastructure Ready, Not Yet Integrated
//! - **Streaming response translation**: The `translate_chunk()` and `finalize()` methods
//!   are fully implemented in `openai/response.rs`, but not yet wired into
//!   `handle_streaming_response()` in `proxy/mod.rs`. This requires:
//!   1. Wrapping the SSE stream to intercept chunks before forwarding to client
//!   2. Calling `translate_chunk()` on each chunk
//!   3. Calling `finalize()` to emit the `data: [DONE]` terminator
//!   4. Managing `TranslationContext` state across async chunk boundaries
//!
//! The streaming types (`OpenAiStreamChunk`, `OpenAiDelta`, etc.) and chunk
//! translation logic are complete—only the proxy integration is pending.
//!
//! # Adding New Format Support
//!
//! 1. Add variant to `ApiFormat` enum
//! 2. Create submodule (e.g., `bedrock/`)
//! 3. Implement `RequestTranslator` and `ResponseTranslator` traits
//! 4. Register in `TranslationPipeline::from_config()`

mod context;
mod detection;
pub mod openai;

pub use context::{ModelMapping, TranslationContext};
pub use detection::FormatDetector;

use axum::http::HeaderMap;

// ============================================================================
// API Format
// ============================================================================

/// Supported API format identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiFormat {
    /// Anthropic Messages API (`/v1/messages`)
    Anthropic,
    /// OpenAI Chat Completions API (`/v1/chat/completions`)
    OpenAI,
    // Future: Bedrock, Vertex, Cohere, etc.
}

impl ApiFormat {
    /// Get the canonical endpoint path for this format
    pub fn endpoint_path(&self) -> &'static str {
        match self {
            ApiFormat::Anthropic => "/v1/messages",
            ApiFormat::OpenAI => "/v1/chat/completions",
        }
    }

    /// Human-readable name for logging
    pub fn name(&self) -> &'static str {
        match self {
            ApiFormat::Anthropic => "Anthropic",
            ApiFormat::OpenAI => "OpenAI",
        }
    }
}

impl std::fmt::Display for ApiFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

// ============================================================================
// Translator Traits
// ============================================================================

/// Trait for request translators (client format → backend format)
///
/// Request translators convert incoming request bodies from one API format
/// to another. They also produce a `TranslationContext` that carries metadata
/// needed for response translation.
///
/// # Example
///
/// ```ignore
/// pub struct OpenAiToAnthropicRequest { /* ... */ }
///
/// impl RequestTranslator for OpenAiToAnthropicRequest {
///     fn name(&self) -> &'static str { "openai-to-anthropic" }
///     fn source_format(&self) -> ApiFormat { ApiFormat::OpenAI }
///     fn target_format(&self) -> ApiFormat { ApiFormat::Anthropic }
///
///     fn translate(&self, body: &[u8], headers: &HeaderMap)
///         -> anyhow::Result<(Vec<u8>, TranslationContext)>
///     {
///         // Parse OpenAI request, convert to Anthropic format
///     }
/// }
/// ```
pub trait RequestTranslator: Send + Sync {
    /// Human-readable name for logging and debugging
    fn name(&self) -> &'static str;

    /// The API format this translator accepts as input
    fn source_format(&self) -> ApiFormat;

    /// The API format this translator produces as output
    fn target_format(&self) -> ApiFormat;

    /// Translate a request body from source to target format
    ///
    /// # Arguments
    /// * `body` - Raw request body bytes
    /// * `headers` - Request headers (may contain format hints)
    ///
    /// # Returns
    /// * `Ok((translated_body, context))` - Translated body and context for response
    /// * `Err(e)` - Translation failed (pass through original or return error)
    fn translate(
        &self,
        body: &[u8],
        headers: &HeaderMap,
    ) -> anyhow::Result<(Vec<u8>, TranslationContext)>;
}

/// Trait for response translators (backend format → client format)
///
/// Response translators convert API responses back to the client's expected
/// format. They support both buffered (JSON) and streaming (SSE) responses.
///
/// # Buffered Response Translation (Integrated)
///
/// For non-streaming responses, `translate_buffered()` converts the complete
/// JSON response body. This is fully integrated via `handle_buffered_response()`
/// in `proxy/mod.rs`.
///
/// # Streaming Response Translation (Infrastructure Ready)
///
/// For SSE responses, `translate_chunk()` is called for each chunk. The translator
/// handles partial data and chunk boundaries using `TranslationContext` state.
///
/// **Current Status**: The streaming translation logic is fully implemented in
/// `openai/response.rs` (see `translate_chunk()`, `translate_sse_data()`, and
/// `finalize()`), but integration into `handle_streaming_response()` is pending.
///
/// **Why not yet integrated**: Streaming translation requires intercepting SSE
/// chunks in the forwarding path, which involves wrapping the response body
/// stream. This adds complexity around:
/// - Async chunk processing with mutable `TranslationContext`
/// - Error handling for mid-stream translation failures
/// - Proper SSE event boundary detection
///
/// The buffered path was prioritized for initial implementation as it covers
/// the `stream: false` use case completely.
pub trait ResponseTranslator: Send + Sync {
    /// Human-readable name for logging and debugging
    fn name(&self) -> &'static str;

    /// The API format this translator accepts (backend response format)
    fn source_format(&self) -> ApiFormat;

    /// The API format this translator produces (client expected format)
    fn target_format(&self) -> ApiFormat;

    /// Translate a complete buffered response
    ///
    /// Used for non-streaming (JSON) responses.
    fn translate_buffered(&self, body: &[u8], ctx: &TranslationContext) -> anyhow::Result<Vec<u8>>;

    /// Translate a single SSE chunk (streaming response translation)
    ///
    /// Called for each chunk in a streaming response. The translator should:
    /// - Parse Anthropic SSE events from the chunk
    /// - Convert to OpenAI SSE format
    /// - Return empty vec to skip/buffer a chunk (e.g., for partial events)
    ///
    /// # Arguments
    /// * `chunk` - Raw chunk bytes (may contain partial SSE events)
    /// * `ctx` - Translation context (mutable for state tracking across chunks)
    ///
    /// # Implementation Notes
    ///
    /// The implementation in `openai/response.rs` handles:
    /// - Line buffering for chunks that split across SSE event boundaries
    /// - Event type mapping (message_start → initial role, content_block_delta → content, etc.)
    /// - Tool call streaming with incremental argument JSON
    /// - State tracking via `TranslationContext` fields (chunk_index, finish_reason, etc.)
    ///
    /// # Integration Status
    ///
    /// **NOT YET INTEGRATED**: This method is fully implemented but not called from
    /// `handle_streaming_response()` in `proxy/mod.rs`. To integrate:
    /// 1. Wrap the response body stream to intercept chunks
    /// 2. For each chunk, call `translate_chunk()` with mutable context
    /// 3. Forward translated bytes to client (or buffer if empty)
    /// 4. Call `finalize()` after stream ends
    ///
    /// See `openai/response.rs` for the complete streaming translation logic.
    #[allow(dead_code)]
    fn translate_chunk(
        &self,
        chunk: &[u8],
        ctx: &mut TranslationContext,
    ) -> anyhow::Result<Vec<u8>>;

    /// Generate the final terminator for the stream
    ///
    /// Called after all chunks have been processed. Returns format-specific
    /// terminator (e.g., `data: [DONE]\n\n` for OpenAI).
    ///
    /// # Integration Status
    ///
    /// **NOT YET INTEGRATED**: Should be called after the last chunk is processed
    /// in `handle_streaming_response()`. The returned bytes must be sent to the
    /// client to properly terminate the OpenAI-format SSE stream.
    #[allow(dead_code)]
    fn finalize(&self, ctx: &TranslationContext) -> Option<Vec<u8>>;
}

// ============================================================================
// Translation Pipeline
// ============================================================================

/// Pipeline that coordinates format detection and translation
///
/// The pipeline is the main interface for the proxy to interact with translators.
/// It handles:
/// - Detecting incoming request format
/// - Selecting appropriate translators
/// - Managing translator lifecycle
pub struct TranslationPipeline {
    /// Format detector for incoming requests
    detector: FormatDetector,
    /// Registered request translators (source_format → translator)
    request_translators: Vec<Box<dyn RequestTranslator>>,
    /// Registered response translators (target_format → translator)
    response_translators: Vec<Box<dyn ResponseTranslator>>,
    /// Whether translation is enabled
    enabled: bool,
}

impl TranslationPipeline {
    /// Create an empty (disabled) pipeline
    pub fn new() -> Self {
        Self {
            detector: FormatDetector::new(),
            request_translators: Vec::new(),
            response_translators: Vec::new(),
            enabled: false,
        }
    }

    /// Create pipeline from configuration
    pub fn from_config(config: &crate::config::Translation) -> Self {
        if !config.enabled {
            tracing::debug!("Translation pipeline disabled");
            return Self::new();
        }

        let mut pipeline = Self {
            detector: FormatDetector::with_config(config.auto_detect),
            request_translators: Vec::new(),
            response_translators: Vec::new(),
            enabled: true,
        };

        // Register bidirectional OpenAI ↔ Anthropic translators
        let model_mapping = ModelMapping::from_config(&config.model_mapping);

        // Direction 1: OpenAI clients → Anthropic backend
        pipeline.register_request_translator(openai::OpenAiToAnthropicRequest::new(
            model_mapping.clone(),
        ));
        pipeline.register_response_translator(openai::AnthropicToOpenAiResponse::new(
            model_mapping.clone(),
        ));

        // Direction 2: Anthropic clients (Claude Code) → OpenAI backend
        pipeline.register_request_translator(openai::AnthropicToOpenAiRequest::new(
            model_mapping.clone(),
        ));
        pipeline
            .register_response_translator(openai::OpenAiToAnthropicResponse::new(model_mapping));

        tracing::info!(
            "Translation pipeline enabled: {} request translator(s), {} response translator(s)",
            pipeline.request_translators.len(),
            pipeline.response_translators.len()
        );

        pipeline
    }

    /// Register a request translator
    pub fn register_request_translator(&mut self, translator: impl RequestTranslator + 'static) {
        tracing::debug!(
            "Registered request translator: {} ({} → {})",
            translator.name(),
            translator.source_format(),
            translator.target_format()
        );
        self.request_translators.push(Box::new(translator));
    }

    /// Register a response translator
    pub fn register_response_translator(&mut self, translator: impl ResponseTranslator + 'static) {
        tracing::debug!(
            "Registered response translator: {} ({} → {})",
            translator.name(),
            translator.source_format(),
            translator.target_format()
        );
        self.response_translators.push(Box::new(translator));
    }

    /// Check if translation is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Detect the API format of an incoming request
    pub fn detect_format(&self, path: &str, headers: &HeaderMap, body: &[u8]) -> ApiFormat {
        self.detector.detect(path, headers, body)
    }

    /// Get a request translator for the given source → target conversion
    pub fn get_request_translator(
        &self,
        source: ApiFormat,
        target: ApiFormat,
    ) -> Option<&dyn RequestTranslator> {
        self.request_translators
            .iter()
            .find(|t| t.source_format() == source && t.target_format() == target)
            .map(|t| t.as_ref())
    }

    /// Get a response translator for the given source → target conversion
    pub fn get_response_translator(
        &self,
        source: ApiFormat,
        target: ApiFormat,
    ) -> Option<&dyn ResponseTranslator> {
        self.response_translators
            .iter()
            .find(|t| t.source_format() == source && t.target_format() == target)
            .map(|t| t.as_ref())
    }

    /// Translate a request if needed, returning translated body and context
    ///
    /// If the request is already in Anthropic format, returns a passthrough context.
    /// This is a convenience wrapper for `translate_request_for_target` with Anthropic as target.
    #[allow(dead_code)] // Kept for backward compatibility and potential external use
    pub fn translate_request(
        &self,
        path: &str,
        headers: &HeaderMap,
        body: &[u8],
    ) -> anyhow::Result<(Vec<u8>, TranslationContext, String)> {
        // Default target is Anthropic (backward compatible)
        self.translate_request_for_target(path, headers, body, ApiFormat::Anthropic)
    }

    /// Translate a request to a specific target format
    ///
    /// Used when the provider expects a specific API format (e.g., OpenRouter expects OpenAI format).
    /// If the request is already in the target format, returns a passthrough context.
    pub fn translate_request_for_target(
        &self,
        path: &str,
        headers: &HeaderMap,
        body: &[u8],
        target: ApiFormat,
    ) -> anyhow::Result<(Vec<u8>, TranslationContext, String)> {
        if !self.enabled {
            return Ok((
                body.to_vec(),
                TranslationContext::passthrough(),
                path.to_string(),
            ));
        }

        let detected = self.detect_format(path, headers, body);

        if detected == target {
            tracing::debug!("Request already in {} format, passthrough", target);
            return Ok((
                body.to_vec(),
                TranslationContext::passthrough(),
                path.to_string(),
            ));
        }

        // Find translator for detected format → target format
        let translator = self
            .get_request_translator(detected, target)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No request translator available for {} → {}",
                    detected,
                    target
                )
            })?;

        tracing::debug!(
            "Translating request: {} → {} (using {})",
            detected,
            target,
            translator.name()
        );

        let (translated_body, ctx) = translator.translate(body, headers)?;

        // Map the path to target endpoint
        let translated_path = target.endpoint_path().to_string();

        Ok((translated_body, ctx, translated_path))
    }
}

impl Default for TranslationPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_format_display() {
        assert_eq!(ApiFormat::Anthropic.to_string(), "Anthropic");
        assert_eq!(ApiFormat::OpenAI.to_string(), "OpenAI");
    }

    #[test]
    fn test_api_format_endpoint_path() {
        assert_eq!(ApiFormat::Anthropic.endpoint_path(), "/v1/messages");
        assert_eq!(ApiFormat::OpenAI.endpoint_path(), "/v1/chat/completions");
    }

    #[test]
    fn test_disabled_pipeline_passthrough() {
        let pipeline = TranslationPipeline::new();
        assert!(!pipeline.is_enabled());

        let body = b"test body";
        let headers = HeaderMap::new();
        let (translated, ctx, path) = pipeline
            .translate_request("/v1/messages", &headers, body)
            .unwrap();

        assert_eq!(translated, body);
        assert!(!ctx.needs_response_translation());
        assert_eq!(path, "/v1/messages");
    }
}
