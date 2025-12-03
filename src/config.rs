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

/// Logging configuration
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    /// Log level: trace, debug, info, warn, error
    pub level: String,
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
}

impl ProviderConfig {
    /// Get display name (falls back to base_url host)
    #[allow(dead_code)] // Reserved for TUI display
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.base_url)
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
    fn load_file_config() -> FileConfig {
        let Some(path) = Self::config_path() else {
            return FileConfig::default();
        };

        match std::fs::read_to_string(&path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_else(|e| {
                eprintln!("Warning: Failed to parse {}: {}", path.display(), e);
                FileConfig::default()
            }),
            Err(_) => FileConfig::default(), // File doesn't exist, use defaults
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
            output.push('\n');
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
# api_base = "https://api.openai.com/v1"  # For remote provider
# auth_method = "bearer"                   # "bearer" or "api-key" (Azure)
poll_interval_secs = {embed_poll_interval}
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

# Model mappings (OpenAI model -> Anthropic model)
# Uncomment and customize as needed. Built-in defaults handle common models.
# [translation.model_mapping]
# "gpt-4" = "claude-sonnet-4-20250514"
# "gpt-4-turbo" = "claude-sonnet-4-20250514"
# "gpt-3.5-turbo" = "claude-3-haiku-20240307"
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
            embed_provider = self.embeddings.provider,
            embed_model = self.embeddings.model,
            embed_poll_interval = self.embeddings.poll_interval_secs,
            embed_batch_size = self.embeddings.batch_size,
            embed_batch_delay = self.embeddings.batch_delay_ms,
            embed_max_content = self.embeddings.max_content_length,
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
        let logging = LoggingConfig {
            level: file_logging.level.unwrap_or_else(|| "info".to_string()),
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
        let file_embeddings = file.embeddings.unwrap_or_default();
        let embed_defaults = EmbeddingsConfig::default();
        let embeddings = EmbeddingsConfig {
            provider: file_embeddings.provider.unwrap_or(embed_defaults.provider),
            model: file_embeddings.model.unwrap_or(embed_defaults.model),
            api_base: file_embeddings.api_base.or(embed_defaults.api_base),
            api_version: file_embeddings.api_version.or(embed_defaults.api_version),
            auth_method: file_embeddings
                .auth_method
                .unwrap_or(embed_defaults.auth_method),
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
