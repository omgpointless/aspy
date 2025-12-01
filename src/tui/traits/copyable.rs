//! Copyable trait for components that provide clipboard content
//!
//! Components that can export their content to the clipboard implement
//! this trait. Supports both human-readable and machine-readable formats.

use super::Component;

/// Trait for components that can provide content for the clipboard
///
/// When the user presses a copy keybind (e.g., `y` or `Y`), the focused
/// component's `Copyable` implementation determines what gets copied.
///
/// # Two Copy Modes
///
/// - **Text** (`y`): Human-readable format for pasting into docs, chat, etc.
/// - **Data** (`Y`): Machine-readable format (JSON, JSONL) for scripting/analysis
///
/// # Example
///
/// ```ignore
/// impl Copyable for EventsPanel {
///     fn copy_text(&self) -> Option<String> {
///         // Format selected event as readable text
///         self.selected_event().map(|e| format_event_line(e))
///     }
///
///     fn copy_data(&self) -> Option<String> {
///         // Serialize selected event as JSONL
///         self.selected_event()
///             .and_then(|e| serde_json::to_string(e).ok())
///     }
/// }
///
/// impl Copyable for ThinkingPanel {
///     fn copy_text(&self) -> Option<String> {
///         // Return raw thinking content
///         self.current_thinking.clone()
///     }
///
///     // Uses default copy_data() -> None
///     // (thinking doesn't have a structured format)
/// }
/// ```
pub trait Copyable: Component {
    /// Get human-readable text for clipboard
    ///
    /// This is the primary copy operation. Should return formatted text
    /// suitable for pasting into documents, chat, etc.
    ///
    /// Returns `None` if there's nothing to copy (e.g., empty panel).
    fn copy_text(&self) -> Option<String>;

    /// Get machine-readable data for clipboard
    ///
    /// Optional: Returns structured data (typically JSON/JSONL) for
    /// scripting and analysis workflows.
    ///
    /// Default implementation returns `None`. Override for components
    /// that have meaningful structured representations.
    #[allow(dead_code)]
    fn copy_data(&self) -> Option<String> {
        None
    }

    /// Get a description of what will be copied (for toast messages)
    ///
    /// Default: uses component ID. Override for more specific descriptions
    /// like "3 selected events" or "thinking block (2.4k tokens)".
    #[allow(dead_code)]
    fn copy_description(&self) -> String {
        format!("{:?}", self.id())
    }
}

/// Result of a copy operation
///
/// Note: Currently unused - intentional infrastructure for future clipboard system
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum CopyResult {
    /// Successfully copied content
    Success {
        /// What was copied (for toast message)
        description: String,
        /// Number of bytes/characters copied
        length: usize,
    },
    /// Nothing to copy (empty selection, no content)
    Empty,
    /// Clipboard access failed
    Error(String),
}

impl CopyResult {
    /// Create a success result
    #[allow(dead_code)]
    pub fn success(description: impl Into<String>, length: usize) -> Self {
        Self::Success {
            description: description.into(),
            length,
        }
    }

    /// Create an error result
    #[allow(dead_code)]
    pub fn error(msg: impl Into<String>) -> Self {
        Self::Error(msg.into())
    }

    /// Get a toast message for this result
    #[allow(dead_code)]
    pub fn toast_message(&self) -> String {
        match self {
            Self::Success { description, .. } => format!("✓ Copied {}", description),
            Self::Empty => "Nothing to copy".to_string(),
            Self::Error(msg) => format!("✗ {}", msg),
        }
    }

    /// Whether the operation was successful
    #[allow(dead_code)]
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }
}
