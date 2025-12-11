//! Client and provider routing configuration
//!
//! This module handles multi-client routing, provider backends,
//! and authentication transformation.

use serde::Deserialize;
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// API Format
// ─────────────────────────────────────────────────────────────────────────────

/// API format expected by a provider backend
///
/// Different providers use different API formats:
/// - Anthropic: `/v1/messages` with Anthropic request/response schema
/// - OpenAI: `/v1/chat/completions` with OpenAI request/response schema
///
/// When a provider expects a different format than the client sends,
/// the proxy will automatically translate requests and responses.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApiFormat {
    /// Anthropic format: /v1/messages (default, no translation needed for Claude Code)
    #[default]
    Anthropic,
    /// OpenAI format: /v1/chat/completions (used by OpenRouter, OpenAI, etc.)
    Openai,
}

impl ApiFormat {
    /// Convert to string for TOML serialization
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::Openai => "openai",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Count Tokens Handling
// ─────────────────────────────────────────────────────────────────────────────

/// How to handle count_tokens requests for a provider
///
/// Claude Code aggressively calls `/v1/messages/count_tokens` at startup.
/// Different providers need different handling:
/// - Anthropic: supports count_tokens natively, pass through
/// - OpenAI-compatible: no count_tokens endpoint, return synthetic response
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CountTokensHandling {
    /// Forward to provider as-is (default for Anthropic providers)
    #[default]
    Passthrough,
    /// Return synthetic response immediately (default for OpenAI providers)
    Synthetic,
    /// Use deduplication and rate limiting (legacy behavior)
    Dedupe,
}

impl CountTokensHandling {
    /// Convert to string for TOML serialization
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Passthrough => "passthrough",
            Self::Synthetic => "synthetic",
            Self::Dedupe => "dedupe",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Authentication
// ─────────────────────────────────────────────────────────────────────────────

/// Authentication method for provider APIs
///
/// Different API providers use different authentication schemes:
/// - Anthropic: `x-api-key` header
/// - OpenRouter/OpenAI: `Authorization: Bearer` header
/// - Some services: Custom headers or basic auth
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    /// Pass through client's auth headers unchanged (default for backward compatibility)
    #[default]
    Passthrough,
    /// OAuth-style: `Authorization: Bearer {key}`
    Bearer,
    /// Anthropic-style: `x-api-key: {key}`
    XApiKey,
    /// HTTP Basic: `Authorization: Basic {base64(user:pass)}`
    Basic,
    /// Custom header: `{header_name}: {key}`
    Header,
}

impl AuthMethod {
    /// Convert to lowercase string for TOML serialization
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Passthrough => "passthrough",
            Self::Bearer => "bearer",
            Self::XApiKey => "x_api_key",
            Self::Basic => "basic",
            Self::Header => "header",
        }
    }
}

/// Provider authentication configuration
///
/// Defines how to authenticate requests to a provider backend.
/// Keys can be sourced from environment variables (preferred) or config.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProviderAuth {
    /// Authentication method (passthrough, bearer, x-api-key, basic, header)
    #[serde(default)]
    pub method: AuthMethod,

    /// API key value (direct, less secure - prefer key_env)
    pub key: Option<String>,

    /// Environment variable name to read key from (preferred)
    pub key_env: Option<String>,

    /// Custom header name (only used when method = "header")
    pub header_name: Option<String>,

    /// Whether to strip incoming auth headers before forwarding
    /// Default: true for bearer/x-api-key/basic/header, false for passthrough
    pub strip_incoming: Option<bool>,
}

impl ProviderAuth {
    /// Check if this is a passthrough config (no auth transformation)
    ///
    /// Reserved for future use: will enable conditional logging of auth mode
    /// in TUI status bar and startup messages.
    #[allow(dead_code)]
    pub fn is_passthrough(&self) -> bool {
        self.method == AuthMethod::Passthrough
    }

    /// Get whether to strip incoming auth headers
    /// Defaults based on method: passthrough=false, others=true
    pub fn should_strip_incoming(&self) -> bool {
        self.strip_incoming.unwrap_or_else(|| {
            // Passthrough keeps client auth, others strip by default
            self.method != AuthMethod::Passthrough
        })
    }

    /// Resolve the API key from env var or direct value
    /// Returns None if no key configured or passthrough mode
    pub fn resolve_key(&self) -> Option<String> {
        if self.method == AuthMethod::Passthrough {
            return None;
        }

        // Priority: env var > direct value
        if let Some(env_name) = &self.key_env {
            if let Ok(value) = std::env::var(env_name) {
                if !value.is_empty() {
                    return Some(value);
                }
            }
        }

        self.key.clone()
    }

    /// Build the authentication header (name, value) for this config
    /// Returns None if passthrough or no key available
    pub fn build_header(&self) -> Option<(String, String)> {
        let key = self.resolve_key()?;

        match &self.method {
            AuthMethod::Passthrough => None,
            AuthMethod::Bearer => Some(("authorization".to_string(), format!("Bearer {}", key))),
            AuthMethod::XApiKey => Some(("x-api-key".to_string(), key)),
            AuthMethod::Basic => {
                // For basic auth, key should be pre-encoded base64 string
                // (i.e., base64("user:pass") - user provides the encoded value)
                Some(("authorization".to_string(), format!("Basic {}", key)))
            }
            AuthMethod::Header => {
                let header_name = self
                    .header_name
                    .clone()
                    .unwrap_or_else(|| "x-api-key".to_string());
                Some((header_name.to_lowercase(), key))
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Provider Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Provider backend configuration
///
/// Defines where to forward API requests for a given provider.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    /// Base URL for the provider's API (e.g., "https://api.anthropic.com")
    pub base_url: String,

    /// Optional display name
    #[allow(dead_code)] // Reserved for TUI display
    pub name: Option<String>,

    /// API format expected by this provider (anthropic or openai)
    /// Default: anthropic (no translation needed for Claude Code clients)
    /// Set to "openai" for OpenRouter, OpenAI, and other OpenAI-compatible APIs
    #[serde(default)]
    pub api_format: ApiFormat,

    /// Custom API path for this provider (e.g., "/chat/completions" without /v1 prefix)
    ///
    /// When set, this path is appended to base_url instead of the default paths:
    /// - Anthropic default: `/v1/messages`
    /// - OpenAI default: `/v1/chat/completions`
    ///
    /// Use this for providers with non-standard paths like:
    /// - z.ai: base_url="https://api.z.ai/api/coding/paas/v4", api_path="/chat/completions"
    /// - OpenRouter: base_url="https://openrouter.ai/api/v1", api_path="/chat/completions"
    #[serde(default)]
    pub api_path: Option<String>,

    /// Authentication configuration for this provider
    /// If not specified, uses passthrough (client's auth headers forwarded)
    #[serde(default)]
    pub auth: Option<ProviderAuth>,

    /// How to handle count_tokens requests for this provider
    ///
    /// If not specified, defaults based on api_format:
    /// - Anthropic: passthrough (forward to provider)
    /// - OpenAI: synthetic (return synthetic response, endpoint doesn't exist)
    #[serde(default)]
    pub count_tokens: Option<CountTokensHandling>,

    /// Model name mappings for this provider (Anthropic pattern → target model)
    ///
    /// When set, these mappings take precedence over global `[translation.model_mapping]`.
    /// Supports partial matching: "haiku" matches "claude-haiku-4-5-20251001".
    ///
    /// Example:
    /// ```toml
    /// [providers.openrouter.model_mapping]
    /// "haiku" = "anthropic/claude-3-haiku"
    /// "sonnet" = "anthropic/claude-sonnet-4"
    /// ```
    #[serde(default)]
    pub model_mapping: HashMap<String, String>,
}

impl ProviderConfig {
    /// Get display name (falls back to base_url host)
    #[allow(dead_code)] // Reserved for TUI display
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.base_url)
    }

    /// Get the effective API path for this provider
    ///
    /// If `api_path` is set, returns that. Otherwise returns the default
    /// path for the provider's api_format:
    /// - Anthropic: `/v1/messages`
    /// - OpenAI: `/v1/chat/completions`
    pub fn effective_api_path(&self) -> &str {
        if let Some(ref path) = self.api_path {
            path.as_str()
        } else {
            match self.api_format {
                ApiFormat::Anthropic => "/v1/messages",
                ApiFormat::Openai => "/v1/chat/completions",
            }
        }
    }

    /// Get the effective count_tokens handling for this provider
    ///
    /// If `count_tokens` is set, returns that. Otherwise returns the default
    /// based on api_format:
    /// - Anthropic: passthrough (endpoint exists, forward as-is)
    /// - OpenAI: synthetic (endpoint doesn't exist, return synthetic response)
    pub fn effective_count_tokens(&self) -> CountTokensHandling {
        self.count_tokens.clone().unwrap_or(match self.api_format {
            ApiFormat::Anthropic => CountTokensHandling::Passthrough,
            ApiFormat::Openai => CountTokensHandling::Synthetic,
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Client Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Named client configuration for multi-user/multi-instance routing
///
/// Each client maps to a provider backend and has optional metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct ClientConfig {
    /// Human-readable name for display in TUI
    pub name: String,

    /// Provider backend to route requests to (references [providers.X])
    pub provider: String,

    /// Optional tags for filtering/grouping
    #[allow(dead_code)] // Reserved for TUI filtering/display
    #[serde(default)]
    pub tags: Vec<String>,

    /// Optional authentication override (takes precedence over provider's auth)
    /// Use this for multi-tenant scenarios where clients need different credentials
    #[serde(default)]
    pub auth: Option<ProviderAuth>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Clients Container
// ─────────────────────────────────────────────────────────────────────────────

/// Container for all client configurations
#[derive(Debug, Clone, Default)]
pub struct ClientsConfig {
    /// Map of client_id -> ClientConfig
    pub clients: HashMap<String, ClientConfig>,

    /// Map of provider_id -> ProviderConfig
    pub providers: HashMap<String, ProviderConfig>,
}

impl ClientsConfig {
    /// Look up a client by ID
    pub fn get_client(&self, client_id: &str) -> Option<&ClientConfig> {
        self.clients.get(client_id)
    }

    /// Get the provider config for a client
    pub fn get_client_provider(&self, client_id: &str) -> Option<&ProviderConfig> {
        self.get_client(client_id)
            .and_then(|c| self.providers.get(&c.provider))
    }

    /// Get the base URL for a client (for routing)
    pub fn get_client_base_url(&self, client_id: &str) -> Option<&str> {
        self.get_client_provider(client_id)
            .map(|p| p.base_url.as_str())
    }

    /// Check if a client ID is configured
    #[allow(dead_code)] // Reserved for token validation
    pub fn has_client(&self, client_id: &str) -> bool {
        self.clients.contains_key(client_id)
    }

    /// List all configured client IDs
    #[allow(dead_code)] // Reserved for API listing endpoint
    pub fn client_ids(&self) -> impl Iterator<Item = &String> {
        self.clients.keys()
    }

    /// Check if clients are configured (not empty)
    pub fn is_configured(&self) -> bool {
        !self.clients.is_empty()
    }

    /// Get the API format expected by a client's provider
    ///
    /// Returns the provider's api_format setting, or None if client not found.
    /// Default is Anthropic format (no translation needed for Claude Code).
    pub fn get_client_api_format(&self, client_id: &str) -> Option<&ApiFormat> {
        self.get_client_provider(client_id).map(|p| &p.api_format)
    }

    /// Get the effective API path for a client's provider
    ///
    /// Returns the provider's custom api_path if set, otherwise the default
    /// path for the provider's api_format. Returns None if client not found.
    pub fn get_client_api_path(&self, client_id: &str) -> Option<&str> {
        self.get_client_provider(client_id)
            .map(|p| p.effective_api_path())
    }

    /// Get the effective count_tokens handling for a client's provider
    ///
    /// Returns the provider's count_tokens setting (or its default based on api_format).
    /// Returns None if client not found.
    pub fn get_client_count_tokens(&self, client_id: &str) -> Option<CountTokensHandling> {
        self.get_client_provider(client_id)
            .map(|p| p.effective_count_tokens())
    }

    /// Get the effective authentication config for a client
    ///
    /// Resolution order:
    /// 1. Client's auth override (if specified)
    /// 2. Provider's auth config (if specified)
    /// 3. None (passthrough mode - forward client's auth headers)
    pub fn get_effective_auth(&self, client_id: &str) -> Option<&ProviderAuth> {
        let client = self.get_client(client_id)?;

        // Client override takes precedence
        if client.auth.is_some() {
            return client.auth.as_ref();
        }

        // Fall back to provider's auth config
        self.providers
            .get(&client.provider)
            .and_then(|p| p.auth.as_ref())
    }

    /// Get the model mapping for a client's provider
    ///
    /// Returns the provider's model_mapping if configured and non-empty,
    /// otherwise None (caller should fall back to global mapping).
    pub fn get_client_model_mapping(&self, client_id: &str) -> Option<&HashMap<String, String>> {
        self.get_client_provider(client_id)
            .map(|p| &p.model_mapping)
            .filter(|m| !m.is_empty())
    }
}
