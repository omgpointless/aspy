//! Logging processor for pipeline validation
//!
//! A simple side-effect processor that logs event types at trace level.
//! Used to validate that the pipeline is processing events correctly.

use super::{EventProcessor, ProcessContext, ProcessResult};
use crate::events::ProxyEvent;

/// Simple processor that logs events for debugging
#[allow(dead_code)] // Test processor for pipeline validation (Phase 1a requirement)
pub struct LoggingProcessor;

impl LoggingProcessor {
    #[allow(dead_code)] // Test processor constructor
    pub fn new() -> Self {
        Self
    }
}

impl EventProcessor for LoggingProcessor {
    fn name(&self) -> &'static str {
        "logging"
    }

    fn process(&self, event: &ProxyEvent, ctx: &ProcessContext) -> ProcessResult {
        // Extract event type name from variant
        let event_type = match event {
            ProxyEvent::ToolCall { .. } => "ToolCall",
            ProxyEvent::ToolResult { .. } => "ToolResult",
            ProxyEvent::Request { .. } => "Request",
            ProxyEvent::Response { .. } => "Response",
            ProxyEvent::Error { .. } => "Error",
            ProxyEvent::HeadersCaptured { .. } => "HeadersCaptured",
            ProxyEvent::RateLimitUpdate { .. } => "RateLimitUpdate",
            ProxyEvent::ApiUsage { .. } => "ApiUsage",
            ProxyEvent::Thinking { .. } => "Thinking",
            ProxyEvent::ContextCompact { .. } => "ContextCompact",
            ProxyEvent::ThinkingStarted { .. } => "ThinkingStarted",
            ProxyEvent::UserPrompt { .. } => "UserPrompt",
            ProxyEvent::AssistantResponse { .. } => "AssistantResponse",
            ProxyEvent::RequestTransformed { .. } => "RequestTransformed",
            ProxyEvent::ResponseAugmented { .. } => "ResponseAugmented",
            ProxyEvent::PreCompactHook { .. } => "PreCompactHook",
            ProxyEvent::ContextRecovery { .. } => "ContextRecovery",
            ProxyEvent::TodoSnapshot { .. } => "TodoSnapshot",
            ProxyEvent::ContextEstimate { .. } => "ContextEstimate",
        };

        // Log event type with context
        tracing::trace!(
            processor = self.name(),
            event_type = event_type,
            session_id = ?ctx.session_id,
            user_id = ?ctx.user_id,
            is_demo = ctx.is_demo,
            "Pipeline event"
        );

        // Pass through unchanged
        ProcessResult::Continue
    }
}

impl Default for LoggingProcessor {
    fn default() -> Self {
        Self::new()
    }
}
