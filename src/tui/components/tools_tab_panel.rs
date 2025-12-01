// Tools tab panel for stats view
//
// Displays tool usage statistics using:
// - BarChart showing call frequency by tool name
// - Duration analysis showing average execution time
// - Success rate indicator

use crate::events::Stats;
use crate::theme::Theme;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Bar, BarChart, BarGroup, Block, Borders, Paragraph},
    Frame,
};

/// Panel displaying tool usage statistics
pub struct ToolsTabPanel;

impl ToolsTabPanel {
    /// Render the panel to a frame
    pub fn render(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        // Split into frequency chart (60%), duration chart (30%), and summary (10%)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(60),
                Constraint::Percentage(30),
                Constraint::Percentage(10),
            ])
            .split(area);

        // === BarChart: Tool Call Frequency ===
        Self::render_frequency_chart(frame, chunks[0], stats, theme);

        // === BarChart: Average Duration ===
        Self::render_duration_chart(frame, chunks[1], stats, theme);

        // === Summary: Success Rate ===
        Self::render_summary(frame, chunks[2], stats, theme);
    }

    fn render_frequency_chart(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        if stats.tool_calls_by_name.is_empty() {
            let placeholder = Paragraph::new("No tool calls yet")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Tool Call Frequency ")
                        .border_style(theme.border),
                )
                .style(Style::default().fg(theme.muted));
            frame.render_widget(placeholder, area);
            return;
        }

        // Sort tools by call count (descending)
        let mut tools: Vec<_> = stats.tool_calls_by_name.iter().collect();
        tools.sort_by(|a, b| b.1.cmp(a.1));

        // Take top 10 tools to avoid overcrowding
        let top_tools: Vec<_> = tools.iter().take(10).collect();

        // Create bars
        let bars: Vec<Bar> = top_tools
            .iter()
            .enumerate()
            .map(|(idx, (tool_name, count))| {
                let color = Self::tool_color(idx);
                Bar::default()
                    .label((*tool_name).clone().into())
                    .value(**count as u64)
                    .style(Style::default().fg(color))
            })
            .collect();

        let max_value = top_tools
            .iter()
            .map(|(_, count)| **count)
            .max()
            .unwrap_or(1) as u64;

        let chart = BarChart::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Tool Call Frequency (Top 10) ")
                    .border_style(theme.border),
            )
            .data(BarGroup::default().bars(&bars))
            .bar_width(6)
            .bar_gap(1)
            .max(max_value)
            .style(Style::default().fg(theme.foreground));

        frame.render_widget(chart, area);
    }

    fn render_duration_chart(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        if stats.tool_durations_ms.is_empty() {
            let placeholder = Paragraph::new("No tool duration data yet")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Average Execution Time ")
                        .border_style(theme.border),
                )
                .style(Style::default().fg(theme.muted));
            frame.render_widget(placeholder, area);
            return;
        }

        // Calculate average duration for each tool
        let mut durations: Vec<(String, u64)> = stats
            .tool_durations_ms
            .iter()
            .filter_map(|(tool_name, durations)| {
                if durations.is_empty() {
                    None
                } else {
                    let avg = durations.iter().sum::<u64>() / durations.len() as u64;
                    Some((tool_name.clone(), avg))
                }
            })
            .collect();

        // Sort by duration (descending)
        durations.sort_by(|a, b| b.1.cmp(&a.1));

        // Take top 10
        let top_durations: Vec<_> = durations.iter().take(10).collect();

        // Create bars
        let bars: Vec<Bar> = top_durations
            .iter()
            .map(|(tool_name, avg_ms)| {
                let color = Self::duration_color(*avg_ms);
                let label = format!("{}ms", avg_ms);
                Bar::default()
                    .label(tool_name.clone().into())
                    .value(*avg_ms)
                    .text_value(label)
                    .style(Style::default().fg(color))
            })
            .collect();

        let max_value = top_durations.iter().map(|(_, ms)| *ms).max().unwrap_or(1);

        let chart = BarChart::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Average Execution Time (ms) ")
                    .border_style(theme.border),
            )
            .data(BarGroup::default().bars(&bars))
            .bar_width(6)
            .bar_gap(1)
            .max(max_value)
            .style(Style::default().fg(theme.foreground));

        frame.render_widget(chart, area);
    }

    fn render_summary(frame: &mut Frame, area: Rect, stats: &Stats, theme: &Theme) {
        let total_calls = stats.total_tool_calls;
        let failed_calls = stats.failed_tool_calls;
        let success_rate = if total_calls == 0 {
            100.0
        } else {
            ((total_calls - failed_calls) as f64 / total_calls as f64) * 100.0
        };

        let success_color = if success_rate >= 95.0 {
            Color::Green
        } else if success_rate >= 90.0 {
            Color::Yellow
        } else {
            Color::Red
        };

        let text = vec![Line::from(vec![
            Span::styled("Total Calls: ", Style::default().fg(theme.foreground)),
            Span::styled(format!("{}", total_calls), Style::default().fg(Color::Cyan)),
            Span::styled("  |  Failed: ", Style::default().fg(theme.foreground)),
            Span::styled(format!("{}", failed_calls), Style::default().fg(Color::Red)),
            Span::styled("  |  Success Rate: ", Style::default().fg(theme.foreground)),
            Span::styled(
                format!("{:.1}%", success_rate),
                Style::default().fg(success_color),
            ),
        ])];

        let summary = Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Summary ")
                .border_style(theme.border),
        );

        frame.render_widget(summary, area);
    }

    /// Get color for tool by index (cycles through palette)
    fn tool_color(idx: usize) -> Color {
        const COLORS: [Color; 8] = [
            Color::Cyan,
            Color::Green,
            Color::Yellow,
            Color::Magenta,
            Color::Blue,
            Color::Red,
            Color::LightCyan,
            Color::LightGreen,
        ];
        COLORS[idx % COLORS.len()]
    }

    /// Get color based on duration (green=fast, yellow=medium, red=slow)
    fn duration_color(ms: u64) -> Color {
        match ms {
            0..=100 => Color::Green,
            101..=500 => Color::Yellow,
            _ => Color::Red,
        }
    }
}
