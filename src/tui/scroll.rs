// Scrollable component system for TUI panels
//
// This module provides a reusable scroll system that can be used by any panel.
// Each component owns its scroll state - App just renders and routes input.
//
// Design principles:
// 1. Component owns state (App is agnostic)
// 2. Auto-follow for streaming content (toggleable)
// 3. Focused panel receives scroll input
// 4. Consistent scroll behavior across all panels
//
// Integration: Used by App for panel state management and ui.rs for focus rendering.
//
// Note: Some methods (page_up, page_down, update_dimensions, etc.) are part of
// the complete scroll API but not yet wired to keyboard shortcuts or rendering.
// They exist to provide a consistent, reusable scroll interface.

/// Scroll state for a single panel
///
/// Owns all state needed for scrolling: position, content size, viewport size.
/// Can be embedded in any component that needs scrolling.
#[allow(dead_code)] // Complete API - some methods pending keyboard/render integration
#[derive(Debug, Clone)]
pub struct ScrollState {
    /// Current scroll offset (line/item index at top of viewport)
    offset: usize,

    /// Total number of items/lines in content
    total: usize,

    /// Number of items/lines visible in viewport
    viewport: usize,

    /// Whether to auto-follow new content (scroll to bottom)
    /// When true, new content keeps the view at the bottom
    /// User scrolling up disables this; scrolling to bottom re-enables
    pub auto_follow: bool,
}

#[allow(dead_code)] // Complete scroll API - some methods pending integration
impl ScrollState {
    /// Create new scroll state with auto-follow enabled
    pub fn new() -> Self {
        Self {
            offset: 0,
            total: 0,
            viewport: 0,
            auto_follow: true,
        }
    }

    /// Create scroll state with auto-follow disabled (manual scroll)
    pub fn manual() -> Self {
        Self {
            offset: 0,
            total: 0,
            viewport: 0,
            auto_follow: false,
        }
    }

    /// Update content and viewport dimensions
    /// Call this each render frame with current sizes
    pub fn update_dimensions(&mut self, total: usize, viewport: usize) {
        self.total = total;
        self.viewport = viewport;

        // If auto-following, snap to bottom
        if self.auto_follow {
            self.offset = self.max_offset();
        } else {
            // Clamp offset to valid range
            self.offset = self.offset.min(self.max_offset());
        }
    }

    /// Scroll up by one unit
    /// Disables auto-follow (user took control)
    pub fn scroll_up(&mut self) {
        if self.offset > 0 {
            self.offset -= 1;
            self.auto_follow = false;
        }
    }

    /// Scroll down by one unit
    /// Re-enables auto-follow if we reach the bottom
    pub fn scroll_down(&mut self) {
        // If dimensions not set (total=0), allow unbounded scroll
        // Render will clamp to actual content size
        if self.total == 0 || self.offset < self.max_offset() {
            self.offset += 1;
        }

        // Re-enable auto-follow when user scrolls to bottom (only if dimensions known)
        if self.total > 0 && self.offset >= self.max_offset() {
            self.auto_follow = true;
        }
    }

    /// Scroll up by a page
    pub fn page_up(&mut self) {
        let page = self.viewport.max(1);
        self.offset = self.offset.saturating_sub(page);
        self.auto_follow = false;
    }

    /// Scroll down by a page
    pub fn page_down(&mut self) {
        let page = self.viewport.max(1);
        self.offset = (self.offset + page).min(self.max_offset());

        if self.offset >= self.max_offset() {
            self.auto_follow = true;
        }
    }

    /// Jump to top
    pub fn scroll_to_top(&mut self) {
        self.offset = 0;
        self.auto_follow = false;
    }

    /// Jump to bottom (and enable auto-follow)
    pub fn scroll_to_bottom(&mut self) {
        self.offset = self.max_offset();
        self.auto_follow = true;
    }

    /// Toggle auto-follow mode
    pub fn toggle_auto_follow(&mut self) {
        self.auto_follow = !self.auto_follow;
        if self.auto_follow {
            self.offset = self.max_offset();
        }
    }

    /// Get current scroll offset
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Get visible range (start_index, end_index)
    pub fn visible_range(&self) -> (usize, usize) {
        let start = self.offset;
        let end = (self.offset + self.viewport).min(self.total);
        (start, end)
    }

    /// Check if content overflows viewport (scrollbar needed)
    pub fn needs_scrollbar(&self) -> bool {
        self.total > self.viewport
    }

    /// Get scrollbar position (0.0 to 1.0)
    pub fn scrollbar_position(&self) -> f64 {
        if self.max_offset() == 0 {
            0.0
        } else {
            self.offset as f64 / self.max_offset() as f64
        }
    }

    /// Maximum valid offset
    fn max_offset(&self) -> usize {
        self.total.saturating_sub(self.viewport)
    }

    /// Get total content size
    pub fn total(&self) -> usize {
        self.total
    }

    /// Get viewport size
    pub fn viewport(&self) -> usize {
        self.viewport
    }
}

impl Default for ScrollState {
    fn default() -> Self {
        Self::new()
    }
}

/// Panels that can be focused for input routing
///
/// Note: Detail is now a modal (not a focusable panel). When detail modal
/// is open, all input is routed to the modal, not via focus system.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FocusablePanel {
    /// Main event list (default focus)
    #[default]
    Events,
    /// Claude's thinking panel
    Thinking,
    /// System logs panel
    Logs,
}

// Focus cycling now handled by Preset::focus_order (see app.rs focus_next/prev)

// Note: PanelStates has been fully migrated to component pattern.
// Each panel (LogsPanel, ThinkingPanel, DetailPanel) now owns its own ScrollState.
// When EventsPanel is extracted, this file will only contain ScrollState and FocusablePanel.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_follow_on_new_content() {
        let mut scroll = ScrollState::new();
        assert!(scroll.auto_follow);

        // Simulate content growing
        scroll.update_dimensions(10, 5);
        assert_eq!(scroll.offset(), 5); // At bottom

        scroll.update_dimensions(15, 5);
        assert_eq!(scroll.offset(), 10); // Still at bottom
    }

    #[test]
    fn test_scroll_up_disables_auto_follow() {
        let mut scroll = ScrollState::new();
        scroll.update_dimensions(20, 5);
        assert!(scroll.auto_follow);

        scroll.scroll_up();
        assert!(!scroll.auto_follow);
        assert_eq!(scroll.offset(), 14); // Moved up one
    }

    #[test]
    fn test_scroll_to_bottom_enables_auto_follow() {
        let mut scroll = ScrollState::new();
        scroll.update_dimensions(20, 5);

        scroll.scroll_up();
        scroll.scroll_up();
        assert!(!scroll.auto_follow);

        scroll.scroll_to_bottom();
        assert!(scroll.auto_follow);
        assert_eq!(scroll.offset(), 15);
    }

    #[test]
    fn test_visible_range() {
        let mut scroll = ScrollState::new();
        scroll.update_dimensions(100, 10);

        // At bottom (auto-follow)
        let (start, end) = scroll.visible_range();
        assert_eq!(start, 90);
        assert_eq!(end, 100);

        // Scroll to top
        scroll.scroll_to_top();
        let (start, end) = scroll.visible_range();
        assert_eq!(start, 0);
        assert_eq!(end, 10);
    }

    // Focus cycling now tested via Preset::focus_order (see preset.rs tests)

    #[test]
    fn test_manual_scroll_mode() {
        let mut scroll = ScrollState::manual();

        // Simulate content growing - offset should stay put
        scroll.update_dimensions(10, 5);
        assert_eq!(scroll.offset(), 0);

        scroll.update_dimensions(15, 5);
        assert_eq!(scroll.offset(), 0); // Still at top, not following
    }
}
