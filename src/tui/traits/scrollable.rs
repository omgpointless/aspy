//! Scrollable trait for components with scrollable content
//!
//! Components that display more content than fits in their viewport
//! implement this trait to get uniform scroll behavior.

use super::Component;
use crate::tui::scroll::ScrollState;

/// Trait for components with scrollable content
///
/// Provides a uniform interface for scroll operations. Components
/// own their `ScrollState` and expose it through this trait.
///
/// # Default Implementations
///
/// Most methods have default implementations that delegate to `ScrollState`.
/// Components only need to implement `scroll_state()` and `scroll_state_mut()`.
///
/// # Example
///
/// ```ignore
/// struct LogsPanel {
///     scroll: ScrollState,
///     entries: Vec<LogEntry>,
/// }
///
/// impl Scrollable for LogsPanel {
///     fn scroll_state(&self) -> &ScrollState {
///         &self.scroll
///     }
///
///     fn scroll_state_mut(&mut self) -> &mut ScrollState {
///         &mut self.scroll
///     }
/// }
///
/// // Now LogsPanel automatically gets:
/// // - scroll_up(), scroll_down()
/// // - scroll_to_top(), scroll_to_bottom()
/// // - page_up(), page_down()
/// // - visible_range(), needs_scrollbar()
/// ```
pub trait Scrollable: Component {
    /// Get immutable reference to scroll state
    fn scroll_state(&self) -> &ScrollState;

    /// Get mutable reference to scroll state
    fn scroll_state_mut(&mut self) -> &mut ScrollState;

    // ─────────────────────────────────────────────────────────────
    // Navigation - default implementations delegate to ScrollState
    // ─────────────────────────────────────────────────────────────

    /// Scroll up by one line/item
    fn scroll_up(&mut self) {
        self.scroll_state_mut().scroll_up();
    }

    /// Scroll down by one line/item
    fn scroll_down(&mut self) {
        self.scroll_state_mut().scroll_down();
    }

    /// Jump to the top of content
    fn scroll_to_top(&mut self) {
        self.scroll_state_mut().scroll_to_top();
    }

    /// Jump to the bottom of content
    fn scroll_to_bottom(&mut self) {
        self.scroll_state_mut().scroll_to_bottom();
    }

    /// Scroll up by a page
    fn page_up(&mut self) {
        self.scroll_state_mut().page_up();
    }

    /// Scroll down by a page
    fn page_down(&mut self) {
        self.scroll_state_mut().page_down();
    }

    // ─────────────────────────────────────────────────────────────
    // State queries - default implementations delegate to ScrollState
    // ─────────────────────────────────────────────────────────────

    /// Get the visible range of items (start_index, end_index)
    #[allow(dead_code)]
    fn visible_range(&self) -> (usize, usize) {
        self.scroll_state().visible_range()
    }

    /// Check if scrollbar is needed (content exceeds viewport)
    #[allow(dead_code)]
    fn needs_scrollbar(&self) -> bool {
        self.scroll_state().needs_scrollbar()
    }

    /// Get current scroll offset
    #[allow(dead_code)]
    fn scroll_offset(&self) -> usize {
        self.scroll_state().offset()
    }

    /// Check if auto-follow is enabled
    #[allow(dead_code)]
    fn is_auto_following(&self) -> bool {
        self.scroll_state().auto_follow
    }

    /// Update content and viewport dimensions
    #[allow(dead_code)]
    ///
    /// Call this each render frame with current sizes.
    /// If auto-follow is enabled, will snap to bottom.
    fn update_dimensions(&mut self, total: usize, viewport: usize) {
        self.scroll_state_mut().update_dimensions(total, viewport);
    }
}

/// Extension trait for components that support selection within scrollable content
///
/// This is separate from `Scrollable` because not all scrollable content
/// has selectable items (e.g., a text detail view scrolls but doesn't select).
///
/// # Example
///
/// ```ignore
/// impl Selectable for EventsPanel {
///     fn selected_index(&self) -> Option<usize> {
///         Some(self.selected)
///     }
///
///     fn select(&mut self, index: usize) {
///         self.selected = index.min(self.events.len().saturating_sub(1));
///     }
///
///     fn item_count(&self) -> usize {
///         self.events.len()
///     }
/// }
/// ```
pub trait Selectable: Scrollable {
    /// Get the currently selected item index
    fn selected_index(&self) -> Option<usize>;

    /// Set the selected item index
    fn select(&mut self, index: usize);

    /// Get total number of selectable items
    fn item_count(&self) -> usize;

    /// Select the next item (with bounds checking)
    fn select_next(&mut self) {
        if let Some(current) = self.selected_index() {
            let max = self.item_count().saturating_sub(1);
            if current < max {
                self.select(current + 1);
            }
        } else if self.item_count() > 0 {
            self.select(0);
        }
    }

    /// Select the previous item (with bounds checking)
    fn select_previous(&mut self) {
        if let Some(current) = self.selected_index() {
            if current > 0 {
                self.select(current - 1);
            }
        } else if self.item_count() > 0 {
            self.select(self.item_count().saturating_sub(1));
        }
    }

    /// Select the first item
    #[allow(dead_code)] // Trait convenience method - panels use scroll_to_top via Interactive
    fn select_first(&mut self) {
        if self.item_count() > 0 {
            self.select(0);
        }
    }

    /// Select the last item
    #[allow(dead_code)] // Trait convenience method - panels use scroll_to_bottom via Interactive
    fn select_last(&mut self) {
        let count = self.item_count();
        if count > 0 {
            self.select(count - 1);
        }
    }
}
