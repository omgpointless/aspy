//! Events panel component
//!
//! A self-contained component that displays proxy events in a scrollable list.
//! Implements the full trait system: Component, Scrollable, Selectable, Copyable, Interactive.
//!
//! # Scroll Architecture
//!
//! Unlike typical scroll components, EventsPanel uses selection-based scrolling:
//! - `selected: None` = **auto-follow mode** (always shows latest events at bottom)
//! - `selected: Some(idx)` = **selection mode** (locked to specific event)
//!
//! This eliminates the need for explicit "follow latest" toggle - pressing G (scroll to bottom)
//! naturally returns to auto-follow by setting selected = None.

use super::scrollbar::{render_scrollbar_raw, ScrollbarStyle};
use crate::events::{ProxyEvent, TrackedEvent};
use crate::theme::Theme;
use crate::tui::scroll::{FocusablePanel, ScrollState};
use crate::tui::traits::{
    Component, ComponentId, Copyable, Handled, Interactive, RenderContext, Scrollable, Selectable,
    Zoomable,
};
use crate::tui::views::format_event_line;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use unicode_width::UnicodeWidthStr;

/// Events panel component
///
/// Displays proxy events with:
/// - Selection-based scrolling (None = auto-follow, Some = locked selection)
/// - Color-coded event types
/// - Keyboard navigation (j/k, g/G)
/// - Copy operations (readable summary or JSONL)
pub struct EventsPanel {
    /// Selected event index (None = auto-follow mode, Some = selection mode)
    pub selected: Option<usize>,

    /// Cached event count (for bounds checking)
    /// Public so App can sync it before delegating operations
    pub event_count: usize,

    /// Scroll state (unused for EventsPanel - exists for trait compliance)
    /// EventsPanel uses selection-based scrolling, not ScrollState
    _scroll: ScrollState,
}

impl EventsPanel {
    /// Create a new events panel with auto-follow enabled
    pub fn new() -> Self {
        Self {
            selected: None, // Auto-follow by default
            event_count: 0,
            _scroll: ScrollState::new(), // Unused - for trait compliance
        }
    }

    /// Update with current event count (call each frame)
    ///
    /// This syncs the component's state with the event list.
    pub fn sync_events(&mut self, event_count: usize) {
        self.event_count = event_count;

        // Clamp selection to valid range
        if let Some(idx) = self.selected {
            if idx >= event_count {
                self.selected = event_count.checked_sub(1);
            }
        }
    }

    /// Calculate visible range for the event list given viewport height and actual event count
    ///
    /// - Auto-follow mode (None): shows latest events at bottom
    /// - Selection mode (Some): keeps selected item visible
    ///
    /// Takes actual event count as parameter to avoid stale cached values during rendering.
    pub fn visible_range(&self, total: usize, height: usize) -> (usize, usize) {
        if total == 0 {
            return (0, 0);
        }

        let offset = match self.selected {
            None => {
                // Auto-follow: show latest events (scroll to bottom)
                total.saturating_sub(height)
            }
            Some(idx) => {
                // Selection mode: keep selected item visible
                if idx >= height {
                    idx.saturating_sub(height - 1)
                } else {
                    0
                }
            }
        };

        let start = offset;
        let end = (offset + height).min(total);

        (start, end)
    }

    /// Render the events panel with owned events slice (backward compatibility)
    ///
    /// This method is kept for backward compatibility with code that passes
    /// `&[TrackedEvent]` directly. New code should use `render_with_filtered_events`
    /// which accepts references for efficient session filtering.
    #[allow(dead_code)]
    pub fn render_with_events(
        &self,
        f: &mut Frame,
        area: Rect,
        events: &[TrackedEvent],
        theme: &Theme,
        focused: bool,
    ) {
        // Convert to references for unified rendering
        let refs: Vec<&TrackedEvent> = events.iter().collect();
        self.render_events_inner(f, area, &refs, theme, focused);
    }

    /// Render with pre-filtered event references (for multi-session support)
    ///
    /// Takes a slice of references - useful when events have been filtered.
    pub fn render_with_filtered_events(
        &self,
        f: &mut Frame,
        area: Rect,
        events: &[&TrackedEvent],
        theme: &Theme,
        focused: bool,
    ) {
        self.render_events_inner(f, area, events, theme, focused);
    }

    /// Internal rendering implementation (works with references)
    fn render_events_inner(
        &self,
        f: &mut Frame,
        area: Rect,
        events: &[&TrackedEvent],
        theme: &Theme,
        focused: bool,
    ) {
        let height = area.height.saturating_sub(2) as usize;
        let (start, end) = self.visible_range(events.len(), height);

        // Calculate available width for content (subtract borders)
        let content_width = area.width.saturating_sub(2) as usize;

        let items: Vec<ListItem> = events[start..end]
            .iter()
            .enumerate()
            .map(|(idx, &tracked)| {
                let actual_idx = start + idx;
                let is_selected = self.selected == Some(actual_idx);

                let mut line = format_event_line(tracked);

                // Truncate with ellipsis if line exceeds available width
                // Use unicode display width (not byte length) for accurate column calculation
                let display_width = line.width();
                if display_width > content_width {
                    // Target width leaves room for ellipsis (1 column)
                    let target_width = content_width.saturating_sub(1);

                    // Find truncation point by accumulating display widths
                    let mut current_width = 0;
                    let mut truncate_at = 0;
                    for (i, c) in line.char_indices() {
                        let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                        if current_width + char_width > target_width {
                            break;
                        }
                        current_width += char_width;
                        truncate_at = i + c.len_utf8();
                    }

                    line.truncate(truncate_at);
                    line.push('…');
                }

                let base_style = event_color_style(&tracked.event, theme);

                // Selected: use theme's selection fg/bg pair for guaranteed contrast
                // Unselected: use event-type color on transparent background
                let style = if is_selected && focused {
                    Style::default()
                        .fg(theme.selection_fg)
                        .bg(theme.selection)
                        .add_modifier(Modifier::BOLD)
                } else {
                    base_style
                };

                ListItem::new(line).style(style)
            })
            .collect();

        // Title shows mode: count only (auto-follow) or position/count [select]
        let title = if events.is_empty() {
            " Events ".to_string()
        } else if let Some(idx) = self.selected {
            // Selection mode: show position
            format!(" Events ({}/{}) [select] ", idx + 1, events.len())
        } else {
            // Auto-follow mode: just show count
            format!(" Events ({}) ", events.len())
        };

        let border_color = theme.panel_border(FocusablePanel::Events, focused);
        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(theme.border_type)
                .border_style(Style::default().fg(border_color))
                .title(title),
        );

        f.render_widget(list, area);

        // Render scrollbar if content overflows
        render_scrollbar_raw(f, area, events.len(), height, start, ScrollbarStyle::Arrows);
    }
}

impl Default for EventsPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Trait Implementations
// ============================================================================

impl Component for EventsPanel {
    fn id(&self) -> ComponentId {
        ComponentId::Events
    }

    fn render(&self, f: &mut Frame, area: Rect, ctx: &RenderContext) {
        // Note: This trait method can't access event data directly.
        // In practice, we use render_with_events() which takes the data.
        // This exists for trait completeness - the view layer handles data passing.

        // Render empty state - actual rendering uses render_with_events
        let border_color = if ctx.is_focused(self.id()) {
            ctx.theme.highlight
        } else {
            ctx.theme.border
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ctx.theme.border_type)
            .border_style(Style::default().fg(border_color))
            .title(" Events ");

        f.render_widget(block, area);
    }
}

impl Scrollable for EventsPanel {
    fn scroll_state(&self) -> &ScrollState {
        &self._scroll
    }

    fn scroll_state_mut(&mut self) -> &mut ScrollState {
        &mut self._scroll
    }

    // Override default implementations with selection-based scrolling
    fn scroll_up(&mut self) {
        if self.event_count == 0 {
            return;
        }

        match self.selected {
            None => {
                // Enter selection mode at last item, then move up
                let last = self.event_count.saturating_sub(1);
                self.selected = Some(last.saturating_sub(1).max(0));
            }
            Some(idx) if idx > 0 => {
                self.selected = Some(idx - 1);
            }
            Some(_) => {} // Already at top
        }
    }

    fn scroll_down(&mut self) {
        if self.event_count == 0 {
            return;
        }

        let last = self.event_count.saturating_sub(1);
        match self.selected {
            None => {
                // Enter selection mode at last item
                self.selected = Some(last);
            }
            Some(idx) if idx < last => {
                self.selected = Some(idx + 1);
            }
            Some(_) => {} // Already at bottom
        }
    }

    fn scroll_to_top(&mut self) {
        if self.event_count > 0 {
            self.selected = Some(0); // Enter selection at top
        }
    }

    fn scroll_to_bottom(&mut self) {
        // Return to auto-follow mode (shows latest)
        self.selected = None;
    }

    // Override visible_range to use selection-based logic
    fn visible_range(&self) -> (usize, usize) {
        // This is overridden, but for completeness: just return (0, 0)
        (0, 0)
    }
}

impl Selectable for EventsPanel {
    fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    fn select(&mut self, index: usize) {
        self.selected = Some(index.min(self.event_count.saturating_sub(1)));
    }

    fn item_count(&self) -> usize {
        self.event_count
    }
}

impl Copyable for EventsPanel {
    fn copy_text(&self) -> Option<String> {
        // Copy summary line for selected event
        // Note: This method can't access events without RenderContext
        // So we provide a helper method below that views can use
        None
    }

    fn copy_data(&self) -> Option<String> {
        // Copy full event as JSONL
        // Note: This method can't access events without RenderContext
        // So we provide a helper method below that views can use
        None
    }
}

impl Interactive for EventsPanel {
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
                // Explicitly select last item so selection highlight is visible
                if self.event_count > 0 {
                    self.selected = Some(self.event_count.saturating_sub(1));
                }
                Handled::Yes
            }
            KeyCode::PageUp => {
                // Page up: move selection up by ~10 items
                if let Some(idx) = self.selected {
                    self.selected = Some(idx.saturating_sub(10));
                } else if self.event_count > 0 {
                    self.selected = Some(self.event_count.saturating_sub(11));
                }
                Handled::Yes
            }
            KeyCode::PageDown => {
                // Page down: move selection down by ~10 items
                let last = self.event_count.saturating_sub(1);
                if let Some(idx) = self.selected {
                    self.selected = Some((idx + 10).min(last));
                } else {
                    self.selected = Some(last);
                }
                Handled::Yes
            }
            KeyCode::Esc => {
                // Clear selection if any, return to auto-follow
                if self.selected.is_some() {
                    self.selected = None;
                    Handled::Yes
                } else {
                    Handled::No // Nothing to clear, let App handle
                }
            }
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn focus_hint(&self) -> Option<&'static str> {
        Some("↑↓:select  g/G:top/end  Enter:detail  y:copy  z:zoom  Esc:follow")
    }
}

impl Zoomable for EventsPanel {
    fn zoom_label(&self) -> &'static str {
        "Events"
    }
}

// Helper methods for copy operations that need event data
impl EventsPanel {
    /// Get formatted text for the selected event (for clipboard)
    pub fn copy_text_with_events(&self, events: &[TrackedEvent]) -> Option<String> {
        self.selected
            .and_then(|idx| events.get(idx))
            .map(format_event_line)
    }

    /// Get JSONL for the selected event (for clipboard)
    pub fn copy_data_with_events(&self, events: &[TrackedEvent]) -> Option<String> {
        self.selected
            .and_then(|idx| events.get(idx))
            .and_then(|tracked| serde_json::to_string(&tracked.event).ok())
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Get appropriate color style for an event
fn event_color_style(event: &ProxyEvent, theme: &Theme) -> Style {
    match event {
        ProxyEvent::ToolCall { .. } => Style::default().fg(theme.tool_call),
        ProxyEvent::ToolResult { success, .. } => {
            if *success {
                Style::default().fg(theme.tool_result_ok)
            } else {
                Style::default().fg(theme.tool_result_fail)
            }
        }
        ProxyEvent::Request { .. } => Style::default().fg(theme.request),
        ProxyEvent::Response { .. } => Style::default().fg(theme.response),
        ProxyEvent::Error { .. } => Style::default()
            .fg(theme.error)
            .add_modifier(Modifier::BOLD),
        ProxyEvent::HeadersCaptured { .. } => Style::default().fg(theme.headers),
        ProxyEvent::RateLimitUpdate { .. } => Style::default().fg(theme.rate_limit),
        ProxyEvent::ApiUsage { .. } => Style::default().fg(theme.api_usage),
        ProxyEvent::Thinking { .. } => Style::default()
            .fg(theme.thinking)
            .add_modifier(Modifier::ITALIC),
        ProxyEvent::ContextCompact { .. } => Style::default()
            .fg(theme.context_compact)
            .add_modifier(Modifier::BOLD),
        ProxyEvent::ThinkingStarted { .. } => Style::default()
            .fg(theme.thinking)
            .add_modifier(Modifier::ITALIC),
        ProxyEvent::UserPrompt { .. } => Style::default()
            .fg(theme.request)
            .add_modifier(Modifier::BOLD),
        ProxyEvent::AssistantResponse { .. } => Style::default()
            .fg(theme.response)
            .add_modifier(Modifier::BOLD),
        ProxyEvent::RequestTransformed { .. } => Style::default()
            .fg(theme.api_usage)
            .add_modifier(Modifier::DIM),
        ProxyEvent::ResponseAugmented { .. } => Style::default()
            .fg(theme.api_usage)
            .add_modifier(Modifier::DIM),
        ProxyEvent::PreCompactHook { .. } => Style::default()
            .fg(theme.context_compact)
            .add_modifier(Modifier::BOLD),
    }
}

/// Convenience render function (backward compatibility)
///
/// Kept for backward compatibility. New code should use `render_filtered`
/// which accepts references for efficient session filtering.
#[allow(dead_code)]
pub fn render(
    f: &mut Frame,
    area: Rect,
    events_panel: &EventsPanel,
    events: &[TrackedEvent],
    theme: &Theme,
    focused: bool,
) {
    events_panel.render_with_events(f, area, events, theme, focused);
}

/// Render with filtered event references (for multi-session support)
pub fn render_filtered(
    f: &mut Frame,
    area: Rect,
    events_panel: &EventsPanel,
    events: &[&TrackedEvent],
    theme: &Theme,
    focused: bool,
) {
    events_panel.render_with_filtered_events(f, area, events, theme, focused);
}
