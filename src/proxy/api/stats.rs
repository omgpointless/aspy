// Stats endpoint - Returns session statistics

use super::ApiError;
use crate::events::Stats;
use crate::proxy::sessions::UserId;
use axum::{extract::Query, extract::State, Json};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Session statistics response
/// JSON structure returned by /api/stats endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStatsResponse {
    pub session: SessionInfo,
    pub tokens: TokenInfo,
    pub cost: CostInfo,
    pub requests: RequestInfo,
    pub tools: ToolInfo,
    pub thinking: ThinkingInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// When the session started (ISO 8601)
    pub started: Option<String>,
    /// Session duration in seconds
    pub duration_secs: u64,
    /// Total events captured this session
    pub events_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub input: u64,
    pub output: u64,
    pub cached: u64,
    /// Tokens written to cache (prompt caching)
    pub cache_created: u64,
    /// Cache hit percentage (0-100)
    pub cache_ratio_pct: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostInfo {
    /// Total cost in USD for this session
    pub total_usd: f64,
    /// Cost savings from cache hits
    pub savings_usd: f64,
    /// Cost breakdown by model name
    pub by_model: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestInfo {
    /// Total number of requests this session
    pub total: usize,
    pub failed: usize,
    /// Success rate as percentage (0-100)
    pub success_rate_pct: f64,
    /// Average time to first byte in milliseconds
    pub avg_ttfb_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    /// Total tool calls made
    pub total_calls: usize,
    pub failed_calls: usize,
    /// Tool calls by name: {"Read": 58, "Edit": 32, ...}
    pub by_tool: HashMap<String, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingInfo {
    /// Number of thinking blocks captured
    pub blocks: usize,
    pub total_tokens: u64,
}

/// Query parameters for /api/stats endpoint
#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    /// Filter to specific user (api_key_hash, e.g., "b0acf41e12907b7b")
    pub user: Option<String>,
}

/// GET /api/stats - Returns session statistics
///
/// Query params:
///   - user: Filter to specific user's session stats (api_key_hash)
pub async fn get_stats(
    State(state): State<crate::proxy::ProxyState>,
    Query(params): Query<StatsQuery>,
) -> Result<Json<SessionStatsResponse>, ApiError> {
    // If user filter provided, get stats from their session; otherwise use global
    let (stats, session_started, event_count) = if let Some(ref user_hash) = params.user {
        let sessions = state
            .sessions
            .lock()
            .map_err(|e| ApiError::Internal(format!("Failed to lock sessions: {}", e)))?;

        let user_id = UserId::new(user_hash);
        if let Some(session) = sessions.get_user_session(&user_id) {
            (
                session.stats.clone(),
                Some(session.started),
                session.events.len(),
            )
        } else {
            // User not found - return empty stats
            (Stats::default(), None, 0)
        }
    } else {
        // No filter - use global stats (no session start time for global)
        let stats = state
            .stats
            .lock()
            .map_err(|e| ApiError::Internal(format!("Failed to lock stats: {}", e)))?;
        let count = stats.total_requests;
        (stats.clone(), None, count)
    };

    // Calculate session duration
    let duration_secs = session_started
        .map(|start| (chrono::Utc::now() - start).num_seconds() as u64)
        .unwrap_or(0);

    // Build response
    let response = SessionStatsResponse {
        session: SessionInfo {
            started: session_started.map(|dt| dt.to_rfc3339()),
            duration_secs,
            events_count: event_count,
        },
        tokens: TokenInfo {
            input: stats.total_input_tokens,
            output: stats.total_output_tokens,
            cached: stats.total_cache_read_tokens,
            cache_created: stats.total_cache_creation_tokens,
            cache_ratio_pct: stats.cache_hit_rate() as u64,
        },
        cost: CostInfo {
            total_usd: stats.total_cost(),
            savings_usd: stats.cache_savings(),
            by_model: calculate_cost_by_model(&stats),
        },
        requests: RequestInfo {
            total: stats.total_requests,
            failed: stats.failed_requests,
            success_rate_pct: stats.success_rate(),
            avg_ttfb_ms: stats.avg_ttfb().as_millis() as u64,
        },
        tools: ToolInfo {
            total_calls: stats.total_tool_calls,
            failed_calls: stats.failed_tool_calls,
            by_tool: stats.tool_calls_by_name.clone(),
        },
        thinking: ThinkingInfo {
            blocks: stats.thinking_blocks,
            total_tokens: stats.thinking_tokens,
        },
    };

    Ok(Json(response))
}

/// Calculate cost breakdown by model
/// Returns a map of model name -> total cost in USD
fn calculate_cost_by_model(stats: &Stats) -> HashMap<String, f64> {
    stats
        .model_tokens
        .iter()
        .map(|(model, tokens)| {
            let cost = crate::pricing::calculate_cost(
                model,
                tokens.input as u32,
                tokens.output as u32,
                tokens.cache_creation as u32,
                tokens.cache_read as u32,
            );
            (model.clone(), cost)
        })
        .collect()
}
