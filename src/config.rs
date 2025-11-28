// Configuration for the proxy server
//
// Configuration is loaded in order of precedence:
// 1. Environment variables (highest priority)
// 2. Config file (~/.config/anthropic-spy/config.toml)
// 3. Built-in defaults (lowest priority)

use serde::Deserialize;
use std::net::SocketAddr;
use std::path::PathBuf;

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

    /// Context window limit for the gauge (empirically ~150K triggers compact)
    pub context_limit: u64,

    /// Theme name: "auto", "dracula", "monokai", "nord", "gruvbox"
    pub theme: String,
}

/// Config file structure (subset of Config that makes sense to persist)
#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    context_limit: Option<u64>,
    bind_addr: Option<String>,
    api_url: Option<String>,
    log_dir: Option<String>,
    theme: Option<String>,
}

impl Config {
    /// Get the config file path: ~/.config/anthropic-spy/config.toml
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("anthropic-spy").join("config.toml"))
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

        // Context limit: env > file > default (150K based on empirical data)
        let context_limit = std::env::var("ANTHROPIC_SPY_CONTEXT_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .or(file.context_limit)
            .unwrap_or(150_000);

        // Theme: env > file > default ("auto" uses terminal palette)
        let theme = std::env::var("ANTHROPIC_SPY_THEME")
            .ok()
            .or(file.theme)
            .unwrap_or_else(|| "auto".to_string());

        Self {
            bind_addr,
            api_url,
            log_dir,
            enable_tui,
            demo_mode,
            context_limit,
            theme,
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
            context_limit: 150_000,
            theme: "auto".to_string(),
        }
    }
}
