// UI rendering logic
//
// This module contains all the rendering code for the TUI. In ratatui,
// you define the UI layout and widgets in a render function that gets
// called on every frame.

use super::app::{App, StreamingState, View};
use super::layout::Breakpoint;
use crate::events::ProxyEvent;
use crate::logging::{LogEntry, LogLevel};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Gauge, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
    Frame,
};

/// Main UI render function - called on every frame
pub fn draw(f: &mut Frame, app: &App) {
    // Split the terminal into five vertical sections:
    // - Title bar (3 lines fixed)
    // - Main content area (fills remaining space)
    // - System logs (6 lines fixed)
    // - Context bar (1 line - context window usage)
    // - Status bar (3 lines fixed)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title bar
            Constraint::Min(10),   // Main content - takes remaining space
            Constraint::Length(6), // System logs - fixed height
            Constraint::Length(1), // Context bar
            Constraint::Length(3), // Status bar
        ])
        .split(f.area());

    // Render title bar
    render_title(f, chunks[0], app);

    // Main content depends on active view
    match app.view {
        View::Events => render_events_content(f, chunks[1], app),
        View::Stats => render_stats_view(f, chunks[1], app),
        View::Help => render_help_view(f, chunks[1], app),
    }

    // Render system logs panel
    render_logs_panel(f, chunks[2], app);

    // Render context bar
    render_context_bar(f, chunks[3], app);

    // Render status bar
    render_status(f, chunks[4], app);
}

/// Render the Events view content (original main view)
fn render_events_content(f: &mut Frame, area: Rect, app: &App) {
    let has_thinking = app.has_thinking_content();
    let bp = Breakpoint::from_width(area.width);

    // Only show thinking panel side-by-side if we have room
    let show_thinking_panel = has_thinking && bp.at_least(Breakpoint::Normal);

    if show_thinking_panel {
        // Split main area: Events | Thinking (responsive widths)
        let thinking_pct = match bp {
            Breakpoint::UltraWide => 30,
            Breakpoint::Wide => 35,
            _ => 40, // Normal
        };
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(100 - thinking_pct),
                Constraint::Percentage(thinking_pct),
            ])
            .split(area);

        // Render events on the left
        if app.show_detail {
            render_split_view(f, main_chunks[0], app);
        } else {
            render_list_view(f, main_chunks[0], app);
        }

        // Render thinking panel on the right
        render_thinking_panel(f, main_chunks[1], app);
    } else {
        // No thinking panel (either no thinking, or too narrow)
        if app.show_detail {
            render_split_view(f, area, app);
        } else {
            render_list_view(f, area, app);
        }
    }
}

/// Render the Stats view - session profile and analytics with panels and gauges
fn render_stats_view(f: &mut Frame, area: Rect, app: &App) {
    let stats = &app.stats;

    // Split into top row (Models + Cache) and bottom row (Tools + Totals)
    // Both rows use same 55/45 split so vertical dividers align
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let top_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(rows[0]);

    let bottom_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(rows[1]);

    // === Models Panel (top-left) ===
    render_models_panel(f, top_row[0], stats);

    // === Tokens Panel (top-right) ===
    render_tokens_panel(f, top_row[1], stats);

    // === Tools Panel (bottom-left) ===
    render_tools_panel(f, bottom_row[0], stats);

    // === Totals Panel (bottom-right) ===
    render_totals_panel(f, bottom_row[1], stats);
}

/// Render the Models distribution panel with visual bars
fn render_models_panel(f: &mut Frame, area: Rect, stats: &crate::events::Stats) {
    let mut lines: Vec<Line> = vec![];

    if stats.model_calls.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No API calls yet",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let total_calls: u32 = stats.model_calls.values().sum();

        // Sort by count descending
        let mut sorted: Vec<_> = stats.model_calls.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));

        for (model, count) in sorted {
            let pct = (*count as f64 / total_calls as f64) * 100.0;
            let bar = ascii_bar(pct, 20);

            // Color based on model type
            let color = if model.contains("haiku") {
                Color::Cyan
            } else if model.contains("opus") {
                Color::Magenta
            } else {
                Color::Yellow // Sonnet or other
            };

            // Shorten model name for display
            let short_name = shorten_model_name(model);

            lines.push(Line::from(vec![
                Span::styled(format!("  {:>8} ", short_name), Style::default().fg(color)),
                Span::styled(bar, Style::default().fg(color)),
                Span::styled(
                    format!(" {:>3.0}% ", pct),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("({} calls)", count),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue))
            .title(" Models "),
    );

    f.render_widget(paragraph, area);
}

/// Render the Token Breakdown panel with proportional colored bars
fn render_tokens_panel(f: &mut Frame, area: Rect, stats: &crate::events::Stats) {
    let mut lines: Vec<Line> = vec![];

    let cached = stats.total_cache_read_tokens;
    let input = stats.total_input_tokens;
    let output = stats.total_output_tokens;
    let total = cached + input + output;

    if total == 0 {
        lines.push(Line::from(Span::styled(
            "  No token data yet",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        // Calculate proportions (bar width = 16 chars max)
        let bar_width: usize = 16;
        let cached_width = ((cached as f64 / total as f64) * bar_width as f64).round() as usize;
        let input_width = ((input as f64 / total as f64) * bar_width as f64).round() as usize;
        let output_width = bar_width.saturating_sub(cached_width + input_width);

        // Cached tokens row (green)
        let cached_pct = if total > 0 {
            (cached as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        lines.push(Line::from(vec![
            Span::styled("  Cached ", Style::default().fg(Color::Green)),
            Span::styled(
                "█".repeat(cached_width.max(if cached > 0 { 1 } else { 0 })),
                Style::default().fg(Color::Green),
            ),
            Span::styled(
                "░".repeat(bar_width.saturating_sub(cached_width)),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!(" {:>7} ", format_compact_number(cached)),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:>5.1}%", cached_pct),
                Style::default().fg(Color::DarkGray),
            ),
        ]));

        // Input tokens row (cyan)
        let input_pct = if total > 0 {
            (input as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        lines.push(Line::from(vec![
            Span::styled("  Input  ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "█".repeat(input_width.max(if input > 0 { 1 } else { 0 })),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                "░".repeat(bar_width.saturating_sub(input_width)),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!(" {:>7} ", format_compact_number(input)),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:>5.1}%", input_pct),
                Style::default().fg(Color::DarkGray),
            ),
        ]));

        // Output tokens row (magenta)
        let output_pct = if total > 0 {
            (output as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        lines.push(Line::from(vec![
            Span::styled("  Output ", Style::default().fg(Color::Magenta)),
            Span::styled(
                "█".repeat(output_width.max(if output > 0 { 1 } else { 0 })),
                Style::default().fg(Color::Magenta),
            ),
            Span::styled(
                "░".repeat(bar_width.saturating_sub(output_width)),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!(" {:>7} ", format_compact_number(output)),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:>5.1}%", output_pct),
                Style::default().fg(Color::DarkGray),
            ),
        ]));

        // Blank line
        lines.push(Line::from(""));

        // Cache efficiency and savings row
        let cache_rate = stats.cache_hit_rate();
        let cache_color = if cache_rate >= 90.0 {
            Color::Green
        } else if cache_rate >= 70.0 {
            Color::Yellow
        } else {
            Color::Red
        };

        lines.push(Line::from(vec![
            Span::styled("  Cache: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.1}%", cache_rate),
                Style::default()
                    .fg(cache_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("   Saved: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("${:.2}", stats.cache_savings()),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green))
            .title(" Tokens "),
    );

    f.render_widget(paragraph, area);
}

/// Render the Tools distribution panel
fn render_tools_panel(f: &mut Frame, area: Rect, stats: &crate::events::Stats) {
    let mut lines: Vec<Line> = vec![];

    if stats.tool_calls_by_name.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No tool calls yet",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        // Sort by count descending
        let mut sorted: Vec<_> = stats.tool_calls_by_name.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));

        for (tool, count) in sorted {
            let avg_ms = stats.avg_tool_duration_ms(tool).unwrap_or(0);

            // Color based on tool type
            let color = tool_color(tool);

            let duration_str = if avg_ms > 1000 {
                format!("{:.1}s", avg_ms as f64 / 1000.0)
            } else {
                format!("{}ms", avg_ms)
            };

            lines.push(Line::from(vec![
                Span::styled(format!("  {:>10} ", tool), Style::default().fg(color)),
                Span::styled(
                    format!("{:>3}", count),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" calls  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("~{}", duration_str),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Tools "),
    );

    f.render_widget(paragraph, area);
}

/// Render the Totals panel
fn render_totals_panel(f: &mut Frame, area: Rect, stats: &crate::events::Stats) {
    let lines = vec![
        Line::from(vec![
            Span::styled("  Requests:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", stats.total_requests),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Tool calls: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", stats.total_tool_calls),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Tokens:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_number(stats.total_tokens()),
                Style::default().fg(Color::LightBlue),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Est. Cost:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("${:.4}", stats.total_cost()),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Thinking:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} blocks", stats.thinking_blocks),
                Style::default().fg(Color::Magenta),
            ),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Session Totals "),
    );

    f.render_widget(paragraph, area);
}

/// Generate ASCII progress bar
fn ascii_bar(pct: f64, width: usize) -> String {
    let filled = ((pct / 100.0) * width as f64) as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

/// Shorten model name for display
fn shorten_model_name(model: &str) -> &str {
    if model.contains("haiku") {
        "Haiku"
    } else if model.contains("opus") {
        "Opus"
    } else if model.contains("sonnet") {
        "Sonnet"
    } else {
        // Return last part after dash if long
        model.split('-').next().unwrap_or(model)
    }
}

/// Get color for tool type
fn tool_color(tool: &str) -> Color {
    match tool {
        "Read" => Color::Blue,
        "Edit" | "Write" => Color::Green,
        "Bash" => Color::Yellow,
        "Glob" | "Grep" => Color::Cyan,
        "TodoWrite" => Color::Magenta,
        _ => Color::White,
    }
}

/// Render the Help view
fn render_help_view(f: &mut Frame, area: Rect, app: &App) {
    let content = format!(
        r#"
  Keyboard Shortcuts
  ──────────────────────────────────

  Navigation
    ↑/↓, j/k    Scroll list / detail
    Enter       Open detail view
    Esc         Close / go back
    Home/End    Jump to start/end

  Views
    e           Events (main view)
    s           Statistics
    ?           Help (this screen)

  General
    q           Quit

  Mouse
    Scroll      Navigate events

  ──────────────────────────────────
  Theme: {}
    "#,
        app.theme.name
    );

    let paragraph = Paragraph::new(content)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(app.theme.border))
                .title(" Help (?) ─ Press Esc to go back "),
        );

    f.render_widget(paragraph, area);
}

/// Render the thinking panel showing Claude's reasoning
fn render_thinking_panel(f: &mut Frame, area: Rect, app: &App) {
    // Get thinking content: streaming (real-time) or completed
    let thinking_content = app
        .current_thinking_content()
        .unwrap_or_else(|| "Waiting for thinking...".to_string());

    // Calculate visible lines based on area height
    let height = area.height.saturating_sub(2) as usize; // Account for borders
    let lines: Vec<&str> = thinking_content.lines().collect();
    let total_lines = lines.len();

    // Show last N lines (most recent thinking)
    let start = total_lines.saturating_sub(height);
    let visible_lines = &lines[start..];
    let visible_text = visible_lines.join("\n");

    let title = if total_lines > height {
        format!(
            " 💭 Thinking ({} lines, ~{} tok) ",
            total_lines, app.stats.thinking_tokens
        )
    } else {
        format!(" 💭 Thinking (~{} tok) ", app.stats.thinking_tokens)
    };

    let paragraph = Paragraph::new(visible_text)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta))
                .title(title),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

/// Render the title bar
fn render_title(f: &mut Frame, area: Rect, app: &App) {
    // Build streaming indicator
    let streaming_indicator = match app.streaming_state {
        StreamingState::Idle => String::new(),
        StreamingState::Thinking => format!(" {} thinking", app.spinner_char()),
        StreamingState::Generating => format!(" {} generating", app.spinner_char()),
        StreamingState::AwaitingApproval => " ⏸ awaiting approval".to_string(),
    };

    let title_text = match &app.topic.title {
        Some(topic) => {
            let indicator = if app.topic.is_new_topic { "●" } else { "◦" };
            format!(
                " 🔍 Anthropic Spy{} ──── {} {}",
                streaming_indicator, indicator, topic
            )
        }
        None => format!(" 🔍 Anthropic Spy{}", streaming_indicator),
    };

    let title = Paragraph::new(title_text)
        .style(
            Style::default()
                .fg(app.theme.title)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(app.theme.border))
                .title_top(ratatui::text::Line::from(" ? ").right_aligned()),
        );

    f.render_widget(title, area);
}

/// Render the context window usage bar (1-line gauge with centered text)
fn render_context_bar(f: &mut Frame, area: Rect, app: &App) {
    let stats = &app.stats;

    let (label, pct, color) = if stats.current_context_tokens > 0 {
        let pct = stats.context_usage_percent().unwrap_or(0.0);
        let color = if pct >= 90.0 {
            app.theme.context_bar_danger
        } else if pct >= 70.0 {
            app.theme.context_bar_warn
        } else {
            app.theme.context_bar_fill
        };
        // Show actual token count with comma formatting for detail
        let label = format!(
            "Context: {} / {} ({:.1}%)",
            format_number(stats.current_context_tokens),
            format_number(stats.context_limit()),
            pct
        );
        (label, pct, color)
    } else {
        (
            "Context: waiting for API call...".to_string(),
            0.0,
            Color::DarkGray,
        )
    };

    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(color).bg(Color::Black))
        .percent(pct.min(100.0) as u16)
        .label(Span::styled(
            label,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));

    f.render_widget(gauge, area);
}

/// Render the status bar with statistics
/// Switches to compact icon-based format when terminal width is narrow
fn render_status(f: &mut Frame, area: Rect, app: &App) {
    let stats = &app.stats;
    let bp = Breakpoint::from_width(area.width);

    let status_text = if !bp.at_least(Breakpoint::Wide) {
        // Compact format with icons for narrow terminals
        let token_info = if stats.total_tokens() > 0 {
            format!(" │ 💰${:.2}", stats.total_cost())
        } else {
            String::new()
        };

        format!(
            " {} │ 📡{} │ 🔧{} │ ✓{:.0}% │ ~{:.0}ms{}",
            app.uptime(),
            stats.total_requests,
            stats.total_tool_calls,
            stats.success_rate(),
            stats.avg_ttfb().as_millis(),
            token_info,
        )
    } else {
        // Full format for wide terminals
        let token_info = if stats.total_tokens() > 0 {
            let cost = stats.total_cost();
            let savings = stats.cache_savings();

            if savings > 0.0 {
                format!(
                    " | {}tok | ${:.2} | Saved: ${:.2}",
                    format_compact_number(stats.total_tokens()),
                    cost,
                    savings
                )
            } else {
                format!(
                    " | {}tok | ${:.2}",
                    format_compact_number(stats.total_tokens()),
                    cost
                )
            }
        } else {
            String::new()
        };

        let tools_info = if stats.failed_tool_calls > 0 {
            format!("🔧{} ✗{}", stats.total_tool_calls, stats.failed_tool_calls)
        } else {
            format!("🔧{}", stats.total_tool_calls)
        };

        format!(
            " {} │ 📡{} │ {} │ ✓{:.1}% │ ~{}ms{}",
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
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(status, area);
}

/// Format a large number with commas for readability
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();

    for (count, ch) in s.chars().rev().enumerate() {
        if count > 0 && count % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, ch);
    }

    result
}

/// Format a number compactly (e.g., 954356 -> "954K")
fn format_compact_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{}K", n / 1_000)
    } else {
        n.to_string()
    }
}

/// Render the main list view showing all events
fn render_list_view(f: &mut Frame, area: Rect, app: &App) {
    let height = area.height.saturating_sub(2) as usize; // Account for borders
    let (start, end) = app.visible_range(height);

    let items: Vec<ListItem> = app.events[start..end]
        .iter()
        .enumerate()
        .map(|(idx, event)| {
            let actual_idx = start + idx;
            let is_selected = actual_idx == app.selected;

            let line = format_event_line(event);
            let style = if is_selected {
                Style::default()
                    .fg(app.theme.highlight)
                    .add_modifier(Modifier::BOLD)
            } else {
                event_color_style(event, &app.theme)
            };

            ListItem::new(line).style(style)
        })
        .collect();

    let title = if app.events.is_empty() {
        " Events ".to_string()
    } else {
        format!(" Events ({}) ", app.events.len())
    };

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));

    f.render_widget(list, area);

    // Render scrollbar if content overflows
    let total_events = app.events.len();
    if total_events > height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let mut scrollbar_state =
            ScrollbarState::new(total_events.saturating_sub(height)).position(start);

        f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

/// Render split view with list on top and details on bottom
fn render_split_view(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Render the list in the top half
    render_list_view(f, chunks[0], app);

    // Render details in the bottom half
    if let Some(event) = app.selected_event() {
        render_detail_view(f, chunks[1], app, event);
    }
}

/// Render detailed view of a single event
fn render_detail_view(f: &mut Frame, area: Rect, app: &App, event: &ProxyEvent) {
    let detail_text = format_event_detail(event);

    // Split detail text into lines for scrolling
    let lines: Vec<&str> = detail_text.lines().collect();
    let total_lines = lines.len();

    // Calculate visible range based on scroll offset
    let height = area.height.saturating_sub(2) as usize; // Account for borders
    let start = app.detail_scroll.min(total_lines.saturating_sub(height));
    let end = (start + height).min(total_lines);

    // Take only the visible lines
    let visible_text = lines[start..end].join("\n");

    let paragraph = Paragraph::new(visible_text)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Event Details - Press Esc to close "),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);

    // Render scrollbar if content overflows
    if total_lines > height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let mut scrollbar_state =
            ScrollbarState::new(total_lines.saturating_sub(height)).position(start);

        // Scrollbar renders inside the block's border area
        let scrollbar_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height,
        };

        f.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }
}

/// Format an event as a single line for the list view
fn format_event_line(event: &ProxyEvent) -> String {
    match event {
        ProxyEvent::ToolCall {
            timestamp,
            tool_name,
            id,
            ..
        } => {
            format!(
                "[{}] 🔧 Tool Call: {} ({})",
                timestamp.format("%H:%M:%S"),
                tool_name,
                &id[..8]
            )
        }
        ProxyEvent::ToolResult {
            timestamp,
            tool_name,
            duration,
            success,
            ..
        } => {
            let status = if *success { "✓" } else { "✗" };
            format!(
                "[{}] {} Tool Result: {} ({:.2}s)",
                timestamp.format("%H:%M:%S"),
                status,
                tool_name,
                duration.as_secs_f64()
            )
        }
        ProxyEvent::Request {
            timestamp,
            method,
            path,
            ..
        } => {
            format!(
                "[{}] ← Request: {} {}",
                timestamp.format("%H:%M:%S"),
                method,
                path
            )
        }
        ProxyEvent::Response {
            timestamp,
            status,
            duration,
            ..
        } => {
            format!(
                "[{}] → Response: {} ({:.2}s)",
                timestamp.format("%H:%M:%S"),
                status,
                duration.as_secs_f64()
            )
        }
        ProxyEvent::Error {
            timestamp, message, ..
        } => {
            format!("[{}] ❌ Error: {}", timestamp.format("%H:%M:%S"), message)
        }
        ProxyEvent::HeadersCaptured {
            timestamp, headers, ..
        } => {
            let beta_info = if !headers.anthropic_beta.is_empty() {
                format!(" [β:{}]", headers.anthropic_beta.join(","))
            } else {
                String::new()
            };
            format!(
                "[{}] 📋 Headers Captured{}",
                timestamp.format("%H:%M:%S"),
                beta_info
            )
        }
        ProxyEvent::RateLimitUpdate {
            timestamp,
            requests_remaining,
            tokens_remaining,
            ..
        } => {
            format!(
                "[{}] ⚖️  Rate Limits: Req={:?} Tok={:?}",
                timestamp.format("%H:%M:%S"),
                requests_remaining,
                tokens_remaining
            )
        }
        ProxyEvent::ApiUsage {
            timestamp,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            ..
        } => {
            if *cache_read_tokens > 0 {
                format!(
                    "[{}] 📊 Usage: {}in + {}out + {}cached",
                    timestamp.format("%H:%M:%S"),
                    format_number(*input_tokens as u64),
                    format_number(*output_tokens as u64),
                    format_number(*cache_read_tokens as u64)
                )
            } else {
                format!(
                    "[{}] 📊 Usage: {}in + {}out",
                    timestamp.format("%H:%M:%S"),
                    format_number(*input_tokens as u64),
                    format_number(*output_tokens as u64)
                )
            }
        }
        ProxyEvent::Thinking {
            timestamp,
            content,
            token_estimate,
        } => {
            // Show first line preview
            let preview: String = content
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(50)
                .collect();
            format!(
                "[{}] 💭 Thinking: {}... (~{} tok)",
                timestamp.format("%H:%M:%S"),
                preview,
                token_estimate
            )
        }
        ProxyEvent::ContextCompact {
            timestamp,
            previous_context,
            new_context,
        } => {
            format!(
                "[{}] 📦 Context Compact: {}K → {}K",
                timestamp.format("%H:%M:%S"),
                previous_context / 1000,
                new_context / 1000
            )
        }
        ProxyEvent::ThinkingStarted { timestamp } => {
            format!("[{}] 💭 Thinking...", timestamp.format("%H:%M:%S"))
        }
    }
}

/// Format an event as detailed text for the detail view
fn format_event_detail(event: &ProxyEvent) -> String {
    match event {
        ProxyEvent::ToolCall {
            id,
            timestamp,
            tool_name,
            input,
        } => {
            format!(
                "Tool Call\n\nID: {}\nTimestamp: {}\nTool: {}\n\nInput:\n{}",
                id,
                timestamp.to_rfc3339(),
                tool_name,
                serde_json::to_string_pretty(input).unwrap_or_else(|_| "N/A".to_string())
            )
        }
        ProxyEvent::ToolResult {
            id,
            timestamp,
            tool_name,
            output,
            duration,
            success,
        } => {
            format!(
                "Tool Result\n\nID: {}\nTimestamp: {}\nTool: {}\nSuccess: {}\nDuration: {:.2}s\n\nOutput:\n{}",
                id,
                timestamp.to_rfc3339(),
                tool_name,
                success,
                duration.as_secs_f64(),
                serde_json::to_string_pretty(output).unwrap_or_else(|_| "N/A".to_string())
            )
        }
        ProxyEvent::Request {
            id,
            timestamp,
            method,
            path,
            body_size,
            body,
        } => {
            let body_content = if let Some(json_body) = body {
                format!(
                    "\n\nRequest Body:\n{}",
                    serde_json::to_string_pretty(json_body)
                        .unwrap_or_else(|_| "Failed to format".to_string())
                )
            } else {
                String::new()
            };

            format!(
                "HTTP Request\n\nID: {}\nTimestamp: {}\nMethod: {}\nPath: {}\nBody Size: {} bytes{}",
                id,
                timestamp.to_rfc3339(),
                method,
                path,
                body_size,
                body_content
            )
        }
        ProxyEvent::Response {
            request_id,
            timestamp,
            status,
            body_size,
            ttfb,
            duration,
            body,
        } => {
            let body_content = if let Some(json_body) = body {
                format!(
                    "\n\nResponse Body:\n{}",
                    serde_json::to_string_pretty(json_body)
                        .unwrap_or_else(|_| "Failed to format".to_string())
                )
            } else {
                String::new()
            };

            format!(
                "HTTP Response\n\nRequest ID: {}\nTimestamp: {}\nStatus: {}\nBody Size: {} bytes\nTTFB: {}ms\nTotal Duration: {:.2}s{}",
                request_id,
                timestamp.to_rfc3339(),
                status,
                body_size,
                ttfb.as_millis(),
                duration.as_secs_f64(),
                body_content
            )
        }
        ProxyEvent::Error {
            timestamp,
            message,
            context,
        } => {
            format!(
                "Error\n\nTimestamp: {}\nMessage: {}\nContext: {}",
                timestamp.to_rfc3339(),
                message,
                context.as_deref().unwrap_or("N/A")
            )
        }
        ProxyEvent::HeadersCaptured {
            timestamp, headers, ..
        } => {
            let beta_features = if !headers.anthropic_beta.is_empty() {
                headers.anthropic_beta.join(", ")
            } else {
                "None".to_string()
            };

            format!(
                "Headers Captured\n\nTimestamp: {}\n\nRequest Headers:\nAPI Version: {}\nBeta Features: {}\nAPI Key Hash: {}\n\nResponse Headers:\nRequest ID: {}\nOrg ID: {}\n\nRate Limits:\nRequests: {}/{} ({}%)\nTokens: {}/{} ({}%)\nReset: {}",
                timestamp.to_rfc3339(),
                headers.anthropic_version.as_deref().unwrap_or("N/A"),
                beta_features,
                headers.api_key_hash.as_deref().unwrap_or("N/A"),
                headers.request_id.as_deref().unwrap_or("N/A"),
                headers.organization_id.as_deref().unwrap_or("N/A"),
                headers.requests_remaining.map(|r| r.to_string()).unwrap_or("?".to_string()),
                headers.requests_limit.map(|l| l.to_string()).unwrap_or("?".to_string()),
                headers.requests_usage_pct().map(|p| format!("{:.1}", p * 100.0)).unwrap_or("?".to_string()),
                headers.tokens_remaining.map(|r| r.to_string()).unwrap_or("?".to_string()),
                headers.tokens_limit.map(|l| l.to_string()).unwrap_or("?".to_string()),
                headers.tokens_usage_pct().map(|p| format!("{:.1}", p * 100.0)).unwrap_or("?".to_string()),
                headers.requests_reset.as_deref().or(headers.tokens_reset.as_deref()).unwrap_or("N/A")
            )
        }
        ProxyEvent::RateLimitUpdate {
            timestamp,
            requests_remaining,
            requests_limit,
            tokens_remaining,
            tokens_limit,
            reset_time,
        } => {
            format!(
                "Rate Limit Update\n\nTimestamp: {}\n\nRequests: {}/{}\nTokens: {}/{}\nReset: {}",
                timestamp.to_rfc3339(),
                requests_remaining
                    .map(|r| r.to_string())
                    .unwrap_or("?".to_string()),
                requests_limit
                    .map(|l| l.to_string())
                    .unwrap_or("?".to_string()),
                tokens_remaining
                    .map(|r| r.to_string())
                    .unwrap_or("?".to_string()),
                tokens_limit
                    .map(|l| l.to_string())
                    .unwrap_or("?".to_string()),
                reset_time.as_deref().unwrap_or("N/A")
            )
        }
        ProxyEvent::ApiUsage {
            timestamp,
            model,
            input_tokens,
            output_tokens,
            cache_creation_tokens,
            cache_read_tokens,
        } => {
            let total =
                *input_tokens + *output_tokens + *cache_creation_tokens + *cache_read_tokens;
            let cost = crate::pricing::calculate_cost(
                model,
                *input_tokens,
                *output_tokens,
                *cache_creation_tokens,
                *cache_read_tokens,
            );
            let cache_savings = if *cache_read_tokens > 0 {
                crate::pricing::calculate_cache_savings(model, *cache_read_tokens)
            } else {
                0.0
            };

            let cache_info = if *cache_read_tokens > 0 || *cache_creation_tokens > 0 {
                format!(
                    "\n\nCache Statistics:\nCache Creation: {} tokens\nCache Read: {} tokens\nCache Savings: ${:.4} (vs regular input)",
                    format_number(*cache_creation_tokens as u64),
                    format_number(*cache_read_tokens as u64),
                    cache_savings
                )
            } else {
                String::new()
            };

            format!(
                "API Usage\n\nTimestamp: {}\nModel: {}\n\nToken Breakdown:\nInput: {} tokens\nOutput: {} tokens\nTotal: {} tokens\n\nEstimated Cost: ${:.4}{}",
                timestamp.to_rfc3339(),
                model,
                format_number(*input_tokens as u64),
                format_number(*output_tokens as u64),
                format_number(total as u64),
                cost,
                cache_info
            )
        }
        ProxyEvent::Thinking {
            timestamp,
            content,
            token_estimate,
        } => {
            format!(
                "💭 Claude's Thinking\n\nTimestamp: {}\nEstimated Tokens: ~{}\n\n─────────────────────────────────────\n\n{}",
                timestamp.to_rfc3339(),
                token_estimate,
                content
            )
        }
        ProxyEvent::ContextCompact {
            timestamp,
            previous_context,
            new_context,
        } => {
            let reduction = previous_context.saturating_sub(*new_context);
            let reduction_pct = if *previous_context > 0 {
                (reduction as f64 / *previous_context as f64) * 100.0
            } else {
                0.0
            };
            format!(
                "📦 Context Compaction Detected\n\n\
                Timestamp: {}\n\n\
                Previous Context: {} tokens ({:.1}K)\n\
                New Context: {} tokens ({:.1}K)\n\
                Reduction: {} tokens ({:.1}%)\n\n\
                Claude Code triggered a context window compaction to\n\
                reduce memory usage and stay within limits.",
                timestamp.to_rfc3339(),
                previous_context,
                *previous_context as f64 / 1000.0,
                new_context,
                *new_context as f64 / 1000.0,
                reduction,
                reduction_pct
            )
        }
        ProxyEvent::ThinkingStarted { timestamp } => {
            format!(
                "💭 Thinking Started\n\nTimestamp: {}\n\nClaude is processing your request...",
                timestamp.to_rfc3339()
            )
        }
    }
}

/// Get appropriate color style for an event
fn event_color_style(event: &ProxyEvent, theme: &crate::theme::Theme) -> Style {
    match event {
        ProxyEvent::ToolCall { .. } => Style::default().fg(theme.tool_call),
        ProxyEvent::ToolResult { success, .. } => {
            if *success {
                Style::default().fg(theme.tool_result_ok)
            } else {
                Style::default().fg(theme.tool_result_fail)
            }
        }
        ProxyEvent::Request { .. } => Style::default().fg(theme.request),
        ProxyEvent::Response { .. } => Style::default().fg(theme.response),
        ProxyEvent::Error { .. } => Style::default()
            .fg(theme.error)
            .add_modifier(Modifier::BOLD),
        ProxyEvent::HeadersCaptured { .. } => Style::default().fg(theme.headers),
        ProxyEvent::RateLimitUpdate { .. } => Style::default().fg(theme.rate_limit),
        ProxyEvent::ApiUsage { .. } => Style::default().fg(theme.api_usage),
        ProxyEvent::Thinking { .. } => Style::default()
            .fg(theme.thinking)
            .add_modifier(Modifier::ITALIC),
        ProxyEvent::ContextCompact { .. } => Style::default()
            .fg(theme.context_compact)
            .add_modifier(Modifier::BOLD),
        ProxyEvent::ThinkingStarted { .. } => Style::default()
            .fg(theme.thinking)
            .add_modifier(Modifier::ITALIC),
    }
}

/// Render system logs panel at the bottom of the screen
pub fn render_logs_panel(f: &mut Frame, area: Rect, app: &App) {
    // Get recent log entries from buffer
    let height = area.height.saturating_sub(2) as usize; // Account for borders
    let log_entries = app.log_buffer.get_recent(height);

    // Convert log entries to list items with color coding
    let items: Vec<ListItem> = log_entries
        .iter()
        .map(|entry| {
            let formatted = format_log_entry(entry);
            let style = log_level_style(&entry.level);
            ListItem::new(formatted).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" System Logs "),
    );

    f.render_widget(list, area);
}

/// Format a log entry for display
fn format_log_entry(entry: &LogEntry) -> String {
    format!(
        "[{}] {:5} {}",
        entry.timestamp.format("%H:%M:%S"),
        entry.level.as_str(),
        entry.message
    )
}

/// Get color style for log level
fn log_level_style(level: &LogLevel) -> Style {
    match level {
        LogLevel::Error => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        LogLevel::Warn => Style::default().fg(Color::Yellow),
        LogLevel::Info => Style::default().fg(Color::Blue),
        LogLevel::Debug => Style::default().fg(Color::Gray),
        LogLevel::Trace => Style::default().fg(Color::DarkGray),
    }
}
