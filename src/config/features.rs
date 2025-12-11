//! Feature flags configuration
//!
//! Feature flags for optional modules (opt-out: default enabled).

use serde::Deserialize;

/// Feature flags for optional modules (opt-out: default enabled)
#[derive(Debug, Clone)]
pub struct Features {
    /// Storage module: write events to JSONL files
    pub json_logging: bool,

    /// Thinking panel: show Claude's extended thinking
    pub thinking_panel: bool,

    /// Stats tracking: token counts, costs, tool distribution
    pub stats: bool,
}

impl Default for Features {
    fn default() -> Self {
        Self {
            json_logging: true,
            thinking_panel: true,
            stats: true,
        }
    }
}

/// Feature flags as loaded from config file
#[derive(Debug, Deserialize, Default)]
pub struct FileFeatures {
    pub storage: Option<bool>,
    pub thinking_panel: Option<bool>,
    pub stats: Option<bool>,
}

impl Features {
    /// Create from file config with defaults
    pub fn from_file(file: Option<FileFeatures>) -> Self {
        let file = file.unwrap_or_default();

        Self {
            json_logging: file.storage.unwrap_or(true),
            thinking_panel: file.thinking_panel.unwrap_or(true),
            stats: file.stats.unwrap_or(true),
        }
    }
}
