// Tokens tab panel for stats view
//
// Displays token usage breakdown using:
// - Grouped BarChart showing input/output/cached tokens per model
// - Cost information and cache savings
// - Token usage sparkline over time

use crate::events::Stats;
use crate::theme::Theme;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Bar, BarChart, BarGroup, Block, Borders, Paragraph, Sparkline},
    Frame,
};

/// Panel displaying token usage statistics
pub struct TokensTabPanel;

impl TokensTabPanel {
    /// Render the panel to a frame
    pub fn render(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        // Split into chart area (70%), sparkline (15%), and summary (15%)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(70),
                Constraint::Percentage(15),
                Constraint::Percentage(15),
            ])
            .split(area);

        // === BarChart: Per-Model Token Breakdown ===
        Self::render_token_chart(frame, chunks[0], stats, theme);

        // === Sparkline: Token Usage Over Time ===
        Self::render_token_sparkline(frame, chunks[1], stats, theme);

        // === Summary: Total Cost & Cache Savings ===
        Self::render_summary(frame, chunks[2], stats, theme);
    }

    fn render_token_chart(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        if stats.model_tokens.is_empty() {
            let placeholder = Paragraph::new("No token usage data yet")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Token Breakdown by Model ")
                        .border_style(theme.border),
                )
                .style(Style::default().fg(theme.muted));
            frame.render_widget(placeholder, area);
            return;
        }

        // Sort models by total tokens (descending)
        let mut models: Vec<_> = stats.model_tokens.iter().collect();
        models.sort_by(|a, b| {
            let a_total = a.1.input + a.1.output + a.1.cache_read;
            let b_total = b.1.input + b.1.output + b.1.cache_read;
            b_total.cmp(&a_total)
        });

        // Create individual bars showing total tokens per model
        // (Grouped bars require more complex layout - using simple bars for MVP)
        let bars: Vec<Bar> = models
            .iter()
            .enumerate()
            .map(|(idx, (model, tokens))| {
                let short_name = Self::shorten_model_name(model);
                let total = tokens.input + tokens.output + tokens.cache_read;
                // Cycle colors
                let color = match idx % 3 {
                    0 => Color::Cyan,
                    1 => Color::Green,
                    _ => Color::Yellow,
                };
                Bar::default()
                    .label(short_name.into())
                    .value(total)
                    .text_value(Self::format_tokens(total))
                    .style(Style::default().fg(color))
            })
            .collect();

        let max_value = models
            .iter()
            .map(|(_, tokens)| tokens.input + tokens.output + tokens.cache_read)
            .max()
            .unwrap_or(1);

        let chart = BarChart::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Total Tokens by Model ")
                    .border_style(theme.border),
            )
            .data(BarGroup::default().bars(&bars))
            .bar_width(8)
            .bar_gap(2)
            .max(max_value)
            .style(Style::default().fg(theme.foreground));

        frame.render_widget(chart, area);
    }

    fn render_token_sparkline(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        if stats.token_history.is_empty() {
            let placeholder = Paragraph::new("No token history yet")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Token Usage Over Time ")
                        .border_style(theme.border),
                )
                .style(Style::default().fg(theme.muted));
            frame.render_widget(placeholder, area);
            return;
        }

        // Use total tokens (input + output + cached) for each snapshot
        let data: Vec<u64> = stats
            .token_history
            .iter()
            .map(|snapshot| snapshot.input + snapshot.output + snapshot.cached)
            .collect();

        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Token Usage Trend ")
                    .border_style(theme.border),
            )
            .data(&data)
            .style(Style::default().fg(theme.highlight));

        frame.render_widget(sparkline, area);
    }

    fn render_summary(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        let total_cost = stats.total_cost();
        let cache_savings = stats.cache_savings();
        let cache_rate = stats.cache_hit_rate();

        let text = vec![Line::from(vec![
            Span::styled("Total Cost: ", Style::default().fg(theme.foreground)),
            Span::styled(
                format!("${:.4}", total_cost),
                Style::default().fg(Color::Green),
            ),
            Span::styled(
                "  |  Cache Savings: ",
                Style::default().fg(theme.foreground),
            ),
            Span::styled(
                format!("${:.4}", cache_savings),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled("  |  Cache Rate: ", Style::default().fg(theme.foreground)),
            Span::styled(
                format!("{:.1}%", cache_rate),
                Style::default().fg(Color::Cyan),
            ),
        ])];

        let summary = Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Cost Summary ")
                .border_style(theme.border),
        );

        frame.render_widget(summary, area);
    }

    /// Format tokens with K/M suffix
    fn format_tokens(tokens: u64) -> String {
        if tokens >= 1_000_000 {
            format!("{:.1}M", tokens as f64 / 1_000_000.0)
        } else if tokens >= 1_000 {
            format!("{:.1}K", tokens as f64 / 1_000.0)
        } else {
            tokens.to_string()
        }
    }

    /// Shorten model name for display
    fn shorten_model_name(model: &str) -> String {
        if model.contains("opus") {
            "Opus".to_string()
        } else if model.contains("sonnet") {
            "Sonnet".to_string()
        } else if model.contains("haiku") {
            "Haiku".to_string()
        } else {
            model.chars().take(10).collect()
        }
    }
}
