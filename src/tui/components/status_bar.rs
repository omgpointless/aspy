// Status bar component
//
// Renders statistics at the bottom: uptime, requests, tools, success rate, cost.

use super::formatters::format_compact_number;
use crate::tui::app::App;
use crate::tui::layout::Breakpoint;
use ratatui::{
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Render the status bar with session statistics
///
/// Adapts to terminal width:
/// - Wide: Full format with labels
/// - Narrow: Compact icon-based format
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let stats = &app.stats;
    let bp = Breakpoint::from_width(area.width);

    let status_text = if !bp.at_least(Breakpoint::Wide) {
        // Compact format for narrow terminals
        let token_info = if stats.total_input_tokens > 0 || stats.total_output_tokens > 0 {
            let cache_info = if stats.total_cache_read_tokens > 0 {
                format!(" 📦{:.0}%", stats.cache_hit_rate())
            } else {
                String::new()
            };
            format!(
                " │ {}/{}{}│ ${:.2}",
                format_compact_number(stats.total_input_tokens),
                format_compact_number(stats.total_output_tokens),
                cache_info,
                stats.total_cost()
            )
        } else {
            String::new()
        };

        format!(
            " {} │ 📡 {} │ 🔧 {} │ ✅ {:.0}% │ ~{:.0}ms{}",
            app.uptime(),
            stats.total_requests,
            stats.total_tool_calls,
            stats.success_rate(),
            stats.avg_ttfb().as_millis(),
            token_info,
        )
    } else {
        // Full format for wide terminals
        let token_info = if stats.total_input_tokens > 0 || stats.total_output_tokens > 0 {
            let cache_info = if stats.total_cache_read_tokens > 0 {
                format!(
                    " │ 📦 {} ({:.0}%)",
                    format_compact_number(stats.total_cache_read_tokens),
                    stats.cache_hit_rate()
                )
            } else {
                String::new()
            };
            format!(
                " │ {}/{}{} │ ${:.2}",
                format_compact_number(stats.total_input_tokens),
                format_compact_number(stats.total_output_tokens),
                cache_info,
                stats.total_cost()
            )
        } else {
            String::new()
        };

        let tools_info = if stats.failed_tool_calls > 0 {
            format!(
                "🔧 {} ✗ {}",
                stats.total_tool_calls, stats.failed_tool_calls
            )
        } else {
            format!("🔧 {}", stats.total_tool_calls)
        };

        format!(
            " {} │ 📡 {} │ {} │ ✅ {:.1}% │ ~{}ms{}",
            app.uptime(),
            stats.total_requests,
            tools_info,
            stats.success_rate(),
            stats.avg_ttfb().as_millis(),
            token_info,
        )
    };

    let status = Paragraph::new(status_text)
        .style(Style::default().fg(app.theme.status_bar))
        .block(Block::default().borders(Borders::TOP));

    f.render_widget(status, area);
}
