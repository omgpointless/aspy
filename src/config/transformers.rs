//! Request transformers configuration
//!
//! Transformers modify API requests before they are forwarded to the provider.
//! Used for editing system-reminders, injecting context, translating formats, etc.

use serde::Deserialize;

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

    /// System editor configuration (modifies system prompts)
    pub system_editor: Option<crate::proxy::transformation::SystemEditorConfig>,

    /// Compact enhancer configuration (enhances compaction prompts with session context)
    pub compact_enhancer: Option<crate::proxy::transformation::CompactEnhancerConfig>,
}

/// Transformers config as loaded from file
#[derive(Debug, Deserialize, Default)]
pub struct FileTransformers {
    pub enabled: Option<bool>,
    #[serde(rename = "tag-editor")]
    pub tag_editor: Option<crate::proxy::transformation::TagEditorConfig>,
    #[serde(rename = "system-editor")]
    pub system_editor: Option<crate::proxy::transformation::SystemEditorConfig>,
    #[serde(rename = "compact-enhancer")]
    pub compact_enhancer: Option<crate::proxy::transformation::CompactEnhancerConfig>,
}

impl Transformers {
    /// Create from file config with defaults
    pub fn from_file(file: Option<FileTransformers>) -> Self {
        let file = file.unwrap_or_default();

        Self {
            enabled: file.enabled.unwrap_or(false),
            tag_editor: file.tag_editor,
            system_editor: file.system_editor,
            compact_enhancer: file.compact_enhancer,
        }
    }
}
