// Configuration for the proxy server
//
// Configuration is loaded in order of precedence:
// 1. Environment variables (highest priority)
// 2. Config file (~/.config/aspy/config.toml)
// 3. Built-in defaults (lowest priority)

use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;

/// Version info
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Feature flags for optional modules (opt-out: default enabled)
#[derive(Debug, Clone)]
pub struct Features {
    /// Storage module: write events to JSONL files
    pub storage: bool,

    /// Thinking panel: show Claude's extended thinking
    pub thinking_panel: bool,

    /// Stats tracking: token counts, costs, tool distribution
    pub stats: bool,
}

impl Default for Features {
    fn default() -> Self {
        Self {
            storage: true,
            thinking_panel: true,
            stats: true,
        }
    }
}

/// Augmentation settings
///
/// Augmentations modify API responses by injecting additional content.
/// Context warning is enabled by default as it's non-intrusive and helpful.
#[derive(Debug, Clone)]
pub struct Augmentation {
    /// Context warning: inject usage alerts when context fills up
    /// Adds styled annotations suggesting /compact when thresholds are crossed
    pub context_warning: bool,

    /// Thresholds at which to warn (percentages)
    /// Default: [60, 80, 85, 90, 95]
    pub context_warning_thresholds: Vec<u8>,
}

impl Default for Augmentation {
    fn default() -> Self {
        Self {
            context_warning: true, // Enabled by default
            context_warning_thresholds: vec![60, 80, 85, 90, 95],
        }
    }
}

/// Log file rotation strategy
#[derive(Debug, Clone, Default, PartialEq)]
pub enum LogRotation {
    /// Rotate log files hourly
    Hourly,
    /// Rotate log files daily (default)
    #[default]
    Daily,
    /// Never rotate - single log file
    Never,
}

impl LogRotation {
    /// Parse rotation string from config
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "hourly" => Self::Hourly,
            "daily" => Self::Daily,
            "never" => Self::Never,
            _ => Self::Daily, // Default to daily for unknown values
        }
    }

    /// Convert to string for TOML serialization
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Never => "never",
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    /// Log level: trace, debug, info, warn, error
    pub level: String,
    /// Enable file logging (in addition to TUI buffer or stdout)
    pub file_enabled: bool,
    /// Directory for log files
    pub file_dir: PathBuf,
    /// Log file rotation strategy
    pub file_rotation: LogRotation,
    /// Prefix for log file names (e.g., "aspy" -> "aspy.2024-01-15.log")
    pub file_prefix: String,
}

/// Embedding configuration for semantic search
#[derive(Debug, Clone)]
pub struct EmbeddingsConfig {
    /// Provider type: "none", "local", "remote"
    pub provider: String,
    /// Model name (e.g., "all-MiniLM-L6-v2", "text-embedding-3-small")
    pub model: String,
    /// API base URL for remote providers
    pub api_base: Option<String>,
    /// API version query parameter (e.g., "preview" for Azure AI Foundry)
    pub api_version: Option<String>,
    /// Authentication method: "bearer" or "api-key"
    pub auth_method: String,
    /// API key for remote providers (resolved from env vars or config)
    pub api_key: Option<String>,
    /// Polling interval for background indexer (seconds)
    pub poll_interval_secs: u64,
    /// Batch size for embedding requests
    pub batch_size: usize,
    /// Delay between batches (milliseconds)
    pub batch_delay_ms: u64,
    /// Maximum content length to embed (characters)
    pub max_content_length: usize,
}

impl Default for EmbeddingsConfig {
    fn default() -> Self {
        Self {
            provider: "none".to_string(),
            model: String::new(),
            api_base: None,
            api_version: None,
            auth_method: "bearer".to_string(),
            api_key: None,
            poll_interval_secs: 30,
            batch_size: 32,
            batch_delay_ms: 100,
            max_content_length: 8000,
        }
    }
}

impl EmbeddingsConfig {
    /// Check if embeddings are enabled
    pub fn is_enabled(&self) -> bool {
        self.provider != "none" && !self.provider.is_empty()
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            file_enabled: false, // Opt-in feature
            file_dir: PathBuf::from("./logs/trace"),
            file_rotation: LogRotation::Daily,
            file_prefix: "aspy".to_string(),
        }
    }
}

/// API translation settings
///
/// Enables bidirectional translation between OpenAI and Anthropic API formats.
/// When enabled, the proxy can accept OpenAI-formatted requests, translate them
/// to Anthropic format, and translate responses back to OpenAI format.
#[derive(Debug, Clone)]
pub struct Translation {
    /// Whether API translation is enabled
    pub enabled: bool,

    /// Auto-detect format from path/headers/body (recommended)
    pub auto_detect: bool,

    /// Model name mappings (OpenAI model -> Anthropic model)
    pub model_mapping: HashMap<String, String>,
}

impl Default for Translation {
    fn default() -> Self {
        Self {
            enabled: false, // Opt-in feature
            auto_detect: true,
            model_mapping: HashMap::new(), // Use built-in defaults
        }
    }
}

/// Request transformation settings
///
/// Transformers modify API requests before they are forwarded to the provider.
/// Used for editing system-reminders, injecting context, translating formats, etc.
#[derive(Debug, Clone, Default)]
pub struct Transformers {
    /// Whether transformation is enabled globally (master kill-switch)
    /// When false, no transformers run regardless of their individual configs.
    /// Set to true to enable transformation pipeline.
    pub enabled: bool,

    /// Tag editor configuration (operates on configurable XML-style tags)
    pub tag_editor: Option<crate::proxy::transformation::TagEditorConfig>,

    /// Compact enhancer configuration (enhances compaction prompts with session context)
    pub compact_enhancer: Option<crate::proxy::transformation::CompactEnhancerConfig>,
}

/// Lifetime statistics storage configuration
#[derive(Debug, Clone)]
pub struct LifestatsConfig {
    /// Whether lifestats storage is enabled
    pub enabled: bool,
    /// Path to SQLite database file
    pub db_path: PathBuf,
    /// Whether to store thinking blocks (can be large)
    pub store_thinking: bool,
    /// Whether to store full tool inputs/outputs
    pub store_tool_io: bool,
    /// Maximum thinking block size to store (bytes)
    pub max_thinking_size: usize,
    /// Retention period in days (0 = forever)
    pub retention_days: u32,
    /// Channel buffer size (backpressure threshold)
    pub channel_buffer: usize,
    /// Batch size before flush
    pub batch_size: usize,
    /// Maximum time before flush (seconds)
    pub flush_interval_secs: u64,
}

impl Default for LifestatsConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Opt-in feature
            db_path: PathBuf::from("./data/lifestats.db"),
            store_thinking: true,
            store_tool_io: true,
            max_thinking_size: 100_000, // ~100KB per thinking block
            retention_days: 90,
            channel_buffer: 10_000, // Buffer before backpressure
            batch_size: 100,        // Flush every 100 events
            flush_interval_secs: 1, // Or every 1 second
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Client and Provider Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Client profile for multi-user differentiation
///
/// Clients are identified by a path prefix in the URL:
///   http://localhost:8080/{client_id}/v1/messages
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

    /// Authentication configuration for this provider
    /// If not specified, uses passthrough (client's auth headers forwarded)
    #[serde(default)]
    pub auth: Option<ProviderAuth>,
}

impl ProviderConfig {
    /// Get display name (falls back to base_url host)
    #[allow(dead_code)] // Reserved for TUI display
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.base_url)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Provider Authentication Configuration
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
}

/// Application configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Address to bind the proxy server to
    pub bind_addr: SocketAddr,

    /// Target Anthropic API URL
    pub api_url: String,

    /// Directory for storing logs
    pub log_dir: PathBuf,

    /// Whether to enable the TUI (can be disabled for headless mode)
    pub enable_tui: bool,

    /// Demo mode: generate mock events for showcasing the TUI
    pub demo_mode: bool,

    /// Context window limit for the gauge (empirically ~147K triggers compact)
    pub context_limit: u64,

    /// Theme name: "basic", "terminal", "dracula", "monokai", "nord", "gruvbox"
    pub theme: String,

    /// Use theme's background color (true) or terminal's default (false)
    pub use_theme_background: bool,

    /// Layout preset name: "classic", "reasoning", "debug"
    pub preset: String,

    /// Feature flags for optional modules
    pub features: Features,

    /// Augmentation settings (opt-in response modifications)
    pub augmentation: Augmentation,

    /// Logging configuration
    pub logging: LoggingConfig,

    /// Lifetime statistics storage configuration
    pub lifestats: LifestatsConfig,

    /// Embeddings configuration for semantic search
    pub embeddings: EmbeddingsConfig,

    /// API translation settings (OpenAI ↔ Anthropic)
    pub translation: Translation,

    /// Request transformation settings
    pub transformers: Transformers,
    /// Client and provider configuration for multi-user routing
    pub clients: ClientsConfig,
}

/// Feature flags as loaded from config file
#[derive(Debug, Deserialize, Default)]
struct FileFeatures {
    storage: Option<bool>,
    thinking_panel: Option<bool>,
    stats: Option<bool>,
}

/// Augmentation settings as loaded from config file
#[derive(Debug, Deserialize, Default)]
struct FileAugmentation {
    context_warning: Option<bool>,
    context_warning_thresholds: Option<Vec<u8>>,
}

/// Logging settings as loaded from config file
#[derive(Debug, Deserialize, Default)]
struct FileLogging {
    level: Option<String>,
    file_enabled: Option<bool>,
    file_dir: Option<String>,
    file_rotation: Option<String>,
    file_prefix: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct FileLifestatsConfig {
    enabled: Option<bool>,
    db_path: Option<String>,
    store_thinking: Option<bool>,
    store_tool_io: Option<bool>,
    max_thinking_size: Option<usize>,
    retention_days: Option<u32>,
    channel_buffer: Option<usize>,
    batch_size: Option<usize>,
    flush_interval_secs: Option<u64>,
}

/// Embeddings config as loaded from file
#[derive(Debug, Deserialize, Default)]
struct FileEmbeddingsConfig {
    provider: Option<String>,
    model: Option<String>,
    api_base: Option<String>,
    api_version: Option<String>,
    auth_method: Option<String>,
    /// API key from config file (env vars take precedence)
    api_key: Option<String>,
    poll_interval_secs: Option<u64>,
    batch_size: Option<usize>,
    batch_delay_ms: Option<u64>,
    max_content_length: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
struct FileTranslation {
    enabled: Option<bool>,
    auto_detect: Option<bool>,
    #[serde(default)]
    model_mapping: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Default)]
struct FileTransformers {
    enabled: Option<bool>,
    #[serde(rename = "tag-editor")]
    tag_editor: Option<crate::proxy::transformation::TagEditorConfig>,
    #[serde(rename = "compact-enhancer")]
    compact_enhancer: Option<crate::proxy::transformation::CompactEnhancerConfig>,
}
/// Config file structure (subset of Config that makes sense to persist)
#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    context_limit: Option<u64>,
    bind_addr: Option<String>,
    api_url: Option<String>,
    log_dir: Option<String>,
    theme: Option<String>,
    use_theme_background: Option<bool>,
    preset: Option<String>,

    /// Optional [features] section
    features: Option<FileFeatures>,

    /// Optional [augmentation] section
    augmentation: Option<FileAugmentation>,

    /// Optional [logging] section
    logging: Option<FileLogging>,

    /// Optional [lifestats] section
    lifestats: Option<FileLifestatsConfig>,

    /// Optional [embeddings] section
    embeddings: Option<FileEmbeddingsConfig>,

    /// Optional [translation] section
    translation: Option<FileTranslation>,

    /// Optional [transformers] section
    transformers: Option<FileTransformers>,
    /// Optional [clients.X] sections for multi-user routing
    #[serde(default)]
    clients: HashMap<String, ClientConfig>,

    /// Optional [providers.X] sections for backend configuration
    #[serde(default)]
    providers: HashMap<String, ProviderConfig>,
}

impl Config {
    /// Get the config file path: ~/.config/aspy/config.toml
    /// Uses Unix-style ~/.config on all platforms for consistency
    pub fn config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|p| p.join(".config").join("aspy").join("config.toml"))
    }

    /// Create config file with defaults if it doesn't exist
    /// Called during startup to help users discover configuration options
    pub fn ensure_config_exists() {
        let Some(path) = Self::config_path() else {
            return;
        };

        // Don't overwrite existing config
        if path.exists() {
            return;
        }

        // Create parent directory
        if let Some(parent) = path.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return; // Silently fail - config is optional
            }
        }

        // Use Config::default().to_toml() as single source of truth
        let template = Self::default().to_toml();

        // Write config (ignore errors - config is optional)
        let _ = std::fs::write(&path, template);
    }

    /// Load file config if it exists
    ///
    /// # Panics
    /// If config file exists but cannot be parsed. This is intentional -
    /// a broken config should fail fast with a clear error, not silently
    /// fall back to defaults while the user debugs the wrong thing.
    fn load_file_config() -> FileConfig {
        let Some(path) = Self::config_path() else {
            return FileConfig::default();
        };

        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                match toml::from_str(&contents) {
                    Ok(config) => config,
                    Err(e) => {
                        // Fatal error - config exists but is invalid
                        // Print a clear, actionable error message
                        eprintln!(
                            "\n╔══════════════════════════════════════════════════════════════╗"
                        );
                        eprintln!(
                            "║  CONFIG ERROR - Failed to parse configuration file          ║"
                        );
                        eprintln!(
                            "╚══════════════════════════════════════════════════════════════╝\n"
                        );
                        eprintln!("  File: {}\n", path.display());
                        eprintln!("  Error: {}\n", e);
                        eprintln!("  Tip: Run `aspy config --show` to validate your config");
                        eprintln!("       Or delete the file to regenerate defaults\n");
                        std::process::exit(1);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File doesn't exist - that's fine, use defaults
                FileConfig::default()
            }
            Err(e) => {
                // File exists but can't be read (permissions, etc.)
                eprintln!("\n╔══════════════════════════════════════════════════════════════╗");
                eprintln!("║  CONFIG ERROR - Cannot read configuration file              ║");
                eprintln!("╚══════════════════════════════════════════════════════════════╝\n");
                eprintln!("  File: {}\n", path.display());
                eprintln!("  Error: {}\n", e);
                std::process::exit(1);
            }
        }
    }

    /// Serialize clients HashMap to TOML sections
    fn clients_to_toml(&self) -> String {
        if self.clients.clients.is_empty() {
            // Show example comments when no clients configured
            return r#"
# [clients.dev-1]
# name = "Dev Laptop"
# provider = "anthropic"       # References [providers.anthropic] below
"#
            .to_string();
        }

        let mut output = String::from("\n");
        // Sort keys for deterministic output
        let mut keys: Vec<_> = self.clients.clients.keys().collect();
        keys.sort();

        for client_id in keys {
            let client = &self.clients.clients[client_id];
            output.push_str(&format!("[clients.{}]\n", client_id));
            output.push_str(&format!("name = \"{}\"\n", client.name));
            output.push_str(&format!("provider = \"{}\"\n", client.provider));
            if !client.tags.is_empty() {
                output.push_str(&format!("tags = {:?}\n", client.tags));
            }
            output.push('\n');
        }
        output
    }

    /// Serialize providers HashMap to TOML sections
    fn providers_to_toml(&self) -> String {
        if self.clients.providers.is_empty() {
            // Show example comments when no providers configured
            return r#"
# [providers.anthropic]
# base_url = "https://api.anthropic.com"
#
# # Provider with OpenAI-compatible API (e.g., OpenRouter)
# [providers.openrouter]
# base_url = "https://openrouter.ai/api"
# api_format = "openai"  # Translate Anthropic <-> OpenAI format
# [providers.openrouter.auth]
# method = "bearer"
# key_env = "OPENROUTER_API_KEY"
# strip_incoming = true
"#
            .to_string();
        }

        let mut output = String::from("\n");
        // Sort keys for deterministic output
        let mut keys: Vec<_> = self.clients.providers.keys().collect();
        keys.sort();

        for provider_id in keys {
            let provider = &self.clients.providers[provider_id];
            output.push_str(&format!("[providers.{}]\n", provider_id));
            output.push_str(&format!("base_url = \"{}\"\n", provider.base_url));
            if let Some(name) = &provider.name {
                output.push_str(&format!("name = \"{}\"\n", name));
            }

            // Serialize api_format if not default (anthropic)
            if provider.api_format != ApiFormat::Anthropic {
                output.push_str(&format!(
                    "api_format = \"{}\"\n",
                    provider.api_format.as_str()
                ));
            }

            // Serialize auth config if present
            if let Some(auth) = &provider.auth {
                output.push_str(&format!("\n[providers.{}.auth]\n", provider_id));
                output.push_str(&format!("method = \"{}\"\n", auth.method.as_str()));
                if let Some(key_env) = &auth.key_env {
                    output.push_str(&format!("key_env = \"{}\"\n", key_env));
                }
                if let Some(key) = &auth.key {
                    output.push_str(&format!("key = \"{}\"\n", key));
                }
                if let Some(header_name) = &auth.header_name {
                    output.push_str(&format!("header_name = \"{}\"\n", header_name));
                }
                if let Some(strip) = auth.strip_incoming {
                    output.push_str(&format!("strip_incoming = {}\n", strip));
                }
            }

            output.push('\n');
        }
        output
    }

    /// Serialize transformers config to TOML (returns empty string if not configured)
    fn transformers_to_toml(&self) -> String {
        // If no tag editor configured, return empty (comments in template suffice)
        let Some(ref editor) = self.transformers.tag_editor else {
            return String::new();
        };

        if !editor.enabled || editor.rules.is_empty() {
            return String::new();
        }

        let mut output = String::from("\n[transformers.tag-editor]\nenabled = true\n");

        use crate::proxy::transformation::{PositionConfig, RuleConfig};

        for rule in &editor.rules {
            output.push_str("\n[[transformers.tag-editor.rules]]\n");
            match rule {
                RuleConfig::Inject {
                    tag,
                    content,
                    position,
                    when,
                } => {
                    output.push_str("type = \"inject\"\n");
                    output.push_str(&format!("tag = \"{}\"\n", tag));
                    // Escape content for TOML multiline if needed
                    if content.contains('\n') {
                        output.push_str(&format!("content = \"\"\"\n{}\n\"\"\"\n", content));
                    } else {
                        output.push_str(&format!("content = \"{}\"\n", content));
                    }
                    match position {
                        PositionConfig::Start => {
                            output.push_str("position = \"start\"\n");
                        }
                        PositionConfig::End => {
                            // end is default, can omit
                        }
                        PositionConfig::Before { pattern } => {
                            output
                                .push_str(&format!("position.before.pattern = \"{}\"\n", pattern));
                        }
                        PositionConfig::After { pattern } => {
                            output.push_str(&format!("position.after.pattern = \"{}\"\n", pattern));
                        }
                    }
                    // Output when condition using dotted keys (valid TOML for array elements)
                    if let Some(cond) = when {
                        if let Some(ref tn) = cond.turn_number {
                            output.push_str(&format!("when.turn_number = \"{}\"\n", tn));
                        }
                        if let Some(ref tr) = cond.has_tool_results {
                            output.push_str(&format!("when.has_tool_results = \"{}\"\n", tr));
                        }
                        if let Some(ref ci) = cond.client_id {
                            output.push_str(&format!("when.client_id = \"{}\"\n", ci));
                        }
                    }
                }
                RuleConfig::Remove { tag, pattern, when } => {
                    output.push_str("type = \"remove\"\n");
                    output.push_str(&format!("tag = \"{}\"\n", tag));
                    output.push_str(&format!("pattern = \"{}\"\n", pattern));
                    if let Some(cond) = when {
                        if let Some(ref tn) = cond.turn_number {
                            output.push_str(&format!("when.turn_number = \"{}\"\n", tn));
                        }
                        if let Some(ref tr) = cond.has_tool_results {
                            output.push_str(&format!("when.has_tool_results = \"{}\"\n", tr));
                        }
                        if let Some(ref ci) = cond.client_id {
                            output.push_str(&format!("when.client_id = \"{}\"\n", ci));
                        }
                    }
                }
                RuleConfig::Replace {
                    tag,
                    pattern,
                    replacement,
                    when,
                } => {
                    output.push_str("type = \"replace\"\n");
                    output.push_str(&format!("tag = \"{}\"\n", tag));
                    output.push_str(&format!("pattern = \"{}\"\n", pattern));
                    output.push_str(&format!("replacement = \"{}\"\n", replacement));
                    if let Some(cond) = when {
                        if let Some(ref tn) = cond.turn_number {
                            output.push_str(&format!("when.turn_number = \"{}\"\n", tn));
                        }
                        if let Some(ref tr) = cond.has_tool_results {
                            output.push_str(&format!("when.has_tool_results = \"{}\"\n", tr));
                        }
                        if let Some(ref ci) = cond.client_id {
                            output.push_str(&format!("when.client_id = \"{}\"\n", ci));
                        }
                    }
                }
            }
        }

        output
    }

    /// Serialize config to TOML string (single source of truth for format)
    pub fn to_toml(&self) -> String {
        format!(
            r#"# aspy configuration

# Theme: Spy Dark, Spy Light, One Half Dark, Dracula, Nord, Gruvbox Dark, Monokai Pro, etc.
# See full list in the theme selector (press 't' in the TUI)
theme = "{theme}"

# Use theme's background color (true) or terminal's default (false)
use_theme_background = {use_bg}

# Layout preset: classic, reasoning, debug
preset = "{preset}"

# Context window limit for the gauge
context_limit = {limit}

# Proxy bind address
bind_addr = "{bind}"

# Log directory for session files
log_dir = "{log_dir}"

# Feature flags
[features]
storage = {storage}
thinking_panel = {thinking}
stats = {stats}

# Augmentation (response modifications)
[augmentation]
context_warning = {ctx_warn}
context_warning_thresholds = {thresholds:?}

# Logging configuration (RUST_LOG env var overrides)
[logging]
level = "{log_level}"
# File logging (in addition to TUI buffer or stdout)
file_enabled = {log_file_enabled}
file_dir = "{log_file_dir}"
file_rotation = "{log_file_rotation}"  # hourly, daily, never
file_prefix = "{log_file_prefix}"

# Lifetime statistics storage (SQLite-backed context recovery)
[lifestats]
enabled = {lifestats_enabled}
db_path = "{lifestats_db_path}"
store_thinking = {lifestats_store_thinking}
store_tool_io = {lifestats_store_tool_io}
max_thinking_size = {lifestats_max_thinking_size}
retention_days = {lifestats_retention_days}
channel_buffer = {lifestats_channel_buffer}
batch_size = {lifestats_batch_size}
flush_interval_secs = {lifestats_flush_interval_secs}

# ─────────────────────────────────────────────────────────────────────────────
# SEMANTIC SEARCH EMBEDDINGS (Optional)
# ─────────────────────────────────────────────────────────────────────────────
# Enable vector embeddings for semantic search alongside FTS5 keyword search.
# API keys should be set via environment variables (OPENAI_API_KEY, etc.)
#
# Provider options: "none" (default), "local", "remote"
# - none: FTS5 keyword search only (no embeddings)
# - local: ONNX models via fastembed (requires --features local-embeddings)
# - remote: OpenAI-compatible API (OpenAI, Azure, OpenRouter)
[embeddings]
provider = "{embed_provider}"
model = "{embed_model}"
{embed_api_base}{embed_auth_method}poll_interval_secs = {embed_poll_interval}
batch_size = {embed_batch_size}
batch_delay_ms = {embed_batch_delay}
max_content_length = {embed_max_content}

# ─────────────────────────────────────────────────────────────────────────────
# API TRANSLATION (Optional - OpenAI ↔ Anthropic)
# ─────────────────────────────────────────────────────────────────────────────
# Enable bidirectional translation between OpenAI and Anthropic API formats.
# When enabled, the proxy can accept OpenAI-formatted requests (/v1/chat/completions),
# translate them to Anthropic format, and translate responses back.
#
# Use case: Run OpenAI-compatible tools through Anthropic's API.

[translation]
enabled = {translation_enabled}
auto_detect = {translation_auto_detect}
{translation_model_mapping}
# ─────────────────────────────────────────────────────────────────────────────
# REQUEST TRANSFORMERS (Optional)
# ─────────────────────────────────────────────────────────────────────────────
# Modify API requests before they are forwarded to the provider.
# Use for editing <system-reminder> tags, injecting context, etc.
#
# IMPORTANT: Set enabled = true to activate transformers.

[transformers]
enabled = {transformers_enabled}

# System Reminder Editor - modify <system-reminder> tags in user messages
# Rules are applied in order. Rule types:
#   inject  - Add new <system-reminder> content (position: start, end, before, after)
#   remove  - Remove reminders matching a regex pattern
#   replace - Replace content within matching reminders
#
# Example: Inject a custom context reminder
# [transformers.tag-editor]
# enabled = true
# [[transformers.tag-editor.rules]]
# type = "inject"
# content = "Always respond in formal English."
# position = "end"
#
# Example: Remove noisy debug reminders
# [[transformers.tag-editor.rules]]
# type = "remove"
# pattern = "debug|noisy"  # Regex: removes reminders containing "debug" or "noisy"
{transformers_section}
# ─────────────────────────────────────────────────────────────────────────────
# MULTI-CLIENT ROUTING (Optional)
# ─────────────────────────────────────────────────────────────────────────────
# Track multiple Claude Code instances through a single proxy using named clients.
# Each client connects via URL path: http://localhost:8080/<client-id>
#
# Example: ANTHROPIC_BASE_URL=http://127.0.0.1:8080/dev-1 claude
{clients_section}
# ─────────────────────────────────────────────────────────────────────────────
# PROVIDER BACKENDS (Optional)
# ─────────────────────────────────────────────────────────────────────────────
# Define where to forward API requests. Clients reference these by name.
{providers_section}
"#,
            theme = self.theme,
            use_bg = self.use_theme_background,
            preset = self.preset,
            limit = self.context_limit,
            bind = self.bind_addr,
            log_dir = self.log_dir.display(),
            storage = self.features.storage,
            thinking = self.features.thinking_panel,
            stats = self.features.stats,
            ctx_warn = self.augmentation.context_warning,
            thresholds = self.augmentation.context_warning_thresholds,
            log_level = self.logging.level,
            log_file_enabled = self.logging.file_enabled,
            log_file_dir = self.logging.file_dir.display(),
            log_file_rotation = self.logging.file_rotation.as_str(),
            log_file_prefix = self.logging.file_prefix,
            lifestats_enabled = self.lifestats.enabled,
            lifestats_db_path = self.lifestats.db_path.display(),
            lifestats_store_thinking = self.lifestats.store_thinking,
            lifestats_store_tool_io = self.lifestats.store_tool_io,
            lifestats_max_thinking_size = self.lifestats.max_thinking_size,
            lifestats_retention_days = self.lifestats.retention_days,
            lifestats_channel_buffer = self.lifestats.channel_buffer,
            lifestats_batch_size = self.lifestats.batch_size,
            lifestats_flush_interval_secs = self.lifestats.flush_interval_secs,
            translation_enabled = self.translation.enabled,
            translation_auto_detect = self.translation.auto_detect,
            translation_model_mapping = if self.translation.model_mapping.is_empty() {
                r#"
# Model mappings (source model -> target model)
# Uncomment and customize as needed. Built-in defaults handle common models.
# [translation.model_mapping]
# "gpt-4" = "claude-sonnet-4-20250514"
# "gpt-3.5-turbo" = "claude-3-haiku-20240307"
"#
                .to_string()
            } else {
                let mut mappings = String::from("\n[translation.model_mapping]\n");
                let mut keys: Vec<_> = self.translation.model_mapping.keys().collect();
                keys.sort();
                for key in keys {
                    let value = &self.translation.model_mapping[key];
                    mappings.push_str(&format!("\"{}\" = \"{}\"\n", key, value));
                }
                mappings
            },
            embed_provider = self.embeddings.provider,
            embed_model = self.embeddings.model,
            embed_api_base = self
                .embeddings
                .api_base
                .as_ref()
                .map(|url| format!("api_base = \"{}\"\n", url))
                .unwrap_or_else(|| {
                    "# api_base = \"https://api.openai.com/v1\"  # For remote provider\n"
                        .to_string()
                }),
            embed_auth_method = if self.embeddings.auth_method != "bearer" {
                format!("auth_method = \"{}\"\n", self.embeddings.auth_method)
            } else {
                "# auth_method = \"bearer\"  # \"bearer\" or \"api-key\" (Azure)\n".to_string()
            },
            embed_poll_interval = self.embeddings.poll_interval_secs,
            embed_batch_size = self.embeddings.batch_size,
            embed_batch_delay = self.embeddings.batch_delay_ms,
            embed_max_content = self.embeddings.max_content_length,
            transformers_enabled = self.transformers.enabled,
            transformers_section = self.transformers_to_toml(),
            clients_section = self.clients_to_toml(),
            providers_section = self.providers_to_toml(),
        )
    }

    /// Save current configuration to file
    pub fn save(&self) -> Result<(), std::io::Error> {
        let Some(path) = Self::config_path() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not determine config path",
            ));
        };

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&path, self.to_toml())
    }

    /// Load configuration: file -> env vars -> defaults
    pub fn from_env() -> Self {
        let file = Self::load_file_config();

        // Bind address: env > file > default
        let bind_addr = std::env::var("ASPY_BIND")
            .ok()
            .or(file.bind_addr)
            .unwrap_or_else(|| "127.0.0.1:8080".to_string())
            .parse()
            .expect("Invalid bind address");

        // API URL: env > file > default
        let api_url = std::env::var("ANTHROPIC_API_URL")
            .ok()
            .or(file.api_url)
            .unwrap_or_else(|| "https://api.anthropic.com".to_string());

        // Log directory: env > file > default
        let log_dir = std::env::var("ASPY_LOG_DIR")
            .ok()
            .or(file.log_dir)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("./logs"));

        // TUI toggle: env only (runtime flag)
        let enable_tui = std::env::var("ASPY_NO_TUI")
            .map(|v| v != "1" && v.to_lowercase() != "true")
            .unwrap_or(true);

        // Demo mode: env only (runtime flag)
        let demo_mode = std::env::var("ASPY_DEMO")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        // Context limit: env > file > default (147K based on empirical data)
        let context_limit = std::env::var("ASPY_CONTEXT_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .or(file.context_limit)
            .unwrap_or(147_000);

        // Theme: env > file > default ("Spy Dark" is the project's signature theme)
        let theme = std::env::var("ASPY_THEME")
            .ok()
            .or(file.theme)
            .unwrap_or_else(|| "Spy Dark".to_string());

        // Use theme background: file > default (true = use theme's bg color)
        let use_theme_background = file.use_theme_background.unwrap_or(true);

        // Preset: file > default ("classic")
        let preset = file.preset.unwrap_or_else(|| "classic".to_string());

        // Feature flags: file config only (env vars would be verbose)
        // Default: enabled (opt-out pattern)
        let file_features = file.features.unwrap_or_default();
        let features = Features {
            storage: file_features.storage.unwrap_or(true),
            thinking_panel: file_features.thinking_panel.unwrap_or(true),
            stats: file_features.stats.unwrap_or(true),
        };

        // Augmentation settings: file config only
        // Default: enabled (context warning is helpful and non-intrusive)
        let file_augmentation = file.augmentation.unwrap_or_default();
        let augmentation = Augmentation {
            context_warning: file_augmentation.context_warning.unwrap_or(true),
            context_warning_thresholds: file_augmentation
                .context_warning_thresholds
                .unwrap_or_else(|| vec![60, 80, 85, 90, 95]),
        };

        // Logging settings: file config only (RUST_LOG env var handled in main.rs)
        let file_logging = file.logging.unwrap_or_default();
        let log_defaults = LoggingConfig::default();
        let logging = LoggingConfig {
            level: file_logging.level.unwrap_or(log_defaults.level),
            file_enabled: file_logging
                .file_enabled
                .unwrap_or(log_defaults.file_enabled),
            file_dir: file_logging
                .file_dir
                .map(PathBuf::from)
                .unwrap_or(log_defaults.file_dir),
            file_rotation: file_logging
                .file_rotation
                .map(|s| LogRotation::from_str(&s))
                .unwrap_or(log_defaults.file_rotation),
            file_prefix: file_logging.file_prefix.unwrap_or(log_defaults.file_prefix),
        };

        // Lifestats settings: file config only
        let file_lifestats = file.lifestats.unwrap_or_default();
        let defaults = LifestatsConfig::default();
        let lifestats = LifestatsConfig {
            enabled: file_lifestats.enabled.unwrap_or(defaults.enabled),
            db_path: file_lifestats
                .db_path
                .map(PathBuf::from)
                .unwrap_or(defaults.db_path),
            store_thinking: file_lifestats
                .store_thinking
                .unwrap_or(defaults.store_thinking),
            store_tool_io: file_lifestats
                .store_tool_io
                .unwrap_or(defaults.store_tool_io),
            max_thinking_size: file_lifestats
                .max_thinking_size
                .unwrap_or(defaults.max_thinking_size),
            retention_days: file_lifestats
                .retention_days
                .unwrap_or(defaults.retention_days),
            channel_buffer: file_lifestats
                .channel_buffer
                .unwrap_or(defaults.channel_buffer),
            batch_size: file_lifestats.batch_size.unwrap_or(defaults.batch_size),
            flush_interval_secs: file_lifestats
                .flush_interval_secs
                .unwrap_or(defaults.flush_interval_secs),
        };

        // Embeddings settings: file config + env var for API key
        // API key precedence: ASPY_EMBEDDINGS_API_KEY env var > config file
        let file_embeddings = file.embeddings.unwrap_or_default();
        let embed_defaults = EmbeddingsConfig::default();
        let embeddings_api_key = std::env::var("ASPY_EMBEDDINGS_API_KEY")
            .ok()
            .or(file_embeddings.api_key.clone());
        let embeddings = EmbeddingsConfig {
            provider: file_embeddings.provider.unwrap_or(embed_defaults.provider),
            model: file_embeddings.model.unwrap_or(embed_defaults.model),
            api_base: file_embeddings.api_base.or(embed_defaults.api_base),
            api_version: file_embeddings.api_version.or(embed_defaults.api_version),
            auth_method: file_embeddings
                .auth_method
                .unwrap_or(embed_defaults.auth_method),
            api_key: embeddings_api_key,
            poll_interval_secs: file_embeddings
                .poll_interval_secs
                .unwrap_or(embed_defaults.poll_interval_secs),
            batch_size: file_embeddings
                .batch_size
                .unwrap_or(embed_defaults.batch_size),
            batch_delay_ms: file_embeddings
                .batch_delay_ms
                .unwrap_or(embed_defaults.batch_delay_ms),
            max_content_length: file_embeddings
                .max_content_length
                .unwrap_or(embed_defaults.max_content_length),
        };

        // Translation settings: file config only
        let file_translation = file.translation.unwrap_or_default();
        let translation_defaults = Translation::default();
        let translation = Translation {
            enabled: file_translation
                .enabled
                .unwrap_or(translation_defaults.enabled),
            auto_detect: file_translation
                .auto_detect
                .unwrap_or(translation_defaults.auto_detect),
            model_mapping: if file_translation.model_mapping.is_empty() {
                translation_defaults.model_mapping
            } else {
                file_translation.model_mapping
            },
        };

        // Transformers settings: file config only
        let file_transformers = file.transformers.unwrap_or_default();
        let transformers = Transformers {
            enabled: file_transformers.enabled.unwrap_or(false),
            tag_editor: file_transformers.tag_editor,
            compact_enhancer: file_transformers.compact_enhancer,
        };

        // Client/provider config: file only
        let clients = ClientsConfig {
            clients: file.clients,
            providers: file.providers,
        };

        // Log client config if present
        if clients.is_configured() {
            eprintln!(
                "Loaded {} client(s) and {} provider(s) from config",
                clients.clients.len(),
                clients.providers.len()
            );
        }

        Self {
            bind_addr,
            api_url,
            log_dir,
            enable_tui,
            demo_mode,
            context_limit,
            theme,
            use_theme_background,
            preset,
            features,
            augmentation,
            logging,
            lifestats,
            embeddings,
            translation,
            transformers,
            clients,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8080".parse().unwrap(),
            api_url: "https://api.anthropic.com".to_string(),
            log_dir: PathBuf::from("./logs"),
            enable_tui: true,
            demo_mode: false,
            context_limit: 147_000,
            theme: "Spy Dark".to_string(),
            use_theme_background: true,
            preset: "classic".to_string(),
            features: Features::default(),
            augmentation: Augmentation::default(),
            logging: LoggingConfig::default(),
            lifestats: LifestatsConfig::default(),
            embeddings: EmbeddingsConfig::default(),
            translation: Translation::default(),
            transformers: Transformers::default(),
            clients: ClientsConfig::default(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature Definitions for StartupRegistry
// ─────────────────────────────────────────────────────────────────────────────
// This is the SINGLE SOURCE OF TRUTH for what features exist.
// Adding a new feature? Add it here, and it shows up in startup automatically.

impl Config {
    /// Get all feature definitions based on current configuration.
    ///
    /// This is the single source of truth for what features exist in Aspy.
    /// The StartupRegistry uses this to build the startup display.
    /// main.rs can update statuses based on actual initialization results.
    pub fn feature_definitions(&self) -> Vec<crate::startup::FeatureDefinition> {
        use crate::startup::{FeatureCategory, FeatureDefinition};

        let mut features = vec![
            // ─────────────────────────────────────────────────────────────────
            // Core (always enabled)
            // ─────────────────────────────────────────────────────────────────
            FeatureDefinition::core("proxy", "proxy", "HTTP interception"),
            FeatureDefinition::core("parser", "parser", "Event extraction"),
            // ─────────────────────────────────────────────────────────────────
            // Interface
            // ─────────────────────────────────────────────────────────────────
            FeatureDefinition::optional(
                "tui",
                "tui",
                FeatureCategory::Interface,
                self.enable_tui,
                "Terminal interface",
            ),
            // ─────────────────────────────────────────────────────────────────
            // Storage
            // ─────────────────────────────────────────────────────────────────
            FeatureDefinition::optional(
                "storage",
                "storage",
                FeatureCategory::Storage,
                self.features.storage,
                "JSONL logging",
            ),
            FeatureDefinition::optional(
                "lifestats",
                "lifestats",
                FeatureCategory::Storage,
                self.lifestats.enabled,
                "SQLite history",
            ),
            // ─────────────────────────────────────────────────────────────────
            // Pipeline
            // ─────────────────────────────────────────────────────────────────
            FeatureDefinition::optional(
                "thinking",
                "thinking",
                FeatureCategory::Pipeline,
                self.features.thinking_panel && self.enable_tui,
                "Thinking panel",
            ),
            FeatureDefinition::optional(
                "stats",
                "stats",
                FeatureCategory::Pipeline,
                self.features.stats,
                "Token tracking",
            ),
            FeatureDefinition::optional(
                "ctx-warn",
                "ctx-warn",
                FeatureCategory::Pipeline,
                self.augmentation.context_warning,
                "Context warnings",
            ),
        ];

        // Embeddings: configurable (needs setup, not just enable/disable)
        let embeddings_def = if self.embeddings.is_enabled() {
            FeatureDefinition::configurable(
                "embeddings",
                "embeddings",
                FeatureCategory::Pipeline,
                true,
                "Semantic search",
            )
            .with_detail(format!(
                "{}: {}",
                self.embeddings.provider, self.embeddings.model
            ))
        } else {
            FeatureDefinition::configurable(
                "embeddings",
                "embeddings",
                FeatureCategory::Pipeline,
                false,
                "Semantic search",
            )
        };
        features.push(embeddings_def);

        // Translation: optional (API format conversion)
        features.push(FeatureDefinition::optional(
            "translation",
            "translation",
            FeatureCategory::Pipeline,
            self.translation.enabled,
            "API translation",
        ));

        // Transformation: optional (request modification before forwarding)
        // Shows as active when enabled=true AND has configured rules
        let transform_active = self.transformers.enabled
            && self
                .transformers
                .tag_editor
                .as_ref()
                .map(|c| c.enabled)
                .unwrap_or(false);
        features.push(FeatureDefinition::optional(
            "transformers",
            "transformers",
            FeatureCategory::Pipeline,
            transform_active,
            "Request editing",
        ));

        // Routing: configurable (needs client definitions)
        features.push(FeatureDefinition::configurable(
            "routing",
            "routing",
            FeatureCategory::Routing,
            self.clients.is_configured(),
            "Multi-client",
        ));

        features
    }
}
