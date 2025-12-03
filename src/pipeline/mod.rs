//! Event processing pipeline for extensible event handling
//!
//! This module provides a trait-based system for processing events before
//! they are dispatched to consumers (TUI, storage, sessions). Processors
//! can transform, filter, or react to events without modifying core logic.
//!
//! # Architecture
//!
//! ```text
//! ProxyEvent → EventPipeline → [Processor₁, Processor₂, ...] → Processed Event
//! ```
//!
//! # Processor Types
//!
//! Processors can perform three operations:
//! - **Filter**: Drop events (return `ProcessResult::Drop`)
//! - **Transform**: Modify events (return `ProcessResult::Transform(modified)`)
//! - **Side-effect**: React to events without modification (return `ProcessResult::Continue`)

use crate::events::ProxyEvent;
use std::borrow::Cow;
use std::sync::Arc;

pub mod lifestats;
pub mod lifestats_query;
pub mod logging;

/// Result of processing an event
#[derive(Debug)]
pub enum ProcessResult {
    /// Event continues unchanged (side-effect only processor)
    Continue,
    /// Event was transformed - use this new version (boxed to reduce enum size)
    #[allow(dead_code)] // Phase 2: Redaction/transformation processors
    Transform(Box<ProxyEvent>),
    /// Event should be dropped (filtered out)
    #[allow(dead_code)] // Phase 2: Filtering processors
    Drop,
    /// Processor encountered an error (event continues, error logged)
    #[allow(dead_code)] // Phase 2: Error handling
    Error(anyhow::Error),
}

/// Context provided to processors for decision-making
///
/// Uses `Arc<str>` for cheap cloning - processor side-effects often need
/// to clone context for async operations, and Arc clone is just a refcount bump.
#[derive(Debug, Clone, Default)]
pub struct ProcessContext {
    /// Current session ID (if known)
    pub session_id: Option<Arc<str>>,
    /// User ID (API key hash, if known)
    #[allow(dead_code)] // Phase 2: User-specific filtering/routing
    pub user_id: Option<Arc<str>>,
    /// Whether this is a demo/test event
    #[allow(dead_code)] // Phase 2: Demo event filtering
    pub is_demo: bool,
}

impl ProcessContext {
    pub fn new(session_id: Option<&str>, user_id: Option<&str>, is_demo: bool) -> Self {
        Self {
            session_id: session_id.map(Arc::from),
            user_id: user_id.map(Arc::from),
            is_demo,
        }
    }
}

/// Trait for event processors
///
/// Processors are called in registration order. Each processor can:
/// - Transform the event (return `ProcessResult::Transform(new_event)`)
/// - Filter the event (return `ProcessResult::Drop`)
/// - Perform side effects and pass through (return `ProcessResult::Continue`)
///
/// # Sync Design
///
/// `process` is intentionally synchronous. For I/O-bound operations
/// (database writes, HTTP calls), processors should use internal
/// channels to offload work to dedicated threads. This ensures the
/// pipeline never blocks the async runtime.
///
/// # Reference Semantics
///
/// Processors receive a reference to the event. Only processors that
/// need to transform the event should clone it. Side-effect processors
/// return `Continue` without any allocation.
pub trait EventProcessor: Send + Sync {
    /// Human-readable name for logging and debugging
    fn name(&self) -> &'static str;

    /// Process an event, returning the result
    ///
    /// # Arguments
    /// * `event` - Reference to the event (clone only if transforming)
    /// * `ctx` - Context about the current session/user
    ///
    /// # Returns
    /// * `ProcessResult::Continue` - Pass event unchanged to next processor
    /// * `ProcessResult::Transform(event)` - Pass modified event to next processor
    /// * `ProcessResult::Drop` - Remove event from pipeline
    /// * `ProcessResult::Error(e)` - Log error, continue with original event
    fn process(&self, event: &ProxyEvent, ctx: &ProcessContext) -> ProcessResult;

    /// Called when the pipeline is shutting down
    ///
    /// Use this for cleanup: flush buffers, signal threads to stop, etc.
    /// Implementations MUST block until cleanup is complete (e.g., background
    /// threads have finished flushing).
    fn shutdown(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Pipeline that runs events through registered processors
pub struct EventPipeline {
    processors: Vec<Arc<dyn EventProcessor>>,
}

impl EventPipeline {
    /// Create an empty pipeline (passthrough)
    pub fn new() -> Self {
        Self {
            processors: Vec::new(),
        }
    }

    /// Register a processor
    ///
    /// Processors are called in registration order.
    pub fn register(&mut self, processor: impl EventProcessor + 'static) {
        self.processors.push(Arc::new(processor));
    }

    /// Process an event through all registered processors
    ///
    /// Returns `Some(Cow::Borrowed(event))` if no transformation occurred,
    /// `Some(Cow::Owned(event))` if any processor transformed the event,
    /// `None` if any processor filtered it out.
    ///
    /// Using `Cow` avoids cloning when all processors are side-effect-only.
    pub fn process<'a>(
        &self,
        event: &'a ProxyEvent,
        ctx: &ProcessContext,
    ) -> Option<Cow<'a, ProxyEvent>> {
        if self.processors.is_empty() {
            return Some(Cow::Borrowed(event));
        }

        // Track whether we've had to clone yet
        let mut current: Cow<'a, ProxyEvent> = Cow::Borrowed(event);

        for processor in &self.processors {
            match processor.process(current.as_ref(), ctx) {
                ProcessResult::Continue => {
                    // No change, keep current (borrowed or owned)
                }
                ProcessResult::Transform(new_event) => {
                    // Processor transformed the event (unbox from heap)
                    current = Cow::Owned(*new_event);
                }
                ProcessResult::Drop => {
                    tracing::trace!("Event dropped by processor '{}'", processor.name());
                    return None;
                }
                ProcessResult::Error(error) => {
                    tracing::error!("Processor '{}' error: {}", processor.name(), error);
                    // Continue with current event despite error
                }
            }
        }
        Some(current)
    }

    /// Shutdown all processors gracefully
    ///
    /// Calls shutdown() on each processor in reverse registration order.
    /// Blocks until all processors have completed cleanup.
    pub fn shutdown(&self) -> anyhow::Result<()> {
        // Shutdown in reverse order (LIFO) - processors registered last
        // may depend on those registered first
        for processor in self.processors.iter().rev() {
            if let Err(e) = processor.shutdown() {
                tracing::warn!("Processor '{}' shutdown error: {}", processor.name(), e);
            }
        }
        Ok(())
    }

    /// Check if pipeline has any processors
    #[allow(dead_code)] // Phase 2: Public API for introspection
    pub fn is_empty(&self) -> bool {
        self.processors.is_empty()
    }

    /// Get names of registered processors (for logging/debug)
    #[allow(dead_code)] // Phase 2: Debugging and health endpoints
    pub fn processor_names(&self) -> Vec<&'static str> {
        self.processors.iter().map(|p| p.name()).collect()
    }
}

impl Default for EventPipeline {
    fn default() -> Self {
        Self::new()
    }
}
