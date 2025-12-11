//! Configuration for the proxy server
//!
//! Configuration is loaded in order of precedence:
//! 1. Environment variables (highest priority)
//! 2. Config file (~/.config/aspy/config.toml)
//! 3. Built-in defaults (lowest priority)

use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;

// ─────────────────────────────────────────────────────────────────────────────
// Submodules
// ─────────────────────────────────────────────────────────────────────────────

mod augmentation;
mod features;
mod observability;
mod routing;
mod serialization;
mod startup;
mod transformers;

#[cfg(test)]
mod tests;

// ─────────────────────────────────────────────────────────────────────────────
// Re-exports (maintain public API)
// ─────────────────────────────────────────────────────────────────────────────

pub use augmentation::{Augmentation, FileAugmentation};
pub use features::{Features, FileFeatures};
pub use observability::{
    CortexConfig, CountTokens, EmbeddingsConfig, FileCortexConfig, FileCountTokens,
    FileEmbeddingsConfig, FileLogging, FileOtelConfig, FileTranslation, LogRotation, LoggingConfig,
    OtelConfig, Translation,
};
// Re-export routing types for public API (some may not be directly imported,
// but are accessed through struct fields like ProviderConfig.auth)
#[allow(unused_imports)]
pub use routing::{
    ApiFormat, AuthMethod, ClientConfig, ClientsConfig, CountTokensHandling, ProviderAuth,
    ProviderConfig,
};
pub use transformers::{FileTransformers, Transformers};

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Version info
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// ─────────────────────────────────────────────────────────────────────────────
// Application Configuration
// ─────────────────────────────────────────────────────────────────────────────

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
    pub cortex: CortexConfig,

    /// Embeddings configuration for semantic search
    pub embeddings: EmbeddingsConfig,

    /// API translation settings (OpenAI ↔ Anthropic)
    pub translation: Translation,

    /// Request transformation settings
    pub transformers: Transformers,

    /// Count tokens endpoint handling (dedup + rate limit)
    pub count_tokens: CountTokens,

    /// OpenTelemetry export configuration
    pub otel: OtelConfig,

    /// Client and provider configuration for multi-user routing
    pub clients: ClientsConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8080".parse().unwrap(),
            api_url: "https://api.anthropic.com".to_string(),
            log_dir: PathBuf::from("./logs"),
            enable_tui: true,
            demo_mode: false,
            context_limit: 150_000,
            theme: "Spy Dark".to_string(),
            use_theme_background: true,
            preset: "classic".to_string(),
            features: Features::default(),
            augmentation: Augmentation::default(),
            logging: LoggingConfig::default(),
            cortex: CortexConfig::default(),
            embeddings: EmbeddingsConfig::default(),
            translation: Translation::default(),
            transformers: Transformers::default(),
            count_tokens: CountTokens::default(),
            otel: OtelConfig::default(),
            clients: ClientsConfig::default(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// File Configuration (deserialization layer)
// ─────────────────────────────────────────────────────────────────────────────

/// Config file structure (subset of Config that makes sense to persist)
#[derive(Debug, Deserialize, Default)]
pub(crate) struct FileConfig {
    pub context_limit: Option<u64>,
    pub bind_addr: Option<String>,
    pub api_url: Option<String>,
    pub log_dir: Option<String>,
    pub theme: Option<String>,
    pub use_theme_background: Option<bool>,
    pub preset: Option<String>,

    /// Optional [features] section
    pub features: Option<FileFeatures>,

    /// Optional [augmentation] section
    pub augmentation: Option<FileAugmentation>,

    /// Optional [logging] section
    pub logging: Option<FileLogging>,

    /// Optional [cortex] section
    pub cortex: Option<FileCortexConfig>,

    /// Optional [embeddings] section
    pub embeddings: Option<FileEmbeddingsConfig>,

    /// Optional [translation] section
    pub translation: Option<FileTranslation>,

    /// Optional [transformers] section
    pub transformers: Option<FileTransformers>,

    /// Optional [count_tokens] section
    pub count_tokens: Option<FileCountTokens>,

    /// Optional [otel] section (OpenTelemetry export)
    pub otel: Option<FileOtelConfig>,

    /// Optional [clients.X] sections for multi-user routing
    #[serde(default)]
    pub clients: HashMap<String, ClientConfig>,

    /// Optional [providers.X] sections for backend configuration
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Configuration Loading
// ─────────────────────────────────────────────────────────────────────────────

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
                        eprintln!("  Tip: Check for:\n");
                        eprintln!("    - Missing quotes around string values");
                        eprintln!("    - Invalid boolean values (use true/false)");
                        eprintln!("    - Malformed array syntax");
                        eprintln!("    - Typos in section names\n");
                        eprintln!("  To reset, delete the file and restart aspy.\n");
                        std::process::exit(1);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Config file doesn't exist - use defaults
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
            .unwrap_or(150_000);

        // Theme: env > file > default ("Spy Dark" is the project's signature theme)
        let theme = std::env::var("ASPY_THEME")
            .ok()
            .or(file.theme)
            .unwrap_or_else(|| "Spy Dark".to_string());

        // Use theme background: file > default (true = use theme's bg color)
        let use_theme_background = file.use_theme_background.unwrap_or(true);

        // Preset: file > default ("classic")
        let preset = file.preset.unwrap_or_else(|| "classic".to_string());

        // Subconfig loading with from_file() helpers
        let features = Features::from_file(file.features);
        let augmentation = Augmentation::from_file(file.augmentation);
        let logging = LoggingConfig::from_file(file.logging);
        let cortex = CortexConfig::from_file(file.cortex);
        let transformers = Transformers::from_file(file.transformers);
        let count_tokens = CountTokens::from_file(file.count_tokens);
        let translation = Translation::from_file(file.translation);

        // Embeddings: env var for API key takes precedence
        let embeddings_api_key = std::env::var("ASPY_EMBEDDINGS_API_KEY").ok();
        let embeddings = EmbeddingsConfig::from_file(file.embeddings, embeddings_api_key);

        // OTel: env var for connection string takes precedence
        let otel_connection_string = std::env::var("APPLICATIONINSIGHTS_CONNECTION_STRING").ok();
        let otel = OtelConfig::from_file(file.otel, otel_connection_string);

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
            cortex,
            embeddings,
            translation,
            transformers,
            count_tokens,
            otel,
            clients,
        }
    }
}
