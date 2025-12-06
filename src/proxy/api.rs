// HTTP API module - Exposes observability data via REST endpoints
//
// This module provides programmatic access to session state, enabling:
// - Slash commands that query proxy stats
// - MCP servers that expose data to Claude
// - External integrations (hooks, dashboards, alerts)
//
// All endpoints return JSON and are designed for local consumption only.
// Security: Binds to 127.0.0.1 by default (localhost only).

use crate::events::{ProxyEvent, Stats};
use crate::proxy::sessions::{EndReason, SessionKey, SessionManager, SessionSource, UserId};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// Shared statistics accessible to API endpoints
/// This allows both the TUI and HTTP API handlers to read session stats
pub type SharedStats = Arc<Mutex<Stats>>;

/// Shared events buffer accessible to API endpoints
/// Ring buffer that keeps the most recent N events for querying
pub type SharedEvents = Arc<Mutex<EventBuffer>>;

/// Shared session manager for multi-user session tracking
pub type SharedSessions = Arc<Mutex<SessionManager>>;

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
fn event_type_name(event: &ProxyEvent) -> &'static str {
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
    }
}

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

// ============================================================================
// Events Endpoint
// ============================================================================

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

// ============================================================================
// Context Endpoint
// ============================================================================

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

// ============================================================================
// Session Management Endpoints
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

/// POST /api/session/start - Register a new session (from SessionStart hook)
///
/// Called by the SessionStart hook when Claude Code starts.
/// Creates a new session, superseding any previous session for this user.
pub async fn session_start(
    State(state): State<crate::proxy::ProxyState>,
    Json(request): Json<SessionStartRequest>,
) -> Result<Json<SessionActionResponse>, ApiError> {
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|e| ApiError::Internal(format!("Failed to lock sessions: {}", e)))?;

    let user_id = UserId::new(&request.user_id);
    let source = match request.source.as_deref() {
        Some("hook") => SessionSource::Hook,
        Some("warmup") => SessionSource::Warmup,
        _ => SessionSource::Hook, // Default for explicit start
    };

    tracing::debug!(
        session_id = %request.session_id,
        user_id = %request.user_id,
        source = %source,
        "Starting session via hook for user {} with session {}",
        user_id.short(),
        request.session_id
    );

    let session = sessions.start_session(
        user_id,
        Some(request.session_id.clone()),
        source,
        request.transcript_path.clone(),
    );
    let session_key = session.key.to_string();

    tracing::info!(
        session_id = %request.session_id,
        source = %source,
        transcript_path = ?request.transcript_path,
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
// Session Todos Endpoint
// ============================================================================

use crate::proxy::sessions::{TodoItem, TodoStatus};

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
// Hook Endpoints
// ============================================================================

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

/// POST /api/hook/precompact - Notify Aspy of an impending compact
///
/// Called by the PreCompact hook when Claude Code is about to compact.
/// Creates a PreCompactHook event in the user's session for timeline tracking.
pub async fn hook_precompact(
    State(state): State<crate::proxy::ProxyState>,
    Json(request): Json<PreCompactHookRequest>,
) -> Result<Json<SessionActionResponse>, ApiError> {
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

    Ok(Json(SessionActionResponse {
        success: true,
        message: format!("PreCompact hook recorded (trigger: {})", request.trigger),
        session_key: Some(session_key),
    }))
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

// ============================================================================
// Log Search Endpoint
// ============================================================================

/// Request body for POST /api/search
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Required: keyword to search for (case-insensitive)
    pub keyword: String,
    /// Filter by message role: "user" or "assistant"
    pub role: Option<String>,
    /// Specific session filename filter (partial match)
    pub session: Option<String>,
    /// Max results (default: 10, max: 100)
    #[serde(default = "default_search_limit")]
    pub limit: usize,
    /// Time range filter: "today", "before_today", "last_3_days", "last_7_days", "last_30_days"
    pub time_range: Option<String>,
}

fn default_search_limit() -> usize {
    10
}

/// Parse time_range string into (after, before) DateTime bounds
fn parse_time_range(time_range: &str) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    use chrono::{Duration, Timelike};

    let now = Utc::now();
    // Start of today (midnight UTC)
    let today_start = now
        .with_hour(0)
        .unwrap()
        .with_minute(0)
        .unwrap()
        .with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap();

    match time_range.to_lowercase().as_str() {
        "today" => (Some(today_start), None),
        "before_today" => (None, Some(today_start)),
        "last_3_days" => (Some(today_start - Duration::days(3)), None),
        "last_7_days" => (Some(today_start - Duration::days(7)), None),
        "last_30_days" => (Some(today_start - Duration::days(30)), None),
        _ => (None, None), // Unknown range, no filtering
    }
}

/// A single search result
#[derive(Debug, Serialize)]
pub struct SearchResult {
    /// Session filename
    pub session: String,
    /// Message timestamp
    pub timestamp: String,
    /// Role: "user" or "assistant"
    pub role: String,
    /// The matching text snippet (truncated around match)
    pub text: String,
}

/// Response for POST /api/search
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    /// The search query
    pub query: String,
    /// Number of session files searched
    pub sessions_searched: usize,
    /// Total matches found
    pub total_matches: usize,
    /// The results (most recent first)
    pub results: Vec<SearchResult>,
}

/// POST /api/search - Search session logs for past conversations
///
/// Searches through session log files for messages containing the keyword.
/// Useful for recovering context lost to compaction or finding previous decisions.
pub async fn search_logs(
    State(state): State<crate::proxy::ProxyState>,
    Json(query): Json<SearchQuery>,
) -> Result<Json<SearchResponse>, ApiError> {
    use std::fs;
    use std::io::{BufRead, BufReader};

    let keyword_lower = query.keyword.to_lowercase();
    let limit = query.limit.min(100); // Cap at 100 results
    let mut results = Vec::new();

    // Parse time range filter
    let (time_after, time_before) = query
        .time_range
        .as_deref()
        .map(parse_time_range)
        .unwrap_or((None, None));

    // List session files (newest first by filename)
    let mut sessions: Vec<_> = fs::read_dir(&state.log_dir)
        .map_err(|e| ApiError::Internal(format!("Failed to read log directory: {}", e)))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
        .collect();

    // Sort by filename descending (newest first, since filenames include timestamp)
    sessions.sort_by_key(|s| std::cmp::Reverse(s.file_name().to_os_string()));

    // Apply session filter if provided
    if let Some(ref session_filter) = query.session {
        let filter_lower = session_filter.to_lowercase();
        sessions.retain(|s| {
            s.file_name()
                .to_string_lossy()
                .to_lowercase()
                .contains(&filter_lower)
        });
    }

    let sessions_searched = sessions.len();

    'outer: for session_entry in &sessions {
        let file = match fs::File::open(session_entry.path()) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let reader = BufReader::new(file);
        let session_name = session_entry.file_name().to_string_lossy().to_string();

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            // Quick pre-filter before JSON parsing (performance optimization)
            if !line.to_lowercase().contains(&keyword_lower) {
                continue;
            }

            // Parse the event
            let event: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Only search Request events (they contain the messages array)
            if event.get("type").and_then(|t| t.as_str()) != Some("Request") {
                continue;
            }

            let timestamp_str = event
                .get("timestamp")
                .and_then(|t| t.as_str())
                .unwrap_or("");

            // Apply time range filter if specified
            if time_after.is_some() || time_before.is_some() {
                if let Ok(event_time) = timestamp_str.parse::<DateTime<Utc>>() {
                    if let Some(after) = time_after {
                        if event_time < after {
                            continue;
                        }
                    }
                    if let Some(before) = time_before {
                        if event_time >= before {
                            continue;
                        }
                    }
                }
            }

            let timestamp = timestamp_str.to_string();

            // Extract matching messages from body.messages[]
            if let Some(matches) =
                extract_matching_messages(&event, &keyword_lower, query.role.as_deref())
            {
                for (role, text) in matches {
                    results.push(SearchResult {
                        session: session_name.clone(),
                        timestamp: timestamp.clone(),
                        role,
                        text: truncate_around_match(&text, &keyword_lower, 500),
                    });

                    if results.len() >= limit {
                        break 'outer;
                    }
                }
            }
        }
    }

    Ok(Json(SearchResponse {
        query: query.keyword,
        sessions_searched,
        total_matches: results.len(),
        results,
    }))
}

/// Extract messages matching keyword and optional role filter
fn extract_matching_messages(
    event: &serde_json::Value,
    keyword: &str,
    role_filter: Option<&str>,
) -> Option<Vec<(String, String)>> {
    let messages = event.get("body")?.get("messages")?.as_array()?;

    let mut matches = Vec::new();

    for msg in messages {
        let role = msg.get("role")?.as_str()?;

        // Apply role filter
        if let Some(filter) = role_filter {
            if !role.eq_ignore_ascii_case(filter) {
                continue;
            }
        }

        // Extract text content from the message
        let content = msg.get("content")?;
        if let Some(text) = extract_text_content(content) {
            if text.to_lowercase().contains(keyword) {
                matches.push((role.to_string(), text));
            }
        }
    }

    if matches.is_empty() {
        None
    } else {
        Some(matches)
    }
}

/// Extract text from a content value (handles both string and array formats)
fn extract_text_content(content: &serde_json::Value) -> Option<String> {
    // Content can be a string directly
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }

    // Or an array of content blocks
    if let Some(blocks) = content.as_array() {
        let mut text_parts = Vec::new();
        for block in blocks {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    text_parts.push(text.to_string());
                }
            }
        }
        if !text_parts.is_empty() {
            return Some(text_parts.join("\n"));
        }
    }

    None
}

/// Truncate text around the first match of keyword, showing context
///
/// This function is defensive - it never panics even with malformed input.
/// If slicing fails, it logs a warning and gracefully degrades to showing more context.
fn truncate_around_match(text: &str, keyword: &str, max_len: usize) -> String {
    let text_lower = text.to_lowercase();

    // Find the position of the keyword
    if let Some(pos) = text_lower.find(keyword) {
        let half_context = max_len / 2;

        // Calculate start position (with some context before match)
        let start = if pos > half_context {
            // Find a word boundary near our desired start
            let desired_start = pos.saturating_sub(half_context);
            // Ensure we're on a character boundary before slicing
            let safe_start = text.floor_char_boundary(desired_start);

            // Use .get() instead of indexing - returns None if out of bounds
            match text.get(safe_start..) {
                Some(slice) => slice.find(' ').map_or(safe_start, |i| safe_start + i + 1),
                None => {
                    tracing::warn!(
                        "Failed to slice text at start position {} (text len: {})",
                        safe_start,
                        text.len()
                    );
                    0 // Fallback to beginning
                }
            }
        } else {
            0
        };

        // Calculate end position
        let end = (pos + keyword.len() + half_context).min(text.len());
        // Ensure we're on a character boundary before slicing
        let safe_end = text.floor_char_boundary(end);

        let end = match text.get(..safe_end) {
            Some(slice) => slice.rfind(' ').map_or(safe_end, |i| i),
            None => {
                tracing::warn!(
                    "Failed to slice text at end position {} (text len: {})",
                    safe_end,
                    text.len()
                );
                text.len() // Fallback to full length
            }
        };

        // Ensure end >= start (rfind can return position before start)
        let end = end.max(start);

        // Final slice with error handling - this is the critical extraction
        let extracted = match text.get(start..end) {
            Some(slice) => slice,
            None => {
                tracing::warn!(
                    "Failed to extract text[{}..{}] (text len: {}), using full text as fallback",
                    start,
                    end,
                    text.len()
                );
                text // Fallback to full text
            }
        };

        let mut result = String::new();
        if start > 0 {
            result.push_str("...");
        }
        result.push_str(extracted.trim());
        if end < text.len() {
            result.push_str("...");
        }
        result
    } else {
        // Keyword not found (shouldn't happen), just truncate
        if text.len() <= max_len {
            text.to_string()
        } else {
            // Ensure we're on a character boundary
            let safe_len = text.floor_char_boundary(max_len);

            // Safe slice with fallback
            match text.get(..safe_len) {
                Some(slice) => format!("{}...", slice),
                None => {
                    tracing::warn!(
                        "Failed to truncate text at {} (text len: {}), using char-based truncation",
                        safe_len,
                        text.len()
                    );
                    // Ultimate fallback: use char iteration which can't panic
                    text.chars().take(100).collect::<String>() + "..."
                }
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Lifestats Endpoints
// ═════════════════════════════════════════════════════════════════════════════

use crate::pipeline::lifestats_query::{
    ContextMatch, LifetimeStats, PromptMatch, ResponseMatch, SearchMode, ThinkingMatch,
};

/// Response for lifestats health endpoint
#[derive(Debug, Serialize)]
pub struct LifestatsHealthResponse {
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_available: Option<bool>,
}

/// GET /api/lifestats/health - Get lifestats processor status
///
/// Returns status of the lifestats storage and query system
pub async fn lifestats_health(State(state): State<super::ProxyState>) -> impl IntoResponse {
    let has_pipeline = state.pipeline.is_some();
    let has_query = state.lifestats_query.is_some();

    if has_pipeline && has_query {
        Json(LifestatsHealthResponse {
            status: "healthy".to_string(),
            message: "Lifestats storage and query interface operational".to_string(),
            query_available: Some(true),
        })
    } else if has_pipeline {
        Json(LifestatsHealthResponse {
            status: "degraded".to_string(),
            message: "Storage operational but query interface unavailable".to_string(),
            query_available: Some(false),
        })
    } else {
        Json(LifestatsHealthResponse {
            status: "disabled".to_string(),
            message: "Lifestats not configured".to_string(),
            query_available: None,
        })
    }
}

/// POST /api/lifestats/cleanup - Trigger manual retention cleanup
///
/// Deletes old events based on retention policy. Returns number of records deleted.
/// Note: Automatic cleanup runs every 24 hours in the background.
pub async fn lifestats_cleanup(
    State(_state): State<super::ProxyState>,
) -> Result<impl IntoResponse, StatusCode> {
    // Phase 2 TODO: Implement manual cleanup trigger
    // This requires exposing a method on LifestatsProcessor to trigger cleanup
    // For now, rely on automatic 24h cleanup
    Ok(Json(serde_json::json!({
        "status": "not_implemented",
        "message": "Manual cleanup not yet implemented (automatic cleanup runs every 24h)",
        "deleted": 0
    })))
}

/// Query parameters for lifestats search endpoints
#[derive(Debug, Deserialize)]
pub struct LifestatsSearchQuery {
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

/// GET /api/lifestats/search/thinking - Search thinking blocks
///
/// Query params:
///   - q: Search query (required)
///   - limit: Max results (default: 10, max: 100)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn lifestats_search_thinking(
    State(state): State<super::ProxyState>,
    Query(params): Query<LifestatsSearchQuery>,
) -> Result<Json<ThinkingSearchResponse>, ApiError> {
    let query_interface = state
        .lifestats_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Lifestats query interface not available".to_string()))?;

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

/// GET /api/lifestats/search/prompts - Search user prompts
///
/// Query params:
///   - q: Search query (required)
///   - limit: Max results (default: 10, max: 100)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn lifestats_search_prompts(
    State(state): State<super::ProxyState>,
    Query(params): Query<LifestatsSearchQuery>,
) -> Result<Json<PromptSearchResponse>, ApiError> {
    let query_interface = state
        .lifestats_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Lifestats query interface not available".to_string()))?;

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

/// GET /api/lifestats/search/responses - Search assistant responses
///
/// Query params:
///   - q: Search query (required)
///   - limit: Max results (default: 10, max: 100)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn lifestats_search_responses(
    State(state): State<super::ProxyState>,
    Query(params): Query<LifestatsSearchQuery>,
) -> Result<Json<ResponseSearchResponse>, ApiError> {
    let query_interface = state
        .lifestats_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Lifestats query interface not available".to_string()))?;

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

/// Query parameters for context recovery endpoint
#[derive(Debug, Deserialize)]
pub struct LifestatsContextQuery {
    /// Topic to search for
    pub topic: String,
    /// Maximum results (default: 10, max: 50)
    #[serde(default = "default_context_limit")]
    pub limit: usize,
    /// Search mode: "phrase" (default), "natural", "raw"
    #[serde(default)]
    pub mode: SearchMode,
}

fn default_context_limit() -> usize {
    10
}

/// GET /api/lifestats/context - Combined context recovery
///
/// Searches across thinking blocks, user prompts, and assistant responses,
/// then returns combined results sorted by relevance.
///
/// Query params:
///   - topic: Topic to search for (required)
///   - limit: Max results (default: 10, max: 50)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn lifestats_context(
    State(state): State<super::ProxyState>,
    Query(params): Query<LifestatsContextQuery>,
) -> Result<Json<ContextSearchResponse>, ApiError> {
    let query_interface = state
        .lifestats_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Lifestats query interface not available".to_string()))?;

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

/// GET /api/lifestats/stats - Get lifetime statistics
///
/// Returns aggregated statistics across all sessions: tokens, costs, tool usage, etc.
pub async fn lifestats_stats(
    State(state): State<super::ProxyState>,
) -> Result<Json<LifetimeStats>, ApiError> {
    let query_interface = state
        .lifestats_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Lifestats query interface not available".to_string()))?;

    let stats = query_interface
        .get_lifetime_stats()
        .map_err(|e| ApiError::Internal(format!("Failed to get lifetime stats: {}", e)))?;

    Ok(Json(stats))
}

// ═════════════════════════════════════════════════════════════════════════════
// User-Scoped Lifestats Endpoints (Cross-Session Context Recovery)
// ═════════════════════════════════════════════════════════════════════════════

/// GET /api/lifestats/search/user/:user_id/thinking - Search thinking blocks for a specific user
///
/// Query params:
///   - q: Search query (required)
///   - limit: Max results (default: 10, max: 100)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn lifestats_search_user_thinking(
    State(state): State<super::ProxyState>,
    Path(user_id): Path<String>,
    Query(params): Query<LifestatsSearchQuery>,
) -> Result<Json<ThinkingSearchResponse>, ApiError> {
    let query_interface = state
        .lifestats_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Lifestats query interface not available".to_string()))?;

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

/// GET /api/lifestats/search/user/:user_id/prompts - Search user prompts for a specific user
///
/// Query params:
///   - q: Search query (required)
///   - limit: Max results (default: 10, max: 100)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn lifestats_search_user_prompts(
    State(state): State<super::ProxyState>,
    Path(user_id): Path<String>,
    Query(params): Query<LifestatsSearchQuery>,
) -> Result<Json<PromptSearchResponse>, ApiError> {
    let query_interface = state
        .lifestats_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Lifestats query interface not available".to_string()))?;

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

/// GET /api/lifestats/search/user/:user_id/responses - Search assistant responses for a specific user
///
/// Query params:
///   - q: Search query (required)
///   - limit: Max results (default: 10, max: 100)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn lifestats_search_user_responses(
    State(state): State<super::ProxyState>,
    Path(user_id): Path<String>,
    Query(params): Query<LifestatsSearchQuery>,
) -> Result<Json<ResponseSearchResponse>, ApiError> {
    let query_interface = state
        .lifestats_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Lifestats query interface not available".to_string()))?;

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

/// GET /api/lifestats/context/user/:user_id - Combined context recovery for a specific user
///
/// Searches across thinking blocks, user prompts, and assistant responses for a specific user,
/// then returns combined results sorted by relevance.
///
/// Query params:
///   - topic: Topic to search for (required)
///   - limit: Max results (default: 10, max: 50)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn lifestats_context_user(
    State(state): State<super::ProxyState>,
    Path(user_id): Path<String>,
    Query(params): Query<LifestatsContextQuery>,
) -> Result<Json<ContextSearchResponse>, ApiError> {
    let query_interface = state
        .lifestats_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Lifestats query interface not available".to_string()))?;

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

/// GET /api/lifestats/stats/user/:user_id - Get lifetime statistics for a specific user
///
/// Returns aggregated statistics across all sessions belonging to the specified user.
pub async fn lifestats_stats_user(
    State(state): State<super::ProxyState>,
    Path(user_id): Path<String>,
) -> Result<Json<LifetimeStats>, ApiError> {
    let query_interface = state
        .lifestats_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Lifestats query interface not available".to_string()))?;

    let stats = query_interface
        .get_user_lifetime_stats(&user_id)
        .map_err(|e| ApiError::Internal(format!("Failed to get user lifetime stats: {}", e)))?;

    Ok(Json(stats))
}

// ═══════════════════════════════════════════════════════════════════════════════
// Semantic Search Endpoints (Hybrid FTS + Vector via RRF)
// ═══════════════════════════════════════════════════════════════════════════════

/// Response for hybrid context search
#[derive(Debug, Serialize)]
pub struct HybridContextResponse {
    pub topic: String,
    pub mode: String,
    pub search_type: String, // "fts_only" or "hybrid"
    pub results: Vec<ContextMatch>,
}

/// Live indexer status response (from running indexer)
#[derive(Debug, Serialize)]
pub struct LiveIndexerStatusResponse {
    pub enabled: bool,
    pub running: bool,
    pub provider: String,
    pub model: String,
    pub dimensions: usize,
    pub documents_indexed: u64,
    pub documents_pending: u64,
    pub index_progress_pct: f64,
}

/// GET /api/lifestats/embeddings/status - Get embedding indexer status
///
/// Returns status of the embedding indexer: provider, model, progress.
/// Uses live indexer handle if available, falls back to database.
pub async fn lifestats_embedding_status(
    State(state): State<super::ProxyState>,
) -> Result<Json<LiveIndexerStatusResponse>, ApiError> {
    // Try to get live status from running indexer
    if let Some(ref handle) = state.embedding_indexer {
        let status = handle.status();
        return Ok(Json(LiveIndexerStatusResponse {
            enabled: status.is_ready,
            running: true,
            provider: format!("{:?}", status.provider).to_lowercase(),
            model: status.model,
            dimensions: status.dimensions,
            documents_indexed: status.documents_indexed,
            documents_pending: status.documents_pending,
            index_progress_pct: status.index_progress_pct,
        }));
    }

    // Fall back to database stats
    let query_interface = state
        .lifestats_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Lifestats query interface not available".to_string()))?;

    let stats = query_interface
        .embedding_stats()
        .map_err(|e| ApiError::Internal(format!("Failed to get embedding stats: {}", e)))?;

    Ok(Json(LiveIndexerStatusResponse {
        enabled: stats.provider != "none",
        running: false, // Indexer not running, using DB fallback
        provider: stats.provider,
        model: stats.model,
        dimensions: stats.dimensions,
        documents_indexed: stats.total_embedded,
        documents_pending: stats.total_documents - stats.total_embedded,
        index_progress_pct: stats.progress_pct,
    }))
}

/// Response for reindex trigger
#[derive(Debug, Serialize)]
pub struct ReindexResponse {
    pub triggered: bool,
    pub message: String,
}

/// POST /api/lifestats/embeddings/reindex - Trigger a full re-index
///
/// Signals the running embedding indexer to clear and re-process all content.
/// Requires the indexer to be running.
pub async fn lifestats_embedding_reindex(
    State(state): State<super::ProxyState>,
) -> Result<Json<ReindexResponse>, ApiError> {
    if let Some(ref handle) = state.embedding_indexer {
        handle.trigger_reindex();
        Ok(Json(ReindexResponse {
            triggered: true,
            message: "Reindex triggered. The indexer will clear existing embeddings and re-process all content.".to_string(),
        }))
    } else {
        Err(ApiError::NotFound(
            "Embedding indexer not running. Start aspy with embeddings configured.".to_string(),
        ))
    }
}

/// POST /api/lifestats/embeddings/poll - Trigger a poll for new content
///
/// Signals the running embedding indexer to check for un-embedded content.
pub async fn lifestats_embedding_poll(
    State(state): State<super::ProxyState>,
) -> Result<Json<ReindexResponse>, ApiError> {
    if let Some(ref handle) = state.embedding_indexer {
        handle.trigger_poll();
        Ok(Json(ReindexResponse {
            triggered: true,
            message: "Poll triggered. The indexer will check for un-embedded content.".to_string(),
        }))
    } else {
        Err(ApiError::NotFound(
            "Embedding indexer not running.".to_string(),
        ))
    }
}

/// Query params for hybrid context search
#[derive(Debug, Deserialize)]
pub struct HybridContextQuery {
    /// Topic to search for
    #[serde(rename = "topic")]
    pub topic: String,
    /// Maximum results to return
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Search mode for FTS component
    #[serde(default)]
    pub mode: SearchMode,
}

/// GET /api/lifestats/context/hybrid/user/:user_id - Hybrid context recovery
///
/// Combines FTS5 keyword search with semantic vector search using
/// Reciprocal Rank Fusion (RRF). Falls back to FTS-only if embeddings
/// are not available.
///
/// Query params:
///   - topic: Topic to search for (required)
///   - limit: Max results (default: 10, max: 50)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn lifestats_context_hybrid_user(
    State(state): State<super::ProxyState>,
    Path(user_id): Path<String>,
    Query(params): Query<HybridContextQuery>,
) -> Result<Json<HybridContextResponse>, ApiError> {
    use crate::config::Config;
    use crate::pipeline::embeddings::{create_provider, AuthMethod, EmbeddingConfig, ProviderType};

    let query_interface = state
        .lifestats_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Lifestats query interface not available".to_string()))?;

    let limit = params.limit.min(50);

    // Check if embeddings are available
    let has_embeddings = query_interface.has_embeddings().unwrap_or(false);

    // Try to create query embedding if embeddings are enabled
    let query_embedding = if has_embeddings {
        // Load config to get embedding settings
        let config = Config::from_env();

        if config.embeddings.is_enabled() {
            // Create embedding provider for query
            let provider_type = match config.embeddings.provider.as_str() {
                "local" => ProviderType::Local,
                "remote" => ProviderType::Remote,
                _ => ProviderType::None,
            };

            let auth_method = match config.embeddings.auth_method.as_str() {
                "api-key" => AuthMethod::ApiKey,
                _ => AuthMethod::Bearer,
            };

            // Use the resolved API key from config (supports ASPY_EMBEDDINGS_API_KEY and others)
            let api_key = config.embeddings.api_key.clone();

            let embed_config = EmbeddingConfig {
                provider: provider_type,
                model: config.embeddings.model.clone(),
                api_key,
                api_base: config.embeddings.api_base.clone(),
                api_version: config.embeddings.api_version.clone(),
                auth_method,
                dimensions: None,
                batch_size: 1,    // Only need one embedding
                timeout_secs: 10, // Short timeout for query
            };

            let provider = create_provider(&embed_config);

            if provider.is_ready() {
                match provider.embed(&params.topic) {
                    Ok(result) => Some(result.embedding),
                    Err(e) => {
                        tracing::warn!("Failed to embed query: {}", e);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Perform hybrid or FTS-only search
    let (search_type, results) = if let Some(ref embedding) = query_embedding {
        let results = query_interface
            .recover_context_hybrid_user(
                &user_id,
                &params.topic,
                Some(embedding),
                limit,
                params.mode,
            )
            .map_err(|e| ApiError::Internal(format!("Hybrid search failed: {}", e)))?;
        ("hybrid".to_string(), results)
    } else {
        let results = query_interface
            .recover_user_context(&user_id, &params.topic, limit, params.mode)
            .map_err(|e| ApiError::Internal(format!("Search failed: {}", e)))?;
        ("fts_only".to_string(), results)
    };

    Ok(Json(HybridContextResponse {
        topic: params.topic,
        mode: format!("{:?}", params.mode),
        search_type,
        results,
    }))
}
