//! Request and response handlers for the proxy
//!
//! This module contains the main request handler (`proxy_handler`) and
//! response handlers for streaming (SSE) and buffered responses.

mod buffered;
mod request;
mod streaming;

pub use request::proxy_handler;

use crate::parser::models::CapturedHeaders;

/// Merge request and response headers into combined struct
pub(super) fn merge_headers(mut req: CapturedHeaders, resp: CapturedHeaders) -> CapturedHeaders {
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
