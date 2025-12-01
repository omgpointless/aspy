//! Thinking panel component
//!
//! Displays Claude's extended thinking/reasoning content with:
//! - Markdown rendering
//! - Auto-follow for streaming content
//! - Scroll support for long thinking blocks

use super::scrollbar::{render_scrollbar, ScrollbarStyle};
use crate::theme::Theme;
use crate::tui::markdown;
use crate::tui::scroll::{FocusablePanel, ScrollState};
use crate::tui::streaming::StreamingState;
use crate::tui::traits::{
    Component, ComponentId, Copyable, Handled, Interactive, RenderContext, Scrollable,
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Rendering state for thinking panel
pub struct ThinkingRenderState<'a> {
    /// The thinking content (None if no thinking yet)
    pub content: Option<&'a str>,
    /// Whether Claude is currently thinking (for animation)
    pub is_thinking: bool,
    /// Animated dots string ("", ".", "..", "...")
    pub thinking_dots: &'a str,
    /// Theme for styling
    pub theme: &'a Theme,
    /// Whether the panel has focus
    pub focused: bool,
}

/// Thinking panel component
///
/// Displays Claude's reasoning/thinking blocks with:
/// - Markdown formatting
/// - Auto-follow for streaming content
/// - Scrollable for long content
pub struct ThinkingPanel {
    /// Scroll state (position, viewport, auto-follow)
    scroll: ScrollState,
}

impl ThinkingPanel {
    /// Create a new thinking panel with auto-follow enabled
    pub fn new() -> Self {
        Self {
            scroll: ScrollState::new(), // Auto-follow by default
        }
    }

    /// Render the thinking panel with content
    ///
    /// # Arguments
    /// * `state` - Rendering state containing content, animation, and styling
    pub fn render_with_content(&mut self, f: &mut Frame, area: Rect, state: &ThinkingRenderState) {
        let height = area.height.saturating_sub(2) as usize;
        let width = area.width.saturating_sub(2) as usize;

        // Parse markdown and convert to styled lines (theme-aware)
        let styled_lines: Vec<Line> = if let Some(text) = state.content {
            markdown::render_markdown(text, width, state.theme)
        } else {
            Vec::new()
        };

        let line_count = styled_lines.len();

        // Update scroll state dimensions
        self.scroll.update_dimensions(line_count, height);
        let scroll_offset = self.scroll.offset();

        // Build title based on state
        let scroll_indicator = if !self.scroll.auto_follow {
            " [scroll]"
        } else {
            ""
        };

        let title = if state.is_thinking {
            format!(" 💭 Thinking{} ", state.thinking_dots)
        } else if state.content.is_some() {
            format!(" 💭 Last thought{} ", scroll_indicator)
        } else {
            " 💭 Thinking ".to_string()
        };

        let border_color = if state.focused {
            state.theme.panel_thinking
        } else {
            state.theme.foreground
        };

        // Use theme.foreground instead of hardcoded Color::White
        // Note: No .wrap() - markdown renderer handles wrapping for accurate line count
        let paragraph = Paragraph::new(styled_lines)
            .style(Style::default().fg(state.theme.foreground))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(state.theme.border_type)
                    .border_style(Style::default().fg(border_color))
                    .title(title),
            )
            .scroll((scroll_offset as u16, 0));

        f.render_widget(paragraph, area);

        // Render scrollbar if content overflows
        render_scrollbar(f, area, &self.scroll, ScrollbarStyle::Arrows);
    }
}

impl Default for ThinkingPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Trait Implementations
// ═══════════════════════════════════════════════════════════════════════════

impl Component for ThinkingPanel {
    fn id(&self) -> ComponentId {
        ComponentId::Thinking
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
            .title(" 💭 Thinking ");

        f.render_widget(block, area);
    }
}

impl Scrollable for ThinkingPanel {
    fn scroll_state(&self) -> &ScrollState {
        &self.scroll
    }

    fn scroll_state_mut(&mut self) -> &mut ScrollState {
        &mut self.scroll
    }
}

impl Copyable for ThinkingPanel {
    fn copy_text(&self) -> Option<String> {
        // Content must be passed in - return None here
        // Actual copy uses get_content_for_copy with the content
        None
    }

    fn copy_description(&self) -> String {
        "thinking content".to_string()
    }
}

impl Interactive for ThinkingPanel {
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

    fn focusable(&self) -> bool {
        true
    }

    fn focus_hint(&self) -> Option<&'static str> {
        Some("↑↓:scroll  y:copy")
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Render Entry Point
// ═══════════════════════════════════════════════════════════════════════════

use crate::tui::app::App;

/// Render the thinking panel using the component owned by App
pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    let content = app.current_thinking_content();
    let state = ThinkingRenderState {
        content: content.as_deref(),
        is_thinking: app.streaming_state() == StreamingState::Thinking,
        thinking_dots: app.thinking_dots(),
        theme: &app.theme,
        focused: app.is_focused(FocusablePanel::Thinking),
    };

    app.thinking_panel.render_with_content(f, area, &state);
}
