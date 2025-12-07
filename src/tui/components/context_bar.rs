// Context bar component
//
// Renders a gauge showing context window usage (tokens used / limit).

use super::formatters::format_number;
use crate::tui::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Gauge,
    Frame,
};

/// Render the context window usage bar
///
/// Shows:
/// - Current tokens / limit with percentage
/// - Color-coded fill based on usage level
/// - Special "compact pending" state when over limit
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    // Use effective_context() to get the selected session's context (or global fallback)
    let ctx = app.effective_context();

    let (label, pct, color) = if ctx.current_tokens > 0 {
        let pct = ctx.percentage();
        let over_limit = pct >= 100.0;

        let color = if over_limit {
            // Over limit: compact is pending, use warning color
            app.theme.context_bar_warn
        } else if pct >= 90.0 {
            app.theme.context_bar_danger
        } else if pct >= 70.0 {
            app.theme.context_bar_warn
        } else {
            app.theme.context_bar_fill
        };

        let label = if over_limit {
            // Don't show embarrassing >100%, signal compact is pending
            format!(
                "Context: {} / {} (~100%, compact pending)",
                format_number(ctx.current_tokens),
                format_number(ctx.limit),
            )
        } else {
            format!(
                "Context: {} / {} ({:.1}%)",
                format_number(ctx.current_tokens),
                format_number(ctx.limit),
                pct
            )
        };
        (label, pct.min(100.0), color) // Cap display at 100%
    } else {
        (
            "Context: waiting for API call...".to_string(),
            0.0,
            Color::DarkGray,
        )
    };

    // Let ratatui's gauge handle color inversion at fill boundary
    // gauge_style fg/bg get swapped in the filled portion for label area
    let gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(color)
                .bg(app.theme.background)
                .add_modifier(Modifier::BOLD),
        )
        .percent(pct as u16)
        .label(label);

    f.render_widget(gauge, area);
}
