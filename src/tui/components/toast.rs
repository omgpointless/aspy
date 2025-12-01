//! Toast notification component
//!
//! A non-blocking overlay that auto-dismisses after a configurable duration.
//! Renders in the bottom-right corner on top of all other content.

use crate::theme::Theme;
use ratatui::{
    layout::{Alignment, Rect},
    style::Style,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::time::{Duration, Instant};

/// A toast notification that auto-dismisses
pub struct Toast {
    /// Message to display
    pub message: String,
    /// When the toast was created
    created_at: Instant,
    /// How long to show the toast
    duration: Duration,
}

impl Toast {
    /// Create a new toast with default 2-second duration
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            created_at: Instant::now(),
            duration: Duration::from_secs(2),
        }
    }

    /// Check if the toast has expired and should be removed
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.duration
    }

    /// Render the toast in the bottom-right corner
    ///
    /// Uses `Clear` widget to ensure toast is visible on top of other content.
    pub fn render(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        // Calculate toast dimensions
        // Add 4 for padding (2 chars each side) and border
        let width = (self.message.len() as u16 + 4).min(area.width.saturating_sub(4));
        let height = 3; // 1 line of text + 2 for borders

        // Position: bottom-right corner, offset by 2 cells from edge
        let x = area.right().saturating_sub(width + 2);
        let y = area.bottom().saturating_sub(height + 2);

        let toast_area = Rect::new(x, y, width, height);

        // Style: highlight border, themed background
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(theme.border_type)
            .border_style(Style::default().fg(theme.highlight))
            .style(Style::default().bg(theme.background));

        let text = Paragraph::new(self.message.as_str())
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme.foreground))
            .block(block);

        // Clear the area first so toast appears on top
        f.render_widget(Clear, toast_area);
        f.render_widget(text, toast_area);
    }
}
