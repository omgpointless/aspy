//! Augmentation configuration
//!
//! Augmentations modify API responses by injecting additional content.

use serde::Deserialize;

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

/// Augmentation settings as loaded from config file
#[derive(Debug, Deserialize, Default)]
pub struct FileAugmentation {
    pub context_warning: Option<bool>,
    pub context_warning_thresholds: Option<Vec<u8>>,
}

impl Augmentation {
    /// Create from file config with defaults
    pub fn from_file(file: Option<FileAugmentation>) -> Self {
        let file = file.unwrap_or_default();

        Self {
            context_warning: file.context_warning.unwrap_or(true),
            context_warning_thresholds: file
                .context_warning_thresholds
                .unwrap_or_else(|| vec![60, 80, 85, 90, 95]),
        }
    }
}
