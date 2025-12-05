//! Detail panel component
//!
//! Displays detailed view of a selected event with:
//! - Formatted event information
//! - Scroll support for long content
//! - Copy support for event data
use crate::tui::scroll::ScrollState;
use crate::tui::traits::{
    Component, ComponentId, Copyable, Handled, Interactive, RenderContext, Scrollable,
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::Style,
    widgets::{Block, Borders},
    Frame,
};

/// Detail panel component
///
/// Displays detailed information about a selected event with:
/// - Formatted text content
/// - 2D scrolling (vertical and horizontal)
/// - Manual scroll mode (no auto-follow)
pub struct DetailPanel {
    /// Vertical scroll state (position, viewport, manual mode)
    scroll: ScrollState,

    /// Horizontal scroll offset (columns from left)
    horizontal_offset: usize,

    /// Cached content for copy operations
    cached_content: Option<String>,
}

impl DetailPanel {
    /// Create a new detail panel in manual scroll mode
    pub fn new() -> Self {
        Self {
            scroll: ScrollState::manual(), // User controls scroll position
            horizontal_offset: 0,
            cached_content: None,
        }
    }

    /// Scroll left by one column
    pub fn scroll_left(&mut self) {
        self.horizontal_offset = self.horizontal_offset.saturating_sub(1);
    }

    /// Scroll right by one column
    pub fn scroll_right(&mut self) {
        self.horizontal_offset = self.horizontal_offset.saturating_add(1);
    }

    #[allow(dead_code)]
    /// Scroll left by one page (width of viewport)
    pub fn page_left(&mut self, viewport_width: usize) {
        self.horizontal_offset = self.horizontal_offset.saturating_sub(viewport_width);
    }

    #[allow(dead_code)]
    /// Scroll right by one page (width of viewport)
    pub fn page_right(&mut self, viewport_width: usize) {
        self.horizontal_offset = self.horizontal_offset.saturating_add(viewport_width);
    }

    /// Reset to leftmost position
    pub fn scroll_to_left(&mut self) {
        self.horizontal_offset = 0;
    }

    /// Get current horizontal offset
    pub fn horizontal_offset(&self) -> usize {
        self.horizontal_offset
    }

    /// Reset scroll position (called when opening detail view)
    pub fn reset(&mut self) {
        self.scroll.scroll_to_top();
        self.horizontal_offset = 0;
        self.cached_content = None;
    }

    /// Set the cached content for copy operations
    /// Called when detail modal opens with formatted event text
    pub fn set_content(&mut self, content: String) {
        self.cached_content = Some(content);
    }
}

impl Default for DetailPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Trait Implementations
// ═══════════════════════════════════════════════════════════════════════════

impl Component for DetailPanel {
    fn id(&self) -> ComponentId {
        ComponentId::Detail
    }

    fn render(&self, f: &mut Frame, area: Rect, ctx: &RenderContext) {
        // Minimal render - actual rendering uses render_with_content
        let border_color = if ctx.is_focused(self.id()) {
            ctx.theme.highlight
        } else {
            ctx.theme.border
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ctx.theme.border_type)
            .border_style(Style::default().fg(border_color))
            .title(" Event Details ");

        f.render_widget(block, area);
    }
}

impl Scrollable for DetailPanel {
    fn scroll_state(&self) -> &ScrollState {
        &self.scroll
    }

    fn scroll_state_mut(&mut self) -> &mut ScrollState {
        &mut self.scroll
    }
}

impl Copyable for DetailPanel {
    fn copy_text(&self) -> Option<String> {
        self.cached_content.clone()
    }

    fn copy_description(&self) -> String {
        "event details".to_string()
    }
}

impl Interactive for DetailPanel {
    fn handle_key(&mut self, key: KeyEvent) -> Handled {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_up();
                Handled::Yes
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_down();
                Handled::Yes
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.scroll_to_top();
                Handled::Yes
            }
            KeyCode::End | KeyCode::Char('G') => {
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

    fn focusable(&self) -> bool {
        true
    }

    fn focus_hint(&self) -> Option<&'static str> {
        Some("↑↓:scroll  g/G:top/end  y:copy  Esc:close")
    }
}

// Note: DetailPanel is now rendered via modal system in ui.rs::render_detail_modal
// The render entry point pattern used by other components doesn't apply here
// because detail is a modal overlay, not a panel in the layout.
