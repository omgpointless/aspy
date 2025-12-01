//! Settings panel component
//!
//! Owns all state for the Settings view:
//! - Category navigation (Appearance, Layout)
//! - Focus tracking (categories vs options pane)
//! - Option selection within each category
//! - Dirty flag for save-on-exit
//!
//! Follows the "components own their state" pattern from CLAUDE.md.

use super::theme_list_panel::ThemeListPanel;
use crate::tui::traits::{Component, ComponentId, Handled, Interactive, RenderContext};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::Style,
    widgets::{Block, Borders},
    Frame,
};

/// Settings categories for navigation
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SettingsCategory {
    #[default]
    Appearance,
    Layout,
}

impl SettingsCategory {
    /// Get next category (wraps at end)
    pub fn next(self) -> Self {
        match self {
            SettingsCategory::Appearance => SettingsCategory::Layout,
            SettingsCategory::Layout => SettingsCategory::Layout, // Stay at end
        }
    }

    /// Get previous category (wraps at start)
    pub fn prev(self) -> Self {
        match self {
            SettingsCategory::Appearance => SettingsCategory::Appearance, // Stay at start
            SettingsCategory::Layout => SettingsCategory::Appearance,
        }
    }
}

/// Which pane is focused in Settings view
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SettingsFocus {
    #[default]
    Categories,
    Options,
}

impl SettingsFocus {
    /// Toggle between panes
    pub fn toggle(self) -> Self {
        match self {
            SettingsFocus::Categories => SettingsFocus::Options,
            SettingsFocus::Options => SettingsFocus::Categories,
        }
    }
}

/// Settings panel component
///
/// Owns all settings view state including nested components:
/// - Category and focus navigation
/// - ThemeListPanel for Appearance options
/// - Layout preset selection
pub struct SettingsPanel {
    /// Which category is selected in the left nav
    pub category: SettingsCategory,

    /// Which pane has focus (categories or options)
    pub focus: SettingsFocus,

    /// Selected option index within Layout category
    /// (Appearance uses ThemeListPanel.selected instead)
    pub layout_option_index: usize,

    /// Track if settings changed (for save on exit)
    pub dirty: bool,

    /// Theme list panel for Appearance category (nested component)
    pub theme_list: ThemeListPanel,
}

impl SettingsPanel {
    pub fn new() -> Self {
        Self {
            category: SettingsCategory::default(),
            focus: SettingsFocus::default(),
            layout_option_index: 0,
            dirty: false,
            theme_list: ThemeListPanel::new(),
        }
    }

    /// Toggle focus between categories and options panes
    pub fn toggle_focus(&mut self) {
        self.focus = self.focus.toggle();
    }

    /// Move to next category
    pub fn next_category(&mut self) {
        self.category = self.category.next();
        self.layout_option_index = 0; // Reset option selection
    }

    /// Move to previous category
    pub fn prev_category(&mut self) {
        self.category = self.category.prev();
        self.layout_option_index = 0; // Reset option selection
    }

    /// Mark settings as dirty (changed)
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Clear dirty flag (after save)
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Check if settings have unsaved changes
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Get selected theme index (from nested ThemeListPanel)
    pub fn selected_theme_index(&self) -> usize {
        self.theme_list.selected
    }

    /// Check if background toggle is selected in Appearance
    #[allow(dead_code)] // Utility method for future use
    pub fn is_bg_toggle_selected(&self) -> bool {
        self.theme_list.is_bg_toggle_selected()
    }

    /// Sync theme list with current theme count and viewport
    pub fn sync_themes(&mut self, theme_count: usize, viewport_height: usize) {
        self.theme_list.sync_themes(theme_count, viewport_height);
    }

    /// Scroll to and select the current theme (call when entering Settings)
    pub fn scroll_to_current_theme(&mut self, themes: &[String], current_theme: &str) {
        self.theme_list.scroll_to_theme(themes, current_theme);
    }

    /// Handle key input for Layout options (up/down selection)
    fn handle_layout_key(&mut self, key: KeyEvent) -> Handled {
        const PRESET_COUNT: usize = 3; // classic, reasoning, debug

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.layout_option_index = self.layout_option_index.saturating_sub(1);
                Handled::Yes
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.layout_option_index < PRESET_COUNT - 1 {
                    self.layout_option_index += 1;
                }
                Handled::Yes
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.layout_option_index = 0;
                Handled::Yes
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.layout_option_index = PRESET_COUNT - 1;
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    /// Handle key input for category navigation
    fn handle_category_key(&mut self, key: KeyEvent) -> Handled {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.prev_category();
                Handled::Yes
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.next_category();
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

impl Default for SettingsPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Trait Implementations
// ═══════════════════════════════════════════════════════════════════════════

impl Component for SettingsPanel {
    fn id(&self) -> ComponentId {
        ComponentId::Events // Reusing - could add ComponentId::Settings later
    }

    fn render(&self, f: &mut Frame, area: Rect, ctx: &RenderContext) {
        // Placeholder - actual rendering is done by views/settings.rs
        let border_color = if ctx.is_focused(self.id()) {
            ctx.theme.highlight
        } else {
            ctx.theme.border
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ctx.theme.border_type)
            .border_style(Style::default().fg(border_color))
            .title(" Settings ");

        f.render_widget(block, area);
    }
}

impl Interactive for SettingsPanel {
    fn handle_key(&mut self, key: KeyEvent) -> Handled {
        match self.focus {
            SettingsFocus::Categories => self.handle_category_key(key),
            SettingsFocus::Options => match self.category {
                SettingsCategory::Appearance => self.theme_list.handle_key(key),
                SettingsCategory::Layout => self.handle_layout_key(key),
            },
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn focus_hint(&self) -> Option<&'static str> {
        match self.focus {
            SettingsFocus::Categories => Some("↑↓:category  Tab/→:options"),
            SettingsFocus::Options => match self.category {
                SettingsCategory::Appearance => Some("↑↓:select  Enter:apply  Tab/←:back"),
                SettingsCategory::Layout => Some("↑↓:select  Enter:apply  Tab/←:back"),
            },
        }
    }
}
