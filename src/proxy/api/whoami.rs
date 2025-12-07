// Whoami and session history endpoints

use super::sessions::{SessionHistoryItem, SessionStatsSummary};
use super::ApiError;
use crate::proxy::sessions::UserId;
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ============================================================================
// Whoami Endpoint
// ============================================================================

/// Extract user ID (api_key_hash) from request headers
///
/// Same logic as proxy's extract_user_id - hashes API key or OAuth token
fn extract_user_id_from_headers(headers: &HeaderMap) -> Option<String> {
    let key_to_hash = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .filter(|s| s.starts_with("Bearer "))
                .map(|s| s[7..].to_string())
        });

    key_to_hash.map(|key| {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let hash = hasher.finalize();
        format!("{:x}", hash)[..16].to_string()
    })
}

/// Response for GET /api/whoami
#[derive(Debug, Serialize)]
pub struct WhoamiResponse {
    /// User ID (first 16 chars of SHA-256 hash of API key, or client_id if using URL routing)
    pub user_id: String,
    /// Current session key (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Claude Code's session ID (if from hook)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_session_id: Option<String>,
    /// When the current session started
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_started: Option<String>,
    /// How the session was detected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_source: Option<String>,
    /// Current session status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_status: Option<String>,
    /// Path to Claude Code's transcript file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
}

/// Query parameters for GET /api/whoami
#[derive(Debug, Deserialize)]
pub struct WhoamiQuery {
    /// User identifier (client_id like "foundry" or api_key_hash)
    /// If provided, overrides header-based identification
    pub user: Option<String>,
}

/// GET /api/whoami - Returns the current user's identity and session info
///
/// Identifies user via (in priority order):
/// 1. `?user=` query param (supports ASPY_CLIENT_ID / URL path routing)
/// 2. x-api-key header (hashed)
/// 3. Authorization: Bearer header (hashed)
///
/// Query params:
///   - user: User identifier (client_id or api_key_hash)
pub async fn get_whoami(
    State(state): State<crate::proxy::ProxyState>,
    headers: HeaderMap,
    Query(params): Query<WhoamiQuery>,
) -> Result<Json<WhoamiResponse>, ApiError> {
    // Priority: query param > header extraction
    let user_id = params
        .user
        .or_else(|| extract_user_id_from_headers(&headers))
        .ok_or_else(|| {
            ApiError::BadRequest(
                "Cannot determine user identity. Provide ?user= param or x-api-key/Authorization header."
                    .to_string(),
            )
        })?;

    let sessions = state
        .sessions
        .lock()
        .map_err(|e| ApiError::Internal(format!("Failed to lock sessions: {}", e)))?;

    let user = UserId::new(&user_id);

    // Check if user has an active session
    if let Some(session) = sessions.get_user_session(&user) {
        let status = match &session.status {
            crate::proxy::sessions::SessionStatus::Active => "active",
            crate::proxy::sessions::SessionStatus::Idle { .. } => "idle",
            crate::proxy::sessions::SessionStatus::Ended { .. } => "ended",
        };

        Ok(Json(WhoamiResponse {
            user_id,
            session_id: Some(session.key.to_string()),
            claude_session_id: session.claude_session_id.clone(),
            session_started: Some(session.started.to_rfc3339()),
            session_source: Some(session.source.to_string()),
            session_status: Some(status.to_string()),
            transcript_path: session.transcript_path.clone(),
        }))
    } else {
        // No active session, just return user identity
        Ok(Json(WhoamiResponse {
            user_id,
            session_id: None,
            claude_session_id: None,
            session_started: None,
            session_source: None,
            session_status: None,
            transcript_path: None,
        }))
    }
}

// ============================================================================
// Session History Endpoint
// ============================================================================

fn default_history_limit() -> usize {
    20
}

/// Query parameters for GET /api/session-history
#[derive(Debug, Deserialize)]
pub struct SessionHistoryQuery {
    /// User identifier (client_id like "foundry" or api_key_hash)
    /// If provided, overrides header-based identification
    pub user: Option<String>,
    /// Maximum sessions to return (default: 20, max: 100)
    #[serde(default = "default_history_limit")]
    pub limit: usize,
    /// Skip first N sessions (default: 0)
    #[serde(default)]
    pub offset: usize,
    /// Only sessions after this timestamp (ISO 8601)
    pub after: Option<String>,
    /// Only sessions before this timestamp (ISO 8601)
    pub before: Option<String>,
}

/// Response for GET /api/session-history
#[derive(Debug, Serialize)]
pub struct SessionHistoryResponse {
    /// User ID queried
    pub user_id: String,
    /// Total sessions returned
    pub count: usize,
    /// Whether there are more sessions available
    pub has_more: bool,
    /// Sessions (most recent first)
    pub sessions: Vec<SessionHistoryItem>,
}

/// GET /api/session-history - Returns session history for the current user
///
/// Returns a list of sessions the user has participated in, including
/// both in-memory history and persisted sessions from the database.
///
/// Identifies user via (in priority order):
/// 1. `?user=` query param (supports ASPY_CLIENT_ID / URL path routing)
/// 2. x-api-key header (hashed)
/// 3. Authorization: Bearer header (hashed)
///
/// Query params:
///   - user: User identifier (client_id or api_key_hash)
///   - limit: Max sessions to return (default: 20, max: 100)
///   - offset: Skip first N sessions (default: 0)
///   - after: Only sessions after this timestamp (ISO 8601)
///   - before: Only sessions before this timestamp (ISO 8601)
pub async fn get_session_history(
    State(state): State<crate::proxy::ProxyState>,
    headers: HeaderMap,
    Query(params): Query<SessionHistoryQuery>,
) -> Result<Json<SessionHistoryResponse>, ApiError> {
    // Priority: query param > header extraction
    let user_id = params
        .user
        .clone()
        .or_else(|| extract_user_id_from_headers(&headers))
        .ok_or_else(|| {
            ApiError::BadRequest(
                "Cannot determine user identity. Provide ?user= param or x-api-key/Authorization header."
                    .to_string(),
            )
        })?;

    let limit = params.limit.min(100);

    // Parse time filters
    let after: Option<DateTime<Utc>> = params.after.as_ref().and_then(|s| s.parse().ok());
    let before: Option<DateTime<Utc>> = params.before.as_ref().and_then(|s| s.parse().ok());

    let mut all_sessions: Vec<SessionHistoryItem> = Vec::new();

    // First, get sessions from in-memory history (recently ended)
    {
        let sessions = state
            .sessions
            .lock()
            .map_err(|e| ApiError::Internal(format!("Failed to lock sessions: {}", e)))?;

        // Include current active session if user has one
        let user = UserId::new(&user_id);
        if let Some(session) = sessions.get_user_session(&user) {
            let (ended, end_reason) = match &session.status {
                crate::proxy::sessions::SessionStatus::Ended { reason, ended } => {
                    (Some(ended.to_rfc3339()), Some(reason.to_string()))
                }
                _ => (None, None),
            };

            all_sessions.push(SessionHistoryItem {
                session_id: session.key.to_string(),
                user_id: session.user_id.to_string(),
                claude_session_id: session.claude_session_id.clone(),
                started: session.started.to_rfc3339(),
                ended,
                source: session.source.to_string(),
                end_reason,
                transcript_path: session.transcript_path.clone(),
                stats: SessionStatsSummary {
                    requests: session.stats.total_requests,
                    tool_calls: session.stats.total_tool_calls,
                    input_tokens: session.stats.total_input_tokens,
                    output_tokens: session.stats.total_output_tokens,
                    cost_usd: session.stats.total_cost(),
                },
            });
        }

        // Add ended sessions from history that belong to this user
        for session in sessions.session_history() {
            if session.user_id.0 != user_id {
                continue;
            }

            let (ended, end_reason) = match &session.status {
                crate::proxy::sessions::SessionStatus::Ended { reason, ended } => {
                    (Some(ended.to_rfc3339()), Some(reason.to_string()))
                }
                _ => (None, None),
            };

            all_sessions.push(SessionHistoryItem {
                session_id: session.key.to_string(),
                user_id: session.user_id.to_string(),
                claude_session_id: session.claude_session_id.clone(),
                started: session.started.to_rfc3339(),
                ended,
                source: session.source.to_string(),
                end_reason,
                transcript_path: session.transcript_path.clone(),
                stats: SessionStatsSummary {
                    requests: session.stats.total_requests,
                    tool_calls: session.stats.total_tool_calls,
                    input_tokens: session.stats.total_input_tokens,
                    output_tokens: session.stats.total_output_tokens,
                    cost_usd: session.stats.total_cost(),
                },
            });
        }
    }

    // Then, query cortex DB for historical sessions (if available)
    if let Some(ref query_interface) = state.cortex_query {
        if let Ok(db_sessions) = query_interface.get_user_sessions(&user_id, 100, 0) {
            // Merge DB sessions, avoiding duplicates
            let existing_ids: std::collections::HashSet<_> =
                all_sessions.iter().map(|s| s.session_id.clone()).collect();

            for db_session in db_sessions {
                if !existing_ids.contains(&db_session.session_id) {
                    all_sessions.push(db_session);
                }
            }
        }
    }

    // Sort by start time (most recent first)
    all_sessions.sort_by(|a, b| b.started.cmp(&a.started));

    // Apply time filters
    if let Some(after_time) = after {
        all_sessions.retain(|s| {
            s.started
                .parse::<DateTime<Utc>>()
                .map(|t| t > after_time)
                .unwrap_or(true)
        });
    }
    if let Some(before_time) = before {
        all_sessions.retain(|s| {
            s.started
                .parse::<DateTime<Utc>>()
                .map(|t| t < before_time)
                .unwrap_or(true)
        });
    }

    // Apply offset and limit
    let total = all_sessions.len();
    let sessions: Vec<_> = all_sessions
        .into_iter()
        .skip(params.offset)
        .take(limit)
        .collect();

    let has_more = params.offset + sessions.len() < total;

    Ok(Json(SessionHistoryResponse {
        user_id,
        count: sessions.len(),
        has_more,
        sessions,
    }))
}
