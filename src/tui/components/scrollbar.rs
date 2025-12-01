//! Scrollbar rendering helper
//!
//! Provides a unified scrollbar rendering function that works with `ScrollState`.
//! Eliminates copy-paste scrollbar code across panels.

use crate::tui::scroll::ScrollState;
use ratatui::{
    layout::Rect,
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

/// Visual style for the scrollbar
#[derive(Debug, Clone, Copy, Default)]
pub enum ScrollbarStyle {
    /// Arrows at top and bottom (↑ ↓)
    Arrows,
    /// Minimal style - no arrows, just the thumb
    #[default]
    Minimal,
}

/// Render a vertical scrollbar for a panel
///
/// Only renders if content exceeds viewport (uses `scroll.needs_scrollbar()`).
///
/// # Arguments
///
/// * `f` - Frame to render to
/// * `area` - The panel area (scrollbar renders on right edge)
/// * `scroll` - ScrollState containing position and dimensions
/// * `style` - Visual style (Arrows or Minimal)
///
/// # Example
///
/// ```ignore
/// // In a panel's render function:
/// render_scrollbar(f, area, &self.scroll, ScrollbarStyle::Arrows);
/// ```
pub fn render_scrollbar(f: &mut Frame, area: Rect, scroll: &ScrollState, style: ScrollbarStyle) {
    if !scroll.needs_scrollbar() {
        return;
    }

    let scrollbar = match style {
        ScrollbarStyle::Arrows => Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓")),
        ScrollbarStyle::Minimal => Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None),
    };

    // ScrollbarState wants: content_length (how much can scroll) and position
    let content_length = scroll.total().saturating_sub(scroll.viewport());
    let mut scrollbar_state = ScrollbarState::new(content_length).position(scroll.offset());

    f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
}

/// Render scrollbar using raw values (for panels not using ScrollState)
///
/// This is a lower-level helper for cases where you have the values
/// but not a ScrollState struct.
///
/// # Arguments
///
/// * `f` - Frame to render to
/// * `area` - The panel area
/// * `total` - Total number of items/lines
/// * `viewport` - Visible items/lines
/// * `offset` - Current scroll position
/// * `style` - Visual style
pub fn render_scrollbar_raw(
    f: &mut Frame,
    area: Rect,
    total: usize,
    viewport: usize,
    offset: usize,
    style: ScrollbarStyle,
) {
    if total <= viewport {
        return;
    }

    let scrollbar = match style {
        ScrollbarStyle::Arrows => Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓")),
        ScrollbarStyle::Minimal => Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None),
    };

    let content_length = total.saturating_sub(viewport);
    let mut scrollbar_state = ScrollbarState::new(content_length).position(offset);

    f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
}
