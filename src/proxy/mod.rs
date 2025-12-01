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

use crate::config::Config;
use crate::events::{generate_id, ProxyEvent};
use crate::parser::models::CapturedHeaders;
use crate::parser::Parser;
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

/// Maximum request body size (50MB) - prevents DoS via huge uploads
const MAX_REQUEST_BODY_SIZE: usize = 50 * 1024 * 1024;

/// Shared state for the proxy server
#[derive(Clone)]
pub struct ProxyState {
    /// HTTP client for forwarding requests
    client: reqwest::Client,
    /// Parser for extracting tool calls
    parser: Parser,
    /// Channel for sending events to TUI
    event_tx_tui: mpsc::Sender<ProxyEvent>,
    /// Channel for sending events to storage
    event_tx_storage: mpsc::Sender<ProxyEvent>,
    /// Target API URL
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
}

// ─────────────────────────────────────────────────────────────────────────────
// Proxy Input Types
// ─────────────────────────────────────────────────────────────────────────────

/// Event broadcast channels for TUI and storage consumers
#[derive(Clone)]
pub struct EventChannels {
    /// Channel for sending events to TUI
    pub tui: mpsc::Sender<ProxyEvent>,
    /// Channel for sending events to storage
    pub storage: mpsc::Sender<ProxyEvent>,
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
    };

    // Build the router - API endpoints + proxy handler
    let app = Router::new()
        // Stats and events endpoints
        .route("/api/stats", axum::routing::get(api::get_stats))
        .route("/api/events", axum::routing::get(api::get_events))
        .route("/api/context", axum::routing::get(api::get_context))
        // Session management endpoints
        .route("/api/sessions", axum::routing::get(api::get_sessions))
        .route(
            "/api/session/start",
            axum::routing::post(api::session_start),
        )
        .route("/api/session/end", axum::routing::post(api::session_end))
        // Log search endpoint
        .route("/api/search", axum::routing::post(api::search_logs))
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
    /// We ignore errors here to avoid blocking the proxy if a receiver is slow or closed
    async fn send_event(&self, event: ProxyEvent, user_id: Option<&str>) {
        // Send to TUI and storage channels
        let _ = self.event_tx_tui.send(event.clone()).await;
        let _ = self.event_tx_storage.send(event.clone()).await;

        // Also record to user's session if we have a user_id
        if let Some(uid) = user_id {
            if let Ok(mut sessions) = self.sessions.lock() {
                sessions.record_event(&sessions::UserId::new(uid), event);
            }
        }
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
    let is_messages_endpoint = uri.path().contains("/messages");

    // Extract user_id early for session tracking (before any events are sent)
    let user_id = extract_user_id(&headers);

    // Backfill session user_id immediately (before any events are sent)
    // This ensures events go to the hook-created session, not a new implicit one
    if let Some(ref uid) = user_id {
        if let Ok(mut sessions) = state.sessions.lock() {
            sessions.backfill_user_id(uid);
        }
    }

    tracing::debug!("Proxying {} {}", method, uri);

    // Read the request body (with size limit)
    let body_bytes = axum::body::to_bytes(req.into_body(), MAX_REQUEST_BODY_SIZE)
        .await
        .map_err(|e| ProxyError::BodyRead(format!("Failed to read request body: {}", e)))?;

    // Try to parse request body as JSON for display
    let request_body = if is_messages_endpoint {
        serde_json::from_slice::<serde_json::Value>(&body_bytes).ok()
    } else {
        None
    };

    // Emit request event
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

    // Parse request for tool results if this is a messages endpoint
    if is_messages_endpoint && method == "POST" {
        match state.parser.parse_request(&body_bytes).await {
            Ok(events) => {
                for event in events {
                    state.send_event(event, user_id.as_deref()).await;
                }
            }
            Err(e) => {
                tracing::warn!("Failed to parse request for tool results: {}", e);
            }
        }
    }

    // Build the forward URL
    let forward_url = format!("{}{}", state.api_url, uri.path());
    let query = uri.query().unwrap_or("");
    let forward_url = if query.is_empty() {
        forward_url
    } else {
        format!("{}?{}", forward_url, query)
    };

    // INTERCEPTOR: Request-side injection disabled - using SSE response injection instead
    // SSE injection is more reliable (no prompt engineering) and appears at end of response
    // See handle_streaming_response() for the active injection point
    let final_body = body_bytes.to_vec();

    // Build the forwarded request
    // With reqwest 0.12, Method types align with axum (both use http 1.0 crate)
    let mut forward_req = state.client.request(method, &forward_url).body(final_body);

    // Copy relevant headers (types align between axum and reqwest 0.12)
    for (key, value) in headers.iter() {
        if key == "host" || key == "connection" || key == "transfer-encoding" {
            continue;
        }
        forward_req = forward_req.header(key, value);
    }

    // Send the request to Anthropic
    let response = forward_req
        .send()
        .await
        .map_err(|e| ProxyError::Upstream(e.to_string()))?;

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
    };

    // Decide: streaming (SSE) or buffered (JSON) response handling
    if sse::is_sse_response(&ctx.headers) && ctx.status.is_success() {
        tracing::debug!("Handling SSE streaming response");
        handle_streaming_response(ctx).await
    } else {
        tracing::debug!("Handling buffered (non-streaming) response");
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
    } = ctx;
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
    let sessions = state.sessions.clone();
    let user_id_clone = user_id.clone();

    // Spawn task to stream response while accumulating
    tokio::spawn(async move {
        let mut accumulated = Vec::new();
        let mut byte_stream = response.bytes_stream();
        let mut total_bytes = 0usize;
        // Buffer for incomplete SSE lines across chunks
        let mut line_buffer = String::new();
        // Track content block index for potential injection
        let mut max_block_index: u32 = 0;
        // Track if we've injected this response (only inject once)
        let mut injected = false;
        // Track model for injection filtering (skip Haiku utility calls)
        let mut response_model = String::new();

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
                                    // Clear buffer for new thinking block
                                    if let Ok(mut buf) = streaming_thinking.lock() {
                                        buf.clear();
                                    }
                                    let _ = event_tx_tui
                                        .send(ProxyEvent::ThinkingStarted {
                                            timestamp: chrono::Utc::now(),
                                        })
                                        .await;
                                }
                                // Stream thinking content in real-time
                                if let Some(thinking_text) = sse::extract_thinking_delta(line) {
                                    if let Ok(mut buf) = streaming_thinking.lock() {
                                        buf.push_str(&thinking_text);
                                    }
                                }
                                line_buffer = line_buffer[newline_pos + 1..].to_string();
                            }
                        }

                        // Check if this chunk contains message_delta - inject before forwarding
                        // Only inject on end_turn responses (not tool_use)
                        if !injected {
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
                                            if let Some(injection) = augmentation.process(&aug_ctx)
                                            {
                                                let _ = tx.send(Ok(Bytes::from(injection))).await;
                                                injected = true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Forward chunk to client immediately
                    if tx.send(Ok(chunk)).await.is_err() {
                        // Client disconnected, but continue accumulating for logging
                        tracing::debug!("Client disconnected during streaming");
                    }
                }
                Err(e) => {
                    let error_msg = format!("Error reading stream chunk: {}", e);
                    tracing::error!("{}", error_msg);

                    // Emit error event for observability (JSONL logs + TUI)
                    let _ = event_tx_tui
                        .send(ProxyEvent::Error {
                            timestamp: chrono::Utc::now(),
                            message: error_msg.clone(),
                            context: Some(format!(
                                "request_id: {}, accumulated: {} bytes",
                                request_id_clone,
                                accumulated.len()
                            )),
                        })
                        .await;
                    let _ = event_tx_storage
                        .send(ProxyEvent::Error {
                            timestamp: chrono::Utc::now(),
                            message: error_msg,
                            context: Some(format!(
                                "request_id: {}, accumulated: {} bytes",
                                request_id_clone,
                                accumulated.len()
                            )),
                        })
                        .await;
                    break;
                }
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

        // Helper to send events (includes session recording)
        let send_event = |event: ProxyEvent| {
            let tx_tui = event_tx_tui.clone();
            let tx_storage = event_tx_storage.clone();
            let sessions_ref = sessions.clone();
            let uid = user_id_clone.clone();
            async move {
                let _ = tx_tui.send(event.clone()).await;
                let _ = tx_storage.send(event.clone()).await;
                // Also record to user's session
                if let Some(ref user_id) = uid {
                    if let Ok(mut sessions) = sessions_ref.lock() {
                        sessions.record_event(&sessions::UserId::new(user_id), event);
                    }
                }
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
            if let Ok(events) = parser.parse_response(&accumulated).await {
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

        tracing::debug!(
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
    } = ctx;
    // Read full response body
    let response_body = response
        .bytes()
        .await
        .map_err(|e| ProxyError::BodyRead(e.to_string()))?;

    let duration = start.elapsed();

    // Try to parse response body for display
    let parsed_response_body = if is_messages_endpoint && status.is_success() {
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&response_body) {
            Some(json)
        } else {
            let body_str = std::str::from_utf8(&response_body).unwrap_or("");
            if body_str.contains("event:") {
                sse::assemble_to_json(body_str)
            } else {
                None
            }
        }
    } else {
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

    // Parse response for tool calls if this is a messages endpoint
    if is_messages_endpoint && status.is_success() {
        if let Ok(events) = state.parser.parse_response(&response_body).await {
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

    // Build response to return to Claude Code
    let mut builder = Response::builder().status(status.as_u16());

    for (key, value) in response_headers.iter() {
        if key == "transfer-encoding" || key == "connection" {
            continue;
        }
        builder = builder.header(key, value);
    }

    builder
        .body(Body::from(response_body))
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
