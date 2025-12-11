//! Utility functions for request/response processing

use crate::parser::models::CapturedHeaders;
use sha2::{Digest, Sha256};

/// Extract user prompt from request body
///
/// Finds the last user message in the messages array and returns its content.
/// Handles both string and array (multipart) content formats.
/// Only extracts from the LAST user message - does not fall back to earlier messages.
pub(crate) fn extract_user_prompt(body: &serde_json::Value) -> Option<String> {
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

/// Check if a header is an authentication header that should be stripped
/// when transforming auth for a different provider
pub(crate) fn is_auth_header(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower == "authorization" || lower == "x-api-key"
}

/// Check if a header is Anthropic-specific and should be stripped
/// when forwarding to non-Anthropic providers (e.g., OpenRouter)
pub(crate) fn is_anthropic_header(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("anthropic-") || lower == "x-stainless-lang" || lower == "x-stainless-arch"
}

/// Extract user ID (api_key_hash) from request headers
/// Used early in the handler to associate events with sessions
pub(crate) fn extract_user_id(headers: &axum::http::HeaderMap) -> Option<String> {
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
pub(crate) fn extract_request_headers(headers: &axum::http::HeaderMap) -> CapturedHeaders {
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
pub(crate) fn extract_response_headers(headers: &reqwest::header::HeaderMap) -> CapturedHeaders {
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
