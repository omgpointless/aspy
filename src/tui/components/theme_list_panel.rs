//! Theme list panel component for Settings view
//!
//! A scrollable list of available themes with selection support.
//! Implements the trait system: Component, Scrollable, Selectable, Interactive.

use crate::theme::Theme;
use crate::tui::scroll::ScrollState;
use crate::tui::traits::{
    Component, ComponentId, Handled, Interactive, RenderContext, Scrollable, Selectable,
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

/// Context for rendering the theme list
/// Groups related parameters to avoid too_many_arguments warning
pub struct ThemeRenderContext<'a> {
    pub themes: &'a [String],
    pub current_theme: &'a str,
    pub use_theme_bg: bool,
    pub theme: &'a Theme,
    pub focused: bool,
}

/// Theme list panel for the Appearance settings
///
/// Displays available themes with:
/// - Scrollable list (shows viewport_height items)
/// - Selection tracking
/// - Current theme indicator (●)
/// - Background toggle option at the end
pub struct ThemeListPanel {
    /// Scroll state (for trait compliance - we track offset manually)
    scroll: ScrollState,

    /// Selected option index
    pub selected: usize,

    /// Scroll offset (first visible item)
    offset: usize,

    /// Viewport height (items visible)
    viewport: usize,

    /// Cached theme count (for bounds checking)
    theme_count: usize,

    /// Whether to include background toggle as last item
    include_bg_toggle: bool,
}

impl ThemeListPanel {
    pub fn new() -> Self {
        Self {
            scroll: ScrollState::new(),
            selected: 0,
            offset: 0,
            viewport: 10, // default, updated in sync
            theme_count: 0,
            include_bg_toggle: true,
        }
    }

    /// Sync with current theme list (call before render)
    pub fn sync_themes(&mut self, theme_count: usize, viewport_height: usize) {
        self.theme_count = theme_count;
        self.viewport = viewport_height.saturating_sub(3); // borders + spacer

        // Clamp selection
        let max = self.total_items().saturating_sub(1);
        if self.selected > max {
            self.selected = max;
        }

        // Ensure selected is visible
        self.ensure_visible();
    }

    /// Scroll to and select the theme matching the given name
    /// Call this when entering settings to show the current theme
    pub fn scroll_to_theme(&mut self, themes: &[String], current_theme: &str) {
        if let Some(index) = themes.iter().position(|t| t == current_theme) {
            self.selected = index;
            self.ensure_visible();
        }
    }

    /// Total item count (themes + optional bg toggle)
    fn total_items(&self) -> usize {
        if self.include_bg_toggle {
            self.theme_count + 1
        } else {
            self.theme_count
        }
    }

    /// Check if selected index is the background toggle
    #[allow(dead_code)] // Used when Enter applies selection
    pub fn is_bg_toggle_selected(&self) -> bool {
        self.include_bg_toggle && self.selected == self.theme_count
    }

    /// Get selected theme name (None if bg toggle selected)
    #[allow(dead_code)] // Used when Enter applies selection
    pub fn selected_theme<'a>(&self, themes: &'a [String]) -> Option<&'a str> {
        if self.is_bg_toggle_selected() {
            None
        } else {
            themes.get(self.selected).map(|s| s.as_str())
        }
    }

    /// Render the theme list with context
    pub fn render_with_context(&self, f: &mut Frame, area: Rect, ctx: &ThemeRenderContext) {
        let end = (self.offset + self.viewport).min(self.total_items());

        let mut items: Vec<ListItem> = Vec::new();

        // Render visible theme items using enumerate for cleaner iteration
        let visible_end = end.min(ctx.themes.len());
        for (i, theme_name) in ctx
            .themes
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(visible_end.saturating_sub(self.offset))
        {
            let is_current = theme_name == ctx.current_theme;
            let is_selected = ctx.focused && i == self.selected;

            let prefix = if is_current { " ● " } else { "   " };

            let style = if is_selected {
                Style::default()
                    .bg(ctx.theme.selection)
                    .fg(ctx.theme.selection_fg)
                    .add_modifier(Modifier::BOLD)
            } else if is_current {
                Style::default()
                    .fg(ctx.theme.tool_result_ok)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(ctx.theme.foreground)
            };

            items.push(ListItem::new(format!("{}{}", prefix, theme_name)).style(style));
        }

        // Add background toggle if visible
        if self.include_bg_toggle {
            let bg_index = self.theme_count;
            if bg_index >= self.offset && bg_index < end {
                // Add spacer before toggle if we just finished themes
                if !items.is_empty() && self.offset <= ctx.themes.len() {
                    items.push(ListItem::new(""));
                }

                let is_selected = ctx.focused && self.selected == bg_index;
                let bg_value = if ctx.use_theme_bg { "Yes" } else { "No" };

                let style = if is_selected {
                    Style::default()
                        .bg(ctx.theme.selection)
                        .fg(ctx.theme.selection_fg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(ctx.theme.foreground)
                };

                items.push(
                    ListItem::new(format!("   Use theme background: {}", bg_value)).style(style),
                );
            }
        }

        let border_color = if ctx.focused {
            ctx.theme.tool_result_ok
        } else {
            ctx.theme.border
        };

        let title = if ctx.focused {
            format!(
                " Appearance ({}/{}) ↑↓ Enter ",
                self.selected + 1,
                self.total_items()
            )
        } else {
            " Appearance ".to_string()
        };

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(ctx.theme.border_type)
                .border_style(Style::default().fg(border_color))
                .title(title),
        );

        f.render_widget(list, area);
    }

    /// Ensure selected item is visible
    fn ensure_visible(&mut self) {
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + self.viewport {
            self.offset = self
                .selected
                .saturating_sub(self.viewport.saturating_sub(1));
        }
    }
}

impl Default for ThemeListPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Trait Implementations
// ═══════════════════════════════════════════════════════════════════════════

impl Component for ThemeListPanel {
    fn id(&self) -> ComponentId {
        ComponentId::Events // Reusing - could add ComponentId::Settings later
    }

    fn render(&self, f: &mut Frame, area: Rect, ctx: &RenderContext) {
        // Placeholder - actual render uses render_with_themes
        let border_color = if ctx.is_focused(self.id()) {
            ctx.theme.highlight
        } else {
            ctx.theme.border
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ctx.theme.border_type)
            .border_style(Style::default().fg(border_color))
            .title(" Appearance ");

        f.render_widget(block, area);
    }
}

impl Scrollable for ThemeListPanel {
    fn scroll_state(&self) -> &ScrollState {
        &self.scroll
    }

    fn scroll_state_mut(&mut self) -> &mut ScrollState {
        &mut self.scroll
    }
}

impl Selectable for ThemeListPanel {
    fn selected_index(&self) -> Option<usize> {
        Some(self.selected)
    }

    fn select(&mut self, index: usize) {
        self.selected = index.min(self.total_items().saturating_sub(1));
        self.ensure_visible();
    }

    fn item_count(&self) -> usize {
        self.total_items()
    }
}

impl Interactive for ThemeListPanel {
    fn handle_key(&mut self, key: KeyEvent) -> Handled {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                    self.ensure_visible();
                }
                Handled::Yes
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.total_items().saturating_sub(1);
                if self.selected < max {
                    self.selected += 1;
                    self.ensure_visible();
                }
                Handled::Yes
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.selected = 0;
                self.ensure_visible();
                Handled::Yes
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.selected = self.total_items().saturating_sub(1);
                self.ensure_visible();
                Handled::Yes
            }
            KeyCode::PageUp => {
                let page = self.viewport.max(1);
                self.selected = self.selected.saturating_sub(page);
                self.ensure_visible();
                Handled::Yes
            }
            KeyCode::PageDown => {
                let page = self.viewport.max(1);
                let max = self.total_items().saturating_sub(1);
                self.selected = (self.selected + page).min(max);
                self.ensure_visible();
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn focus_hint(&self) -> Option<&'static str> {
        Some("↑↓:select  Enter:apply")
    }
}
