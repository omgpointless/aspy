// HTTP API module - Exposes observability data via REST endpoints
//
// This module provides programmatic access to session state, enabling:
// - Slash commands that query proxy stats
// - MCP servers that expose data to Claude
// - External integrations (hooks, dashboards, alerts)
//
// All endpoints return JSON and are designed for local consumption only.
// Security: Binds to 127.0.0.1 by default (localhost only).

mod context;
mod cortex;
mod embeddings;
mod events;
mod hooks;
mod search;
mod sessions;
mod stats;
mod whoami;

use crate::events::{ProxyEvent, Stats};
use axum::{http::StatusCode, response::IntoResponse};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

// Re-export endpoint handlers
pub use context::{get_context, get_context_snapshot};
pub use cortex::{
    cortex_cleanup, cortex_context, cortex_context_user, cortex_health, cortex_search_prompts,
    cortex_search_responses, cortex_search_thinking, cortex_search_user_prompts,
    cortex_search_user_responses, cortex_search_user_thinking, cortex_stats, cortex_stats_user,
    cortex_todos,
};
pub use embeddings::{
    cortex_context_hybrid_user, cortex_embedding_poll, cortex_embedding_reindex,
    cortex_embedding_status,
};
pub use events::get_events;
pub use hooks::hook_precompact;
pub use search::search_logs;
pub use sessions::{
    get_session_todos, get_sessions, session_end, session_reconnect, session_start,
};
pub use stats::get_stats;
pub use whoami::{get_session_history, get_whoami};

// Re-export types used by proxy/mod.rs
pub use sessions::SessionHistoryItem;
pub use sessions::SessionStatsSummary;

/// Shared statistics accessible to API endpoints
/// This allows both the TUI and HTTP API handlers to read session stats
pub type SharedStats = Arc<Mutex<Stats>>;

/// Shared events buffer accessible to API endpoints
/// Ring buffer that keeps the most recent N events for querying
pub type SharedEvents = Arc<Mutex<EventBuffer>>;

/// Shared session manager for multi-user session tracking
pub type SharedSessions = Arc<Mutex<crate::proxy::sessions::SessionManager>>;

/// Maximum number of events to keep in the shared buffer
const MAX_EVENTS: usize = 500;

/// Ring buffer for events with max capacity
#[derive(Debug, Default)]
pub struct EventBuffer {
    events: VecDeque<ProxyEvent>,
}

impl EventBuffer {
    pub fn new() -> Self {
        Self {
            events: VecDeque::with_capacity(MAX_EVENTS),
        }
    }

    /// Add an event, dropping oldest if at capacity
    pub fn push(&mut self, event: ProxyEvent) {
        if self.events.len() >= MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    /// Get events, optionally filtered by type
    pub fn get_events(&self, filter: Option<&str>, limit: usize) -> Vec<&ProxyEvent> {
        self.events
            .iter()
            .rev() // Most recent first
            .filter(|e| {
                filter.is_none_or(|f| {
                    let type_name = event_type_name(e);
                    type_name.eq_ignore_ascii_case(f)
                })
            })
            .take(limit)
            .collect()
    }

    /// Total events in buffer
    pub fn len(&self) -> usize {
        self.events.len()
    }
}

/// Get the type name of an event (matches JSON "type" field)
pub(crate) fn event_type_name(event: &ProxyEvent) -> &'static str {
    match event {
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
    }
}

/// API error responses
/// Converted to HTTP status codes via IntoResponse
#[derive(Debug)]
pub enum ApiError {
    Internal(String),
    BadRequest(String),
    NotFound(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
        };

        tracing::error!("API error: {} - {}", status, message);

        (status, message).into_response()
    }
}
