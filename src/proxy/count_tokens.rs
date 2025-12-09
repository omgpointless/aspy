//! Count tokens request deduplication and rate limiting
//!
//! Claude Code aggressively calls `/v1/messages/count_tokens` at startup,
//! which can overwhelm rate-limited backends or backends that don't support
//! this endpoint (like OpenAI-compatible APIs).
//!
//! This module provides:
//! - **Request deduplication**: Identical requests within a TTL window return cached responses
//! - **Rate limiting**: Prevents more than N requests per second from reaching the backend
//!
//! # Architecture
//!
//! ```text
//! count_tokens request arrives
//!     ↓
//! Hash request body (messages array)
//!     ↓
//! Check cache → Hit? Return cached response
//!     ↓
//! Check rate limit → Exceeded? Return last cached or synthetic response
//!     ↓
//! Forward to backend, cache response
//! ```

use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Configuration for count_tokens handling
#[derive(Debug, Clone)]
pub struct CountTokensConfig {
    /// Enable deduplication and rate limiting
    pub enabled: bool,
    /// Cache TTL in seconds (how long to reuse responses)
    pub cache_ttl_seconds: u64,
    /// Maximum requests per second to forward to backend
    pub rate_limit_per_second: f64,
}

impl Default for CountTokensConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_ttl_seconds: 10,
            rate_limit_per_second: 2.0,
        }
    }
}

/// Cached count_tokens response
#[derive(Debug, Clone)]
struct CachedResponse {
    /// The response body bytes
    body: Bytes,
    /// Status code
    status: u16,
    /// When this entry was cached
    cached_at: Instant,
}

/// Rate limiter state
struct RateLimiter {
    /// Tokens available (starts at rate_limit_per_second)
    tokens: f64,
    /// Last time tokens were replenished
    last_update: Instant,
    /// Tokens per second
    rate: f64,
}

impl RateLimiter {
    fn new(rate_per_second: f64) -> Self {
        Self {
            tokens: rate_per_second,
            last_update: Instant::now(),
            rate: rate_per_second,
        }
    }

    /// Try to consume a token, returns true if allowed
    fn try_acquire(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();

        // Replenish tokens based on time elapsed
        self.tokens = (self.tokens + elapsed * self.rate).min(self.rate);
        self.last_update = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Count tokens cache and rate limiter
pub struct CountTokensCache {
    /// Configuration
    config: CountTokensConfig,
    /// Per-user caches (keyed by user_id/api_key_hash)
    /// Each user gets their own cache to prevent cross-user leakage
    caches: Mutex<HashMap<String, UserCache>>,
}

/// Per-user cache state
struct UserCache {
    /// Cached responses keyed by request hash
    responses: HashMap<String, CachedResponse>,
    /// Rate limiter
    rate_limiter: RateLimiter,
    /// Last response (fallback when rate limited)
    last_response: Option<CachedResponse>,
}

impl UserCache {
    fn new(rate_per_second: f64) -> Self {
        Self {
            responses: HashMap::new(),
            rate_limiter: RateLimiter::new(rate_per_second),
            last_response: None,
        }
    }
}

/// Result of checking the cache
#[derive(Debug)]
pub enum CacheResult {
    /// Cache hit - return this response
    Hit { body: Bytes, status: u16 },
    /// Rate limited - return this fallback response
    RateLimited { body: Bytes, status: u16 },
    /// Cache miss, proceed with request
    Miss,
}

impl CountTokensCache {
    /// Create a new cache with the given configuration
    pub fn new(config: CountTokensConfig) -> Self {
        Self {
            config,
            caches: Mutex::new(HashMap::new()),
        }
    }

    /// Create a new cache wrapped in Arc for sharing
    pub fn new_shared(config: CountTokensConfig) -> Arc<Self> {
        Arc::new(Self::new(config))
    }

    /// Check if count_tokens handling is enabled
    ///
    /// Useful for conditional logic in proxy handlers or diagnostics.
    #[allow(dead_code)]
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Check the cache for a count_tokens request
    ///
    /// Returns:
    /// - `CacheResult::Hit` if we have a cached response
    /// - `CacheResult::RateLimited` if rate limited but have a fallback
    /// - `CacheResult::Miss` if we should forward the request
    pub fn check(&self, user_id: Option<&str>, body: &[u8]) -> CacheResult {
        if !self.config.enabled {
            return CacheResult::Miss;
        }

        let user_key = user_id.unwrap_or("anonymous").to_string();
        let request_hash = Self::hash_request(body);
        let ttl = Duration::from_secs(self.config.cache_ttl_seconds);

        let mut caches = match self.caches.lock() {
            Ok(guard) => guard,
            Err(_) => return CacheResult::Miss, // Poisoned mutex - skip caching
        };
        let user_cache = caches
            .entry(user_key)
            .or_insert_with(|| UserCache::new(self.config.rate_limit_per_second));

        // Check cache first
        if let Some(cached) = user_cache.responses.get(&request_hash) {
            if cached.cached_at.elapsed() < ttl {
                tracing::debug!(
                    hash = %request_hash,
                    age_ms = cached.cached_at.elapsed().as_millis(),
                    "count_tokens cache hit"
                );
                return CacheResult::Hit {
                    body: cached.body.clone(),
                    status: cached.status,
                };
            }
            // Expired, remove it
            user_cache.responses.remove(&request_hash);
        }

        // Check rate limit
        if !user_cache.rate_limiter.try_acquire() {
            tracing::debug!("count_tokens rate limited");
            // Return last response if available, otherwise let it through
            if let Some(ref last) = user_cache.last_response {
                return CacheResult::RateLimited {
                    body: last.body.clone(),
                    status: last.status,
                };
            }
        }

        CacheResult::Miss
    }

    /// Store a response in the cache
    pub fn store(
        &self,
        user_id: Option<&str>,
        request_body: &[u8],
        response_body: Bytes,
        status: u16,
    ) {
        if !self.config.enabled {
            return;
        }

        let user_key = user_id.unwrap_or("anonymous").to_string();
        let request_hash = Self::hash_request(request_body);

        let cached = CachedResponse {
            body: response_body,
            status,
            cached_at: Instant::now(),
        };

        let mut caches = match self.caches.lock() {
            Ok(guard) => guard,
            Err(_) => return, // Poisoned mutex - skip caching
        };
        let user_cache = caches
            .entry(user_key)
            .or_insert_with(|| UserCache::new(self.config.rate_limit_per_second));

        // Store in hash-keyed cache
        user_cache.responses.insert(request_hash, cached.clone());
        // Also update last_response for rate limit fallback
        user_cache.last_response = Some(cached);

        // Cleanup old entries (simple eviction - remove expired)
        let ttl = Duration::from_secs(self.config.cache_ttl_seconds);
        user_cache
            .responses
            .retain(|_, v| v.cached_at.elapsed() < ttl);
    }

    /// Hash request body for cache key
    fn hash_request(body: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(body);
        let result = hasher.finalize();
        // Use first 16 bytes as hex (32 chars) - enough for dedup
        result[..16].iter().map(|b| format!("{:02x}", b)).collect()
    }
}

/// Check if a path is a count_tokens endpoint
pub fn is_count_tokens_path(path: &str) -> bool {
    path.ends_with("/count_tokens")
}

/// Generate a synthetic count_tokens response
///
/// Returns a minimal valid response for providers that don't support count_tokens
/// (e.g., OpenAI-compatible APIs). The response format matches Anthropic's schema:
/// `{"input_tokens": 0}`
///
/// This is used when a provider's `count_tokens` handling is set to `Synthetic`.
pub fn synthetic_response() -> Bytes {
    Bytes::from_static(b"{\"input_tokens\":0}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_count_tokens_path() {
        assert!(is_count_tokens_path("/v1/messages/count_tokens"));
        assert!(is_count_tokens_path("/dev-1/v1/messages/count_tokens"));
        assert!(!is_count_tokens_path("/v1/messages"));
        assert!(!is_count_tokens_path("/v1/chat/completions"));
    }

    #[test]
    fn test_cache_miss_then_hit() {
        let config = CountTokensConfig {
            enabled: true,
            cache_ttl_seconds: 10,
            rate_limit_per_second: 100.0, // High limit to not interfere
        };
        let cache = CountTokensCache::new(config);

        let body = b"test request body";
        let user = Some("user1");

        // First request - miss
        assert!(matches!(cache.check(user, body), CacheResult::Miss));

        // Store response
        cache.store(user, body, Bytes::from("response"), 200);

        // Second request - hit
        match cache.check(user, body) {
            CacheResult::Hit { body, status } => {
                assert_eq!(body, Bytes::from("response"));
                assert_eq!(status, 200);
            }
            other => panic!("Expected Hit, got {:?}", other),
        }
    }

    #[test]
    fn test_rate_limiting() {
        let config = CountTokensConfig {
            enabled: true,
            cache_ttl_seconds: 10,
            rate_limit_per_second: 1.0, // Very low limit
        };
        let cache = CountTokensCache::new(config);

        let user = Some("user1");

        // Store a response first (for fallback)
        cache.store(user, b"req1", Bytes::from("resp1"), 200);

        // First request with different body - should pass (consumes token)
        assert!(matches!(cache.check(user, b"req2"), CacheResult::Miss));

        // Second request immediately - should be rate limited
        match cache.check(user, b"req3") {
            CacheResult::RateLimited { status, .. } => {
                assert_eq!(status, 200);
            }
            other => panic!("Expected RateLimited, got {:?}", other),
        }
    }

    #[test]
    fn test_per_user_isolation() {
        let config = CountTokensConfig::default();
        let cache = CountTokensCache::new(config);

        let body = b"test body";

        // Store for user1
        cache.store(Some("user1"), body, Bytes::from("user1 response"), 200);

        // user1 gets hit
        assert!(matches!(
            cache.check(Some("user1"), body),
            CacheResult::Hit { .. }
        ));

        // user2 gets miss (different user, different cache)
        assert!(matches!(
            cache.check(Some("user2"), body),
            CacheResult::Miss
        ));
    }

    #[test]
    fn test_disabled_cache() {
        let config = CountTokensConfig {
            enabled: false,
            ..Default::default()
        };
        let cache = CountTokensCache::new(config);

        cache.store(Some("user"), b"body", Bytes::from("response"), 200);
        assert!(matches!(
            cache.check(Some("user"), b"body"),
            CacheResult::Miss
        ));
    }

    #[test]
    fn test_synthetic_response_is_valid_json() {
        let response = super::synthetic_response();
        let json: serde_json::Value =
            serde_json::from_slice(&response).expect("synthetic response should be valid JSON");

        // Verify it has the expected structure
        assert_eq!(json["input_tokens"], 0);
    }
}
