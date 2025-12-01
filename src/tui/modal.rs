// Modal system for TUI overlays
//
// Self-contained modal dialogs that handle their own input and return actions.
// App just holds Option<Modal>, input routing acts on returned ModalAction.

use crossterm::event::KeyCode;

/// Actions returned by modal input handling
#[derive(Debug, Clone)]
pub enum ModalAction {
    /// Input consumed, no state change needed
    None,
    /// Close the modal
    Close,
    /// Scroll up in content
    ScrollUp,
    /// Scroll down in content
    ScrollDown,
    /// Scroll left in content
    ScrollLeft,
    /// Scroll right in content
    ScrollRight,
    /// Jump to top
    ScrollTop,
    /// Jump to bottom
    ScrollBottom,
    /// Jump to leftmost position
    ScrollLeftmost,
    /// Page up vertically
    PageUp,
    /// Page down vertically
    PageDown,
    /// Copy content (readable format)
    CopyReadable,
    /// Copy content (JSONL format)
    CopyJsonl,
}

/// Available modal types
#[derive(Debug, Clone)]
pub enum Modal {
    /// Help overlay - shows keyboard shortcuts
    Help,
    /// Event detail view - shows full event information
    /// Stores the index of the event being viewed
    Detail(usize),
    /// Log entry detail view - content cached in DetailPanel
    LogDetail,
}

impl Modal {
    /// Create a help modal
    pub fn help() -> Self {
        Modal::Help
    }

    /// Create a detail modal for the given event index
    pub fn detail(event_index: usize) -> Self {
        Modal::Detail(event_index)
    }

    /// Create a log detail modal (content cached in DetailPanel)
    pub fn log_detail() -> Self {
        Modal::LogDetail
    }

    /// Handle keyboard input, return action for caller to execute
    pub fn handle_input(&mut self, key: KeyCode) -> ModalAction {
        match self {
            Modal::Help => match key {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => ModalAction::Close,
                _ => ModalAction::None,
            },
            Modal::Detail(_) | Modal::LogDetail => match key {
                KeyCode::Esc | KeyCode::Char('q') => ModalAction::Close,
                // Vertical scroll
                KeyCode::Up | KeyCode::Char('k') => ModalAction::ScrollUp,
                KeyCode::Down | KeyCode::Char('j') => ModalAction::ScrollDown,
                KeyCode::PageUp => ModalAction::PageUp,
                KeyCode::PageDown => ModalAction::PageDown,
                // Horizontal scroll
                KeyCode::Left | KeyCode::Char('h') => ModalAction::ScrollLeft,
                KeyCode::Right | KeyCode::Char('l') => ModalAction::ScrollRight,
                // Jump positions
                KeyCode::Home => ModalAction::ScrollTop,
                KeyCode::End => ModalAction::ScrollBottom,
                KeyCode::Char('0') => ModalAction::ScrollLeftmost,
                // Copy
                KeyCode::Char('y') => ModalAction::CopyReadable,
                KeyCode::Char('Y') => ModalAction::CopyJsonl,
                _ => ModalAction::None,
            },
        }
    }

    /// Get the event index if this is a Detail modal
    pub fn event_index(&self) -> Option<usize> {
        match self {
            Modal::Detail(idx) => Some(*idx),
            _ => None,
        }
    }
}
