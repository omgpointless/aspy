// Context Warning Augmenter
//
// Injects context usage warnings into API responses when token usage
// exceeds configurable thresholds (60%, 80%, 85%, 90%, 95%).
//
// The warnings appear as styled annotation boxes in Claude Code's output,
// alerting the user when context is filling up and `/compact` may be needed.
//
// # Filtering
//
// This augmenter skips:
// - Haiku responses (utility calls, not main conversation)
// - tool_use responses (intermediate steps, not user-facing)
//
// It only injects on end_turn responses from Opus/Sonnet models.

use super::{AugmentationContext, AugmentedContent, Augmenter, StopReason};

/// Augmenter that injects context usage warnings
///
/// # Configuration (Future)
///
/// ```toml
/// [augmentation.context_warning]
/// enabled = true
/// thresholds = [60, 80, 85, 90, 95]
/// ```
pub struct ContextWarningAugmenter {
    /// Thresholds at which to warn (percentages)
    thresholds: Vec<u8>,
}

impl ContextWarningAugmenter {
    /// Create with default thresholds
    pub fn new() -> Self {
        Self {
            thresholds: vec![60, 80, 85, 90, 95],
        }
    }

    /// Create with custom thresholds
    pub fn with_thresholds(thresholds: Vec<u8>) -> Self {
        Self { thresholds }
    }

    /// Generate tiered message based on usage percentage
    fn format_message(&self, percent: u8, current_k: u64, limit_k: u64) -> String {
        match percent {
            95.. => format!(
                "Context at ~{}% ({}K/{}K). `/aspy:tempcontext` recommended.",
                percent, current_k, limit_k
            ),
            85..=94 => format!(
                "Context at ~{}% ({}K/{}K). Consider `/aspy:tempcontext` soon.",
                percent, current_k, limit_k
            ),
            80..=84 => format!("Context at {}% ({}K/{}K).", percent, current_k, limit_k),
            _ => format!(
                "Context now at ~{}% ({}K/{}K). Halfway there 🚀",
                percent, current_k, limit_k
            ),
        }
    }

    /// Build the annotation text with styled borders
    fn format_annotation(&self, message: &str) -> String {
        format!(
            "\n\n`★ aspy (context) ─────────────────────────────────────`\n\
             {}\n\
             `─────────────────────────────────────────────────────────`",
            message
        )
    }

    /// Generate SSE events for a text content block injection
    fn generate_sse_block(&self, index: u32, text: &str) -> Vec<u8> {
        // Escape text for JSON
        let escaped_text = serde_json::to_string(text).unwrap_or_default();

        // Build SSE events for content block
        // IMPORTANT: SSE format requires "data:" at column 0, no leading whitespace
        let sse = format!(
            "event: content_block_start\n\
             data: {{\"type\":\"content_block_start\",\"index\":{idx},\"content_block\":{{\"type\":\"text\",\"text\":\"\"}}}}\n\n\
             event: content_block_delta\n\
             data: {{\"type\":\"content_block_delta\",\"index\":{idx},\"delta\":{{\"type\":\"text_delta\",\"text\":{text}}}}}\n\n\
             event: content_block_stop\n\
             data: {{\"type\":\"content_block_stop\",\"index\":{idx}}}\n\n",
            idx = index,
            text = escaped_text
        );

        sse.into_bytes()
    }
}

impl Default for ContextWarningAugmenter {
    fn default() -> Self {
        Self::new()
    }
}

impl Augmenter for ContextWarningAugmenter {
    fn name(&self) -> &'static str {
        "context-warning"
    }

    fn should_apply(&self, ctx: &AugmentationContext) -> bool {
        // Skip Haiku responses (utility calls like topic generation)
        if ctx.model.to_lowercase().contains("haiku") {
            tracing::trace!("context-warning: skipping Haiku response");
            return false;
        }

        // Skip tool_use responses (intermediate, not user-facing)
        if ctx.stop_reason == StopReason::ToolUse {
            tracing::trace!("context-warning: skipping tool_use response");
            return false;
        }

        // Only inject on end_turn (final response to user)
        if ctx.stop_reason != StopReason::EndTurn {
            tracing::trace!("context-warning: skipping non-end_turn response");
            return false;
        }

        true
    }

    fn generate(&self, ctx: &AugmentationContext) -> Option<AugmentedContent> {
        // Lock context state and check if we should warn at our thresholds
        let mut state = ctx.context_state.lock().ok()?;
        let threshold = state.should_warn_at(&self.thresholds)?;

        // Calculate values for message
        let percent = threshold;
        let current_k = state.current_tokens / 1000;
        let limit_k = state.limit / 1000;

        // Mark that we warned at this threshold (prevents duplicate warnings)
        state.mark_warned(threshold);

        // Drop the lock before doing string formatting
        drop(state);

        // Generate the message and annotation
        let message = self.format_message(percent, current_k, limit_k);
        let annotation = self.format_annotation(&message);

        tracing::info!(
            "Context warning: {}% ({}K/{}K) at block #{}",
            percent,
            current_k,
            limit_k,
            ctx.next_block_index
        );

        let sse_bytes = self.generate_sse_block(ctx.next_block_index, &annotation);
        Some(AugmentedContent::from_text(sse_bytes, &annotation))
    }
}
