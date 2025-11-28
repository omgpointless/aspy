// UI rendering logic
//
// This module contains all the rendering code for the TUI. In ratatui,
// you define the UI layout and widgets in a render function that gets
// called on every frame.

use super::app::{App, View};
use super::theme::Theme;
use crate::events::ProxyEvent;
use crate::logging::{LogEntry, LogLevel};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap},
    Frame,
};

/// Main UI render function - called on every frame
pub fn draw(f: &mut Frame, app: &App) {
    let theme = app.theme.theme();

    // Split the terminal into four vertical sections:
    // - Title bar (3 lines)
    // - Main content area (flexible, 65% of remaining)
    // - System logs (15% of remaining)
    // - Status bar (3 lines)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),      // Title bar
            Constraint::Percentage(65), // Main content
            Constraint::Percentage(15), // System logs
            Constraint::Length(3),      // Status bar
        ])
        .split(f.area());

    // Render title bar with view tabs
    render_title(f, chunks[0], app, &theme);

    // Render the current view
    match app.current_view {
        View::Main => render_main_view(f, chunks[1], app, &theme),
        View::Stats => render_stats_view(f, chunks[1], app, &theme),
        View::Help => render_help_view(f, chunks[1], app, &theme),
    }

    // Render system logs panel
    render_logs_panel(f, chunks[2], app, &theme);

    // Render status bar
    render_status(f, chunks[3], app, &theme);
}

/// Render the main events view
fn render_main_view(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    // Check if we have thinking and it's not collapsed
    let show_thinking = app.stats.current_thinking.is_some() && !app.thinking_collapsed;

    if show_thinking {
        // Split main area: Events (65%) | Thinking (35%)
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(65), // Events
                Constraint::Percentage(35), // Thinking panel
            ])
            .split(area);

        // Render events on the left
        if app.show_detail {
            render_split_view(f, main_chunks[0], app, theme);
        } else {
            render_list_view(f, main_chunks[0], app, theme);
        }

        // Render thinking panel on the right
        render_thinking_panel(f, main_chunks[1], app, theme);
    } else {
        // No thinking - full width for events
        if app.show_detail {
            render_split_view(f, area, app, theme);
        } else {
            render_list_view(f, area, app, theme);
        }
    }
}

/// Render the thinking panel showing Claude's reasoning
fn render_thinking_panel(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let thinking_content = app
        .stats
        .current_thinking
        .as_deref()
        .unwrap_or("Waiting for thinking...");

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
            " 💭 Thinking ({} lines, ~{} tok) [t to toggle] ",
            total_lines, app.stats.thinking_tokens
        )
    } else {
        format!(
            " 💭 Thinking (~{} tok) [t to toggle] ",
            app.stats.thinking_tokens
        )
    };

    let paragraph = Paragraph::new(visible_text)
        .style(Style::default().fg(theme.fg))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.thinking))
                .title(title),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

/// Render the title bar with view tabs
fn render_title(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    // Create tab indicators
    let views = [View::Main, View::Stats, View::Help];
    let tabs: Vec<Span> = views
        .iter()
        .map(|v| {
            let name = format!(" {} ", v.name());
            if *v == app.current_view {
                Span::styled(
                    name,
                    Style::default()
                        .fg(theme.selected_fg)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED),
                )
            } else {
                Span::styled(name, Style::default().fg(theme.border))
            }
        })
        .collect();

    let title_text = vec![
        Span::styled("Anthropic Spy ", theme.title_style()),
        Span::raw("│ "),
        tabs[0].clone(),
        Span::raw(" "),
        tabs[1].clone(),
        Span::raw(" "),
        tabs[2].clone(),
        Span::raw(" │ Theme: "),
        Span::styled(app.theme.name(), Style::default().fg(theme.stats_highlight)),
    ];

    let title = Paragraph::new(Line::from(title_text)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style()),
    );

    f.render_widget(title, area);
}

/// Render the status bar with statistics and context window
fn render_status(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let stats = &app.stats;

    // Build context window display
    let context_info = if stats.context_window_size > 0 {
        let usage_pct = stats.context_usage_pct();
        format!(
            " | Ctx: {}/{} ({:.0}%)",
            format_number(stats.current_context_tokens),
            format_number(stats.context_window_size),
            usage_pct * 100.0
        )
    } else {
        String::new()
    };

    // Format token and cost information
    let token_info = if stats.total_tokens() > 0 {
        let cost = stats.total_cost();
        let savings = stats.cache_savings();

        if savings > 0.0 {
            format!(
                " | {} tok | ${:.4} | -${:.4}",
                format_number(stats.total_tokens()),
                cost,
                savings
            )
        } else {
            format!(
                " | {} tok | ${:.4}",
                format_number(stats.total_tokens()),
                cost
            )
        }
    } else {
        String::new()
    };

    let status_text = format!(
        " {} | Req:{} | Tools:{} | {:.1}% | {:.2}s{}{} | Tab:views T:theme ?:help q:quit",
        app.uptime(),
        stats.total_requests,
        stats.total_tool_calls,
        stats.success_rate(),
        stats.avg_duration().as_secs_f64(),
        context_info,
        token_info,
    );

    let status = Paragraph::new(status_text)
        .style(theme.status_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style()),
        );

    f.render_widget(status, area);
}

/// Render the Stats view with comprehensive metrics
fn render_stats_view(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let stats = &app.stats;

    // Split into two columns
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left column: Overview stats
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // General stats
            Constraint::Length(6), // Context window
            Constraint::Min(5),    // Top tools
        ])
        .split(chunks[0]);

    // General statistics
    let general_stats = format!(
        r#"
 Uptime:          {}
 Total Requests:  {}
 Total Tool Calls: {}
 Success Rate:    {:.2}%
 Avg Duration:    {:.3}s
"#,
        app.uptime(),
        stats.total_requests,
        stats.total_tool_calls,
        stats.success_rate(),
        stats.avg_duration().as_secs_f64()
    );

    let general = Paragraph::new(general_stats)
        .style(Style::default().fg(theme.fg))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style())
                .title(" Overview "),
        );
    f.render_widget(general, left_chunks[0]);

    // Context window gauge
    let usage_pct = stats.context_usage_pct();
    let context_color = if usage_pct < 0.5 {
        theme.context_low
    } else if usage_pct < 0.8 {
        theme.context_medium
    } else {
        theme.context_high
    };

    let gauge_label = if stats.context_window_size > 0 {
        format!(
            "{} / {} tokens ({:.1}%)",
            format_number(stats.current_context_tokens),
            format_number(stats.context_window_size),
            usage_pct * 100.0
        )
    } else {
        "No context data yet".to_string()
    };

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style())
                .title(" Context Window "),
        )
        .gauge_style(Style::default().fg(context_color))
        .ratio(usage_pct.min(1.0))
        .label(gauge_label);
    f.render_widget(gauge, left_chunks[1]);

    // Top tools by call count
    let top_tools = stats.top_tools(10);
    let tool_items: Vec<ListItem> = top_tools
        .iter()
        .map(|(name, count)| {
            let avg = stats.avg_tool_duration(name);
            let avg_str = avg
                .map(|d| format!("{:.2}s", d.as_secs_f64()))
                .unwrap_or_else(|| "N/A".to_string());
            ListItem::new(format!(" {}: {} calls (avg: {})", name, count, avg_str))
                .style(Style::default().fg(theme.tool_call))
        })
        .collect();

    let tools_list = List::new(tool_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style())
            .title(" Top Tools "),
    );
    f.render_widget(tools_list, left_chunks[2]);

    // Right column: Token stats
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(12), // Token breakdown
            Constraint::Min(5),     // Cost analysis
        ])
        .split(chunks[1]);

    // Token breakdown
    let token_stats = format!(
        r#"
 Input Tokens:          {}
 Output Tokens:         {}
 Cache Creation:        {}
 Cache Read:            {}
 ─────────────────────────
 Total Tokens:          {}

 Thinking Blocks:       {}
 Thinking Tokens:       ~{}
"#,
        format_number(stats.total_input_tokens),
        format_number(stats.total_output_tokens),
        format_number(stats.total_cache_creation_tokens),
        format_number(stats.total_cache_read_tokens),
        format_number(stats.total_tokens()),
        stats.thinking_blocks,
        format_number(stats.thinking_tokens)
    );

    let tokens = Paragraph::new(token_stats)
        .style(Style::default().fg(theme.fg))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style())
                .title(" Token Usage "),
        );
    f.render_widget(tokens, right_chunks[0]);

    // Cost analysis
    let model_name = stats.current_model.as_deref().unwrap_or("Unknown");
    let total_cost = stats.total_cost();
    let cache_savings = stats.cache_savings();
    let cost_without_cache = total_cost + cache_savings;

    let cost_stats = format!(
        r#"
 Model:           {}

 Total Cost:      ${:.4}
 Cache Savings:   ${:.4}
 Without Cache:   ${:.4}

 Peak Context:    {} tokens
"#,
        model_name,
        total_cost,
        cache_savings,
        cost_without_cache,
        format_number(stats.peak_context_tokens)
    );

    let costs = Paragraph::new(cost_stats)
        .style(Style::default().fg(theme.fg))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style())
                .title(" Cost Analysis "),
        );
    f.render_widget(costs, right_chunks[1]);
}

/// Render the Help view with keybindings
fn render_help_view(f: &mut Frame, area: Rect, _app: &App, theme: &Theme) {
    let help_text = r#"
  NAVIGATION
  ──────────────────────────────────────────────────────
  Tab / Shift+Tab     Cycle through views (Events → Stats → Help)
  1, 2, 3             Jump to view (1=Events, 2=Stats, 3=Help)

  EVENTS VIEW
  ──────────────────────────────────────────────────────
  ↑/k, ↓/j            Navigate event list
  Enter               Toggle detail panel for selected event
  Esc                 Close detail panel
  Home / End          Jump to first/last event
  t                   Toggle thinking panel visibility

  THEMES
  ──────────────────────────────────────────────────────
  T / Shift+T         Cycle through themes

  Available themes:
    • Dark (default)  - Classic dark terminal colors
    • Light           - Light background for bright environments
    • Monokai         - Popular code editor theme
    • Dracula         - Dark purple-tinted theme
    • Nord            - Arctic, bluish color palette
    • Solarized       - Ethan Schoonover's precision colors

  GENERAL
  ──────────────────────────────────────────────────────
  q / Q               Quit the application
  ?                   Show this help (same as Help view)

  MOUSE
  ──────────────────────────────────────────────────────
  Scroll Up/Down      Navigate event list

  ABOUT
  ──────────────────────────────────────────────────────
  Anthropic Spy is an observability proxy for Claude Code.
  It intercepts API traffic and displays tool calls, responses,
  token usage, and thinking blocks in real-time.

  For more information, see the README.md file.
"#;

    let paragraph = Paragraph::new(help_text)
        .style(Style::default().fg(theme.fg))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style())
                .title(" Help - Keybindings "),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
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

/// Render the main list view showing all events
fn render_list_view(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let (start, end) = app.visible_range(area.height.saturating_sub(2) as usize);

    let items: Vec<ListItem> = app.events[start..end]
        .iter()
        .enumerate()
        .map(|(idx, event)| {
            let actual_idx = start + idx;
            let is_selected = actual_idx == app.selected;

            let line = format_event_line(event);
            let style = if is_selected {
                theme.selected_style()
            } else {
                event_color_style(event, theme)
            };

            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style())
            .title(format!(
                " Events ({}/{}) - ↑↓ to navigate ",
                app.selected + 1,
                app.events.len()
            )),
    );

    f.render_widget(list, area);
}

/// Render split view with list on top and details on bottom
fn render_split_view(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Render the list in the top half
    render_list_view(f, chunks[0], app, theme);

    // Render details in the bottom half
    if let Some(event) = app.selected_event() {
        render_detail_view(f, chunks[1], app, event, theme);
    }
}

/// Render detailed view of a single event
fn render_detail_view(f: &mut Frame, area: Rect, app: &App, event: &ProxyEvent, theme: &Theme) {
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

    // Show scroll indicator if there's more content
    let scroll_indicator = if total_lines > height {
        format!(" ({}/{}) ↑↓ to scroll ", start + 1, total_lines)
    } else {
        String::from(" ")
    };

    let paragraph = Paragraph::new(visible_text)
        .style(Style::default().fg(theme.fg))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_focused_style())
                .title(format!(
                    " Event Details - Press Enter to close {}",
                    scroll_indicator
                )),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
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
                &id[..8.min(id.len())]
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
                "HTTP Response\n\nRequest ID: {}\nTimestamp: {}\nStatus: {}\nBody Size: {} bytes\nDuration: {:.2}s{}",
                request_id,
                timestamp.to_rfc3339(),
                status,
                body_size,
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
    }
}

/// Get appropriate color style for an event
fn event_color_style(event: &ProxyEvent, theme: &Theme) -> Style {
    match event {
        ProxyEvent::ToolCall { .. } => Style::default().fg(theme.tool_call),
        ProxyEvent::ToolResult { success, .. } => {
            if *success {
                Style::default().fg(theme.tool_result_success)
            } else {
                Style::default().fg(theme.tool_result_failure)
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
    }
}

/// Render system logs panel at the bottom of the screen
pub fn render_logs_panel(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    // Get recent log entries from buffer
    let height = area.height.saturating_sub(2) as usize; // Account for borders
    let log_entries = app.log_buffer.get_recent(height);

    // Convert log entries to list items with color coding
    let items: Vec<ListItem> = log_entries
        .iter()
        .map(|entry| {
            let formatted = format_log_entry(entry);
            let style = log_level_style(&entry.level, theme);
            ListItem::new(formatted).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style())
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
fn log_level_style(level: &LogLevel, theme: &Theme) -> Style {
    match level {
        LogLevel::Error => Style::default()
            .fg(theme.log_error)
            .add_modifier(Modifier::BOLD),
        LogLevel::Warn => Style::default().fg(theme.log_warn),
        LogLevel::Info => Style::default().fg(theme.log_info),
        LogLevel::Debug => Style::default().fg(theme.log_debug),
        LogLevel::Trace => Style::default().fg(theme.log_trace),
    }
}
