//! Request transformation pipeline for extensible request modification
//!
//! This module provides a trait-based system for modifying API requests before
//! they are forwarded to the upstream provider. Transformers can inject context,
//! edit system reminders, translate formats, or filter content.
//!
//! # Architecture
//!
//! ```text
//! Request Body → TransformationPipeline → [Transformer₁, Transformer₂, ...] → Modified Body
//! ```
//!
//! # Transformer Types
//!
//! Transformers can perform four operations:
//! - **Pass-through**: Return `TransformResult::Unchanged` (zero-cost)
//! - **Transform**: Modify request (return `TransformResult::Modified(body)`)
//! - **Block**: Reject request (return `TransformResult::Block { reason, status }`)
//! - **Error**: Log and continue (return `TransformResult::Error(e)`)
//!
//! # Fail-Safe Guarantee
//!
//! The pipeline ALWAYS returns - one transformer failing never breaks the request.
//! Worst case: the original unmodified request goes through.

mod compact_enhancer;
mod system_reminder;

// Re-exports for config parsing and transformer implementations
pub use compact_enhancer::{CompactEnhancer, CompactEnhancerConfig};
#[allow(unused_imports)]
pub use system_reminder::{
    InjectPosition, PositionConfig, RuleConfig, TagEditor, TagEditorConfig, TagRule, WhenCondition,
};

use axum::http::StatusCode;
use serde_json::Value;
use std::borrow::Cow;

// ============================================================================
// Transform Result
// ============================================================================

/// Token delta information for a transformation
#[derive(Debug, Clone, Copy, Default)]
pub struct TransformTokens {
    /// Estimated tokens before transformation
    pub before: u32,
    /// Estimated tokens after transformation
    pub after: u32,
}

impl TransformTokens {
    /// Create a new token delta
    pub fn new(before: u32, after: u32) -> Self {
        Self { before, after }
    }

    /// Tokens added (positive) or removed (negative)
    pub fn delta(&self) -> i32 {
        self.after as i32 - self.before as i32
    }
}

/// Result of transforming a request
#[derive(Debug)]
pub enum TransformResult {
    /// Request unchanged - zero-copy passthrough
    Unchanged,

    /// Request modified - use this new body
    /// Includes optional token delta for tracking
    Modified {
        /// The transformed request body
        body: Value,
        /// Token counts before/after (for stats tracking)
        tokens: Option<TransformTokens>,
    },

    /// Block request entirely (e.g., content policy violation)
    ///
    /// Use sparingly - most errors should return `Error` and continue.
    /// Used by content policy transformers (future: PII redaction, content filtering).
    #[allow(dead_code)]
    Block {
        /// Human-readable reason for blocking
        reason: String,
        /// HTTP status code to return
        status: StatusCode,
    },

    /// Error during transformation - log and continue with current body
    ///
    /// This is the fail-safe path. If embeddings are down, config is invalid,
    /// or any other error occurs, the request should still go through.
    /// Used by transformers that can fail gracefully (ContextEnricher when embeddings down).
    #[allow(dead_code)]
    Error(anyhow::Error),
}

impl TransformResult {
    /// Helper to create a Modified result without token tracking
    pub fn modified(body: Value) -> Self {
        Self::Modified { body, tokens: None }
    }

    /// Helper to create a Modified result with token tracking
    pub fn modified_with_tokens(body: Value, before: u32, after: u32) -> Self {
        Self::Modified {
            body,
            tokens: Some(TransformTokens::new(before, after)),
        }
    }
}

// ============================================================================
// Transform Context
// ============================================================================

/// Context provided to transformers for decision-making
///
/// Contains information available at request handling time.
/// Future fields (like `semantic_context`) will be populated by
/// async prep work in the proxy handler before the sync pipeline runs.
///
/// Fields are read by transformer implementations via pattern matching or direct access.
/// Even if not currently used by TagEditor, they are part of the public API
/// for future transformers (ContextEnricher, ModelRouter, etc.).
#[derive(Debug, Clone, Default)]
pub struct TransformContext<'a> {
    /// Client ID from routing (e.g., "dev-1")
    /// Used by: per-client transformation rules, client_id condition
    pub client_id: Option<&'a str>,

    /// Request path (e.g., "/v1/messages")
    pub path: &'a str,

    /// Model being requested (extracted from body)
    /// Used by: ModelRouter, model-specific transformations (future)
    #[allow(dead_code)]
    pub model: Option<&'a str>,

    /// Current token usage from context state
    /// Used by: ContextEnricher for context-aware injection (future)
    #[allow(dead_code)]
    pub context_tokens: Option<u64>,

    /// Context limit for the model
    /// Used by: ContextEnricher for context-aware injection (future)
    #[allow(dead_code)]
    pub context_limit: Option<u64>,

    /// Turn number in the conversation (1-indexed, counts user messages)
    /// Used by: turn_number condition for frequency-based rules
    pub turn_number: Option<u64>,

    /// Number of tool_result blocks in current user message
    /// Used by: has_tool_results condition
    pub tool_result_count: Option<usize>,
    // Future: semantic_context for RAG injection
    // pub semantic_context: Option<&'a SemanticContext>,
}

impl<'a> TransformContext<'a> {
    /// Create a new transform context
    pub fn new(client_id: Option<&'a str>, path: &'a str, model: Option<&'a str>) -> Self {
        Self {
            client_id,
            path,
            model,
            context_tokens: None,
            context_limit: None,
            turn_number: None,
            tool_result_count: None,
        }
    }

    /// Add context token information
    /// Used by: ContextEnricher for context-aware injection (future)
    #[allow(dead_code)]
    pub fn with_context_state(mut self, tokens: u64, limit: u64) -> Self {
        self.context_tokens = Some(tokens);
        self.context_limit = Some(limit);
        self
    }
}

// ============================================================================
// Request Transformer Trait
// ============================================================================

/// Trait for request transformers
///
/// Transformers are called in registration order. Each transformer can:
/// - Pass through unchanged (return `TransformResult::Unchanged`)
/// - Transform the body (return `TransformResult::Modified(body)`)
/// - Block the request (return `TransformResult::Block { ... }`)
/// - Signal an error (return `TransformResult::Error(e)`)
///
/// # Sync Design
///
/// `transform` is intentionally synchronous, matching `EventProcessor`.
/// For async operations (like embeddings lookup), do the async work in
/// the proxy handler and pass results via `TransformContext`.
///
/// # Fail-Safe Contract
///
/// Implementations should return `TransformResult::Error` rather than
/// panicking. The pipeline will log the error and continue with the
/// current body. This ensures one transformer failing never breaks the request.
pub trait RequestTransformer: Send + Sync {
    /// Human-readable name for logging and debugging
    fn name(&self) -> &'static str;

    /// Check if this transformer should run for this request
    ///
    /// Called before `transform()`. Return `false` to skip this transformer.
    /// Use for fast-path filtering (e.g., only apply to /v1/messages).
    fn should_apply(&self, ctx: &TransformContext) -> bool;

    /// Transform the request body
    ///
    /// # Arguments
    /// * `body` - Reference to the parsed JSON body
    /// * `ctx` - Context about the current request
    ///
    /// # Returns
    /// * `TransformResult::Unchanged` - No modification (zero-cost)
    /// * `TransformResult::Modified(body)` - Use this new body
    /// * `TransformResult::Block { ... }` - Reject the request
    /// * `TransformResult::Error(e)` - Log and continue with current body
    fn transform(&self, body: &Value, ctx: &TransformContext) -> TransformResult;
}

// ============================================================================
// Transformation Pipeline
// ============================================================================

/// Pipeline that runs requests through registered transformers
///
/// # Fail-Safe Guarantee
///
/// - Pipeline ALWAYS returns (never panics/hangs)
/// - One transformer failing ≠ request fails
/// - Worst case: original unmodified request goes through
pub struct TransformationPipeline {
    transformers: Vec<Box<dyn RequestTransformer>>,
}

impl TransformationPipeline {
    /// Create an empty pipeline (passthrough)
    pub fn new() -> Self {
        Self {
            transformers: Vec::new(),
        }
    }

    /// Create pipeline from config
    ///
    /// Registers transformers based on config settings.
    /// Transformers are opt-in: only enabled when explicitly configured.
    pub fn from_config(config: &crate::config::Transformers) -> Self {
        let mut pipeline = Self::new();

        // Tag editor (opt-in)
        if let Some(ref editor_config) = config.tag_editor {
            if editor_config.enabled {
                match TagEditor::from_config(editor_config) {
                    Ok(editor) => {
                        let rule_count = editor.rule_count();
                        pipeline.register(editor);
                        tracing::info!("Registered tag-editor transformer ({} rules)", rule_count);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to create tag-editor: {}. Transformer disabled.", e);
                    }
                }
            }
        }

        // Compact enhancer (opt-in)
        if let Some(ref compact_config) = config.compact_enhancer {
            if compact_config.enabled {
                pipeline.register(CompactEnhancer::new());
                tracing::info!("Registered compact-enhancer transformer");
            }
        }

        pipeline
    }

    /// Register a transformer
    ///
    /// Transformers are called in registration order.
    pub fn register(&mut self, transformer: impl RequestTransformer + 'static) {
        self.transformers.push(Box::new(transformer));
    }

    /// Process a request through all registered transformers
    ///
    /// Returns `TransformResult::Unchanged` if no transformation occurred,
    /// `TransformResult::Modified(body)` if any transformer modified the body,
    /// `TransformResult::Block { ... }` if any transformer blocked the request.
    ///
    /// Uses `Cow` internally to avoid cloning when all transformers pass through.
    pub fn transform<'a>(&self, body: &'a Value, ctx: &TransformContext) -> TransformResult {
        if self.transformers.is_empty() {
            return TransformResult::Unchanged;
        }

        // Track whether we've had to clone yet
        let mut current: Cow<'a, Value> = Cow::Borrowed(body);

        for transformer in &self.transformers {
            // Fast-path: skip if transformer doesn't apply
            if !transformer.should_apply(ctx) {
                continue;
            }

            match transformer.transform(current.as_ref(), ctx) {
                TransformResult::Unchanged => {
                    // No change, keep current (borrowed or owned)
                }
                TransformResult::Modified { body, tokens } => {
                    tracing::debug!(
                        transformer = transformer.name(),
                        tokens_delta = tokens.map(|t| t.delta()).unwrap_or(0),
                        "Request body transformed"
                    );
                    current = Cow::Owned(body);
                }
                TransformResult::Block { reason, status } => {
                    // Hard stop - only content policy violations should reach here
                    tracing::info!(
                        transformer = transformer.name(),
                        reason = %reason,
                        status = %status,
                        "Request blocked by transformer {}: {} (status {})",
                        transformer.name(),
                        reason,
                        status
                    );
                    return TransformResult::Block { reason, status };
                }
                TransformResult::Error(error) => {
                    // LOG AND CONTINUE - never break the request
                    tracing::warn!(
                        transformer = transformer.name(),
                        error = %error,
                        "Transformer {} failed: {}, continuing with current body",
                        transformer.name(),
                        error
                    );
                    // Don't modify current - proceed with what we have
                }
            }
        }

        // Convert Cow back to TransformResult
        match current {
            Cow::Borrowed(_) => TransformResult::Unchanged,
            Cow::Owned(modified) => TransformResult::modified(modified),
        }
    }

    /// Check if pipeline has any transformers
    pub fn is_empty(&self) -> bool {
        self.transformers.is_empty()
    }

    /// Get names of registered transformers (for logging/debug)
    #[allow(dead_code)]
    pub fn transformer_names(&self) -> Vec<&'static str> {
        self.transformers.iter().map(|t| t.name()).collect()
    }
}

impl Default for TransformationPipeline {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Test transformer that always passes through
    struct PassthroughTransformer;

    impl RequestTransformer for PassthroughTransformer {
        fn name(&self) -> &'static str {
            "passthrough"
        }
        fn should_apply(&self, _ctx: &TransformContext) -> bool {
            true
        }
        fn transform(&self, _body: &Value, _ctx: &TransformContext) -> TransformResult {
            TransformResult::Unchanged
        }
    }

    /// Test transformer that adds a field
    struct AddFieldTransformer {
        field: String,
        value: Value,
    }

    impl RequestTransformer for AddFieldTransformer {
        fn name(&self) -> &'static str {
            "add-field"
        }
        fn should_apply(&self, _ctx: &TransformContext) -> bool {
            true
        }
        fn transform(&self, body: &Value, _ctx: &TransformContext) -> TransformResult {
            let mut new_body = body.clone();
            if let Some(obj) = new_body.as_object_mut() {
                obj.insert(self.field.clone(), self.value.clone());
            }
            TransformResult::modified(new_body)
        }
    }

    /// Test transformer that errors
    struct ErrorTransformer;

    impl RequestTransformer for ErrorTransformer {
        fn name(&self) -> &'static str {
            "error"
        }
        fn should_apply(&self, _ctx: &TransformContext) -> bool {
            true
        }
        fn transform(&self, _body: &Value, _ctx: &TransformContext) -> TransformResult {
            TransformResult::Error(anyhow::anyhow!("Simulated error"))
        }
    }

    #[test]
    fn test_empty_pipeline_returns_unchanged() {
        let pipeline = TransformationPipeline::new();
        let body = serde_json::json!({"model": "claude-3"});
        let ctx = TransformContext::new(None, "/v1/messages", Some("claude-3"));

        match pipeline.transform(&body, &ctx) {
            TransformResult::Unchanged => {}
            other => panic!("Expected Unchanged, got {:?}", other),
        }
    }

    #[test]
    fn test_passthrough_returns_unchanged() {
        let mut pipeline = TransformationPipeline::new();
        pipeline.register(PassthroughTransformer);

        let body = serde_json::json!({"model": "claude-3"});
        let ctx = TransformContext::new(None, "/v1/messages", Some("claude-3"));

        match pipeline.transform(&body, &ctx) {
            TransformResult::Unchanged => {}
            other => panic!("Expected Unchanged, got {:?}", other),
        }
    }

    #[test]
    fn test_transform_modifies_body() {
        let mut pipeline = TransformationPipeline::new();
        pipeline.register(AddFieldTransformer {
            field: "injected".to_string(),
            value: serde_json::json!(true),
        });

        let body = serde_json::json!({"model": "claude-3"});
        let ctx = TransformContext::new(None, "/v1/messages", Some("claude-3"));

        match pipeline.transform(&body, &ctx) {
            TransformResult::Modified { body: new_body, .. } => {
                assert_eq!(new_body["injected"], serde_json::json!(true));
                assert_eq!(new_body["model"], serde_json::json!("claude-3"));
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_error_continues_with_original() {
        let mut pipeline = TransformationPipeline::new();
        pipeline.register(ErrorTransformer);
        pipeline.register(AddFieldTransformer {
            field: "after_error".to_string(),
            value: serde_json::json!(true),
        });

        let body = serde_json::json!({"model": "claude-3"});
        let ctx = TransformContext::new(None, "/v1/messages", Some("claude-3"));

        // Error should be logged but pipeline continues
        match pipeline.transform(&body, &ctx) {
            TransformResult::Modified { body: new_body, .. } => {
                // Second transformer should have run
                assert_eq!(new_body["after_error"], serde_json::json!(true));
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_multiple_transforms_compose() {
        let mut pipeline = TransformationPipeline::new();
        pipeline.register(AddFieldTransformer {
            field: "first".to_string(),
            value: serde_json::json!(1),
        });
        pipeline.register(AddFieldTransformer {
            field: "second".to_string(),
            value: serde_json::json!(2),
        });

        let body = serde_json::json!({"original": true});
        let ctx = TransformContext::new(None, "/v1/messages", None);

        match pipeline.transform(&body, &ctx) {
            TransformResult::Modified { body: new_body, .. } => {
                assert_eq!(new_body["original"], serde_json::json!(true));
                assert_eq!(new_body["first"], serde_json::json!(1));
                assert_eq!(new_body["second"], serde_json::json!(2));
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }
}
