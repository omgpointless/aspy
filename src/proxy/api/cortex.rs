// Cortex endpoints - Health, search, stats, and context recovery

use super::ApiError;
use crate::pipeline::cortex_query::{
    ContextMatch, LifetimeStats, PromptMatch, ResponseMatch, SearchMode, ThinkingMatch, TodoMatch,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

// ============================================================================
// Default values
// ============================================================================

fn default_search_limit() -> usize {
    10
}

fn default_context_limit() -> usize {
    10
}

fn default_limit() -> usize {
    50
}

// ============================================================================
// Health and Cleanup
// ============================================================================

/// Response for cortex health endpoint
#[derive(Debug, Serialize)]
pub struct CortexHealthResponse {
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_available: Option<bool>,
}

/// GET /api/cortex/health - Get cortex processor status
///
/// Returns status of the cortex storage and query system
pub async fn cortex_health(State(state): State<crate::proxy::ProxyState>) -> impl IntoResponse {
    let has_pipeline = state.pipeline.is_some();
    let has_query = state.cortex_query.is_some();

    if has_pipeline && has_query {
        Json(CortexHealthResponse {
            status: "healthy".to_string(),
            message: "Cortex storage and query interface operational".to_string(),
            query_available: Some(true),
        })
    } else if has_pipeline {
        Json(CortexHealthResponse {
            status: "degraded".to_string(),
            message: "Storage operational but query interface unavailable".to_string(),
            query_available: Some(false),
        })
    } else {
        Json(CortexHealthResponse {
            status: "disabled".to_string(),
            message: "Cortex not configured".to_string(),
            query_available: None,
        })
    }
}

/// POST /api/cortex/cleanup - Trigger manual retention cleanup
///
/// Deletes old events based on retention policy. Returns number of records deleted.
/// Note: Automatic cleanup runs every 24 hours in the background.
pub async fn cortex_cleanup(
    State(_state): State<crate::proxy::ProxyState>,
) -> Result<impl IntoResponse, StatusCode> {
    // Phase 2 TODO: Implement manual cleanup trigger
    // This requires exposing a method on CortexProcessor to trigger cleanup
    // For now, rely on automatic 24h cleanup
    Ok(Json(serde_json::json!({
        "status": "not_implemented",
        "message": "Manual cleanup not yet implemented (automatic cleanup runs every 24h)",
        "deleted": 0
    })))
}

// ============================================================================
// Search Endpoints
// ============================================================================

/// Query parameters for cortex search endpoints
#[derive(Debug, Deserialize)]
pub struct CortexSearchQuery {
    /// Search query string
    #[serde(rename = "q")]
    pub query: String,
    /// Maximum results (default: 10, max: 100)
    #[serde(default = "default_search_limit")]
    pub limit: usize,
    /// Search mode: "phrase" (default), "natural", "raw"
    #[serde(default)]
    pub mode: SearchMode,
}

/// Response wrapper for thinking search
#[derive(Debug, Serialize)]
pub struct ThinkingSearchResponse {
    pub query: String,
    pub mode: String,
    pub results: Vec<ThinkingMatch>,
}

/// Response wrapper for prompt search
#[derive(Debug, Serialize)]
pub struct PromptSearchResponse {
    pub query: String,
    pub mode: String,
    pub results: Vec<PromptMatch>,
}

/// Response wrapper for response search
#[derive(Debug, Serialize)]
pub struct ResponseSearchResponse {
    pub query: String,
    pub mode: String,
    pub results: Vec<ResponseMatch>,
}

/// Response wrapper for context recovery
#[derive(Debug, Serialize)]
pub struct ContextSearchResponse {
    pub topic: String,
    pub mode: String,
    pub results: Vec<ContextMatch>,
}

/// Response wrapper for todo history search
#[derive(Debug, Serialize)]
pub struct TodoSearchResponse {
    pub query: Option<String>,
    pub timeframe: Option<String>,
    pub results: Vec<TodoMatch>,
}

/// GET /api/cortex/search/thinking - Search thinking blocks
///
/// Query params:
///   - q: Search query (required)
///   - limit: Max results (default: 10, max: 100)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn cortex_search_thinking(
    State(state): State<crate::proxy::ProxyState>,
    Query(params): Query<CortexSearchQuery>,
) -> Result<Json<ThinkingSearchResponse>, ApiError> {
    let query_interface = state
        .cortex_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Cortex query interface not available".to_string()))?;

    let limit = params.limit.min(100);
    let results = query_interface
        .search_thinking(&params.query, limit, params.mode)
        .map_err(|e| ApiError::Internal(format!("Search failed: {}", e)))?;

    Ok(Json(ThinkingSearchResponse {
        query: params.query,
        mode: format!("{:?}", params.mode),
        results,
    }))
}

/// GET /api/cortex/search/prompts - Search user prompts
///
/// Query params:
///   - q: Search query (required)
///   - limit: Max results (default: 10, max: 100)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn cortex_search_prompts(
    State(state): State<crate::proxy::ProxyState>,
    Query(params): Query<CortexSearchQuery>,
) -> Result<Json<PromptSearchResponse>, ApiError> {
    let query_interface = state
        .cortex_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Cortex query interface not available".to_string()))?;

    let limit = params.limit.min(100);
    let results = query_interface
        .search_prompts(&params.query, limit, params.mode)
        .map_err(|e| ApiError::Internal(format!("Search failed: {}", e)))?;

    Ok(Json(PromptSearchResponse {
        query: params.query,
        mode: format!("{:?}", params.mode),
        results,
    }))
}

/// GET /api/cortex/search/responses - Search assistant responses
///
/// Query params:
///   - q: Search query (required)
///   - limit: Max results (default: 10, max: 100)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn cortex_search_responses(
    State(state): State<crate::proxy::ProxyState>,
    Query(params): Query<CortexSearchQuery>,
) -> Result<Json<ResponseSearchResponse>, ApiError> {
    let query_interface = state
        .cortex_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Cortex query interface not available".to_string()))?;

    let limit = params.limit.min(100);
    let results = query_interface
        .search_responses(&params.query, limit, params.mode)
        .map_err(|e| ApiError::Internal(format!("Search failed: {}", e)))?;

    Ok(Json(ResponseSearchResponse {
        query: params.query,
        mode: format!("{:?}", params.mode),
        results,
    }))
}

// ============================================================================
// Todo History Endpoint
// ============================================================================

/// Query parameters for todo history endpoint
#[derive(Debug, Deserialize)]
pub struct TodoHistoryQuery {
    /// Optional search query (searches todo content)
    #[serde(rename = "q")]
    pub query: Option<String>,
    /// Maximum results (default: 10, max: 100)
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Days to look back (default: all time)
    pub days: Option<u32>,
    /// Search mode: "phrase" (default), "natural", "raw"
    #[serde(default)]
    pub mode: SearchMode,
}

/// GET /api/cortex/todos - Search or list todo history
///
/// Query params:
///   - q: Optional search query (searches todo content)
///   - limit: Max results (default: 10, max: 100)
///   - days: Optional days to look back
///   - mode: phrase|natural|raw (default: phrase)
pub async fn cortex_todos(
    State(state): State<crate::proxy::ProxyState>,
    Query(params): Query<TodoHistoryQuery>,
) -> Result<Json<TodoSearchResponse>, ApiError> {
    let query_interface = state
        .cortex_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Cortex query interface not available".to_string()))?;

    let limit = params.limit.min(100);

    let results = if let Some(ref q) = params.query {
        // Search mode: use FTS
        query_interface
            .search_todos(q, limit, params.mode)
            .map_err(|e| ApiError::Internal(format!("Todo search failed: {}", e)))?
    } else {
        // List mode: get recent todos
        query_interface
            .get_recent_todos(limit, params.days)
            .map_err(|e| ApiError::Internal(format!("Todo list failed: {}", e)))?
    };

    Ok(Json(TodoSearchResponse {
        query: params.query,
        timeframe: params.days.map(|d| format!("{} days", d)),
        results,
    }))
}

// ============================================================================
// Context Recovery Endpoint
// ============================================================================

/// Query parameters for context recovery endpoint
#[derive(Debug, Deserialize)]
pub struct CortexContextQuery {
    /// Topic to search for
    pub topic: String,
    /// Maximum results (default: 10, max: 50)
    #[serde(default = "default_context_limit")]
    pub limit: usize,
    /// Search mode: "phrase" (default), "natural", "raw"
    #[serde(default)]
    pub mode: SearchMode,
}

/// GET /api/cortex/context - Combined context recovery
///
/// Searches across thinking blocks, user prompts, and assistant responses,
/// then returns combined results sorted by relevance.
///
/// Query params:
///   - topic: Topic to search for (required)
///   - limit: Max results (default: 10, max: 50)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn cortex_context(
    State(state): State<crate::proxy::ProxyState>,
    Query(params): Query<CortexContextQuery>,
) -> Result<Json<ContextSearchResponse>, ApiError> {
    let query_interface = state
        .cortex_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Cortex query interface not available".to_string()))?;

    let limit = params.limit.min(50);
    let results = query_interface
        .recover_context(&params.topic, limit, params.mode)
        .map_err(|e| ApiError::Internal(format!("Context recovery failed: {}", e)))?;

    Ok(Json(ContextSearchResponse {
        topic: params.topic,
        mode: format!("{:?}", params.mode),
        results,
    }))
}

/// GET /api/cortex/stats - Get lifetime statistics
///
/// Returns aggregated statistics across all sessions: tokens, costs, tool usage, etc.
pub async fn cortex_stats(
    State(state): State<crate::proxy::ProxyState>,
) -> Result<Json<LifetimeStats>, ApiError> {
    let query_interface = state
        .cortex_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Cortex query interface not available".to_string()))?;

    let stats = query_interface
        .get_lifetime_stats()
        .map_err(|e| ApiError::Internal(format!("Failed to get lifetime stats: {}", e)))?;

    Ok(Json(stats))
}

// ============================================================================
// User-Scoped Cortex Endpoints (Cross-Session Context Recovery)
// ============================================================================

/// GET /api/cortex/search/user/:user_id/thinking - Search thinking blocks for a specific user
///
/// Query params:
///   - q: Search query (required)
///   - limit: Max results (default: 10, max: 100)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn cortex_search_user_thinking(
    State(state): State<crate::proxy::ProxyState>,
    Path(user_id): Path<String>,
    Query(params): Query<CortexSearchQuery>,
) -> Result<Json<ThinkingSearchResponse>, ApiError> {
    let query_interface = state
        .cortex_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Cortex query interface not available".to_string()))?;

    let limit = params.limit.min(100);
    let results = query_interface
        .search_user_thinking(&user_id, &params.query, limit, params.mode)
        .map_err(|e| ApiError::Internal(format!("Search failed: {}", e)))?;

    Ok(Json(ThinkingSearchResponse {
        query: params.query,
        mode: format!("{:?}", params.mode),
        results,
    }))
}

/// GET /api/cortex/search/user/:user_id/prompts - Search user prompts for a specific user
///
/// Query params:
///   - q: Search query (required)
///   - limit: Max results (default: 10, max: 100)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn cortex_search_user_prompts(
    State(state): State<crate::proxy::ProxyState>,
    Path(user_id): Path<String>,
    Query(params): Query<CortexSearchQuery>,
) -> Result<Json<PromptSearchResponse>, ApiError> {
    let query_interface = state
        .cortex_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Cortex query interface not available".to_string()))?;

    let limit = params.limit.min(100);
    let results = query_interface
        .search_user_prompts(&user_id, &params.query, limit, params.mode)
        .map_err(|e| ApiError::Internal(format!("Search failed: {}", e)))?;

    Ok(Json(PromptSearchResponse {
        query: params.query,
        mode: format!("{:?}", params.mode),
        results,
    }))
}

/// GET /api/cortex/search/user/:user_id/responses - Search assistant responses for a specific user
///
/// Query params:
///   - q: Search query (required)
///   - limit: Max results (default: 10, max: 100)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn cortex_search_user_responses(
    State(state): State<crate::proxy::ProxyState>,
    Path(user_id): Path<String>,
    Query(params): Query<CortexSearchQuery>,
) -> Result<Json<ResponseSearchResponse>, ApiError> {
    let query_interface = state
        .cortex_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Cortex query interface not available".to_string()))?;

    let limit = params.limit.min(100);
    let results = query_interface
        .search_user_responses(&user_id, &params.query, limit, params.mode)
        .map_err(|e| ApiError::Internal(format!("Search failed: {}", e)))?;

    Ok(Json(ResponseSearchResponse {
        query: params.query,
        mode: format!("{:?}", params.mode),
        results,
    }))
}

/// GET /api/cortex/context/user/:user_id - Combined context recovery for a specific user
///
/// Searches across thinking blocks, user prompts, and assistant responses for a specific user,
/// then returns combined results sorted by relevance.
///
/// Query params:
///   - topic: Topic to search for (required)
///   - limit: Max results (default: 10, max: 50)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn cortex_context_user(
    State(state): State<crate::proxy::ProxyState>,
    Path(user_id): Path<String>,
    Query(params): Query<CortexContextQuery>,
) -> Result<Json<ContextSearchResponse>, ApiError> {
    let query_interface = state
        .cortex_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Cortex query interface not available".to_string()))?;

    let limit = params.limit.min(50);
    let results = query_interface
        .recover_user_context(&user_id, &params.topic, limit, params.mode)
        .map_err(|e| ApiError::Internal(format!("Context recovery failed: {}", e)))?;

    Ok(Json(ContextSearchResponse {
        topic: params.topic,
        mode: format!("{:?}", params.mode),
        results,
    }))
}

/// GET /api/cortex/stats/user/:user_id - Get lifetime statistics for a specific user
///
/// Returns aggregated statistics across all sessions belonging to the specified user.
pub async fn cortex_stats_user(
    State(state): State<crate::proxy::ProxyState>,
    Path(user_id): Path<String>,
) -> Result<Json<LifetimeStats>, ApiError> {
    let query_interface = state
        .cortex_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Cortex query interface not available".to_string()))?;

    let stats = query_interface
        .get_user_lifetime_stats(&user_id)
        .map_err(|e| ApiError::Internal(format!("Failed to get user lifetime stats: {}", e)))?;

    Ok(Json(stats))
}
