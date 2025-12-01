// Configuration for the proxy server
//
// Configuration is loaded in order of precedence:
// 1. Environment variables (highest priority)
// 2. Config file (~/.config/anthropic-spy/config.toml)
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

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
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

    /// Optional [clients.X] sections for multi-user routing
    #[serde(default)]
    clients: HashMap<String, ClientConfig>,

    /// Optional [providers.X] sections for backend configuration
    #[serde(default)]
    providers: HashMap<String, ProviderConfig>,
}

impl Config {
    /// Get the config file path: ~/.config/anthropic-spy/config.toml
    /// Uses Unix-style ~/.config on all platforms for consistency
    pub fn config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|p| p.join(".config").join("anthropic-spy").join("config.toml"))
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

    /// Serialize config to TOML string (single source of truth for format)
    pub fn to_toml(&self) -> String {
        format!(
            r#"# anthropic-spy configuration

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
        let bind_addr = std::env::var("ANTHROPIC_SPY_BIND")
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
        let log_dir = std::env::var("ANTHROPIC_SPY_LOG_DIR")
            .ok()
            .or(file.log_dir)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("./logs"));

        // TUI toggle: env only (runtime flag)
        let enable_tui = std::env::var("ANTHROPIC_SPY_NO_TUI")
            .map(|v| v != "1" && v.to_lowercase() != "true")
            .unwrap_or(true);

        // Demo mode: env only (runtime flag)
        let demo_mode = std::env::var("ANTHROPIC_SPY_DEMO")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        // Context limit: env > file > default (147K based on empirical data)
        let context_limit = std::env::var("ANTHROPIC_SPY_CONTEXT_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .or(file.context_limit)
            .unwrap_or(147_000);

        // Theme: env > file > default ("Spy Dark" is the project's signature theme)
        let theme = std::env::var("ANTHROPIC_SPY_THEME")
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
            clients: ClientsConfig::default(),
        }
    }
}
