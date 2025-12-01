// Augmentation module - extensible response modification
//
// This module provides a trait-based system for modifying API responses.
// Augmenters can inject content into SSE streams (e.g., context warnings,
// annotations, debug info) without touching core proxy logic.
//
// # Architecture
//
// ```
// Response Stream → AugmentationPipeline → [Augmenter1, Augmenter2, ...] → Injections
// ```
//
// Each augmenter implements the `Augmenter` trait:
// - `name()`: Human-readable identifier for logging
// - `should_apply()`: Determines if augmenter should run for this response
// - `generate_sse()`: Produces SSE bytes to inject
//
// # Adding New Augmenters
//
// 1. Create a new file in this directory (e.g., `my_augmenter.rs`)
// 2. Implement the `Augmenter` trait
// 3. Register in `AugmentationPipeline::default()` or via config

mod context_warning;

pub use context_warning::ContextWarningAugmenter;

use crate::SharedContextState;

// ============================================================================
// Augmentation Context
// ============================================================================

/// Context provided to augmenters for decision-making and content generation
pub struct AugmentationContext<'a> {
    /// The model that generated this response (e.g., "claude-3-opus-20240229")
    pub model: &'a str,

    /// Why the response ended ("end_turn", "tool_use", "max_tokens", etc.)
    pub stop_reason: StopReason,

    /// Next available content block index for injection
    pub next_block_index: u32,

    /// Shared context state (token counts, warning thresholds)
    pub context_state: &'a SharedContextState,
}

/// Parsed stop reason for cleaner pattern matching
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Other,
}

impl StopReason {
    /// Parse stop_reason from the SSE chunk text
    pub fn from_chunk(chunk: &str) -> Option<Self> {
        if chunk.contains("\"stop_reason\":\"end_turn\"")
            || chunk.contains("\"stop_reason\": \"end_turn\"")
        {
            Some(StopReason::EndTurn)
        } else if chunk.contains("\"stop_reason\":\"tool_use\"")
            || chunk.contains("\"stop_reason\": \"tool_use\"")
        {
            Some(StopReason::ToolUse)
        } else if chunk.contains("\"stop_reason\":\"max_tokens\"")
            || chunk.contains("\"stop_reason\": \"max_tokens\"")
        {
            Some(StopReason::MaxTokens)
        } else if chunk.contains("\"stop_reason\"") {
            Some(StopReason::Other)
        } else {
            None
        }
    }
}

// ============================================================================
// Augmenter Trait
// ============================================================================

/// Trait for response augmenters (plugins that modify API responses)
///
/// Augmenters are called at the end of SSE streams to optionally inject
/// additional content blocks. They should be stateless where possible,
/// with shared state accessed through `AugmentationContext`.
///
/// # Example
///
/// ```ignore
/// pub struct MyAugmenter;
///
/// impl Augmenter for MyAugmenter {
///     fn name(&self) -> &'static str { "my-augmenter" }
///
///     fn should_apply(&self, ctx: &AugmentationContext) -> bool {
///         ctx.stop_reason == StopReason::EndTurn
///     }
///
///     fn generate_sse(&self, ctx: &AugmentationContext) -> Option<Vec<u8>> {
///         Some(b"event: ...\ndata: ...\n\n".to_vec())
///     }
/// }
/// ```
pub trait Augmenter: Send + Sync {
    /// Human-readable name for logging and debugging
    fn name(&self) -> &'static str;

    /// Check if this augmenter should run for this response
    ///
    /// Called before `generate_sse()`. Return `false` to skip this augmenter.
    /// Common checks: stop_reason, model type, feature flags.
    fn should_apply(&self, ctx: &AugmentationContext) -> bool;

    /// Generate SSE bytes to inject into the response stream
    ///
    /// Only called if `should_apply()` returned `true`.
    /// Return `None` if no injection is needed (e.g., threshold not met).
    /// Return `Some(bytes)` with valid SSE format to inject content.
    fn generate_sse(&self, ctx: &AugmentationContext) -> Option<Vec<u8>>;
}

// ============================================================================
// Augmentation Pipeline
// ============================================================================

/// Pipeline that runs augmenters and collects injections
///
/// The pipeline is the main interface for the proxy to interact with augmenters.
/// It handles iterating through registered augmenters and collecting their output.
pub struct AugmentationPipeline {
    augmenters: Vec<Box<dyn Augmenter>>,
}

impl AugmentationPipeline {
    /// Create an empty pipeline
    pub fn new() -> Self {
        Self {
            augmenters: Vec::new(),
        }
    }

    /// Create pipeline from config (the recommended constructor)
    ///
    /// Registers augmenters based on config settings.
    /// Augmentations are opt-in: only enabled when explicitly configured.
    pub fn from_config(config: &crate::config::Augmentation) -> Self {
        let mut pipeline = Self::new();

        // Context warning augmenter (opt-in)
        if config.context_warning {
            pipeline.register(ContextWarningAugmenter::with_thresholds(
                config.context_warning_thresholds.clone(),
            ));
            tracing::debug!(
                "Registered context-warning augmenter (thresholds: {:?})",
                config.context_warning_thresholds
            );
        }

        pipeline
    }

    /// Register an augmenter
    pub fn register(&mut self, augmenter: impl Augmenter + 'static) {
        self.augmenters.push(Box::new(augmenter));
    }

    /// Check if pipeline has any augmenters
    pub fn is_empty(&self) -> bool {
        self.augmenters.is_empty()
    }

    /// Process a response context and return any injections
    ///
    /// Iterates through all augmenters, calling `should_apply()` and `generate_sse()`.
    /// Returns the first successful injection (augmenters are mutually exclusive for now).
    ///
    /// Future: Could return Vec<Vec<u8>> to allow multiple injections.
    pub fn process(&self, ctx: &AugmentationContext) -> Option<Vec<u8>> {
        for augmenter in &self.augmenters {
            if augmenter.should_apply(ctx) {
                if let Some(sse) = augmenter.generate_sse(ctx) {
                    tracing::debug!(
                        "Augmenter '{}' generated injection ({} bytes)",
                        augmenter.name(),
                        sse.len()
                    );
                    return Some(sse);
                }
            }
        }
        None
    }

    /// Get names of registered augmenters (for logging/debug)
    pub fn augmenter_names(&self) -> Vec<&'static str> {
        self.augmenters.iter().map(|a| a.name()).collect()
    }
}

impl Default for AugmentationPipeline {
    /// Create empty pipeline (no augmenters registered)
    ///
    /// Use `from_config()` for config-driven registration.
    fn default() -> Self {
        Self::new()
    }
}
