//! Observability configuration: logging, cortex storage, embeddings, OTel
//!
//! This module handles all observability-related configuration:
//! - Logging: level, file output, rotation
//! - Cortex: SQLite storage for session memory
//! - Embeddings: semantic search configuration
//! - OpenTelemetry: telemetry export to Azure App Insights, etc.

use serde::Deserialize;
use std::path::PathBuf;

use super::VERSION;

// ─────────────────────────────────────────────────────────────────────────────
// Log Rotation
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// Logging Configuration
// ─────────────────────────────────────────────────────────────────────────────

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

/// Logging settings as loaded from config file
#[derive(Debug, Deserialize, Default)]
pub struct FileLogging {
    pub level: Option<String>,
    pub file_enabled: Option<bool>,
    pub file_dir: Option<String>,
    pub file_rotation: Option<String>,
    pub file_prefix: Option<String>,
}

impl LoggingConfig {
    /// Create from file config with defaults
    pub fn from_file(file: Option<FileLogging>) -> Self {
        let file = file.unwrap_or_default();
        let defaults = Self::default();

        Self {
            level: file.level.unwrap_or(defaults.level),
            file_enabled: file.file_enabled.unwrap_or(defaults.file_enabled),
            file_dir: file
                .file_dir
                .map(PathBuf::from)
                .unwrap_or(defaults.file_dir),
            file_rotation: file
                .file_rotation
                .map(|s| LogRotation::from_str(&s))
                .unwrap_or(defaults.file_rotation),
            file_prefix: file.file_prefix.unwrap_or(defaults.file_prefix),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Cortex Storage Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Cortex storage configuration
#[derive(Debug, Clone)]
pub struct CortexConfig {
    /// Whether cortex storage is enabled
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

impl Default for CortexConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Opt-in feature
            db_path: PathBuf::from("./data/cortex.db"),
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

/// Cortex config as loaded from file
#[derive(Debug, Deserialize, Default)]
pub struct FileCortexConfig {
    pub enabled: Option<bool>,
    pub db_path: Option<String>,
    pub store_thinking: Option<bool>,
    pub store_tool_io: Option<bool>,
    pub max_thinking_size: Option<usize>,
    pub retention_days: Option<u32>,
    pub channel_buffer: Option<usize>,
    pub batch_size: Option<usize>,
    pub flush_interval_secs: Option<u64>,
}

impl CortexConfig {
    /// Create from file config with defaults
    pub fn from_file(file: Option<FileCortexConfig>) -> Self {
        let file = file.unwrap_or_default();
        let defaults = Self::default();

        Self {
            enabled: file.enabled.unwrap_or(defaults.enabled),
            db_path: file.db_path.map(PathBuf::from).unwrap_or(defaults.db_path),
            store_thinking: file.store_thinking.unwrap_or(defaults.store_thinking),
            store_tool_io: file.store_tool_io.unwrap_or(defaults.store_tool_io),
            max_thinking_size: file.max_thinking_size.unwrap_or(defaults.max_thinking_size),
            retention_days: file.retention_days.unwrap_or(defaults.retention_days),
            channel_buffer: file.channel_buffer.unwrap_or(defaults.channel_buffer),
            batch_size: file.batch_size.unwrap_or(defaults.batch_size),
            flush_interval_secs: file
                .flush_interval_secs
                .unwrap_or(defaults.flush_interval_secs),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Embeddings Configuration
// ─────────────────────────────────────────────────────────────────────────────

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

/// Embeddings config as loaded from file
#[derive(Debug, Deserialize, Default)]
pub struct FileEmbeddingsConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_base: Option<String>,
    pub api_version: Option<String>,
    pub auth_method: Option<String>,
    /// API key from config file (env vars take precedence)
    pub api_key: Option<String>,
    pub poll_interval_secs: Option<u64>,
    pub batch_size: Option<usize>,
    pub batch_delay_ms: Option<u64>,
    pub max_content_length: Option<usize>,
}

impl EmbeddingsConfig {
    /// Create from file config with defaults
    /// Note: api_key should be resolved separately (env var takes precedence)
    pub fn from_file(file: Option<FileEmbeddingsConfig>, api_key_override: Option<String>) -> Self {
        let file = file.unwrap_or_default();
        let defaults = Self::default();

        // API key precedence: env var override > config file > none
        let api_key = api_key_override.or(file.api_key.clone());

        Self {
            provider: file.provider.unwrap_or(defaults.provider),
            model: file.model.unwrap_or(defaults.model),
            api_base: file.api_base.or(defaults.api_base),
            api_version: file.api_version.or(defaults.api_version),
            auth_method: file.auth_method.unwrap_or(defaults.auth_method),
            api_key,
            poll_interval_secs: file
                .poll_interval_secs
                .unwrap_or(defaults.poll_interval_secs),
            batch_size: file.batch_size.unwrap_or(defaults.batch_size),
            batch_delay_ms: file.batch_delay_ms.unwrap_or(defaults.batch_delay_ms),
            max_content_length: file
                .max_content_length
                .unwrap_or(defaults.max_content_length),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// OpenTelemetry Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// OpenTelemetry export configuration
///
/// Enables exporting telemetry data (traces, metrics) to OpenTelemetry-compatible
/// backends like Azure Application Insights, Jaeger, Grafana Tempo, etc.
#[derive(Debug, Clone)]
pub struct OtelConfig {
    /// Whether OpenTelemetry export is enabled
    pub enabled: bool,
    /// Azure Application Insights connection string
    /// Format: InstrumentationKey=xxx;IngestionEndpoint=https://...
    pub connection_string: Option<String>,
    /// Service name for telemetry (defaults to "aspy")
    pub service_name: String,
    /// Service version (defaults to crate version)
    pub service_version: String,
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Opt-in feature
            connection_string: None,
            service_name: "aspy".to_string(),
            service_version: VERSION.to_string(),
        }
    }
}

impl OtelConfig {
    /// Check if OTel export is properly configured and enabled
    pub fn is_configured(&self) -> bool {
        self.enabled && self.connection_string.is_some()
    }
}

/// OpenTelemetry config as loaded from file
#[derive(Debug, Deserialize, Default)]
pub struct FileOtelConfig {
    pub enabled: Option<bool>,
    pub connection_string: Option<String>,
    pub service_name: Option<String>,
    pub service_version: Option<String>,
}

impl OtelConfig {
    /// Create from file config with defaults
    /// Note: connection_string should be resolved separately (env var takes precedence)
    pub fn from_file(
        file: Option<FileOtelConfig>,
        connection_string_override: Option<String>,
    ) -> Self {
        let file = file.unwrap_or_default();
        let defaults = Self::default();

        // Connection string precedence: env var override > config file > none
        let connection_string = connection_string_override.or(file.connection_string.clone());

        Self {
            enabled: file.enabled.unwrap_or(defaults.enabled),
            connection_string,
            service_name: file.service_name.unwrap_or(defaults.service_name),
            service_version: file.service_version.unwrap_or(defaults.service_version),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Count Tokens Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Count tokens endpoint handling configuration
///
/// Claude Code aggressively calls `/v1/messages/count_tokens` at startup,
/// which can overwhelm rate-limited backends or backends that don't support
/// this endpoint (like OpenAI-compatible APIs).
///
/// This config enables request deduplication and rate limiting.
#[derive(Debug, Clone)]
pub struct CountTokens {
    /// Enable count_tokens request caching and rate limiting
    pub enabled: bool,
    /// Cache TTL in seconds (how long to reuse responses)
    pub cache_ttl_seconds: u64,
    /// Maximum requests per second to forward to backend
    pub rate_limit_per_second: f64,
}

impl Default for CountTokens {
    fn default() -> Self {
        Self {
            enabled: true, // On by default - prevents startup spam
            cache_ttl_seconds: 10,
            rate_limit_per_second: 2.0,
        }
    }
}

/// Count tokens config as loaded from file
#[derive(Debug, Deserialize, Default)]
pub struct FileCountTokens {
    pub enabled: Option<bool>,
    pub cache_ttl_seconds: Option<u64>,
    pub rate_limit_per_second: Option<f64>,
}

impl CountTokens {
    /// Create from file config with defaults
    pub fn from_file(file: Option<FileCountTokens>) -> Self {
        let file = file.unwrap_or_default();
        let defaults = Self::default();

        Self {
            enabled: file.enabled.unwrap_or(defaults.enabled),
            cache_ttl_seconds: file.cache_ttl_seconds.unwrap_or(defaults.cache_ttl_seconds),
            rate_limit_per_second: file
                .rate_limit_per_second
                .unwrap_or(defaults.rate_limit_per_second),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Translation Configuration
// ─────────────────────────────────────────────────────────────────────────────

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
    pub model_mapping: std::collections::HashMap<String, String>,
}

impl Default for Translation {
    fn default() -> Self {
        Self {
            enabled: false, // Opt-in feature
            auto_detect: true,
            model_mapping: std::collections::HashMap::new(), // Use built-in defaults
        }
    }
}

/// Translation config as loaded from file
#[derive(Debug, Deserialize, Default)]
pub struct FileTranslation {
    pub enabled: Option<bool>,
    pub auto_detect: Option<bool>,
    #[serde(default)]
    pub model_mapping: std::collections::HashMap<String, String>,
}

impl Translation {
    /// Create from file config with defaults
    pub fn from_file(file: Option<FileTranslation>) -> Self {
        let file = file.unwrap_or_default();
        let defaults = Self::default();

        Self {
            enabled: file.enabled.unwrap_or(defaults.enabled),
            auto_detect: file.auto_detect.unwrap_or(defaults.auto_detect),
            model_mapping: if file.model_mapping.is_empty() {
                defaults.model_mapping
            } else {
                file.model_mapping
            },
        }
    }
}
