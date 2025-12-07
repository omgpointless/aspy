// Context endpoints - Returns context window status and snapshot

use super::ApiError;
use crate::proxy::sessions::UserId;
use axum::{extract::Query, extract::State, Json};
use serde::{Deserialize, Serialize};

/// Context window status response
#[derive(Debug, Serialize)]
pub struct ContextResponse {
    /// Current context tokens (input + cache_creation + cache_read from last API call)
    pub current_tokens: u64,
    /// Configured context limit
    pub limit_tokens: u64,
    /// Usage percentage (0-100)
    pub usage_pct: f64,
    /// Warning level based on usage
    pub warning_level: ContextWarningLevel,
    /// Number of context compacts detected this session
    pub compacts: usize,
    /// Breakdown of token sources
    pub breakdown: ContextBreakdown,
}

#[derive(Debug, Serialize)]
pub struct ContextBreakdown {
    /// Fresh input tokens (input + cache_creation, i.e., not read from existing cache)
    pub input: u64,
    /// Cached tokens read from prompt cache
    pub cached: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextWarningLevel {
    /// Under 70% - safe
    Normal,
    /// 70-85% - approaching limit
    Warning,
    /// 85-95% - high risk
    High,
    /// Over 95% - imminent compact
    Critical,
}

impl ContextWarningLevel {
    fn from_percentage(pct: f64) -> Self {
        match pct {
            p if p >= 95.0 => Self::Critical,
            p if p >= 85.0 => Self::High,
            p if p >= 70.0 => Self::Warning,
            _ => Self::Normal,
        }
    }
}

/// Query parameters for /api/context endpoint
#[derive(Debug, Deserialize)]
pub struct ContextQuery {
    /// Filter to specific user (api_key_hash, e.g., "b0acf41e12907b7b")
    pub user: Option<String>,
}

/// GET /api/context - Returns context window status for a specific user session
///
/// Query params:
///   - user: REQUIRED - User's session context (api_key_hash, e.g., "b0acf41e12907b7b")
///
/// Context is inherently per-session, so user filter is required.
pub async fn get_context(
    State(state): State<crate::proxy::ProxyState>,
    Query(params): Query<ContextQuery>,
) -> Result<Json<ContextResponse>, ApiError> {
    // User filter is required - context is per-session, not global
    let user_hash = params.user.ok_or_else(|| {
        ApiError::BadRequest(
            "Context is per-session. Please provide ?user=<api_key_hash> parameter. \
             Use /api/sessions to list active user sessions and their IDs."
                .to_string(),
        )
    })?;

    let sessions = state
        .sessions
        .lock()
        .map_err(|e| ApiError::Internal(format!("Failed to lock sessions: {}", e)))?;

    let user_id = UserId::new(&user_hash);
    let session = sessions
        .get_user_session(&user_id)
        .ok_or_else(|| ApiError::NotFound(format!("No active session for user: {}", user_hash)))?;

    // Get context from session's ContextState
    let ctx = &session.context;
    let usage_pct = ctx.percentage();

    let response = ContextResponse {
        current_tokens: ctx.current_tokens,
        limit_tokens: ctx.limit,
        usage_pct,
        warning_level: ContextWarningLevel::from_percentage(usage_pct),
        compacts: session.stats.compact_count,
        breakdown: ContextBreakdown {
            input: ctx.input_tokens(),
            cached: ctx.last_cached,
        },
    };

    Ok(Json(response))
}

// ============================================================================
// Context Snapshot Endpoint
// ============================================================================

/// Context snapshot response - detailed breakdown of what's in context
#[derive(Debug, Serialize)]
pub struct ContextSnapshotResponse {
    /// User ID this belongs to
    pub user_id: String,
    /// Whether snapshot data is available
    pub available: bool,
    /// Message count in context
    pub message_count: u32,
    /// Breakdown of content types (chars, roughly ~4 chars per token)
    pub breakdown: ContextSnapshotBreakdown,
    /// Human-readable summary
    pub summary: String,
}

#[derive(Debug, Serialize)]
pub struct ContextSnapshotBreakdown {
    /// Tool results (output from tools like Read, Glob, etc.)
    pub tool_results: ContentBreakdown,
    /// Tool inputs (input JSON for tool calls)
    pub tool_inputs: ContentBreakdown,
    /// Thinking blocks (Claude's reasoning)
    pub thinking: ContentBreakdown,
    /// Text content (conversation text)
    pub text: ContentBreakdown,
    /// System prompt
    pub system: ContentBreakdown,
}

#[derive(Debug, Serialize)]
pub struct ContentBreakdown {
    /// Number of items (blocks/messages)
    pub count: u32,
    /// Total characters
    pub chars: u64,
    /// Estimated tokens (~4 chars per token)
    pub estimated_tokens: u64,
    /// Percentage of total chars
    pub pct: f64,
}

/// GET /api/context/snapshot - Returns detailed context breakdown for a user session
///
/// Query params:
///   - user: REQUIRED - User's session context (api_key_hash)
///
/// This endpoint answers "Why is my context so high?" by breaking down
/// context content into categories: tool_results, thinking, text, etc.
pub async fn get_context_snapshot(
    State(state): State<crate::proxy::ProxyState>,
    Query(params): Query<ContextQuery>,
) -> Result<Json<ContextSnapshotResponse>, ApiError> {
    // User filter is required
    let user_hash = params.user.ok_or_else(|| {
        ApiError::BadRequest(
            "Context snapshot is per-session. Please provide ?user=<api_key_hash> parameter."
                .to_string(),
        )
    })?;

    // Get snapshot from parser
    let snapshot = state.parser.get_context_snapshot(&user_hash).await;

    if let Some(snap) = snapshot {
        let total_chars = snap.tool_result_chars
            + snap.tool_use_chars
            + snap.thinking_chars
            + snap.text_chars
            + snap.system_chars;

        let pct = |chars: u64| {
            if total_chars > 0 {
                (chars as f64 / total_chars as f64) * 100.0
            } else {
                0.0
            }
        };

        // Build human-readable summary (top 3 categories)
        let mut categories: Vec<(&str, u64)> = vec![
            ("tool_results", snap.tool_result_chars),
            ("tool_inputs", snap.tool_use_chars),
            ("thinking", snap.thinking_chars),
            ("text", snap.text_chars),
            ("system", snap.system_chars),
        ];
        categories.sort_by(|a, b| b.1.cmp(&a.1));

        let summary_parts: Vec<String> = categories
            .iter()
            .filter(|(_, chars)| *chars > 0)
            .take(3)
            .map(|(name, chars)| format!("{}: ~{}K tokens", name, chars / 4000))
            .collect();

        let summary = if summary_parts.is_empty() {
            "No content tracked yet".to_string()
        } else {
            summary_parts.join(", ")
        };

        Ok(Json(ContextSnapshotResponse {
            user_id: user_hash,
            available: true,
            message_count: snap.message_count,
            breakdown: ContextSnapshotBreakdown {
                tool_results: ContentBreakdown {
                    count: snap.tool_result_count,
                    chars: snap.tool_result_chars,
                    estimated_tokens: snap.tool_result_chars / 4,
                    pct: pct(snap.tool_result_chars),
                },
                tool_inputs: ContentBreakdown {
                    count: snap.tool_use_count,
                    chars: snap.tool_use_chars,
                    estimated_tokens: snap.tool_use_chars / 4,
                    pct: pct(snap.tool_use_chars),
                },
                thinking: ContentBreakdown {
                    count: 0, // Not tracked at block level yet
                    chars: snap.thinking_chars,
                    estimated_tokens: snap.thinking_chars / 4,
                    pct: pct(snap.thinking_chars),
                },
                text: ContentBreakdown {
                    count: 0, // Not tracked at block level yet
                    chars: snap.text_chars,
                    estimated_tokens: snap.text_chars / 4,
                    pct: pct(snap.text_chars),
                },
                system: ContentBreakdown {
                    count: if snap.system_chars > 0 { 1 } else { 0 },
                    chars: snap.system_chars,
                    estimated_tokens: snap.system_chars / 4,
                    pct: pct(snap.system_chars),
                },
            },
            summary,
        }))
    } else {
        Ok(Json(ContextSnapshotResponse {
            user_id: user_hash,
            available: false,
            message_count: 0,
            breakdown: ContextSnapshotBreakdown {
                tool_results: ContentBreakdown {
                    count: 0,
                    chars: 0,
                    estimated_tokens: 0,
                    pct: 0.0,
                },
                tool_inputs: ContentBreakdown {
                    count: 0,
                    chars: 0,
                    estimated_tokens: 0,
                    pct: 0.0,
                },
                thinking: ContentBreakdown {
                    count: 0,
                    chars: 0,
                    estimated_tokens: 0,
                    pct: 0.0,
                },
                text: ContentBreakdown {
                    count: 0,
                    chars: 0,
                    estimated_tokens: 0,
                    pct: 0.0,
                },
                system: ContentBreakdown {
                    count: 0,
                    chars: 0,
                    estimated_tokens: 0,
                    pct: 0.0,
                },
            },
            summary: "No snapshot available - session may be new".to_string(),
        }))
    }
}
