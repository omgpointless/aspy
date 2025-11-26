// Proxy module - HTTP server that forwards requests to Anthropic API
//
// This module implements a transparent HTTP proxy using Axum. It intercepts
// requests to the Anthropic API, forwards them unchanged, and emits events
// based on the request/response content.

pub mod interceptor;

use crate::config::Config;
use crate::events::{generate_id, ProxyEvent};
use crate::parser::models::CapturedHeaders;
use crate::parser::Parser;
use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::State,
    http::{Request, Response, StatusCode},
    response::IntoResponse,
    routing::any,
    Router,
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

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
}

/// Start the proxy server
pub async fn start_proxy(
    config: Config,
    event_tx_tui: mpsc::Sender<ProxyEvent>,
    event_tx_storage: mpsc::Sender<ProxyEvent>,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
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
    // When shutdown_rx receives a signal, the server will stop accepting new connections
    // and gracefully finish processing existing requests
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

/// Parse SSE response into a JSON representation for display
fn parse_sse_to_json(body: &str) -> Option<serde_json::Value> {
    use serde_json::json;

    let mut content_blocks = Vec::new();
    let mut model = String::new();
    let mut stop_reason: Option<String> = None;
    let mut usage_data: Option<serde_json::Value> = None;

    // Parse SSE line by line
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
                            // Append text deltas to the last content block
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

    // Build a JSON representation similar to non-streaming response
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

    tracing::debug!("Proxying {} {}", method, uri);

    // Read the request body
    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|e| ProxyError::BodyRead(e.to_string()))?;

    // Try to parse request body as JSON for display
    let request_body = if uri.path().contains("/messages") {
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
    if uri.path().contains("/messages") && method == "POST" {
        if let Ok(events) = state.parser.parse_request(&body_bytes).await {
            for event in events {
                state.send_event(event).await;
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

    // Build the forwarded request
    // Convert axum's Method (http v1.0) to reqwest's Method (http v0.2) via string
    let forward_method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|e| ProxyError::Upstream(format!("Invalid HTTP method: {}", e)))?;

    let mut forward_req = state
        .client
        .request(forward_method, &forward_url)
        .body(body_bytes.to_vec());

    // Copy relevant headers
    // Convert from axum's http v1.0 types to reqwest's http v0.2 types
    for (key, value) in headers.iter() {
        // Skip hop-by-hop headers
        if key == "host" || key == "connection" || key == "transfer-encoding" {
            continue;
        }
        // Convert header value from bytes slice to Vec<u8> for reqwest
        forward_req = forward_req.header(key.as_str(), value.as_bytes().to_vec());
    }

    // Send the request to Anthropic
    let response = forward_req
        .send()
        .await
        .map_err(|e| ProxyError::Upstream(e.to_string()))?;

    let status = response.status();
    let response_headers = response.headers().clone();
    let response_body = response
        .bytes()
        .await
        .map_err(|e| ProxyError::BodyRead(e.to_string()))?;

    let duration = start.elapsed();

    // Capture request and response headers
    let req_headers = extract_request_headers(&headers);
    let resp_headers = extract_response_headers(&response_headers);

    // Merge headers (request + response)
    let mut combined_headers = req_headers;
    combined_headers.request_id = resp_headers.request_id.clone();
    combined_headers.organization_id = resp_headers.organization_id.clone();
    combined_headers.requests_limit = resp_headers.requests_limit;
    combined_headers.requests_remaining = resp_headers.requests_remaining;
    combined_headers.requests_reset = resp_headers.requests_reset.clone();
    combined_headers.tokens_limit = resp_headers.tokens_limit;
    combined_headers.tokens_remaining = resp_headers.tokens_remaining;
    combined_headers.tokens_reset = resp_headers.tokens_reset.clone();

    // Try to parse response body for display
    let parsed_response_body = if uri.path().contains("/messages") && status.is_success() {
        // Try parsing as JSON first (non-streaming)
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&response_body) {
            Some(json)
        } else {
            // Try parsing as SSE (streaming)
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
            duration,
            body: parsed_response_body,
        })
        .await;

    // Emit headers captured event
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
                    .or(combined_headers.tokens_reset),
            })
            .await;
    }

    // Parse response for tool calls if this is a messages endpoint
    if uri.path().contains("/messages") && status.is_success() {
        if let Ok(events) = state.parser.parse_response(&response_body).await {
            for event in events {
                state.send_event(event).await;
            }
        }
    }

    // Build the response to return to Claude Code
    // Convert reqwest's StatusCode (http v0.2) to axum's StatusCode (http v1.0) via u16
    let mut builder = Response::builder().status(status.as_u16());

    // Copy response headers
    // Convert from reqwest's http v0.2 types to axum's http v1.0 types
    for (key, value) in response_headers.iter() {
        if key == "transfer-encoding" || key == "connection" {
            continue;
        }
        // Convert header value from bytes slice to Vec<u8> for axum
        builder = builder.header(key.as_str(), value.as_bytes().to_vec());
    }

    let response = builder
        .body(Body::from(response_body))
        .map_err(|e| ProxyError::ResponseBuild(e.to_string()))?;

    Ok(response)
}

/// Extract request headers into CapturedHeaders struct
fn extract_request_headers(headers: &axum::http::HeaderMap) -> CapturedHeaders {
    let mut captured = CapturedHeaders::new();

    // Anthropic API version
    if let Some(version) = headers.get("anthropic-version") {
        captured.anthropic_version = version.to_str().ok().map(String::from);
    }

    // Beta features (can appear multiple times or comma-separated)
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

    // Request ID for correlation
    if let Some(request_id) = headers.get("request-id") {
        captured.request_id = request_id.to_str().ok().map(String::from);
    }

    // Organization ID
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
            .unwrap()
    }
}
