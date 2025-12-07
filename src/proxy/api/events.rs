// Events endpoint - Returns recent events with optional filtering

use super::{event_type_name, ApiError, MAX_EVENTS};
use crate::events::ProxyEvent;
use crate::proxy::sessions::UserId;
use axum::{extract::Query, extract::State, Json};
use serde::{Deserialize, Serialize};

/// Query parameters for /api/events endpoint
#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    /// Maximum number of events to return (default: 50)
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Filter by event type (e.g., "ToolCall", "Thinking", "ApiUsage")
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    /// Filter to specific user (api_key_hash, e.g., "b0acf41e12907b7b")
    pub user: Option<String>,
}

fn default_limit() -> usize {
    50
}

/// Response wrapper for events list
#[derive(Debug, Serialize)]
pub struct EventsResponse {
    /// Total events in buffer
    pub total_in_buffer: usize,
    /// Number of events returned (after filtering)
    pub returned: usize,
    /// The events (most recent first)
    pub events: Vec<ProxyEvent>,
}

/// GET /api/events - Returns recent events with optional filtering
///
/// Query params:
///   - limit: Max events to return (default: 50, max: 500)
///   - type: Filter by event type (ToolCall, ToolResult, Thinking, etc.)
///   - user: Filter to specific user's session events (api_key_hash)
pub async fn get_events(
    State(state): State<crate::proxy::ProxyState>,
    Query(params): Query<EventsQuery>,
) -> Result<Json<EventsResponse>, ApiError> {
    // Cap limit at MAX_EVENTS
    let limit = params.limit.min(MAX_EVENTS);

    // If user filter provided, get events from their session; otherwise use global
    let (total_in_buffer, filtered) = if let Some(ref user_hash) = params.user {
        let sessions = state
            .sessions
            .lock()
            .map_err(|e| ApiError::Internal(format!("Failed to lock sessions: {}", e)))?;

        let user_id = UserId::new(user_hash);
        if let Some(session) = sessions.get_user_session(&user_id) {
            let total = session.events.len();
            let events: Vec<ProxyEvent> = session
                .events
                .iter()
                .rev() // Most recent first
                .filter(|e| {
                    params.event_type.as_ref().is_none_or(|f| {
                        let type_name = event_type_name(e);
                        type_name.eq_ignore_ascii_case(f)
                    })
                })
                .take(limit)
                .cloned()
                .collect();
            (total, events)
        } else {
            // User not found - return empty
            (0, Vec::new())
        }
    } else {
        // No filter - use global events buffer
        let events_buffer = state
            .events
            .lock()
            .map_err(|e| ApiError::Internal(format!("Failed to lock events: {}", e)))?;

        let total = events_buffer.len();
        let events: Vec<ProxyEvent> = events_buffer
            .get_events(params.event_type.as_deref(), limit)
            .into_iter()
            .cloned()
            .collect();
        (total, events)
    };

    let response = EventsResponse {
        total_in_buffer,
        returned: filtered.len(),
        events: filtered,
    };

    Ok(Json(response))
}
