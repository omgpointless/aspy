// Proxy module - HTTP server that forwards requests to Anthropic API
//
// This module implements a transparent HTTP proxy using Axum. It intercepts
// requests to the Anthropic API, forwards them unchanged, and emits events
// based on the request/response content.
//
// STREAMING: For SSE responses, we stream chunks directly to the client
// while accumulating a copy for parsing. This ensures low latency for
// Claude Code while maintaining full observability.

pub mod interceptor;

use crate::config::Config;
use crate::events::{generate_id, ProxyEvent};
use crate::parser::models::CapturedHeaders;
use crate::parser::Parser;
use crate::{SharedContextState, StreamingThinking};
use anyhow::{Context, Result};
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
    /// Shared context state for interceptor injection
    context_state: SharedContextState,
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
}

/// Start the proxy server
pub async fn start_proxy(
    config: Config,
    event_tx_tui: mpsc::Sender<ProxyEvent>,
    event_tx_storage: mpsc::Sender<ProxyEvent>,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    streaming_thinking: StreamingThinking,
    context_state: SharedContextState,
) -> Result<()> {
    let bind_addr = config.bind_addr;
    let api_url = config.api_url.clone();

    // Build the HTTP client with timeout and connection pooling
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300)) // 5 minute timeout for API calls
        .pool_max_idle_per_host(10)
        .build()
        .context("Failed to create HTTP client")?;

    let state = ProxyState {
        client,
        parser: Parser::new(),
        event_tx_tui,
        event_tx_storage,
        api_url,
        streaming_thinking,
        context_state,
    };

    // Build the router - all requests go to the proxy handler
    let app = Router::new()
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
    /// Send an event to both TUI and storage
    /// We ignore errors here to avoid blocking the proxy if a receiver is slow or closed
    async fn send_event(&self, event: ProxyEvent) {
        let _ = self.event_tx_tui.send(event.clone()).await;
        let _ = self.event_tx_storage.send(event).await;
    }
}

/// Check if response is SSE based on content-type header
fn is_sse_response(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.contains("text/event-stream"))
        .unwrap_or(false)
}

/// Parse SSE response into a JSON representation for display
fn parse_sse_to_json(body: &str) -> Option<serde_json::Value> {
    use serde_json::json;

    let mut content_blocks = Vec::new();
    let mut model = String::new();
    let mut stop_reason: Option<String> = None;
    let mut usage_data: Option<serde_json::Value> = None;

    for line in body.lines() {
        let line = line.trim();

        if line.starts_with("data:") {
            let json_str = line.strip_prefix("data:").unwrap_or("").trim();

            if json_str.is_empty() || json_str == "[DONE]" {
                continue;
            }

            if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                let event_type = data.get("type").and_then(|v| v.as_str()).unwrap_or("");

                match event_type {
                    "message_start" => {
                        if let Some(message) = data.get("message") {
                            model = message
                                .get("model")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                        }
                    }
                    "content_block_start" => {
                        if let Some(block) = data.get("content_block") {
                            content_blocks.push(block.clone());
                        }
                    }
                    "content_block_delta" => {
                        if let Some(delta) = data.get("delta") {
                            if let Some(last_block) = content_blocks.last_mut() {
                                if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                    if let Some(existing_text) = last_block.get_mut("text") {
                                        if let Some(s) = existing_text.as_str() {
                                            *existing_text = json!(format!("{}{}", s, text));
                                        }
                                    } else if let Some(obj) = last_block.as_object_mut() {
                                        obj.insert("text".to_string(), json!(text));
                                    }
                                }
                            }
                        }
                    }
                    "message_delta" => {
                        if let Some(delta) = data.get("delta") {
                            stop_reason = delta
                                .get("stop_reason")
                                .and_then(|v| v.as_str())
                                .map(String::from);
                        }
                        if let Some(usage) = data.get("usage") {
                            usage_data = Some(usage.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if !content_blocks.is_empty() || !model.is_empty() {
        Some(json!({
            "model": model,
            "content": content_blocks,
            "stop_reason": stop_reason,
            "usage": usage_data,
            "_note": "Assembled from SSE stream"
        }))
    } else {
        None
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
        .send_event(ProxyEvent::Request {
            id: request_id.clone(),
            timestamp: Utc::now(),
            method: method.to_string(),
            path: uri.path().to_string(),
            body_size: body_bytes.len(),
            body: request_body,
        })
        .await;

    // Parse request for tool results if this is a messages endpoint
    if is_messages_endpoint && method == "POST" {
        match state.parser.parse_request(&body_bytes).await {
            Ok(events) => {
                for event in events {
                    state.send_event(event).await;
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

    // INTERCEPTOR: Potentially modify request body for context warnings
    // Only applies to POST /messages requests
    let final_body = if is_messages_endpoint && method == "POST" {
        if let Some(modified) =
            interceptor::maybe_inject_context_warning(&body_bytes, &state.context_state)
        {
            tracing::info!("Injected context warning annotation");
            modified
        } else {
            body_bytes.to_vec()
        }
    } else {
        body_bytes.to_vec()
    };

    // Build the forwarded request
    // With reqwest 0.12, Method types align with axum (both use http 1.0 crate)
    let mut forward_req = state
        .client
        .request(method, &forward_url)
        .body(final_body);

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
        .send_event(ProxyEvent::HeadersCaptured {
            request_id: request_id.clone(),
            timestamp: Utc::now(),
            headers: combined_headers.clone(),
        })
        .await;

    // Emit rate limit update if available
    if combined_headers.has_rate_limits() {
        state
            .send_event(ProxyEvent::RateLimitUpdate {
                timestamp: Utc::now(),
                requests_remaining: combined_headers.requests_remaining,
                requests_limit: combined_headers.requests_limit,
                tokens_remaining: combined_headers.tokens_remaining,
                tokens_limit: combined_headers.tokens_limit,
                reset_time: combined_headers
                    .requests_reset
                    .clone()
                    .or(combined_headers.tokens_reset.clone()),
            })
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
    };

    // Decide: streaming (SSE) or buffered (JSON) response handling
    if is_sse_response(&ctx.headers) && ctx.status.is_success() {
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

    // Spawn task to stream response while accumulating
    tokio::spawn(async move {
        let mut accumulated = Vec::new();
        let mut byte_stream = response.bytes_stream();
        let mut total_bytes = 0usize;
        // Buffer for incomplete SSE lines across chunks
        let mut line_buffer = String::new();

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
                                if let Some(tool_info) = extract_tool_use_from_sse_line(line) {
                                    parser.register_pending_tool(tool_info.0, tool_info.1).await;
                                }
                                // Emit ThinkingStarted immediately for real-time feedback
                                if is_thinking_block_start(line) {
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
                                if let Some(thinking_text) = extract_thinking_delta(line) {
                                    if let Ok(mut buf) = streaming_thinking.lock() {
                                        buf.push_str(&thinking_text);
                                    }
                                }
                                line_buffer = line_buffer[newline_pos + 1..].to_string();
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
                    tracing::error!("Error reading stream chunk: {}", e);
                    break;
                }
            }
        }
        // Process any remaining data in line buffer
        if is_messages_endpoint && !line_buffer.is_empty() {
            if let Some(tool_info) = extract_tool_use_from_sse_line(line_buffer.trim()) {
                parser.register_pending_tool(tool_info.0, tool_info.1).await;
            }
        }

        // Stream complete - now parse and emit events
        let duration = start.elapsed();

        // Helper to send events
        let send_event = |event: ProxyEvent| {
            let tx_tui = event_tx_tui.clone();
            let tx_storage = event_tx_storage.clone();
            async move {
                let _ = tx_tui.send(event.clone()).await;
                let _ = tx_storage.send(event).await;
            }
        };

        // Parse accumulated response for display
        let parsed_body = if is_messages_endpoint {
            let body_str = std::str::from_utf8(&accumulated).unwrap_or("");
            parse_sse_to_json(body_str)
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
                    // Update context state when we see ApiUsage
                    if let ProxyEvent::ApiUsage {
                        input_tokens,
                        cache_read_tokens,
                        ..
                    } = &event
                    {
                        if let Ok(mut ctx) = context_state.lock() {
                            ctx.update(*input_tokens as u64, *cache_read_tokens as u64);
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
                parse_sse_to_json(body_str)
            } else {
                None
            }
        }
    } else {
        None
    };

    // Emit response event
    state
        .send_event(ProxyEvent::Response {
            request_id: request_id.clone(),
            timestamp: Utc::now(),
            status: status.as_u16(),
            body_size: response_body.len(),
            ttfb,
            duration,
            body: parsed_response_body,
        })
        .await;

    // Parse response for tool calls if this is a messages endpoint
    if is_messages_endpoint && status.is_success() {
        if let Ok(events) = state.parser.parse_response(&response_body).await {
            for event in events {
                // Update context state when we see ApiUsage
                if let ProxyEvent::ApiUsage {
                    input_tokens,
                    cache_read_tokens,
                    ..
                } = &event
                {
                    if let Ok(mut ctx) = state.context_state.lock() {
                        ctx.update(*input_tokens as u64, *cache_read_tokens as u64);
                    }
                }
                // Reset warnings on context compact
                if let ProxyEvent::ContextCompact { .. } = &event {
                    if let Ok(mut ctx) = state.context_state.lock() {
                        ctx.reset_warnings();
                    }
                }
                state.send_event(event).await;
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
    if let Some(api_key) = headers.get("x-api-key") {
        let mut hasher = Sha256::new();
        hasher.update(api_key.as_bytes());
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

/// Extract tool_use ID and name from an SSE data line if it's a content_block_start for tool_use
///
/// This is used during streaming to register tool_use IDs immediately, before the stream
/// completes. This prevents a race condition where the next request (with tool_result)
/// arrives before we've finished parsing the response.
///
/// Returns Some((id, name)) if this line starts a tool_use block, None otherwise.
fn extract_tool_use_from_sse_line(line: &str) -> Option<(String, String)> {
    // Only process "data:" lines
    let json_str = line.strip_prefix("data:")?.trim();
    if json_str.is_empty() || json_str == "[DONE]" {
        return None;
    }

    // Parse the JSON
    let data: serde_json::Value = serde_json::from_str(json_str).ok()?;

    // Check if this is a content_block_start event
    let event_type = data.get("type")?.as_str()?;
    if event_type != "content_block_start" {
        return None;
    }

    // Check if the content_block is a tool_use
    let content_block = data.get("content_block")?;
    let block_type = content_block.get("type")?.as_str()?;
    if block_type != "tool_use" {
        return None;
    }

    // Extract ID and name
    let id = content_block.get("id")?.as_str()?.to_string();
    let name = content_block.get("name")?.as_str()?.to_string();

    Some((id, name))
}

/// Check if an SSE line indicates the start of a thinking block
/// Used for real-time "Thinking..." feedback before the full block arrives
fn is_thinking_block_start(line: &str) -> bool {
    let Some(json_str) = line.strip_prefix("data:") else {
        return false;
    };
    let json_str = json_str.trim();
    if json_str.is_empty() || json_str == "[DONE]" {
        return false;
    }

    let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return false;
    };

    // Check for content_block_start with type "thinking"
    let event_type = data.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if event_type != "content_block_start" {
        return false;
    }

    data.get("content_block")
        .and_then(|b| b.get("type"))
        .and_then(|t| t.as_str())
        .map(|t| t == "thinking")
        .unwrap_or(false)
}

/// Extract thinking text from a thinking_delta SSE event
/// Returns Some(text) if this is a thinking delta, None otherwise
fn extract_thinking_delta(line: &str) -> Option<String> {
    let json_str = line.strip_prefix("data:")?.trim();
    if json_str.is_empty() || json_str == "[DONE]" {
        return None;
    }

    let data: serde_json::Value = serde_json::from_str(json_str).ok()?;

    // Check for content_block_delta event
    let event_type = data.get("type")?.as_str()?;
    if event_type != "content_block_delta" {
        return None;
    }

    // Check if delta type is thinking_delta
    let delta = data.get("delta")?;
    let delta_type = delta.get("type")?.as_str()?;
    if delta_type != "thinking_delta" {
        return None;
    }

    // Extract the thinking text
    delta.get("thinking")?.as_str().map(String::from)
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
