// Models tab panel for stats view
//
// Displays model distribution using:
// - BarChart showing API calls per model
// - Sparkline showing recent activity trend
// - Model usage summary text

use crate::events::Stats;
use crate::theme::Theme;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Bar, BarChart, BarGroup, Block, Borders, Paragraph, Sparkline},
    Frame,
};

/// Panel displaying model usage statistics
pub struct ModelsTabPanel;

impl ModelsTabPanel {
    /// Render the panel to a frame
    pub fn render(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        // Split into top chart area (75%) and bottom sparkline (25%)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
            .split(area);

        // === BarChart: Model Distribution ===
        Self::render_model_chart(frame, chunks[0], stats, theme);

        // === Sparkline: Recent Activity ===
        Self::render_activity_sparkline(frame, chunks[1], stats, theme);
    }

    fn render_model_chart(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        if stats.model_calls.is_empty() {
            let placeholder = Paragraph::new("No model usage data yet")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Model Distribution ")
                        .border_style(theme.border),
                )
                .style(Style::default().fg(theme.muted));
            frame.render_widget(placeholder, area);
            return;
        }

        // Sort models by call count (descending)
        let mut models: Vec<_> = stats.model_calls.iter().collect();
        models.sort_by(|a, b| b.1.cmp(a.1));

        // Create bars for each model
        let bars: Vec<Bar> = models
            .iter()
            .enumerate()
            .map(|(idx, (model, count))| {
                // Shorten model name for display
                let short_name = Self::shorten_model_name(model);
                // Cycle through colors
                let color = Self::model_color(idx);
                Bar::default()
                    .label(short_name.into())
                    .value(**count as u64)
                    .style(Style::default().fg(color))
            })
            .collect();

        let max_value = models.iter().map(|(_, count)| **count).max().unwrap_or(1) as u64;

        let chart = BarChart::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Model Distribution (API Calls) ")
                    .border_style(theme.border),
            )
            .data(BarGroup::default().bars(&bars))
            .bar_width(8)
            .bar_gap(2)
            .max(max_value)
            .style(Style::default().fg(theme.foreground));

        frame.render_widget(chart, area);
    }

    fn render_activity_sparkline(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        if stats.tool_call_history.is_empty() {
            let placeholder = Paragraph::new("No activity history yet")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Recent Activity ")
                        .border_style(theme.border),
                )
                .style(Style::default().fg(theme.muted));
            frame.render_widget(placeholder, area);
            return;
        }

        // Convert tool call history to sparkline data
        let data: Vec<u64> = stats
            .tool_call_history
            .iter()
            .map(|count| *count as u64)
            .collect();

        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Recent Activity (Tool Calls Over Time) ")
                    .border_style(theme.border),
            )
            .data(&data)
            .style(Style::default().fg(theme.highlight));

        frame.render_widget(sparkline, area);
    }

    /// Shorten model name for display (e.g., "claude-opus-4-5-..." -> "Opus 4.5")
    fn shorten_model_name(model: &str) -> String {
        if model.contains("opus") {
            "Opus".to_string()
        } else if model.contains("sonnet") {
            "Sonnet".to_string()
        } else if model.contains("haiku") {
            "Haiku".to_string()
        } else {
            // Fallback: take first 10 chars
            model.chars().take(10).collect()
        }
    }

    /// Get color for model by index (cycles through palette)
    fn model_color(idx: usize) -> ratatui::style::Color {
        use ratatui::style::Color;
        const COLORS: [Color; 6] = [
            Color::Cyan,
            Color::Magenta,
            Color::Yellow,
            Color::Green,
            Color::Blue,
            Color::Red,
        ];
        COLORS[idx % COLORS.len()]
    }
}
