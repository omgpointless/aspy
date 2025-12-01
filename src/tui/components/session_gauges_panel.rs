// Session health gauges panel
//
// Displays three stacked gauges for key session metrics:
// - Cache hit rate (higher is better)
// - Context window usage (monitor for compacts)
// - Request success rate (reliability indicator)

use crate::events::Stats;
use crate::theme::Theme;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Gauge},
    Frame,
};

/// Panel displaying session health gauges
pub struct SessionGaugesPanel;

impl SessionGaugesPanel {
    /// Render the panel to a frame
    pub fn render(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        // Split into 3 equal vertical sections for gauges
        let gauge_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34), // Extra percent for rounding
            ])
            .split(area);

        // === Cache Hit Rate Gauge ===
        let cache_rate = stats.cache_hit_rate();
        let cache_color = match cache_rate as u8 {
            90..=100 => Color::Green,
            70..=89 => Color::Yellow,
            _ => Color::Red,
        };
        let cache_label = format!("{:.1}% (${:.3} saved)", cache_rate, stats.cache_savings());
        let cache_gauge = Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Cache Hit Rate ")
                    .border_style(theme.border),
            )
            .gauge_style(Style::default().fg(cache_color))
            .ratio(cache_rate / 100.0)
            .label(cache_label);
        frame.render_widget(cache_gauge, gauge_chunks[0]);

        // === Context Window Gauge ===
        let context_pct = stats.context_usage_percent().unwrap_or(0.0);
        let context_color = match context_pct as u8 {
            0..=69 => Color::Green,
            70..=84 => Color::Yellow,
            _ => Color::Red,
        };
        let context_label = format!(
            "{:.0}K / {:.0}K ({:.1}%)",
            stats.current_context_tokens as f64 / 1000.0,
            stats.context_limit() as f64 / 1000.0,
            context_pct
        );
        let context_gauge = Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Context Window ")
                    .border_style(theme.border),
            )
            .gauge_style(Style::default().fg(context_color))
            .ratio(context_pct / 100.0)
            .label(context_label);
        frame.render_widget(context_gauge, gauge_chunks[1]);

        // === Success Rate Gauge ===
        let success_rate = stats.success_rate();
        let success_color = match success_rate as u8 {
            95..=100 => Color::Green,
            90..=94 => Color::Yellow,
            _ => Color::Red,
        };
        let success_label = format!("{:.1}%", success_rate);
        let success_gauge = Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Request Success ")
                    .border_style(theme.border),
            )
            .gauge_style(Style::default().fg(success_color))
            .ratio(success_rate / 100.0)
            .label(success_label);
        frame.render_widget(success_gauge, gauge_chunks[2]);
    }
}
