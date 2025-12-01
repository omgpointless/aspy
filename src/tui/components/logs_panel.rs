//! Logs panel component
//!
//! A self-contained component that displays system log entries.
//! Implements the full trait system: Component, Scrollable, Selectable, Copyable, Interactive.
//!
//! # Architecture
//!
//! This is the first component extracted from the "god object" App pattern.
//! It demonstrates how components:
//! - Own their state (scroll position, selection)
//! - Receive data (log entries) rather than reaching into App
//! - Implement trait contracts for uniform behavior

use super::scrollbar::{render_scrollbar, ScrollbarStyle};
use crate::logging::{LogEntry, LogLevel};
use crate::theme::Theme;
use crate::tui::scroll::ScrollState;
use crate::tui::traits::{
    Component, ComponentId, Copyable, Handled, Interactive, RenderContext, Scrollable, Selectable,
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

/// Logs panel component
///
/// Displays system log entries with:
/// - Color-coded severity levels
/// - Scroll support (auto-follow for streaming logs)
/// - Selection for copy operations
/// - Keyboard navigation
pub struct LogsPanel {
    /// Scroll state (position, viewport, auto-follow)
    scroll: ScrollState,

    /// Selected log entry index (None = auto-follow mode)
    pub selected: Option<usize>,

    /// Cached entry count (for bounds checking)
    /// Public so App can sync it before delegating operations
    pub entry_count: usize,
}

impl LogsPanel {
    /// Create a new logs panel with auto-follow enabled
    pub fn new() -> Self {
        Self {
            scroll: ScrollState::new(), // Auto-follow by default
            selected: None,
            entry_count: 0,
        }
    }

    /// Update with current log entries (call each frame)
    ///
    /// This syncs the component's state with the log buffer.
    /// We don't own the LogBuffer because it's shared with the logging system.
    pub fn sync_entries(&mut self, entries: &[LogEntry], viewport_height: usize) {
        self.entry_count = entries.len();
        self.scroll
            .update_dimensions(entries.len(), viewport_height);

        // Clamp selection to valid range
        if let Some(idx) = self.selected {
            if idx >= entries.len() {
                self.selected = entries.len().checked_sub(1);
            }
        }
    }

    /// Render the logs panel (internal implementation)
    ///
    /// Takes log entries and theme directly - doesn't need full App access.
    pub fn render_with_entries(
        &self,
        f: &mut Frame,
        area: Rect,
        entries: &[LogEntry],
        theme: &Theme,
        focused: bool,
    ) {
        let (start, end) = self.scroll.visible_range();
        let visible_entries: Vec<_> = entries.iter().skip(start).take(end - start).collect();

        // Convert log entries to list items with color coding
        let items: Vec<ListItem> = visible_entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let absolute_idx = start + i;
                let formatted = format_log_entry(entry);
                let base_style = log_level_style(&entry.level, theme);

                // Selected: use theme's selection fg/bg pair for guaranteed contrast
                let style = if focused && self.selected == Some(absolute_idx) {
                    Style::default()
                        .fg(theme.selection_fg)
                        .bg(theme.selection)
                        .add_modifier(Modifier::BOLD)
                } else {
                    base_style
                };

                ListItem::new(formatted).style(style)
            })
            .collect();

        let border_color = if focused {
            theme.panel_logs
        } else {
            theme.foreground
        };

        // Show scroll/selection indicator in title
        let title = if self.selected.is_some() && focused {
            " System Logs [select] "
        } else if self.scroll.auto_follow {
            " System Logs "
        } else {
            " System Logs [scroll] "
        };

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(theme.border_type)
                .border_style(Style::default().fg(border_color))
                .title(title),
        );

        f.render_widget(list, area);

        // Render scrollbar if content overflows
        render_scrollbar(f, area, &self.scroll, ScrollbarStyle::Minimal);
    }

    /// Get formatted text for the selected log entry (for clipboard)
    pub fn selected_entry_text(&self, entries: &[LogEntry]) -> Option<String> {
        self.selected.and_then(|idx| {
            entries.get(idx).map(|entry| {
                format!(
                    "[{}] {:5} {}",
                    entry.timestamp.format("%H:%M:%S"),
                    entry.level.as_str(),
                    entry.message
                )
            })
        })
    }
}

impl Default for LogsPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Trait Implementations
// ═══════════════════════════════════════════════════════════════════════════

impl Component for LogsPanel {
    fn id(&self) -> ComponentId {
        ComponentId::Logs
    }

    fn render(&self, f: &mut Frame, area: Rect, ctx: &RenderContext) {
        // Note: This trait method can't access log entries directly.
        // In practice, we use render_with_entries() which takes the data.
        // This exists for trait completeness - the view layer handles data passing.

        // Render empty state - actual rendering uses render_with_entries
        let border_color = if ctx.is_focused(self.id()) {
            ctx.theme.highlight
        } else {
            ctx.theme.border
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ctx.theme.border_type)
            .border_style(Style::default().fg(border_color))
            .title(" System Logs ");

        f.render_widget(block, area);
    }
}

impl Scrollable for LogsPanel {
    fn scroll_state(&self) -> &ScrollState {
        &self.scroll
    }

    fn scroll_state_mut(&mut self) -> &mut ScrollState {
        &mut self.scroll
    }
}

impl Selectable for LogsPanel {
    fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    fn select(&mut self, index: usize) {
        self.selected = Some(index.min(self.entry_count.saturating_sub(1)));
    }

    fn item_count(&self) -> usize {
        self.entry_count
    }

    /// Override: selecting starts from last entry (most recent)
    fn select_next(&mut self) {
        match self.selected {
            Some(idx) if idx < self.entry_count.saturating_sub(1) => {
                self.selected = Some(idx + 1);
                self.scroll.scroll_down();
            }
            None if self.entry_count > 0 => {
                // First selection: start at bottom (most recent)
                self.selected = Some(self.entry_count.saturating_sub(1));
            }
            _ => {}
        }
    }

    /// Override: selecting starts from last entry (most recent)
    fn select_previous(&mut self) {
        match self.selected {
            Some(idx) if idx > 0 => {
                self.selected = Some(idx - 1);
                self.scroll.scroll_up();
            }
            None if self.entry_count > 0 => {
                // First selection: start at bottom (most recent)
                self.selected = Some(self.entry_count.saturating_sub(1));
            }
            _ => {}
        }
    }
}

impl Copyable for LogsPanel {
    fn copy_text(&self) -> Option<String> {
        // Note: We can't access entries here - need to use selected_entry_text()
        // This is a limitation we'll address when App passes entries
        None // Placeholder - actual copy uses selected_entry_text() with entries
    }

    fn copy_description(&self) -> String {
        "log entry".to_string()
    }
}

impl Interactive for LogsPanel {
    fn handle_key(&mut self, key: KeyEvent) -> Handled {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_previous();
                Handled::Yes
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
                Handled::Yes
            }
            KeyCode::Home => {
                self.scroll_to_top();
                if self.entry_count > 0 {
                    self.selected = Some(0);
                }
                Handled::Yes
            }
            KeyCode::End => {
                self.scroll_to_bottom();
                if self.entry_count > 0 {
                    self.selected = Some(self.entry_count.saturating_sub(1));
                }
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
            KeyCode::Esc => {
                // Clear selection if any, return to auto-follow
                if self.selected.is_some() {
                    self.selected = None;
                    self.scroll.auto_follow = true;
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
        Some("↑↓:select  y:copy  Esc:clear")
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper Functions
// ═══════════════════════════════════════════════════════════════════════════

/// Format a log entry for display
fn format_log_entry(entry: &LogEntry) -> String {
    format!(
        "[{}] {:5} {}",
        entry.timestamp.format("%H:%M:%S"),
        entry.level.as_str(),
        entry.message
    )
}

/// Get color style for log level
fn log_level_style(level: &LogLevel, theme: &Theme) -> Style {
    match level {
        LogLevel::Error => Style::default()
            .fg(theme.error)
            .add_modifier(Modifier::BOLD),
        LogLevel::Warn => Style::default().fg(theme.rate_limit),
        LogLevel::Info => Style::default().fg(theme.api_usage),
        LogLevel::Debug => Style::default().fg(theme.headers),
        LogLevel::Trace => Style::default().fg(theme.headers),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Render Entry Point
// ═══════════════════════════════════════════════════════════════════════════

use crate::tui::app::App;
use crate::tui::scroll::FocusablePanel;

/// Render the logs panel using the component owned by App
///
/// This is the main render entry point called by views/mod.rs.
/// The component (app.logs_panel) owns its state - we just sync
/// the entry count and call render.
pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    let height = area.height.saturating_sub(2) as usize;
    let entries = app.log_buffer.get_all();
    let focused = app.is_focused(FocusablePanel::Logs);

    // Sync dimensions with current data
    app.logs_panel.sync_entries(&entries, height);

    // Render using the component's method
    app.logs_panel
        .render_with_entries(f, area, &entries, &app.theme, focused);
}
