// Stats view - tabbed dashboard with rich visualizations
//
// Displays a 5-tab dashboard:
// - Overview: Session gauges + summary
// - Models: API call distribution with BarChart and sparkline
// - Tokens: Token usage breakdown with grouped bars
// - Tools: Tool call frequency and duration analysis
// - Trends: Sparklines grid showing trends over time

use crate::tui::{
    app::App,
    components::{
        models_tab_panel::ModelsTabPanel, session_gauges_panel::SessionGaugesPanel,
        tokens_tab_panel::TokensTabPanel, tools_tab_panel::ToolsTabPanel,
        trends_tab_panel::TrendsTabPanel,
    },
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};

// Import shared formatters from components
use super::super::components::{format_compact_number, format_number};

/// Main render function for the Stats view
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    // Split into tab bar (3 lines) and content area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // === Tab Bar ===
    render_tab_bar(f, chunks[0], app);

    // === Tab Content ===
    render_tab_content(f, chunks[1], app);
}

/// Render the tab navigation bar
fn render_tab_bar(f: &mut Frame, area: Rect, app: &App) {
    let tab_titles = vec![
        " 1│Overview ",
        " 2│Models ",
        " 3│Tokens ",
        " 4│Tools ",
        " 5│Trends ",
    ];

    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(app.theme.border),
        )
        .select(app.stats_selected_tab)
        .style(Style::default().fg(app.theme.foreground))
        .highlight_style(
            Style::default()
                .fg(app.theme.highlight)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(tabs, area);
}

/// Render the content of the selected tab
fn render_tab_content(f: &mut Frame, area: Rect, app: &App) {
    match app.stats_selected_tab {
        0 => render_overview_tab(f, area, app),
        1 => ModelsTabPanel::render(f, area, &app.stats, &app.theme),
        2 => TokensTabPanel::render(f, area, &app.stats, &app.theme),
        3 => ToolsTabPanel::render(f, area, &app.stats, &app.theme),
        4 => TrendsTabPanel::render(f, area, &app.stats, &app.theme),
        _ => {
            // Fallback for invalid tab index
            let msg = Paragraph::new("Invalid tab selected")
                .block(Block::default().borders(Borders::ALL).title(" Error "));
            f.render_widget(msg, area);
        }
    }
}

/// Render the Overview tab (gauges + summary)
fn render_overview_tab(f: &mut Frame, area: Rect, app: &App) {
    // Split into gauges (40%) and summary (60%)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    // === Left: Session Gauges ===
    SessionGaugesPanel::render(f, chunks[0], &app.stats, &app.context_state, &app.theme);

    // === Right: Session Summary ===
    render_session_summary(f, chunks[1], app);
}

/// Render session summary with key metrics
fn render_session_summary(f: &mut Frame, area: Rect, app: &App) {
    let stats = &app.stats;
    let fg = app.theme.foreground;
    let muted = app.theme.muted;

    let mut lines = vec![
        Line::from(vec![
            Span::styled("  Requests:     ", Style::default().fg(muted)),
            Span::styled(
                format!("{}", stats.total_requests),
                Style::default().fg(fg).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ({} failed)", stats.failed_requests),
                Style::default().fg(if stats.failed_requests > 0 {
                    Color::Red
                } else {
                    muted
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Tool Calls:   ", Style::default().fg(muted)),
            Span::styled(
                format!("{}", stats.total_tool_calls),
                Style::default().fg(fg).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ({} failed)", stats.failed_tool_calls),
                Style::default().fg(if stats.failed_tool_calls > 0 {
                    Color::Red
                } else {
                    muted
                }),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Total Tokens: ", Style::default().fg(muted)),
            Span::styled(
                format_number(stats.total_tokens()),
                Style::default().fg(app.theme.api_usage),
            ),
        ]),
        Line::from(vec![
            Span::styled("    Input:      ", Style::default().fg(muted)),
            Span::styled(
                format_compact_number(stats.total_input_tokens),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("    Output:     ", Style::default().fg(muted)),
            Span::styled(
                format_compact_number(stats.total_output_tokens),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled("    Cached:     ", Style::default().fg(muted)),
            Span::styled(
                format_compact_number(stats.total_cache_read_tokens),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Est. Cost:    ", Style::default().fg(muted)),
            Span::styled(
                format!("${:.4}", stats.total_cost()),
                Style::default()
                    .fg(app.theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Cache Savings:", Style::default().fg(muted)),
            Span::styled(
                format!("${:.4}", stats.cache_savings()),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Thinking:     ", Style::default().fg(muted)),
            Span::styled(
                format!("{} blocks", stats.thinking_blocks),
                Style::default().fg(app.theme.thinking),
            ),
        ]),
        Line::from(vec![
            Span::styled("    Tokens:     ", Style::default().fg(muted)),
            Span::styled(
                format_compact_number(stats.thinking_tokens),
                Style::default().fg(app.theme.thinking),
            ),
        ]),
    ];

    // Add context compacts if any occurred
    if stats.compact_count > 0 {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  Compacts:     ", Style::default().fg(muted)),
            Span::styled(
                format!("{}", stats.compact_count),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    // Add Aspy modification stats if any transformations/augmentations occurred
    let has_modifications = stats.transform_stats.tokens_injected > 0
        || stats.transform_stats.tokens_removed > 0
        || stats.augment_stats.tokens_injected > 0;

    if has_modifications {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "  ── Aspy Mods ──",
            Style::default().fg(muted).add_modifier(Modifier::DIM),
        )]));

        // Transform stats (request modifications)
        if stats.transform_stats.tokens_injected > 0 || stats.transform_stats.tokens_removed > 0 {
            lines.push(Line::from(vec![
                Span::styled("  Transform:    ", Style::default().fg(muted)),
                Span::styled(
                    format!(
                        "+{}",
                        format_compact_number(stats.transform_stats.tokens_injected)
                    ),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(" / ", Style::default().fg(muted)),
                Span::styled(
                    format!(
                        "-{}",
                        format_compact_number(stats.transform_stats.tokens_removed)
                    ),
                    Style::default().fg(Color::Red),
                ),
            ]));
        }

        // Augment stats (response injections)
        if stats.augment_stats.tokens_injected > 0 {
            lines.push(Line::from(vec![
                Span::styled("  Augment:      ", Style::default().fg(muted)),
                Span::styled(
                    format!(
                        "+{}",
                        format_compact_number(stats.augment_stats.tokens_injected)
                    ),
                    Style::default().fg(Color::Magenta),
                ),
                Span::styled(" injected", Style::default().fg(muted)),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Session Summary ")
            .border_style(app.theme.border),
    );

    f.render_widget(paragraph, area);
}
