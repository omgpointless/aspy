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

    /// Generate tiered message based on urgency level
    ///
    /// `urgency` is the threshold that was crossed (60, 80, 85, 90, 95) - used to select message tone.
    /// The actual percentage is calculated from current/limit and displayed to the user.
    fn format_message(&self, urgency: u8, current: u64, limit: u64) -> String {
        // Calculate actual percentage, rounded to one decimal place
        let actual_percent = if limit > 0 {
            (current as f64 / limit as f64) * 100.0
        } else {
            0.0
        };

        match urgency {
            95.. => format!(
                "User! Context is now at {:.1}% ({}/{}). Aspy recommends you execute `/aspy:pre-compact` before 98% to ensure continuity and advices to not start complex tool flows.\n",
                actual_percent, current, limit
            ),
            85..=94 => format!(
                "For user's consideration, context is at {:.1}% ({}/{}).",
                actual_percent, current, limit
            ),
            80..=84 => format!("Context at {:.1}% ({}/{}).", actual_percent, current, limit),
            _ => format!("Context at {:.1}% ({}/{}).", actual_percent, current, limit),
        }
    }

    /// Build the annotation text with styled borders
    fn format_annotation(&self, message: &str) -> String {
        format!(
            "\n\n`★ aspy (context-warning augmentation) ────────────────`\n\
             {}\n\
             `───────────────────────────────────────────────────────`",
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

        // Skip warning if context is recovering (CC crunched tool_results)
        // The high context value is stale - actual context will be lower post-crunch
        if state.is_recovering() {
            tracing::debug!(
                "context-warning: skipping warning due to recovery_pending (stale high-water mark)"
            );
            return None;
        }

        let threshold = state.should_warn_at(&self.thresholds)?;

        // Calculate values for message
        let urgency = threshold;
        let current = state.current_tokens;
        let limit = state.limit;

        // Mark that we warned at this threshold (prevents duplicate warnings)
        state.mark_warned(threshold);

        // Drop the lock before doing string formatting
        drop(state);

        // Generate the message and annotation
        let message = self.format_message(urgency, current, limit);
        let annotation = self.format_annotation(&message);

        tracing::info!(
            "Context warning: urgency={}, actual={:.1}% ({}/{}) at block #{}",
            urgency,
            if limit > 0 {
                (current as f64 / limit as f64) * 100.0
            } else {
                0.0
            },
            current,
            limit,
            ctx.next_block_index
        );

        let sse_bytes = self.generate_sse_block(ctx.next_block_index, &annotation);
        Some(AugmentedContent::from_text(sse_bytes, &annotation))
    }
}
