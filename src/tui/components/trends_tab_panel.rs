// Trends tab panel for stats view
//
// Displays sparkline trends in a grid layout:
// - Token usage over time (input/output/cached)
// - Tool call frequency progression
// - Cache hit rate trend
// - Thinking token progression

use crate::events::Stats;
use crate::theme::Theme;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Sparkline},
    Frame,
};

/// Panel displaying trend sparklines
pub struct TrendsTabPanel;

impl TrendsTabPanel {
    /// Render the panel to a frame
    pub fn render(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        // Create 2x2 grid layout
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        let top_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[0]);

        let bottom_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[1]);

        // === Top Left: Input Tokens Trend ===
        Self::render_input_tokens_sparkline(frame, top_cols[0], stats, theme);

        // === Top Right: Output Tokens Trend ===
        Self::render_output_tokens_sparkline(frame, top_cols[1], stats, theme);

        // === Bottom Left: Cache Hit Rate Trend ===
        Self::render_cache_rate_sparkline(frame, bottom_cols[0], stats, theme);

        // === Bottom Right: Tool Calls Trend ===
        Self::render_tool_calls_sparkline(frame, bottom_cols[1], stats, theme);
    }

    fn render_input_tokens_sparkline(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        if stats.token_history.is_empty() {
            Self::render_placeholder(frame, area, "No data yet", " Input Tokens ", theme);
            return;
        }

        let data: Vec<u64> = stats
            .token_history
            .iter()
            .map(|snapshot| snapshot.input)
            .collect();

        let max_val = data.iter().max().copied().unwrap_or(1);
        let min_val = data.iter().min().copied().unwrap_or(0);
        let latest = data.last().copied().unwrap_or(0);

        let title = format!(
            " Input Tokens (Latest: {}, Min: {}, Max: {}) ",
            Self::format_tokens(latest),
            Self::format_tokens(min_val),
            Self::format_tokens(max_val)
        );

        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(theme.border),
            )
            .data(&data)
            .style(Style::default().fg(Color::Cyan));

        frame.render_widget(sparkline, area);
    }

    fn render_output_tokens_sparkline(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        if stats.token_history.is_empty() {
            Self::render_placeholder(frame, area, "No data yet", " Output Tokens ", theme);
            return;
        }

        let data: Vec<u64> = stats
            .token_history
            .iter()
            .map(|snapshot| snapshot.output)
            .collect();

        let max_val = data.iter().max().copied().unwrap_or(1);
        let min_val = data.iter().min().copied().unwrap_or(0);
        let latest = data.last().copied().unwrap_or(0);

        let title = format!(
            " Output Tokens (Latest: {}, Min: {}, Max: {}) ",
            Self::format_tokens(latest),
            Self::format_tokens(min_val),
            Self::format_tokens(max_val)
        );

        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(theme.border),
            )
            .data(&data)
            .style(Style::default().fg(Color::Green));

        frame.render_widget(sparkline, area);
    }

    fn render_cache_rate_sparkline(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        if stats.cache_rate_history.is_empty() {
            Self::render_placeholder(frame, area, "No data yet", " Cache Hit Rate ", theme);
            return;
        }

        let data: Vec<u64> = stats
            .cache_rate_history
            .iter()
            .map(|rate| *rate as u64)
            .collect();

        let max_val = data.iter().max().copied().unwrap_or(100);
        let min_val = data.iter().min().copied().unwrap_or(0);
        let latest = data.last().copied().unwrap_or(0);

        let title = format!(
            " Cache Hit Rate % (Latest: {}%, Min: {}%, Max: {}%) ",
            latest, min_val, max_val
        );

        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(theme.border),
            )
            .data(&data)
            .style(Style::default().fg(Color::Yellow));

        frame.render_widget(sparkline, area);
    }

    fn render_tool_calls_sparkline(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        if stats.tool_call_history.is_empty() {
            Self::render_placeholder(frame, area, "No data yet", " Tool Calls ", theme);
            return;
        }

        let data: Vec<u64> = stats
            .tool_call_history
            .iter()
            .map(|count| *count as u64)
            .collect();

        let max_val = data.iter().max().copied().unwrap_or(1);
        let min_val = data.iter().min().copied().unwrap_or(0);
        let latest = data.last().copied().unwrap_or(0);

        let title = format!(
            " Cumulative Tool Calls (Latest: {}, Min: {}, Max: {}) ",
            latest, min_val, max_val
        );

        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(theme.border),
            )
            .data(&data)
            .style(Style::default().fg(Color::Magenta));

        frame.render_widget(sparkline, area);
    }

    fn render_placeholder(
        frame: &mut Frame,
        area: Rect,
        message: &str,
        title: &str,
        theme: &Theme,
    ) {
        let placeholder = Paragraph::new(message)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(theme.border),
            )
            .style(Style::default().fg(theme.muted));
        frame.render_widget(placeholder, area);
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
}
