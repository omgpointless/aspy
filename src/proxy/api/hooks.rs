// Hook endpoints - Lifecycle hooks from Claude Code

use super::ApiError;
use crate::events::ProxyEvent;
use crate::proxy::sessions::{SessionSource, UserId};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

/// Request body for POST /api/hook/precompact
#[derive(Debug, Deserialize)]
pub struct PreCompactHookRequest {
    /// User's API key hash (first 16 chars of SHA-256)
    pub user_id: String,
    /// Trigger type: "manual" or "auto"
    #[serde(default = "default_trigger")]
    pub trigger: String,
}

fn default_trigger() -> String {
    "manual".to_string()
}

/// Response for hook operations
#[derive(Debug, Serialize)]
pub struct HookActionResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
}

/// POST /api/hook/precompact - Notify Aspy of an impending compact
///
/// Called by the PreCompact hook when Claude Code is about to compact.
/// Creates a PreCompactHook event in the user's session for timeline tracking.
pub async fn hook_precompact(
    State(state): State<crate::proxy::ProxyState>,
    Json(request): Json<PreCompactHookRequest>,
) -> Result<Json<HookActionResponse>, ApiError> {
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|e| ApiError::Internal(format!("Failed to lock sessions: {}", e)))?;

    let user_id = UserId::new(&request.user_id);

    // Create the PreCompactHook event
    let event = ProxyEvent::PreCompactHook {
        timestamp: chrono::Utc::now(),
        trigger: request.trigger.clone(),
    };

    // Try to get existing session, or create one if needed
    let session_key = if let Some(session) = sessions.get_user_session_mut(&user_id) {
        session.events.push_back(event);
        session.key.to_string()
    } else {
        // No session exists - create one via hook source
        let session = sessions.start_session(user_id.clone(), None, SessionSource::Hook, None);
        let key = session.key.to_string();
        // Need to get mutable ref again after start_session
        if let Some(s) = sessions.get_user_session_mut(&user_id) {
            s.events.push_back(event);
        }
        key
    };

    tracing::info!(
        user_id = %user_id.short(),
        trigger = %request.trigger,
        "PreCompact hook received"
    );

    Ok(Json(HookActionResponse {
        success: true,
        message: format!("PreCompact hook recorded (trigger: {})", request.trigger),
        session_key: Some(session_key),
    }))
}
