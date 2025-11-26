// Configuration for the proxy server
//
// This module handles loading configuration from environment variables
// and providing sensible defaults. In Rust, we typically use builder patterns
// or struct defaults rather than complex configuration files for simple cases.

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
}

impl Config {
    /// Load configuration from environment variables with sensible defaults
    pub fn from_env() -> Self {
        let bind_addr = std::env::var("ANTHROPIC_SPY_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
            .parse()
            .expect("Invalid bind address");

        let api_url = std::env::var("ANTHROPIC_API_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());

        let log_dir = std::env::var("ANTHROPIC_SPY_LOG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./logs"));

        let enable_tui = std::env::var("ANTHROPIC_SPY_NO_TUI")
            .map(|v| v != "1" && v.to_lowercase() != "true")
            .unwrap_or(true);

        let demo_mode = std::env::var("ANTHROPIC_SPY_DEMO")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        Self {
            bind_addr,
            api_url,
            log_dir,
            enable_tui,
            demo_mode,
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
        }
    }
}
