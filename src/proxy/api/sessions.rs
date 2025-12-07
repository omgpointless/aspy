// Session management endpoints

use super::ApiError;
use crate::events::{ProxyEvent, TrackedEvent};
use crate::proxy::sessions::{EndReason, SessionKey, SessionSource, TodoItem, TodoStatus, UserId};
use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// Session Start/End/Reconnect
// ============================================================================

/// Request body for POST /api/session/start
#[derive(Debug, Deserialize)]
pub struct SessionStartRequest {
    /// Claude Code's session_id (from SessionStart hook)
    pub session_id: String,
    /// User's API key hash (first 16 chars of SHA-256)
    pub user_id: String,
    /// How the session was started
    #[serde(default)]
    pub source: Option<String>,
    /// Path to Claude Code's transcript file (e.g., ~/.claude/projects/.../abc123.jsonl)
    #[serde(default)]
    pub transcript_path: Option<String>,
}

/// Request body for POST /api/session/end
#[derive(Debug, Deserialize)]
pub struct SessionEndRequest {
    /// Claude Code's session_id
    pub session_id: String,
    /// User's API key hash
    pub user_id: String,
    /// End reason from hook
    #[serde(default)]
    pub reason: Option<String>,
}

/// Response for session lifecycle operations
#[derive(Debug, Serialize)]
pub struct SessionActionResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
}

/// POST /api/session/start - Register a new session (from SessionStart hook)
///
/// Called by the SessionStart hook when Claude Code starts.
/// Creates a new session, superseding any previous session for this user.
/// If a transcript_path is provided and exists in the database, estimates
/// the context window from historical data.
pub async fn session_start(
    State(state): State<crate::proxy::ProxyState>,
    Json(request): Json<SessionStartRequest>,
) -> Result<Json<SessionActionResponse>, ApiError> {
    let user_id = UserId::new(&request.user_id);
    let source = match request.source.as_deref() {
        Some("hook") => SessionSource::Hook,
        Some("warmup") => SessionSource::Warmup,
        _ => SessionSource::Hook, // Default for explicit start
    };

    // If transcript_path is provided, check if we have historical context data
    let estimated_context = if let Some(ref transcript_path) = request.transcript_path {
        if let Some(query) = state.cortex_query.as_ref() {
            // First find the session_id for this transcript
            match query.find_session_by_transcript(transcript_path) {
                Ok(Some((session_id, _user_id))) => {
                    // Then get the last context tokens for that session
                    match query.get_session_last_context(&session_id) {
                        Ok(context) => {
                            if context.is_some() {
                                tracing::debug!(
                                    transcript_path = %transcript_path,
                                    session_id = %session_id,
                                    estimated_context = ?context,
                                    "Found historical context for transcript"
                                );
                            }
                            context
                        }
                        Err(e) => {
                            tracing::warn!(
                                transcript_path = %transcript_path,
                                error = %e,
                                "Failed to query context estimate"
                            );
                            None
                        }
                    }
                }
                Ok(None) => None, // New transcript, no history
                Err(e) => {
                    tracing::warn!(
                        transcript_path = %transcript_path,
                        error = %e,
                        "Failed to find session by transcript"
                    );
                    None
                }
            }
        } else {
            None // No cortex query available
        }
    } else {
        None // No transcript_path provided
    };

    tracing::debug!(
        session_id = %request.session_id,
        user_id = %request.user_id,
        source = %source,
        estimated_context = ?estimated_context,
        "Starting session via hook for user {} with session {}",
        user_id.short(),
        request.session_id
    );

    let mut sessions = state
        .sessions
        .lock()
        .map_err(|e| ApiError::Internal(format!("Failed to lock sessions: {}", e)))?;

    let session = sessions.start_session(
        user_id,
        Some(request.session_id.clone()),
        source,
        request.transcript_path.clone(),
        estimated_context,
    );
    let session_key = session.key.to_string();

    // Drop the sessions lock before sending events
    drop(sessions);

    // If we have an estimated context from historical data, emit an event
    // so the TUI can update its per-user context state immediately
    if let Some(tokens) = estimated_context {
        let event = ProxyEvent::ContextEstimate {
            timestamp: Utc::now(),
            estimated_tokens: tokens,
        };
        let tracked = TrackedEvent {
            user_id: Some(request.user_id.clone()),
            session_id: Some(request.session_id.clone()),
            tracked_at: Utc::now(),
            event,
        };

        // Send to TUI and storage (use try_send to avoid async, ignore errors)
        let _ = state.event_tx_tui.try_send(tracked.clone());
        let _ = state.event_tx_storage.try_send(tracked);
    }

    tracing::info!(
        session_id = %request.session_id,
        source = %source,
        transcript_path = ?request.transcript_path,
        estimated_context = ?estimated_context,
        "Session started with session_key {}",
        session_key
    );

    Ok(Json(SessionActionResponse {
        success: true,
        message: "Session started".to_string(),
        session_key: Some(session_key),
    }))
}

/// POST /api/session/end - End a session (from SessionEnd hook)
///
/// Called by the SessionEnd hook when Claude Code exits.
/// Archives the session for history.
pub async fn session_end(
    State(state): State<crate::proxy::ProxyState>,
    Json(request): Json<SessionEndRequest>,
) -> Result<Json<SessionActionResponse>, ApiError> {
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|e| ApiError::Internal(format!("Failed to lock sessions: {}", e)))?;

    let session_key = SessionKey::explicit(&request.session_id);

    let reason = match request.reason.as_deref() {
        Some("clear") | Some("logout") | Some("prompt_input_exit") => EndReason::Hook,
        _ => EndReason::Hook,
    };

    sessions.end_session(&session_key, reason);

    tracing::info!(
        session_id = %request.session_id,
        user_id = %request.user_id,
        reason = ?request.reason,
        "Session ended via hook: session_id={} user_id={} reason={:?}",
        request.session_id,
        request.user_id,
        request.reason
    );

    Ok(Json(SessionActionResponse {
        success: true,
        message: "Session ended".to_string(),
        session_key: None,
    }))
}

// ============================================================================
// Session Reconnect Endpoint
// ============================================================================

/// Request body for POST /api/session/reconnect
#[derive(Debug, Deserialize)]
pub struct SessionReconnectRequest {
    /// User's API key hash
    pub user_id: String,
    /// Path to Claude Code's transcript file
    pub transcript_path: String,
    /// Current session_id from Claude Code (for logging)
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Response for session reconnect
#[derive(Debug, Serialize)]
pub struct SessionReconnectResponse {
    /// Whether reconnection happened
    pub reconnected: bool,
    /// The session_id (either reconnected or newly associated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Human-readable message
    pub message: String,
}

/// POST /api/session/reconnect - Reconnect to existing session by transcript_path
///
/// Called by UserPromptSubmit hook on every user message. Checks if this
/// transcript_path was seen before (in cortex DB) and reconnects the
/// current user to that session, preserving continuity across proxy restarts.
pub async fn session_reconnect(
    State(state): State<crate::proxy::ProxyState>,
    Json(request): Json<SessionReconnectRequest>,
) -> Result<Json<SessionReconnectResponse>, ApiError> {
    tracing::debug!(
        user_id = %request.user_id,
        transcript_path = %request.transcript_path,
        cc_session_id = ?request.session_id,
        "UserPromptSubmit hook triggered"
    );

    // Need cortex_query to check DB
    let query = state
        .cortex_query
        .as_ref()
        .ok_or_else(|| ApiError::Internal("Cortex query not available".to_string()))?;

    // Check if we've seen this transcript before
    let existing = query
        .find_session_by_transcript(&request.transcript_path)
        .map_err(|e| ApiError::Internal(format!("DB query failed: {}", e)))?;

    if let Some((session_id, stored_user_id)) = existing {
        // Verify user matches (security check)
        if stored_user_id != request.user_id {
            tracing::warn!(
                transcript_path = %request.transcript_path,
                stored_user = %stored_user_id,
                request_user = %request.user_id,
                "Transcript path belongs to different user, ignoring reconnect"
            );
            return Ok(Json(SessionReconnectResponse {
                reconnected: false,
                session_id: None,
                message: "Transcript belongs to different user".to_string(),
            }));
        }

        // Query for the last context tokens to estimate context window on resume
        let estimated_context = query
            .get_session_last_context(&session_id)
            .map_err(|e| ApiError::Internal(format!("Context query failed: {}", e)))?;

        // Reconnect: update in-memory session to use this session_id
        let mut sessions = state
            .sessions
            .lock()
            .map_err(|e| ApiError::Internal(format!("Failed to lock sessions: {}", e)))?;

        let user_id = UserId::new(&request.user_id);
        let reconnected = sessions.reconnect_to_session(
            &user_id,
            &session_id,
            request.transcript_path.clone(),
            estimated_context,
        );

        if reconnected {
            tracing::info!(
                session_id = %session_id,
                cc_session_id = ?request.session_id,
                user_id = %request.user_id,
                transcript_path = %request.transcript_path,
                estimated_context = ?estimated_context,
                "Session reconnected via transcript_path"
            );

            return Ok(Json(SessionReconnectResponse {
                reconnected: true,
                session_id: Some(session_id),
                message: "Reconnected to existing session".to_string(),
            }));
        }
    }

    // No existing session found, or reconnect failed
    // Just store the transcript_path association for the current session
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|e| ApiError::Internal(format!("Failed to lock sessions: {}", e)))?;

    let user_id = UserId::new(&request.user_id);
    if let Some(session) = sessions.get_user_session_mut(&user_id) {
        if session.transcript_path.is_none() {
            session.transcript_path = Some(request.transcript_path.clone());
            tracing::debug!(
                session_key = %session.key,
                transcript_path = %request.transcript_path,
                "Associated transcript_path with current session"
            );
        }
    }

    Ok(Json(SessionReconnectResponse {
        reconnected: false,
        session_id: None,
        message: "No existing session found for transcript".to_string(),
    }))
}

// ============================================================================
// Session List Endpoint
// ============================================================================

/// Single session info for GET /api/sessions
#[derive(Debug, Serialize)]
pub struct SessionListItem {
    /// Session key (session:xxx or user:xxx)
    pub key: String,
    /// User ID (api_key_hash)
    pub user_id: String,
    /// Claude Code's session ID (if from hook)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_session_id: Option<String>,
    /// Path to Claude Code's transcript file (from SessionStart hook)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    /// How session was detected
    pub source: String,
    /// When session started
    pub started: DateTime<Utc>,
    /// Session status (active, idle, ended)
    pub status: String,
    /// Event count in this session
    pub event_count: usize,
    /// Session-specific stats summary
    pub stats: SessionStatsSummary,
}

/// Abbreviated stats for session list
#[derive(Debug, Serialize)]
pub struct SessionStatsSummary {
    pub requests: usize,
    pub tool_calls: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

/// Response for GET /api/sessions
#[derive(Debug, Serialize)]
pub struct SessionListResponse {
    /// Number of active sessions
    pub active_count: usize,
    /// All sessions (including idle)
    pub sessions: Vec<SessionListItem>,
}

/// GET /api/sessions - List all active sessions
///
/// Returns information about all tracked sessions, including their status
/// and summary statistics. Useful for debugging multi-user scenarios.
pub async fn get_sessions(
    State(state): State<crate::proxy::ProxyState>,
) -> Result<Json<SessionListResponse>, ApiError> {
    let sessions = state
        .sessions
        .lock()
        .map_err(|e| ApiError::Internal(format!("Failed to lock sessions: {}", e)))?;

    let session_list: Vec<SessionListItem> = sessions
        .all_sessions()
        .map(|s| {
            let status = match &s.status {
                crate::proxy::sessions::SessionStatus::Active => "active",
                crate::proxy::sessions::SessionStatus::Idle { .. } => "idle",
                crate::proxy::sessions::SessionStatus::Ended { .. } => "ended",
            };

            SessionListItem {
                key: s.key.to_string(),
                user_id: s.user_id.to_string(),
                claude_session_id: s.claude_session_id.clone(),
                transcript_path: s.transcript_path.clone(),
                source: s.source.to_string(),
                started: s.started,
                status: status.to_string(),
                event_count: s.events.len(),
                stats: SessionStatsSummary {
                    requests: s.stats.total_requests,
                    tool_calls: s.stats.total_tool_calls,
                    input_tokens: s.stats.total_input_tokens,
                    output_tokens: s.stats.total_output_tokens,
                    cost_usd: s.stats.total_cost(),
                },
            }
        })
        .collect();

    Ok(Json(SessionListResponse {
        active_count: sessions.active_count(),
        sessions: session_list,
    }))
}

// ============================================================================
// Session Todos Endpoint
// ============================================================================

/// Response for GET /api/session/:user_id/todos
#[derive(Debug, Serialize)]
pub struct SessionTodosResponse {
    /// User ID this belongs to
    pub user_id: String,
    /// When todos were last updated (None if never)
    pub updated: Option<String>,
    /// Number of todos
    pub count: usize,
    /// Breakdown by status
    pub summary: TodoSummary,
    /// The actual todo items
    pub todos: Vec<TodoItemResponse>,
}

#[derive(Debug, Serialize)]
pub struct TodoSummary {
    pub pending: usize,
    pub in_progress: usize,
    pub completed: usize,
}

#[derive(Debug, Serialize)]
pub struct TodoItemResponse {
    pub content: String,
    pub status: String,
    #[serde(rename = "activeForm")]
    pub active_form: String,
}

impl From<&TodoItem> for TodoItemResponse {
    fn from(item: &TodoItem) -> Self {
        Self {
            content: item.content.clone(),
            status: item.status.to_string(),
            active_form: item.active_form.clone(),
        }
    }
}

/// GET /api/session/:user_id/todos - Get tracked todos for a user session
///
/// Returns the current todo list state intercepted from TodoWrite tool calls.
/// This is useful for:
/// - Understanding what Claude is currently working on
/// - Context recovery after compaction (todos are searchable keywords!)
/// - Session visualization
pub async fn get_session_todos(
    State(state): State<crate::proxy::ProxyState>,
    Path(user_id): Path<String>,
) -> Result<Json<SessionTodosResponse>, ApiError> {
    let sessions = state
        .sessions
        .lock()
        .map_err(|e| ApiError::Internal(format!("Failed to lock sessions: {}", e)))?;

    let user = UserId::new(&user_id);
    let session = sessions
        .get_user_session(&user)
        .ok_or_else(|| ApiError::NotFound(format!("No active session for user: {}", user_id)))?;

    // Build summary
    let mut pending = 0;
    let mut in_progress = 0;
    let mut completed = 0;

    for todo in &session.todos {
        match todo.status {
            TodoStatus::Pending => pending += 1,
            TodoStatus::InProgress => in_progress += 1,
            TodoStatus::Completed => completed += 1,
        }
    }

    let response = SessionTodosResponse {
        user_id: user_id.clone(),
        updated: session.todos_updated.map(|dt| dt.to_rfc3339()),
        count: session.todos.len(),
        summary: TodoSummary {
            pending,
            in_progress,
            completed,
        },
        todos: session.todos.iter().map(TodoItemResponse::from).collect(),
    };

    Ok(Json(response))
}

// ============================================================================
// Session History Item (re-exported for whoami module)
// ============================================================================

/// Session info for history listing
#[derive(Debug, Serialize)]
pub struct SessionHistoryItem {
    /// Session key
    pub session_id: String,
    /// User ID
    pub user_id: String,
    /// Claude Code's session ID (if from hook)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_session_id: Option<String>,
    /// When session started
    pub started: String,
    /// When session ended (if ended)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended: Option<String>,
    /// How session was detected
    pub source: String,
    /// Why session ended (if ended)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_reason: Option<String>,
    /// Transcript path (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    /// Session statistics summary
    pub stats: SessionStatsSummary,
}
