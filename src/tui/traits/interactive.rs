//! Interactive trait for components that handle keyboard input
//!
//! Components that can receive and process keyboard events implement
//! this trait. The App routes input to the focused component.

use super::Component;
use crossterm::event::KeyEvent;

/// Result of handling a key event
///
/// Tells the App whether the component consumed the event or
/// if it should bubble up for global handling.
///
/// Note: Currently unused - intentional infrastructure for future event routing system
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handled {
    /// Event was consumed by the component
    Yes,
    /// Event was not handled, should bubble up
    No,
}

impl Handled {
    /// Create from a boolean (true = handled)
    #[allow(dead_code)]
    pub fn from_bool(handled: bool) -> Self {
        if handled {
            Self::Yes
        } else {
            Self::No
        }
    }

    /// Check if the event was handled
    #[allow(dead_code)]
    pub fn was_handled(self) -> bool {
        self == Self::Yes
    }
}

impl From<bool> for Handled {
    fn from(handled: bool) -> Self {
        Self::from_bool(handled)
    }
}

/// Trait for components that handle keyboard input
///
/// When a key event arrives, the App routes it to the focused component.
/// The component decides whether to handle it or let it bubble up.
///
/// # Event Flow
///
/// ```text
/// KeyEvent
///    │
///    ▼
/// App (global handlers: ?, q, F1-F3)
///    │
///    │ if not handled
///    ▼
/// Focused Component (via Interactive trait)
///    │
///    │ returns Handled::Yes or Handled::No
///    ▼
/// App (fallback handlers)
/// ```
///
/// # Example
///
/// ```ignore
/// impl Interactive for EventsPanel {
///     fn handle_key(&mut self, key: KeyEvent) -> Handled {
///         use crossterm::event::KeyCode;
///
///         match key.code {
///             KeyCode::Up | KeyCode::Char('k') => {
///                 self.select_previous();
///                 Handled::Yes
///             }
///             KeyCode::Down | KeyCode::Char('j') => {
///                 self.select_next();
///                 Handled::Yes
///             }
///             KeyCode::Enter => {
///                 self.toggle_detail();
///                 Handled::Yes
///             }
///             _ => Handled::No, // Let App handle it
///         }
///     }
/// }
/// ```
///
/// Note: Currently unused as trait bound - intentional infrastructure for future event routing
#[allow(dead_code)]
pub trait Interactive: Component {
    /// Handle a key event
    ///
    /// Returns `Handled::Yes` if the component consumed the event,
    /// `Handled::No` if it should bubble up to the App.
    fn handle_key(&mut self, key: KeyEvent) -> Handled;

    /// Whether this component can receive focus
    ///
    /// Default is `true`. Override to return `false` for components
    /// like status bars or titles that display info but don't take input.
    fn focusable(&self) -> bool {
        true
    }

    /// Hint text for status bar when this component is focused
    ///
    /// Optional: Returns keybind hints specific to this component.
    /// Displayed in status bar to help user discover functionality.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn focus_hint(&self) -> Option<&'static str> {
    ///     Some("↑↓:navigate  Enter:expand  y:copy")
    /// }
    /// ```
    fn focus_hint(&self) -> Option<&'static str> {
        None
    }
}

/// Convenience trait for components that are both Interactive and Scrollable
///
/// Provides default key handling for common scroll operations.
/// Components can use this as a building block.
///
/// Note: Currently unused - intentional infrastructure for future simplified scroll handling
#[allow(dead_code)]
pub trait ScrollableInteractive: Interactive + super::Scrollable {
    /// Handle common scroll keys
    ///
    /// Call this from `handle_key()` for standard scroll behavior.
    /// Returns `Handled::Yes` for: Up, Down, k, j, Home, End, PageUp, PageDown
    fn handle_scroll_keys(&mut self, key: KeyEvent) -> Handled {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_up();
                Handled::Yes
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_down();
                Handled::Yes
            }
            KeyCode::Home => {
                self.scroll_to_top();
                Handled::Yes
            }
            KeyCode::End => {
                self.scroll_to_bottom();
                Handled::Yes
            }
            KeyCode::PageUp => {
                self.page_up();
                Handled::Yes
            }
            KeyCode::PageDown => {
                self.page_down();
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

// Blanket implementation: anything that implements both traits gets this for free
impl<T: Interactive + super::Scrollable> ScrollableInteractive for T {}
