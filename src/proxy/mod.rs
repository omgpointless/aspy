// Proxy module - HTTP server that forwards requests to Anthropic API
//
// This module implements a transparent HTTP proxy using Axum. It intercepts
// requests to the Anthropic API, forwards them unchanged, and emits events
// based on the request/response content.
//
// STREAMING: For SSE responses, we stream chunks directly to the client
// while accumulating a copy for parsing. This ensures low latency for
// Claude Code while maintaining full observability.

pub mod api;
pub mod augmentation;
pub mod sessions;
pub mod sse;
pub mod transformation;
pub mod translation;

use std::error::Error as StdError;

use crate::config::{ClientsConfig, Config};
use crate::events::{generate_id, ProxyEvent, TrackedEvent};
use crate::parser::models::CapturedHeaders;
use crate::parser::Parser;
use crate::pipeline::{EventPipeline, ProcessContext};
use crate::{SharedContextState, StreamingThinking};
use anyhow::{Context, Result};
use augmentation::{AugmentationContext, AugmentationPipeline, StopReason};
use axum::{
    body::Body,
    extract::State,
    http::{Request, Response, StatusCode},
    response::IntoResponse,
    routing::any,
    Router,
};
use bytes::Bytes;
use chrono::Utc;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use translation::{ApiFormat, TranslationPipeline};

/// Maximum request body size (50MB) - prevents DoS via huge uploads
const MAX_REQUEST_BODY_SIZE: usize = 50 * 1024 * 1024;

/// Extract user prompt from request body
///
/// Finds the last user message in the messages array and returns its content.
/// Handles both string and array (multipart) content formats.
/// Only extracts from the LAST user message - does not fall back to earlier messages.
fn extract_user_prompt(body: &serde_json::Value) -> Option<String> {
    // Get the messages array
    let messages = body.get("messages")?.as_array()?;

    // Find the last user message (iterate in reverse)
    let last_user = messages
        .iter()
        .rev()
        .find(|msg| msg.get("role").and_then(|r| r.as_str()) == Some("user"))?;

    // Handle both string and array content formats
    match last_user.get("content")? {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(parts) => {
            // Concatenate text parts
            let text: Vec<&str> = parts
                .iter()
                .filter_map(|p| {
                    if p.get("type")?.as_str()? == "text" {
                        p.get("text")?.as_str()
                    } else {
                        None
                    }
                })
                .collect();
            if !text.is_empty() {
                Some(text.join("\n"))
            } else {
                // Last user message has no text blocks (only tool_result, etc.)
                // Don't fall back to earlier messages - return None
                None
            }
        }
        _ => None,
    }
}

/// Shared state for the proxy server
#[derive(Clone)]
pub struct ProxyState {
    /// HTTP client for forwarding requests
    client: reqwest::Client,
    /// Parser for extracting tool calls
    parser: Parser,
    /// Channel for sending tracked events to TUI (includes user/session context)
    event_tx_tui: mpsc::Sender<TrackedEvent>,
    /// Channel for sending tracked events to storage (includes user/session context)
    event_tx_storage: mpsc::Sender<TrackedEvent>,
    /// Target API URL (default, used when no client routing configured)
    api_url: String,
    /// Shared buffer for streaming thinking content to TUI
    streaming_thinking: StreamingThinking,
    /// Shared context state for augmentation
    context_state: SharedContextState,
    /// Augmentation pipeline for response modification
    augmentation: Arc<AugmentationPipeline>,
    /// Shared statistics for API endpoints
    stats: api::SharedStats,
    /// Shared events buffer for API endpoints
    events: api::SharedEvents,
    /// Session manager for multi-user tracking
    pub sessions: api::SharedSessions,
    /// Log directory for session log search
    pub log_dir: std::path::PathBuf,
    /// Client and provider configuration for multi-user routing
    clients: ClientsConfig,
    /// Event processing pipeline (optional, for lifestats storage and other processors)
    pipeline: Option<Arc<EventPipeline>>,
    /// Query interface for lifestats database (optional, requires lifestats enabled)
    pub lifestats_query: Option<Arc<crate::pipeline::lifestats_query::LifestatsQuery>>,
    /// Translation pipeline for OpenAI ↔ Anthropic format conversion
    translation: Arc<TranslationPipeline>,
    /// Transformation pipeline for request modification (system-reminder editing, etc.)
    transformation: Arc<transformation::TransformationPipeline>,
    /// Transformers config (for enabled flag and future per-transformer settings)
    transformers_config: crate::config::Transformers,
    /// Handle to the embedding indexer (optional, requires embeddings enabled)
    pub embedding_indexer: Option<crate::pipeline::embedding_indexer::IndexerHandle>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Proxy Input Types
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
    /// Query interface for lifestats database (optional, requires lifestats enabled)
    pub lifestats_query: Option<Arc<crate::pipeline::lifestats_query::LifestatsQuery>>,
    /// Handle to the embedding indexer (optional, requires embeddings enabled)
    pub embedding_indexer: Option<crate::pipeline::embedding_indexer::IndexerHandle>,
}

/// Context for handling an API response
struct ResponseContext {
    response: reqwest::Response,
    status: reqwest::StatusCode,
    headers: reqwest::header::HeaderMap,
    start: Instant,
    ttfb: std::time::Duration,
    request_id: String,
    is_messages_endpoint: bool,
    state: ProxyState,
    /// User ID (api_key_hash) for session tracking
    user_id: Option<String>,
    /// Translation context for response translation (if format differs)
    translation_ctx: translation::TranslationContext,
}

/// Start the proxy server
pub async fn start_proxy(
    config: Config,
    channels: EventChannels,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    shared: SharedState,
) -> Result<()> {
    let bind_addr = config.bind_addr;
    let api_url = config.api_url.clone();

    // Build the HTTP client with timeout and connection pooling
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300)) // 5 minute timeout for API calls
        .pool_max_idle_per_host(10)
        // Force HTTP/1.1 to avoid HTTP/2 connection reset issues with some providers
        .http1_only()
        .user_agent("aspy/0.1.0")
        .build()
        .context("Failed to create HTTP client")?;

    // Create augmentation pipeline from config (opt-in augmenters)
    let augmentation = Arc::new(AugmentationPipeline::from_config(&config.augmentation));
    if augmentation.is_empty() {
        tracing::debug!("Augmentation pipeline: no augmenters enabled");
    } else {
        tracing::debug!(
            "Augmentation pipeline initialized with: {:?}",
            augmentation.augmenter_names()
        );
    }

    // Create translation pipeline from config (opt-in feature)
    let translation = Arc::new(TranslationPipeline::from_config(&config.translation));
    if translation.is_enabled() {
        tracing::info!("Translation pipeline enabled (OpenAI ↔ Anthropic)");
    } else {
        tracing::debug!("Translation pipeline: disabled");
    }

    // Create transformation pipeline from config (opt-in feature)
    let transformation = Arc::new(transformation::TransformationPipeline::from_config(
        &config.transformers,
    ));
    if !transformation.is_empty() {
        tracing::info!(
            "Transformation pipeline enabled with: {:?}",
            transformation.transformer_names()
        );
    } else {
        tracing::debug!("Transformation pipeline: no transformers enabled");
    }

    // Log client routing config if present
    if config.clients.is_configured() {
        tracing::info!(
            "Client routing enabled: {} client(s), {} provider(s)",
            config.clients.clients.len(),
            config.clients.providers.len()
        );
        for (id, client) in &config.clients.clients {
            tracing::info!(
                "  Client '{}': {} -> provider '{}'",
                id,
                client.name,
                client.provider
            );
        }
    }

    let state = ProxyState {
        client,
        parser: Parser::new(),
        event_tx_tui: channels.tui,
        event_tx_storage: channels.storage,
        api_url,
        streaming_thinking: shared.streaming_thinking,
        context_state: shared.context,
        augmentation,
        stats: shared.stats,
        events: shared.events,
        sessions: shared.sessions,
        log_dir: config.log_dir.clone(),
        clients: config.clients.clone(),
        pipeline: shared.pipeline,
        lifestats_query: shared.lifestats_query,
        embedding_indexer: shared.embedding_indexer,
        translation,
        transformation,
        transformers_config: config.transformers.clone(),
    };

    // Build the router - API endpoints + proxy handler
    let app = Router::new()
        // Stats and events endpoints
        .route("/api/stats", axum::routing::get(api::get_stats))
        .route("/api/events", axum::routing::get(api::get_events))
        .route("/api/context", axum::routing::get(api::get_context))
        .route(
            "/api/context/snapshot",
            axum::routing::get(api::get_context_snapshot),
        )
        // Session management endpoints
        .route("/api/sessions", axum::routing::get(api::get_sessions))
        .route(
            "/api/session/start",
            axum::routing::post(api::session_start),
        )
        .route("/api/session/end", axum::routing::post(api::session_end))
        // Session todos endpoint
        .route(
            "/api/session/:user_id/todos",
            axum::routing::get(api::get_session_todos),
        )
        // Hook endpoints
        .route(
            "/api/hook/precompact",
            axum::routing::post(api::hook_precompact),
        )
        // Log search endpoint
        .route("/api/search", axum::routing::post(api::search_logs))
        // Lifestats endpoints
        .route(
            "/api/lifestats/health",
            axum::routing::get(api::lifestats_health),
        )
        .route(
            "/api/lifestats/cleanup",
            axum::routing::post(api::lifestats_cleanup),
        )
        .route(
            "/api/lifestats/search/thinking",
            axum::routing::get(api::lifestats_search_thinking),
        )
        .route(
            "/api/lifestats/search/prompts",
            axum::routing::get(api::lifestats_search_prompts),
        )
        .route(
            "/api/lifestats/search/responses",
            axum::routing::get(api::lifestats_search_responses),
        )
        .route(
            "/api/lifestats/context",
            axum::routing::get(api::lifestats_context),
        )
        .route(
            "/api/lifestats/stats",
            axum::routing::get(api::lifestats_stats),
        )
        // User-scoped lifestats endpoints
        .route(
            "/api/lifestats/search/user/:user_id/thinking",
            axum::routing::get(api::lifestats_search_user_thinking),
        )
        .route(
            "/api/lifestats/search/user/:user_id/prompts",
            axum::routing::get(api::lifestats_search_user_prompts),
        )
        .route(
            "/api/lifestats/search/user/:user_id/responses",
            axum::routing::get(api::lifestats_search_user_responses),
        )
        .route(
            "/api/lifestats/context/user/:user_id",
            axum::routing::get(api::lifestats_context_user),
        )
        .route(
            "/api/lifestats/stats/user/:user_id",
            axum::routing::get(api::lifestats_stats_user),
        )
        // Semantic search / hybrid endpoints
        .route(
            "/api/lifestats/embeddings/status",
            axum::routing::get(api::lifestats_embedding_status),
        )
        .route(
            "/api/lifestats/embeddings/reindex",
            axum::routing::post(api::lifestats_embedding_reindex),
        )
        .route(
            "/api/lifestats/embeddings/poll",
            axum::routing::post(api::lifestats_embedding_poll),
        )
        .route(
            "/api/lifestats/context/hybrid/user/:user_id",
            axum::routing::get(api::lifestats_context_hybrid_user),
        )
        // Proxy handler (catch-all)
        .route("/*path", any(proxy_handler))
        .with_state(state);

    tracing::info!("Starting proxy on {}", bind_addr);

    // Bind and serve
    let listener = TcpListener::bind(bind_addr)
        .await
        .context("Failed to bind to address")?;

    tracing::info!("Proxy listening on {}", bind_addr);

    // Start serving requests with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_rx.await.ok();
        })
        .await
        .context("Server error")?;

    tracing::info!("Proxy server shut down gracefully");
    Ok(())
}

impl ProxyState {
    /// Send an event to TUI, storage, and user's session
    ///
    /// Events are processed through the pipeline (if configured) before dispatch.
    /// Events are wrapped in TrackedEvent with user/session context for filtering.
    /// We ignore errors here to avoid blocking the proxy if a receiver is slow or closed.
    async fn send_event(&self, event: ProxyEvent, user_id: Option<&str>) {
        // Build ProcessContext for pipeline
        let session_id = user_id.and_then(|uid| {
            self.sessions
                .lock()
                .ok()
                .and_then(|sessions| sessions.get_session_id(&sessions::UserId::new(uid)))
        });

        let ctx = ProcessContext::new(
            session_id.as_deref(),
            user_id,
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
            session_id,
        );

        // Send tracked event to TUI and storage channels
        let _ = self.event_tx_tui.send(tracked.clone()).await;
        let _ = self.event_tx_storage.send(tracked).await;

        // Also record raw event to user's session (SessionManager tracks its own events)
        if let Some(uid) = user_id {
            if let Ok(mut sessions) = self.sessions.lock() {
                sessions.record_event(&sessions::UserId::new(uid), final_event);
            }
        }
    }
}

/// Result of extracting client routing from a path
struct ClientRouting {
    /// Client ID (if matched)
    client_id: Option<String>,
    /// Base URL to forward to
    base_url: String,
    /// API path (with client prefix stripped)
    api_path: String,
}

/// Extract client routing information from request path
///
/// If clients are configured and the path starts with a known client ID,
/// routes to that client's provider. Otherwise falls back to default api_url.
///
/// Examples:
///   /dev-1/v1/messages -> client_id="dev-1", api_path="/v1/messages"
///   /v1/messages -> client_id=None, api_path="/v1/messages"
fn extract_client_routing(
    path: &str,
    clients: &ClientsConfig,
    default_api_url: &str,
) -> ClientRouting {
    // Only try client routing if clients are configured
    if !clients.is_configured() {
        return ClientRouting {
            client_id: None,
            base_url: default_api_url.to_string(),
            api_path: path.to_string(),
        };
    }

    // Path format: /{client_id}/v1/messages...
    // First segment after leading / is the potential client ID
    let segments: Vec<&str> = path.trim_start_matches('/').splitn(2, '/').collect();

    if segments.is_empty() {
        return ClientRouting {
            client_id: None,
            base_url: default_api_url.to_string(),
            api_path: path.to_string(),
        };
    }

    let potential_client_id = segments[0];

    // Check if this is a configured client
    if let Some(base_url) = clients.get_client_base_url(potential_client_id) {
        let api_path = if segments.len() > 1 {
            format!("/{}", segments[1])
        } else {
            "/".to_string()
        };

        tracing::trace!(
            "Client routing: '{}' -> base_url='{}', api_path='{}'",
            potential_client_id,
            base_url,
            api_path
        );

        return ClientRouting {
            client_id: Some(potential_client_id.to_string()),
            base_url: base_url.to_string(),
            api_path,
        };
    }

    // Not a known client - use default routing
    ClientRouting {
        client_id: None,
        base_url: default_api_url.to_string(),
        api_path: path.to_string(),
    }
}

/// Main proxy handler - intercepts and forwards all requests
///
/// For SSE (streaming) responses: Streams chunks directly to client while
/// accumulating for parsing. This gives Claude Code low-latency token delivery.
///
/// For JSON responses: Buffers the (small) response for parsing before forwarding.
async fn proxy_handler(
    State(state): State<ProxyState>,
    req: Request<Body>,
) -> Result<Response<Body>, ProxyError> {
    let start = Instant::now();
    let request_id = generate_id();

    // Extract request details
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();

    // Extract client routing from path (before we consume the request)
    let routing = extract_client_routing(uri.path(), &state.clients, &state.api_url);

    // Use client_id for user identification if available, otherwise fall back to API key hash
    let user_id = routing
        .client_id
        .clone()
        .or_else(|| extract_user_id(&headers));

    // Backfill session user_id immediately (before any events are sent)
    // This ensures events go to the hook-created session, not a new implicit one
    if let Some(ref uid) = user_id {
        if let Ok(mut sessions) = state.sessions.lock() {
            sessions.backfill_user_id(uid);
        }
    }

    tracing::trace!(
        "Proxying {} {} -> {} (client: {:?})",
        method,
        uri,
        routing.base_url,
        routing.client_id
    );

    // Read the request body (with size limit)
    let body_bytes = axum::body::to_bytes(req.into_body(), MAX_REQUEST_BODY_SIZE)
        .await
        .map_err(|e| ProxyError::BodyRead(format!("Failed to read request body: {}", e)))?;

    // Pre-check: is this a messages endpoint? (for transformation and parsing)
    let is_likely_messages =
        routing.api_path.contains("/messages") || routing.api_path.contains("/chat/completions");

    // ─────────────────────────────────────────────────────────────────────────
    // REQUEST TRANSFORMATION (runs BEFORE translation, on known Anthropic format)
    // ─────────────────────────────────────────────────────────────────────────
    // SystemReminderEditor and future transformers expect Anthropic message format.
    // We transform first, then translate to target format if needed.
    let (body_bytes, body_was_transformed, transform_tokens, transform_modifications) =
        if is_likely_messages
            && method == "POST"
            && state.transformers_config.enabled
            && !state.transformation.is_empty()
        {
            if let Ok(body_json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                let model = body_json.get("model").and_then(|m| m.as_str());
                let mut ctx = transformation::TransformContext::new(
                    user_id.as_deref(),
                    &routing.api_path,
                    model,
                );

                // Extract tool_result_count and compute session turn_number
                if let Some(messages) = body_json.get("messages").and_then(|m| m.as_array()) {
                    // Tool result count = count in last user message
                    let tool_results = messages
                        .iter()
                        .rev()
                        .find(|msg| msg.get("role").and_then(|r| r.as_str()) == Some("user"))
                        .and_then(|last_user| {
                            last_user
                                .get("content")
                                .and_then(|c| c.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter(|b| {
                                            b.get("type").and_then(|t| t.as_str())
                                                == Some("tool_result")
                                        })
                                        .count()
                                })
                        })
                        .unwrap_or(0);

                    ctx.tool_result_count = Some(tool_results);

                    // Session-level turn count (persists across compaction)
                    // - Fresh prompt (tool_results == 0): increment and use new count
                    // - Tool continuation (tool_results > 0): use existing count
                    //
                    // Also extract todos for CompactEnhancer (if any)
                    let mut session_todos: Vec<sessions::TodoItem> = Vec::new();
                    if let Some(ref uid) = user_id {
                        if let Ok(mut sessions) = state.sessions.lock() {
                            let sid = sessions::UserId::new(uid);
                            if tool_results == 0 {
                                // Fresh user prompt - increment session turn count
                                let turn = sessions.increment_turn_count(&sid);
                                ctx.turn_number = Some(turn);
                            } else {
                                // Tool continuation - use existing count
                                ctx.turn_number = Some(sessions.get_turn_count(&sid));
                            }

                            // Extract todos for CompactEnhancer
                            if let Some(session) = sessions.get_user_session(&sid) {
                                if !session.todos.is_empty() {
                                    session_todos = session.todos.clone();
                                }
                            }
                        }
                    }

                    // Attach todos to context (empty slice if none)
                    if !session_todos.is_empty() {
                        // SAFETY: session_todos lives until ctx is used
                        // We leak this to get a static lifetime since ctx is used synchronously
                        /*
                           The Box::leak pattern for todos is intentional - compaction is rare (once per
                           context fill) and the leaked memory is bounded by todo list size. A small trade-off
                           for clean lifetime management in the sync transformer pipeline.
                        */
                        let todos_static: &'static [sessions::TodoItem] =
                            Box::leak(session_todos.into_boxed_slice());
                        ctx.todos = Some(todos_static);
                    }

                    // Fallback: if no session available, count user messages in request
                    if ctx.turn_number.is_none() {
                        let msg_turn = messages
                            .iter()
                            .filter(|msg| msg.get("role").and_then(|r| r.as_str()) == Some("user"))
                            .count() as u64;
                        ctx.turn_number = Some(msg_turn);
                    }
                }

                tracing::debug!(
                    turn = ctx.turn_number,
                    tool_results = ctx.tool_result_count,
                    client = ctx.client_id,
                    "Transformer context: turn={} tool_results={} client={}",
                    ctx.turn_number.unwrap_or(0),
                    ctx.tool_result_count.unwrap_or(0),
                    ctx.client_id.unwrap_or("unknown")
                );

                tracing::debug!(
                    transformers = ?state.transformation.transformer_names(),
                    "Running transformation pipeline on request"
                );
                match state.transformation.transform(&body_json, &ctx) {
                    transformation::TransformResult::Modified {
                        body: new_body,
                        tokens,
                        modifications,
                    } => {
                        if let Some(t) = &tokens {
                            tracing::info!(
                                tokens_before = t.before,
                                tokens_after = t.after,
                                delta = t.delta(),
                                modifications = ?modifications,
                                "✓ Request transformed: {} tokens → {} tokens (Δ{})",
                                t.before,
                                t.after,
                                t.delta()
                            );
                        } else {
                            tracing::info!(modifications = ?modifications, "✓ Request transformed (no token tracking)");
                        }
                        (
                            serde_json::to_vec(&new_body).unwrap_or_else(|_| body_bytes.to_vec()),
                            true,
                            tokens,
                            modifications,
                        )
                    }
                    transformation::TransformResult::Block { reason, status } => {
                        tracing::info!(
                            "Request blocked by transformation pipeline: {} (status {})",
                            reason,
                            status
                        );
                        return Err(ProxyError::BodyRead(format!(
                            "Request blocked: {} (status {})",
                            reason, status
                        )));
                    }
                    transformation::TransformResult::Error(e) => {
                        tracing::warn!("Transformation error (continuing with original): {}", e);
                        (body_bytes.to_vec(), false, None, Vec::new())
                    }
                    transformation::TransformResult::Unchanged => {
                        (body_bytes.to_vec(), false, None, Vec::new())
                    }
                }
            } else {
                (body_bytes.to_vec(), false, None, Vec::new())
            }
        } else {
            (body_bytes.to_vec(), false, None, Vec::new())
        };

    // Determine target API format based on provider config
    // If provider expects OpenAI format, translate Anthropic → OpenAI
    let target_format = routing
        .client_id
        .as_ref()
        .and_then(|cid| state.clients.get_client_api_format(cid))
        .map(|fmt| match fmt {
            crate::config::ApiFormat::Anthropic => translation::ApiFormat::Anthropic,
            crate::config::ApiFormat::Openai => translation::ApiFormat::OpenAI,
        })
        .unwrap_or(translation::ApiFormat::Anthropic);

    // Apply translation if enabled, targeting the provider's expected format
    let (translated_body, translation_ctx, translated_path) = state
        .translation
        .translate_request_for_target(&routing.api_path, &headers, &body_bytes, target_format)
        .map_err(|e| ProxyError::BodyRead(format!("Translation failed: {}", e)))?;

    // Use translated path for endpoint detection (may have changed from /chat/completions to /messages)
    let effective_api_path = if translation_ctx.needs_response_translation() {
        // Log translated body model for debugging
        if let Ok(translated_json) = serde_json::from_slice::<serde_json::Value>(&translated_body) {
            let model = translated_json
                .get("model")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown");
            tracing::info!(
                "Translation: {} -> {} | model: {} | path: {} -> {}",
                translation_ctx.client_format,
                translation_ctx.backend_format,
                model,
                routing.api_path,
                translated_path
            );
        }
        translated_path.clone()
    } else {
        routing.api_path.clone()
    };

    // Check if this is a completion endpoint (Anthropic /messages or OpenAI /chat/completions)
    let is_messages_endpoint = effective_api_path.contains("/messages")
        || effective_api_path.contains("/chat/completions");

    // Parse transformed body for Request event display
    let request_body = if is_messages_endpoint {
        serde_json::from_slice::<serde_json::Value>(&body_bytes).ok()
    } else {
        None
    };

    // Extract and emit user prompt from request (if POST to /messages)
    if is_messages_endpoint && method == "POST" {
        if let Some(ref body) = request_body {
            if let Some(user_prompt) = extract_user_prompt(body) {
                state
                    .send_event(
                        ProxyEvent::UserPrompt {
                            timestamp: Utc::now(),
                            content: user_prompt,
                        },
                        user_id.as_deref(),
                    )
                    .await;
            }
        }
    }

    // Emit request event (use original path for logging, not stripped path)
    state
        .send_event(
            ProxyEvent::Request {
                id: request_id.clone(),
                timestamp: Utc::now(),
                method: method.to_string(),
                path: uri.path().to_string(),
                body_size: body_bytes.len(),
                body: request_body,
            },
            user_id.as_deref(),
        )
        .await;

    // Emit transformation event if tokens were tracked
    if let Some(tokens) = transform_tokens {
        state
            .send_event(
                ProxyEvent::RequestTransformed {
                    timestamp: Utc::now(),
                    transformer: "transformation-pipeline".to_string(),
                    tokens_before: tokens.before,
                    tokens_after: tokens.after,
                    modifications: transform_modifications,
                },
                user_id.as_deref(),
            )
            .await;
    }

    // Parse request for tool results if this is a messages endpoint
    if is_messages_endpoint && method == "POST" {
        match state
            .parser
            .parse_request(&body_bytes, user_id.as_deref())
            .await
        {
            Ok(events) => {
                for event in events {
                    state.send_event(event, user_id.as_deref()).await;
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to parse request for tool results: {} - request body may be malformed",
                    e
                );
            }
        }
    }

    // Build the forward URL using client routing and translated path
    let forward_url = format!("{}{}", routing.base_url, effective_api_path);

    // Don't pass Anthropic-specific query params (like ?beta=true) to OpenAI targets
    let forward_url = if target_format == translation::ApiFormat::OpenAI {
        // Strip query params entirely for OpenAI targets
        forward_url
    } else {
        let query = uri.query().unwrap_or("");
        if query.is_empty() {
            forward_url
        } else {
            format!("{}?{}", forward_url, query)
        }
    };

    // Final body is the translated body (transformation already happened earlier)
    let final_body = translated_body;
    let body_size = final_body.len();

    // Build the forwarded request
    // With reqwest 0.12, Method types align with axum (both use http 1.0 crate)
    let mut forward_req = state.client.request(method, &forward_url).body(final_body);

    // Get effective auth config for this client (if routed)
    let auth_config = routing
        .client_id
        .as_ref()
        .and_then(|cid| state.clients.get_effective_auth(cid));

    // Copy relevant headers with auth transformation
    for (key, value) in headers.iter() {
        let key_str = key.as_str();

        // Skip connection control headers
        if key_str == "host" || key_str == "connection" || key_str == "transfer-encoding" {
            continue;
        }

        // Skip content-length if body was modified (transformation or translation changed size)
        if key_str == "content-length"
            && (body_was_transformed || translation_ctx.needs_response_translation())
        {
            continue;
        }

        // Skip auth headers if strip_incoming is enabled
        if let Some(auth) = auth_config {
            if auth.should_strip_incoming() && is_auth_header(key_str) {
                tracing::debug!("Stripping auth header: {}", key);
                continue;
            }
        }

        // Skip Anthropic-specific headers when targeting OpenAI format
        if target_format == translation::ApiFormat::OpenAI && is_anthropic_header(key_str) {
            tracing::debug!("Stripping Anthropic header for OpenAI target: {}", key);
            continue;
        }

        forward_req = forward_req.header(key, value);
    }

    // Ensure Content-Type is set for translated requests
    if translation_ctx.needs_response_translation() {
        forward_req = forward_req.header("content-type", "application/json");
    }

    // Add provider's auth header if configured (after stripping incoming auth)
    let auth_header_added = if let Some(auth) = auth_config {
        if let Some((header_name, header_value)) = auth.build_header() {
            forward_req = forward_req.header(&header_name, &header_value);
            Some((header_name, header_value.len()))
        } else {
            tracing::warn!(
                "Auth config present but no header built - check key_env is set for client {}",
                routing.client_id.as_deref().unwrap_or("unknown")
            );
            None
        }
    } else {
        None
    };

    // Log forwarding summary
    tracing::trace!(
        "Forwarding: {} | body: {} bytes | auth: {} | format: {}",
        forward_url,
        body_size,
        auth_header_added
            .as_ref()
            .map(|(h, len)| format!("{}({} chars)", h, len))
            .unwrap_or_else(|| "passthrough".to_string()),
        target_format
    );

    // Send the request
    let response = forward_req.send().await.map_err(|e| {
        // Provide detailed error information with full source chain
        let mut error_chain = format!("{}", e);
        let mut source = StdError::source(&e);
        while let Some(s) = source {
            error_chain.push_str(&format!(" <- {}", s));
            source = s.source();
        }

        let error_type = if e.is_connect() {
            "Connection"
        } else if e.is_timeout() {
            "Timeout"
        } else if e.is_request() {
            "Request"
        } else if e.is_body() {
            "Body"
        } else if e.is_decode() {
            "Decode"
        } else {
            "Unknown"
        };

        tracing::error!(
            "Upstream {} error: {} | Body size: {} bytes",
            error_type,
            error_chain,
            body_size
        );
        ProxyError::Upstream(format!("{} error: {}", error_type, error_chain))
    })?;

    // TTFB: Time to first byte - captured immediately after headers received
    let ttfb = start.elapsed();

    let status = response.status();
    let response_headers = response.headers().clone();

    // Extract headers before consuming response
    let req_headers = extract_request_headers(&headers);
    let resp_headers = extract_response_headers(&response_headers);
    let combined_headers = merge_headers(req_headers, resp_headers);

    // Emit headers captured event early (we have them now)
    state
        .send_event(
            ProxyEvent::HeadersCaptured {
                request_id: request_id.clone(),
                timestamp: Utc::now(),
                headers: combined_headers.clone(),
            },
            user_id.as_deref(),
        )
        .await;

    // Emit rate limit update if available
    if combined_headers.has_rate_limits() {
        state
            .send_event(
                ProxyEvent::RateLimitUpdate {
                    timestamp: Utc::now(),
                    requests_remaining: combined_headers.requests_remaining,
                    requests_limit: combined_headers.requests_limit,
                    tokens_remaining: combined_headers.tokens_remaining,
                    tokens_limit: combined_headers.tokens_limit,
                    reset_time: combined_headers
                        .requests_reset
                        .clone()
                        .or(combined_headers.tokens_reset.clone()),
                },
                user_id.as_deref(),
            )
            .await;
    }

    // Bundle context for handler
    let ctx = ResponseContext {
        response,
        status,
        headers: response_headers,
        start,
        ttfb,
        request_id,
        is_messages_endpoint,
        state,
        user_id,
        translation_ctx,
    };

    // Decide: streaming (SSE) or buffered (JSON) response handling
    if sse::is_sse_response(&ctx.headers) && ctx.status.is_success() {
        tracing::trace!("Handling SSE streaming response");
        handle_streaming_response(ctx).await
    } else {
        tracing::trace!("Handling buffered (non-streaming) response");
        handle_buffered_response(ctx).await
    }
}

/// Handle SSE streaming responses - forward chunks immediately while accumulating
async fn handle_streaming_response(ctx: ResponseContext) -> Result<Response<Body>, ProxyError> {
    let ResponseContext {
        response,
        status,
        headers: response_headers,
        start,
        ttfb,
        request_id,
        is_messages_endpoint,
        state,
        user_id,
        translation_ctx,
    } = ctx;

    // ─────────────────────────────────────────────────────────────────────────
    // STREAMING RESPONSE TRANSLATION
    // ─────────────────────────────────────────────────────────────────────────
    //
    // If the request was translated (e.g., OpenAI → Anthropic), the streaming
    // response is translated back (Anthropic → OpenAI) using the translator's
    // `translate_chunk()` and `finalize()` methods.
    //
    // Key design decisions:
    // - Real-time extraction (tools, thinking, block index) operates on RAW format
    // - Augmentation injects Anthropic-format SSE, then gets translated
    // - Translation errors log and skip (graceful degradation)
    // - Accumulation for post-stream parsing stays RAW for correct event emission
    // ─────────────────────────────────────────────────────────────────────────
    let needs_translation = translation_ctx.needs_response_translation();
    // Create channel for streaming to client
    // Buffer size of 64 provides some cushion without excessive memory use
    let (tx, rx) = mpsc::channel::<Result<Bytes, Infallible>>(64);

    // Clone what we need for the background task
    let parser = state.parser.clone();
    let event_tx_tui = state.event_tx_tui.clone();
    let event_tx_storage = state.event_tx_storage.clone();
    let request_id_clone = request_id.clone();
    let streaming_thinking = state.streaming_thinking.clone();
    let context_state = state.context_state.clone();
    let augmentation = state.augmentation.clone();
    let _sessions = state.sessions.clone();
    let user_id_clone = user_id.clone();
    let translation_pipeline = state.translation.clone();
    let mut translation_ctx = translation_ctx;

    // Spawn task to stream response while accumulating
    tokio::spawn(async move {
        let mut accumulated = Vec::new();
        let mut byte_stream = response.bytes_stream();
        let mut total_bytes = 0usize;
        // Buffer for incomplete SSE lines across chunks
        let mut line_buffer = String::new();
        // Track content block index for potential injection
        let mut max_block_index: u32 = 0;
        // Track if we've injected this response and how many tokens (only inject once)
        let mut injected_tokens: Option<u32> = None;
        // Track model for injection filtering (skip Haiku utility calls)
        let mut response_model = String::new();

        // Get translator reference if translation is needed (OpenAI ↔ Anthropic)
        let translator = if needs_translation {
            translation_pipeline
                .get_response_translator(ApiFormat::Anthropic, translation_ctx.client_format)
        } else {
            None
        };

        // Stream chunks to client while accumulating
        while let Some(chunk_result) = byte_stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    total_bytes += chunk.len();
                    accumulated.extend_from_slice(&chunk);

                    // CRITICAL: Register tool_use IDs immediately as we see them
                    // This fixes the race condition where the next request arrives
                    // before we finish parsing the stream
                    if is_messages_endpoint {
                        if let Ok(chunk_str) = std::str::from_utf8(&chunk) {
                            line_buffer.push_str(chunk_str);
                            // Process complete lines
                            while let Some(newline_pos) = line_buffer.find('\n') {
                                let line = line_buffer[..newline_pos].trim();
                                // Register tool_use IDs immediately
                                if let Some(tool_info) = sse::extract_tool_use(line) {
                                    parser.register_pending_tool(tool_info.0, tool_info.1).await;
                                }
                                // Track content block index for injection
                                if let Some(idx) = sse::extract_content_block_index(line) {
                                    if idx >= max_block_index {
                                        max_block_index = idx + 1;
                                    }
                                }
                                // Extract model from message_start (for injection filtering)
                                if response_model.is_empty() {
                                    if let Some(model) = sse::extract_model(line) {
                                        response_model = model;
                                    }
                                }
                                // Emit ThinkingStarted immediately for real-time feedback
                                if sse::is_thinking_block_start(line) {
                                    // Clear buffer for new thinking block (keyed by user_id)
                                    if let Ok(mut map) = streaming_thinking.lock() {
                                        let key = user_id_clone.as_deref().unwrap_or("unknown");
                                        map.insert(key.to_string(), String::new());
                                    }
                                    let _ = event_tx_tui
                                        .send(TrackedEvent::new(
                                            ProxyEvent::ThinkingStarted {
                                                timestamp: chrono::Utc::now(),
                                            },
                                            user_id_clone.clone(),
                                            None, // session_id not available in streaming context
                                        ))
                                        .await;
                                }
                                // Stream thinking content in real-time (keyed by user_id)
                                if let Some(thinking_text) = sse::extract_thinking_delta(line) {
                                    if let Ok(mut map) = streaming_thinking.lock() {
                                        let key = user_id_clone.as_deref().unwrap_or("unknown");
                                        map.entry(key.to_string())
                                            .or_default()
                                            .push_str(&thinking_text);
                                    }
                                }
                                line_buffer = line_buffer[newline_pos + 1..].to_string();
                            }
                        }

                        // Check if this chunk contains message_delta - inject before forwarding
                        // Only inject on end_turn responses (not tool_use)
                        if injected_tokens.is_none() {
                            match std::str::from_utf8(&chunk) {
                                Err(e) => {
                                    tracing::warn!(
                                        "SSE injection skipped: UTF-8 decode failed: {}",
                                        e
                                    );
                                }
                                Ok(chunk_str) => {
                                    // Check for message_delta to trigger augmentation
                                    if chunk_str.contains("message_delta") {
                                        if let Some(stop_reason) = StopReason::from_chunk(chunk_str)
                                        {
                                            // Build augmentation context
                                            let aug_ctx = AugmentationContext {
                                                model: &response_model,
                                                stop_reason,
                                                next_block_index: max_block_index,
                                                context_state: &context_state,
                                            };

                                            // Run augmentation pipeline
                                            if let Some(augmented) = augmentation.process(&aug_ctx)
                                            {
                                                // Translate injection if needed (augmentation produces Anthropic SSE)
                                                let injection_to_send = if let Some(t) = &translator
                                                {
                                                    match t.translate_chunk(
                                                        &augmented.sse_bytes,
                                                        &mut translation_ctx,
                                                    ) {
                                                        Ok(translated)
                                                            if !translated.is_empty() =>
                                                        {
                                                            Bytes::from(translated)
                                                        }
                                                        Ok(_) => {
                                                            Bytes::from(augmented.sse_bytes.clone())
                                                        } // Empty = shouldn't happen for complete injection
                                                        Err(e) => {
                                                            tracing::warn!("Augmentation translation failed: {}", e);
                                                            Bytes::from(augmented.sse_bytes.clone())
                                                            // Fallback to raw
                                                        }
                                                    }
                                                } else {
                                                    Bytes::from(augmented.sse_bytes.clone())
                                                };
                                                let _ = tx.send(Ok(injection_to_send)).await;
                                                injected_tokens = Some(augmented.tokens_injected);

                                                // Track token injection for stats
                                                tracing::debug!(
                                                    tokens_injected = augmented.tokens_injected,
                                                    "Augmentation injected ~{} tokens",
                                                    augmented.tokens_injected
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Forward chunk to client (translate if needed)
                    let chunk_to_send = if let Some(t) = &translator {
                        match t.translate_chunk(&chunk, &mut translation_ctx) {
                            Ok(translated) if !translated.is_empty() => {
                                Some(Bytes::from(translated))
                            }
                            Ok(_) => None, // Partial line buffered, nothing to send yet
                            Err(e) => {
                                tracing::warn!("Chunk translation failed (skipping): {}", e);
                                None // Log and skip per error handling strategy
                            }
                        }
                    } else {
                        Some(chunk)
                    };

                    if let Some(bytes_to_send) = chunk_to_send {
                        if tx.send(Ok(bytes_to_send)).await.is_err() {
                            // Client disconnected, but continue accumulating for logging
                            tracing::debug!("Client disconnected during streaming");
                        }
                    }
                }
                Err(e) => {
                    let error_msg = format!("Error reading stream chunk: {}", e);
                    tracing::error!("{}", error_msg);

                    // Emit error event for observability (JSONL logs + TUI)
                    let error_event = ProxyEvent::Error {
                        timestamp: chrono::Utc::now(),
                        message: error_msg,
                        context: Some(format!(
                            "request_id: {}, accumulated: {} bytes",
                            request_id_clone,
                            accumulated.len()
                        )),
                    };
                    let tracked = TrackedEvent::new(
                        error_event,
                        user_id_clone.clone(),
                        None, // session_id not available in streaming context
                    );
                    let _ = event_tx_tui.send(tracked.clone()).await;
                    let _ = event_tx_storage.send(tracked).await;
                    break;
                }
            }
        }

        // Send stream terminator if translation is active (e.g., "data: [DONE]" for OpenAI)
        if let Some(t) = &translator {
            if let Some(terminator) = t.finalize(&translation_ctx) {
                let _ = tx.send(Ok(Bytes::from(terminator))).await;
            }
        }

        // Process any remaining data in line buffer
        if is_messages_endpoint && !line_buffer.is_empty() {
            if let Some(tool_info) = sse::extract_tool_use(line_buffer.trim()) {
                parser.register_pending_tool(tool_info.0, tool_info.1).await;
            }
        }

        // Stream complete - now parse and emit events
        let duration = start.elapsed();

        // Helper to send events through pipeline (includes session recording, lifestats, etc.)
        let send_event = |event: ProxyEvent| {
            let state_ref = state.clone();
            let uid = user_id_clone.clone();
            async move {
                state_ref.send_event(event, uid.as_deref()).await;
            }
        };

        // Parse accumulated response for display
        let parsed_body = if is_messages_endpoint {
            let body_str = std::str::from_utf8(&accumulated).unwrap_or("");
            sse::assemble_to_json(body_str)
        } else {
            None
        };

        // Emit response event
        send_event(ProxyEvent::Response {
            request_id: request_id_clone.clone(),
            timestamp: Utc::now(),
            status: status.as_u16(),
            body_size: total_bytes,
            ttfb,
            duration,
            body: parsed_body,
        })
        .await;

        // Parse for tool calls, thinking blocks, usage, etc.
        if is_messages_endpoint {
            if let Ok(events) = parser
                .parse_response(&accumulated, user_id_clone.as_deref())
                .await
            {
                for event in events {
                    // Update context state when we see ApiUsage (skip Haiku utility calls)
                    if let ProxyEvent::ApiUsage {
                        input_tokens,
                        cache_creation_tokens,
                        cache_read_tokens,
                        model,
                        ..
                    } = &event
                    {
                        if !model.to_lowercase().contains("haiku") {
                            if let Ok(mut ctx) = context_state.lock() {
                                ctx.update(
                                    *input_tokens as u64,
                                    *cache_creation_tokens as u64,
                                    *cache_read_tokens as u64,
                                );
                            }
                        }
                    }
                    // Reset warnings on context compact
                    if let ProxyEvent::ContextCompact { .. } = &event {
                        if let Ok(mut ctx) = context_state.lock() {
                            ctx.reset_warnings();
                        }
                    }
                    send_event(event).await;
                }
            }
        }

        // Emit augmentation event if tokens were injected
        if let Some(tokens) = injected_tokens {
            send_event(ProxyEvent::ResponseAugmented {
                timestamp: Utc::now(),
                augmenter: "context-warning".to_string(),
                tokens_injected: tokens,
            })
            .await;
        }

        tracing::trace!(
            "Streaming complete: {} bytes in {:.2}s",
            total_bytes,
            duration.as_secs_f64()
        );
    });

    // Build streaming response to return to client immediately
    let stream = ReceiverStream::new(rx);
    let body = Body::from_stream(stream);

    let mut builder = Response::builder().status(status.as_u16());

    // Copy response headers (types align between reqwest 0.12 and axum)
    for (key, value) in response_headers.iter() {
        if key == "transfer-encoding" || key == "connection" || key == "content-length" {
            // Skip these - we're streaming so content-length doesn't apply
            continue;
        }
        builder = builder.header(key, value);
    }

    builder
        .body(body)
        .map_err(|e| ProxyError::ResponseBuild(e.to_string()))
}

/// Handle non-streaming responses (JSON) - buffer and forward
async fn handle_buffered_response(ctx: ResponseContext) -> Result<Response<Body>, ProxyError> {
    let ResponseContext {
        response,
        status,
        headers: response_headers,
        start,
        ttfb,
        request_id,
        is_messages_endpoint,
        state,
        user_id,
        translation_ctx,
    } = ctx;
    // Read full response body
    let response_body = response
        .bytes()
        .await
        .map_err(|e| ProxyError::BodyRead(e.to_string()))?;

    let duration = start.elapsed();

    // Try to parse response body for display
    let parsed_response_body = if is_messages_endpoint && status.is_success() {
        match serde_json::from_slice::<serde_json::Value>(&response_body) {
            Ok(json) => Some(json),
            Err(e) => {
                let body_preview =
                    String::from_utf8_lossy(&response_body[..response_body.len().min(200)]);
                tracing::warn!(
                    "Failed to parse response JSON: {} | Preview: {}",
                    e,
                    body_preview
                );
                let body_str = std::str::from_utf8(&response_body).unwrap_or("");
                if body_str.contains("event:") {
                    sse::assemble_to_json(body_str)
                } else {
                    None
                }
            }
        }
    } else {
        tracing::error!(
            "Skipping response body parse: is_messages_endpoint={}, status={}",
            is_messages_endpoint,
            status
        );
        None
    };

    // Emit response event
    state
        .send_event(
            ProxyEvent::Response {
                request_id: request_id.clone(),
                timestamp: Utc::now(),
                status: status.as_u16(),
                body_size: response_body.len(),
                ttfb,
                duration,
                body: parsed_response_body,
            },
            user_id.as_deref(),
        )
        .await;

    // Apply response translation FIRST (so parser sees Anthropic format)
    let final_response_body = if translation_ctx.needs_response_translation() && status.is_success()
    {
        // Get response translator (backend_format → client_format)
        if let Some(translator) = state.translation.get_response_translator(
            translation_ctx.backend_format,
            translation_ctx.client_format,
        ) {
            match translator.translate_buffered(&response_body, &translation_ctx) {
                Ok(translated) => {
                    tracing::debug!(
                        "Response translated: {} -> {} ({} -> {} bytes)",
                        translation_ctx.backend_format,
                        translation_ctx.client_format,
                        response_body.len(),
                        translated.len()
                    );
                    translated.into()
                }
                Err(e) => {
                    tracing::warn!("Response translation failed, returning original: {}", e);
                    response_body
                }
            }
        } else {
            response_body
        }
    } else {
        response_body
    };

    // Parse response for tool calls, assistant content, usage (uses translated body for correct format)
    if is_messages_endpoint && status.is_success() {
        if let Ok(events) = state
            .parser
            .parse_response(&final_response_body, user_id.as_deref())
            .await
        {
            for event in events {
                // Update context state when we see ApiUsage (skip Haiku utility calls)
                if let ProxyEvent::ApiUsage {
                    input_tokens,
                    cache_creation_tokens,
                    cache_read_tokens,
                    model,
                    ..
                } = &event
                {
                    if !model.to_lowercase().contains("haiku") {
                        if let Ok(mut ctx) = state.context_state.lock() {
                            ctx.update(
                                *input_tokens as u64,
                                *cache_creation_tokens as u64,
                                *cache_read_tokens as u64,
                            );
                        }
                    }
                }
                // Reset warnings on context compact
                if let ProxyEvent::ContextCompact { .. } = &event {
                    if let Ok(mut ctx) = state.context_state.lock() {
                        ctx.reset_warnings();
                    }
                }
                state.send_event(event, user_id.as_deref()).await;
            }
        }
    }

    // Build response to return to client
    let mut builder = Response::builder().status(status.as_u16());

    for (key, value) in response_headers.iter() {
        if key == "transfer-encoding" || key == "connection" {
            continue;
        }
        // Update content-length if translation occurred
        if key == "content-length" && translation_ctx.needs_response_translation() {
            continue; // Will be set automatically from body
        }
        builder = builder.header(key, value);
    }

    // Update content-type for translated responses
    if translation_ctx.needs_response_translation() {
        builder = builder.header("content-type", "application/json");
    }

    builder
        .body(Body::from(final_response_body))
        .map_err(|e| ProxyError::ResponseBuild(e.to_string()))
}

/// Merge request and response headers into combined struct
fn merge_headers(mut req: CapturedHeaders, resp: CapturedHeaders) -> CapturedHeaders {
    req.request_id = resp.request_id;
    req.organization_id = resp.organization_id;
    req.requests_limit = resp.requests_limit;
    req.requests_remaining = resp.requests_remaining;
    req.requests_reset = resp.requests_reset;
    req.tokens_limit = resp.tokens_limit;
    req.tokens_remaining = resp.tokens_remaining;
    req.tokens_reset = resp.tokens_reset;
    req
}

/// Check if a header is an authentication header that should be stripped
/// when transforming auth for a different provider
fn is_auth_header(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower == "authorization" || lower == "x-api-key"
}

/// Check if a header is Anthropic-specific and should be stripped
/// when forwarding to non-Anthropic providers (e.g., OpenRouter)
fn is_anthropic_header(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("anthropic-") || lower == "x-stainless-lang" || lower == "x-stainless-arch"
}

/// Extract user ID (api_key_hash) from request headers
/// Used early in the handler to associate events with sessions
fn extract_user_id(headers: &axum::http::HeaderMap) -> Option<String> {
    // Hash API key or OAuth token for user identity
    // Note: Hook script can override this by setting user_id in /api/session/start
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

/// Extract request headers into CapturedHeaders struct
fn extract_request_headers(headers: &axum::http::HeaderMap) -> CapturedHeaders {
    let mut captured = CapturedHeaders::new();

    if let Some(version) = headers.get("anthropic-version") {
        captured.anthropic_version = version.to_str().ok().map(String::from);
    }

    if let Some(beta) = headers.get("anthropic-beta") {
        if let Ok(beta_str) = beta.to_str() {
            captured.anthropic_beta = beta_str.split(',').map(|s| s.trim().to_string()).collect();
        }
    }

    // Hash the API key for tracking (never log the actual key!)
    // Check x-api-key first, then Authorization: Bearer (for OAuth users)
    let key_to_hash = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            // OAuth: Authorization: Bearer <token>
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .filter(|s| s.starts_with("Bearer "))
                .map(|s| s[7..].to_string()) // Strip "Bearer " prefix
        });

    if let Some(key) = key_to_hash {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let hash = hasher.finalize();
        captured.api_key_hash = Some(format!("{:x}", hash)[..16].to_string());
    }

    captured
}

/// Extract response headers into CapturedHeaders struct
fn extract_response_headers(headers: &reqwest::header::HeaderMap) -> CapturedHeaders {
    let mut captured = CapturedHeaders::new();

    if let Some(request_id) = headers.get("request-id") {
        captured.request_id = request_id.to_str().ok().map(String::from);
    }

    if let Some(org_id) = headers.get("anthropic-organization-id") {
        captured.organization_id = org_id.to_str().ok().map(String::from);
    }

    // Rate limit headers
    if let Some(val) = headers.get("anthropic-ratelimit-requests-limit") {
        captured.requests_limit = val.to_str().ok().and_then(|s| s.parse().ok());
    }
    if let Some(val) = headers.get("anthropic-ratelimit-requests-remaining") {
        captured.requests_remaining = val.to_str().ok().and_then(|s| s.parse().ok());
    }
    if let Some(val) = headers.get("anthropic-ratelimit-requests-reset") {
        captured.requests_reset = val.to_str().ok().map(String::from);
    }
    if let Some(val) = headers.get("anthropic-ratelimit-tokens-limit") {
        captured.tokens_limit = val.to_str().ok().and_then(|s| s.parse().ok());
    }
    if let Some(val) = headers.get("anthropic-ratelimit-tokens-remaining") {
        captured.tokens_remaining = val.to_str().ok().and_then(|s| s.parse().ok());
    }
    if let Some(val) = headers.get("anthropic-ratelimit-tokens-reset") {
        captured.tokens_reset = val.to_str().ok().map(String::from);
    }

    captured
}

/// Errors that can occur during proxying
#[derive(Debug)]
enum ProxyError {
    BodyRead(String),
    Upstream(String),
    ResponseBuild(String),
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response<Body> {
        let (status, message) = match self {
            ProxyError::BodyRead(msg) => (StatusCode::BAD_REQUEST, msg),
            ProxyError::Upstream(msg) => (StatusCode::BAD_GATEWAY, msg),
            ProxyError::ResponseBuild(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        tracing::error!("Proxy error: {} - {}", status, message);

        Response::builder()
            .status(status)
            .body(Body::from(message))
            .unwrap_or_else(|_| Response::new(Body::from("Internal error building error response")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ClientConfig, ProviderConfig};
    use std::collections::HashMap;

    fn make_test_clients() -> ClientsConfig {
        let mut clients = HashMap::new();
        clients.insert(
            "dev-1".to_string(),
            ClientConfig {
                name: "Dev Laptop".to_string(),
                provider: "anthropic".to_string(),
                tags: vec!["dev".to_string()],
                auth: None,
            },
        );
        clients.insert(
            "ci".to_string(),
            ClientConfig {
                name: "CI Runner".to_string(),
                provider: "foundry".to_string(),
                tags: vec![],
                auth: None,
            },
        );

        let mut providers = HashMap::new();
        providers.insert(
            "anthropic".to_string(),
            ProviderConfig {
                base_url: "https://api.anthropic.com".to_string(),
                name: Some("Anthropic Direct".to_string()),
                api_format: crate::config::ApiFormat::Anthropic,
                auth: None,
            },
        );
        providers.insert(
            "foundry".to_string(),
            ProviderConfig {
                base_url: "https://foundry.example.com".to_string(),
                name: Some("Foundry".to_string()),
                api_format: crate::config::ApiFormat::Anthropic,
                auth: None,
            },
        );

        ClientsConfig { clients, providers }
    }

    #[test]
    fn test_client_routing_with_known_client() {
        let clients = make_test_clients();
        let default_url = "https://default.example.com";

        let routing = extract_client_routing("/dev-1/v1/messages", &clients, default_url);

        assert_eq!(routing.client_id, Some("dev-1".to_string()));
        assert_eq!(routing.base_url, "https://api.anthropic.com");
        assert_eq!(routing.api_path, "/v1/messages");
    }

    #[test]
    fn test_client_routing_with_different_client() {
        let clients = make_test_clients();
        let default_url = "https://default.example.com";

        let routing = extract_client_routing("/ci/v1/messages", &clients, default_url);

        assert_eq!(routing.client_id, Some("ci".to_string()));
        assert_eq!(routing.base_url, "https://foundry.example.com");
        assert_eq!(routing.api_path, "/v1/messages");
    }

    #[test]
    fn test_client_routing_unknown_client_uses_default() {
        let clients = make_test_clients();
        let default_url = "https://default.example.com";

        // Unknown client ID should fall back to default
        let routing = extract_client_routing("/unknown/v1/messages", &clients, default_url);

        assert_eq!(routing.client_id, None);
        assert_eq!(routing.base_url, default_url);
        assert_eq!(routing.api_path, "/unknown/v1/messages");
    }

    #[test]
    fn test_client_routing_no_client_prefix() {
        let clients = make_test_clients();
        let default_url = "https://default.example.com";

        // Standard path without client prefix
        let routing = extract_client_routing("/v1/messages", &clients, default_url);

        assert_eq!(routing.client_id, None);
        assert_eq!(routing.base_url, default_url);
        assert_eq!(routing.api_path, "/v1/messages");
    }

    #[test]
    fn test_client_routing_empty_clients_config() {
        let clients = ClientsConfig::default();
        let default_url = "https://default.example.com";

        // With no clients configured, always use default
        let routing = extract_client_routing("/dev-1/v1/messages", &clients, default_url);

        assert_eq!(routing.client_id, None);
        assert_eq!(routing.base_url, default_url);
        assert_eq!(routing.api_path, "/dev-1/v1/messages");
    }

    #[test]
    fn test_client_routing_preserves_deep_paths() {
        let clients = make_test_clients();
        let default_url = "https://default.example.com";

        let routing =
            extract_client_routing("/dev-1/v1/messages/count_tokens", &clients, default_url);

        assert_eq!(routing.client_id, Some("dev-1".to_string()));
        assert_eq!(routing.api_path, "/v1/messages/count_tokens");
    }
}
