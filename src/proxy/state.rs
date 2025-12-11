//! Proxy state types and shared coordination structures

use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use tokio::sync::mpsc;

use crate::config::ClientsConfig;
use crate::events::{ProxyEvent, TrackedEvent};
use crate::parser::Parser;
use crate::pipeline::{EventPipeline, ProcessContext};
use crate::{SharedContextState, StreamingThinking};

use super::api;
use super::augmentation::AugmentationPipeline;
use super::count_tokens;
use super::sessions;
use super::transformation;
use super::translation::{TranslationContext, TranslationPipeline};

// ─────────────────────────────────────────────────────────────────────────────
// Proxy State
// ─────────────────────────────────────────────────────────────────────────────

/// Shared state for the proxy server
#[derive(Clone)]
pub struct ProxyState {
    /// HTTP client for forwarding requests
    pub(super) client: reqwest::Client,
    /// Parser for extracting tool calls
    pub(super) parser: Parser,
    /// Channel for sending tracked events to TUI (includes user/session context)
    pub(crate) event_tx_tui: mpsc::Sender<TrackedEvent>,
    /// Channel for sending tracked events to storage (includes user/session context)
    pub(crate) event_tx_storage: mpsc::Sender<TrackedEvent>,
    /// Target API URL (default, used when no client routing configured)
    pub(super) api_url: String,
    /// Shared buffer for streaming thinking content to TUI
    pub(super) streaming_thinking: StreamingThinking,
    /// Shared context state for augmentation
    pub(super) context_state: SharedContextState,
    /// Augmentation pipeline for response modification
    pub(super) augmentation: Arc<AugmentationPipeline>,
    /// Shared statistics for API endpoints
    pub(super) stats: api::SharedStats,
    /// Shared events buffer for API endpoints
    pub(super) events: api::SharedEvents,
    /// Session manager for multi-user tracking
    pub sessions: api::SharedSessions,
    /// Log directory for session log search
    pub log_dir: std::path::PathBuf,
    /// Client and provider configuration for multi-user routing
    pub(super) clients: ClientsConfig,
    /// Event processing pipeline (optional, for cortex storage and other processors)
    pub(super) pipeline: Option<Arc<EventPipeline>>,
    /// Query interface for cortex database (optional, requires cortex enabled)
    pub cortex_query: Option<Arc<crate::pipeline::cortex_query::CortexQuery>>,
    /// Translation pipeline for OpenAI ↔ Anthropic format conversion
    pub(super) translation: Arc<TranslationPipeline>,
    /// Transformation pipeline for request modification (system-reminder editing, etc.)
    pub(super) transformation: Arc<transformation::TransformationPipeline>,
    /// Transformers config (for enabled flag and future per-transformer settings)
    pub(super) transformers_config: crate::config::Transformers,
    /// Count tokens request cache and rate limiter
    pub(super) count_tokens_cache: Arc<count_tokens::CountTokensCache>,
    /// Handle to the embedding indexer (optional, requires embeddings enabled)
    pub embedding_indexer: Option<crate::pipeline::embedding_indexer::IndexerHandle>,
}

impl ProxyState {
    /// Send an event to TUI, storage, and user's session
    ///
    /// Events are processed through the pipeline (if configured) before dispatch.
    /// Events are wrapped in TrackedEvent with user/session context for filtering.
    /// We ignore errors here to avoid blocking the proxy if a receiver is slow or closed.
    pub(super) async fn send_event(&self, event: ProxyEvent, user_id: Option<&str>) {
        // Build ProcessContext for pipeline
        // Extract session_id and transcript_path together (single lock)
        let (session_id, transcript_path) = user_id
            .and_then(|uid| {
                self.sessions.lock().ok().and_then(|sessions| {
                    sessions
                        .get_user_session(&sessions::UserId::new(uid))
                        .map(|s| (s.key.to_string(), s.transcript_path.clone()))
                })
            })
            .map(|(sid, tp)| (Some(sid), tp))
            .unwrap_or((None, None));

        let ctx = ProcessContext::new(
            session_id.as_deref(),
            user_id,
            transcript_path.as_deref(),
            false, // is_demo = false for real traffic
        );

        // Process through pipeline if available
        let final_event = if let Some(pipeline) = &self.pipeline {
            match pipeline.process(&event, &ctx) {
                Some(processed) => processed.into_owned(),
                None => return, // Event was filtered out
            }
        } else {
            event
        };

        // Wrap in TrackedEvent with user/session context
        let tracked = TrackedEvent::new(
            final_event.clone(),
            user_id.map(|s| s.to_string()),
            session_id.clone(),
        );

        // Send tracked event to TUI and storage channels
        let _ = self.event_tx_tui.send(tracked.clone()).await;
        let _ = self.event_tx_storage.send(tracked).await;

        // Also record raw event to user's session (SessionManager tracks its own events)
        // May return synthesized events (e.g., TodoSnapshot) to emit
        if let Some(uid) = user_id {
            let synthesized = if let Ok(mut sessions) = self.sessions.lock() {
                sessions.record_event(&sessions::UserId::new(uid), final_event)
            } else {
                None
            };

            // Emit any synthesized events (goes to storage only, not back to sessions)
            if let Some(synth_event) = synthesized {
                self.emit_synthesized_event(synth_event, user_id, session_id)
                    .await;
            }
        }
    }

    /// Emit a synthesized event (e.g., TodoSnapshot)
    ///
    /// Unlike `emit_event`, this does NOT record back to sessions (to avoid recursion).
    /// Used for derived/computed events that should only go to storage and TUI.
    pub(super) async fn emit_synthesized_event(
        &self,
        event: ProxyEvent,
        user_id: Option<&str>,
        session_id: Option<String>,
    ) {
        // Get transcript path from session if available
        let transcript_path: Option<String> = user_id.and_then(|uid| {
            self.sessions.lock().ok().and_then(|s| {
                s.get_user_session(&sessions::UserId::new(uid))
                    .and_then(|sess| sess.transcript_path.clone())
            })
        });

        let ctx = ProcessContext::new(
            session_id.as_deref(),
            user_id,
            transcript_path.as_deref(),
            false,
        );

        // Process through pipeline (for cortex storage)
        let final_event = if let Some(pipeline) = &self.pipeline {
            match pipeline.process(&event, &ctx) {
                Some(processed) => processed.into_owned(),
                None => return, // Event was filtered out
            }
        } else {
            event
        };

        // Wrap in TrackedEvent
        let tracked = TrackedEvent::new(final_event, user_id.map(|s| s.to_string()), session_id);

        // Send to TUI and storage (NOT to sessions - that would recurse)
        let _ = self.event_tx_tui.send(tracked.clone()).await;
        let _ = self.event_tx_storage.send(tracked).await;

        tracing::debug!("Emitted synthesized TodoSnapshot event");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Input Types
// ─────────────────────────────────────────────────────────────────────────────

/// Event broadcast channels for TUI and storage consumers
#[derive(Clone)]
pub struct EventChannels {
    /// Channel for sending tracked events to TUI (includes user/session context)
    pub tui: mpsc::Sender<TrackedEvent>,
    /// Channel for sending tracked events to storage (includes user/session context)
    pub storage: mpsc::Sender<TrackedEvent>,
}

/// Shared state passed to the proxy for cross-task coordination
///
/// All fields are Arc<Mutex<T>> for safe concurrent access across:
/// - Proxy (writes stats, events, session updates)
/// - TUI (reads for display)
/// - HTTP API (reads for external queries)
#[derive(Clone)]
pub struct SharedState {
    /// Accumulated statistics (tokens, costs, tool calls)
    pub stats: api::SharedStats,
    /// Event buffer for API queries
    pub events: api::SharedEvents,
    /// Session manager for multi-user tracking
    pub sessions: api::SharedSessions,
    /// Context window state for augmentation decisions
    pub context: SharedContextState,
    /// Buffer for streaming thinking content to TUI
    pub streaming_thinking: StreamingThinking,
    /// Event processing pipeline (optional)
    pub pipeline: Option<Arc<EventPipeline>>,
    /// Query interface for cortex database (optional, requires cortex enabled)
    pub cortex_query: Option<Arc<crate::pipeline::cortex_query::CortexQuery>>,
    /// Handle to the embedding indexer (optional, requires embeddings enabled)
    pub embedding_indexer: Option<crate::pipeline::embedding_indexer::IndexerHandle>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Response Context
// ─────────────────────────────────────────────────────────────────────────────

/// Context for handling an API response
pub(super) struct ResponseContext {
    pub response: reqwest::Response,
    pub status: reqwest::StatusCode,
    pub headers: reqwest::header::HeaderMap,
    pub start: Instant,
    pub ttfb: std::time::Duration,
    pub request_id: String,
    pub is_messages_endpoint: bool,
    pub state: ProxyState,
    /// User ID (api_key_hash) for session tracking
    pub user_id: Option<String>,
    /// Translation context for response translation (if format differs)
    pub translation_ctx: TranslationContext,
    /// Original request body for count_tokens caching (if applicable)
    pub count_tokens_request_body: Option<Bytes>,
}
