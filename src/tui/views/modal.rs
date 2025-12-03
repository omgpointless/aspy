// Modal overlay rendering
//
// Modals are rendered on top of the main content:
// - Help modal: keyboard shortcuts and current config
// - Detail modal: event details (full screen overlay)

use crate::tui::app::App;
use crate::tui::components::scrollbar::{render_scrollbar_raw, ScrollbarStyle};
use crate::tui::markdown;
use crate::tui::modal::Modal;
use crate::tui::traits::{Copyable, Scrollable};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::events::RenderableContent;

/// Render a modal dialog as a centered overlay
pub fn render(f: &mut Frame, modal: &Modal, app: &mut App) {
    match modal {
        Modal::Help => render_help(f, app),
        Modal::Detail(event_idx) => render_detail(f, app, *event_idx),
        Modal::LogDetail => render_log_detail(f, app),
    }
}

/// Calculate centered rect for modal dialog
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

/// Render the help modal overlay
fn render_help(f: &mut Frame, app: &App) {
    // Styles
    let key_style = Style::default().fg(app.theme.tool_call);
    let desc_style = Style::default().fg(app.theme.foreground);
    let header_style = Style::default()
        .fg(app.theme.highlight)
        .add_modifier(Modifier::BOLD);
    let divider_style = Style::default().fg(app.theme.border);

    // Helper to create a keybind line: "    key         description"
    let kb = |key: &str, desc: &str| -> Line {
        Line::from(vec![
            Span::raw("    "),
            Span::styled(format!("{:<12}", key), key_style),
            Span::styled(desc.to_string(), desc_style),
        ])
    };

    let content = Text::from(vec![
        Line::raw(""),
        Line::from(Span::styled("  Views", header_style)),
        kb("F1, e", "Events (main view)"),
        kb("F2, s", "Statistics"),
        kb("F3", "Settings"),
        Line::raw(""),
        Line::from(Span::styled("  Navigation", header_style)),
        kb("↑/↓, j/k", "Scroll list / detail"),
        kb("Enter", "Open detail / apply"),
        kb("Esc", "Close / go back"),
        kb("Home/End", "Jump to start/end"),
        Line::raw(""),
        Line::from(Span::styled("  Settings Navigation", header_style)),
        kb("Tab/→", "Switch pane focus"),
        kb("↑/↓", "Navigate options"),
        kb("Enter", "Apply selection"),
        Line::raw(""),
        Line::from(Span::styled("  Events View", header_style)),
        kb("Tab", "Cycle panel focus"),
        kb("Shift+Tab", "Focus previous panel"),
        Line::raw(""),
        Line::from(Span::styled("  Clipboard", header_style)),
        kb("y", "Copy to clipboard (text)"),
        kb("Y", "Copy to clipboard (JSONL)"),
        Line::raw(""),
        Line::from(Span::styled("  General", header_style)),
        kb("?", "Toggle this help"),
        kb("q", "Quit"),
        Line::raw(""),
        Line::from(Span::styled("  Mouse", header_style)),
        kb("Scroll", "Navigate events"),
        Line::raw(""),
        Line::from(Span::styled(
            "  ──────────────────────────────────",
            divider_style,
        )),
        Line::from(vec![
            Span::styled("  Theme: ", desc_style),
            Span::styled(&app.theme.name, key_style),
            Span::styled("  |  Preset: ", desc_style),
            Span::styled(&app.preset.name, key_style),
        ]),
    ]);

    // Calculate modal size
    let width = 44;
    let height = 34;
    let area = centered_rect(width, height, f.area());

    // Clear the area behind the modal
    f.render_widget(Clear, area);

    let paragraph = Paragraph::new(content)
        .style(Style::default().bg(app.theme.background))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(app.theme.highlight))
                .border_type(app.theme.border_type)
                .title(" Help ")
                .title_bottom(Line::from(" Press ? or Esc to close ").centered()),
        );

    f.render_widget(paragraph, area);
}

/// Render the detail modal overlay
///
/// Dispatches on RenderableContent type:
/// - Markdown: text wrapping + markdown formatting, vertical scroll only
/// - Structured: preserve formatting, 2D scrolling for wide content (JSON)
fn render_detail(f: &mut Frame, app: &mut App, event_idx: usize) {
    use super::format_event_detail;

    // Get event if it exists
    let Some(event) = app.events.get(event_idx) else {
        // Event no longer exists (rare race condition) - close modal
        return;
    };

    let renderable = format_event_detail(event);

    // Use nearly full screen (90% width, 85% height)
    let frame_area = f.area();
    let width = (frame_area.width * 90 / 100).max(60);
    let height = (frame_area.height * 85 / 100).max(20);
    let area = centered_rect(width, height, frame_area);

    // Clear area first
    f.render_widget(Clear, area);

    // Calculate viewport dimensions (subtract borders)
    let viewport_height = area.height.saturating_sub(2) as usize;
    let viewport_width = area.width.saturating_sub(2) as usize;

    match renderable {
        RenderableContent::Markdown(content) => {
            render_markdown_detail(f, app, area, &content, viewport_width, viewport_height);
        }
        RenderableContent::Structured(content) => {
            render_structured_detail(f, app, area, &content, viewport_width, viewport_height);
        }
    }
}

/// Render markdown content with text wrapping (vertical scroll only)
fn render_markdown_detail(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    content: &str,
    viewport_width: usize,
    viewport_height: usize,
) {
    // Use markdown renderer for text wrapping and formatting
    let lines = markdown::render_markdown(content, viewport_width, &app.theme);
    let total_lines = lines.len();

    // Update scroll dimensions
    app.detail_panel
        .scroll_state_mut()
        .update_dimensions(total_lines, viewport_height);

    let vertical_offset = app.detail_panel.scroll_state().offset();
    let v_start = vertical_offset.min(total_lines.saturating_sub(viewport_height));

    // Scroll info
    let scroll_info = if total_lines > viewport_height {
        format!(" ({}/{}) ", v_start + 1, total_lines)
    } else {
        String::new()
    };

    let paragraph = Paragraph::new(lines)
        .style(
            Style::default()
                .fg(app.theme.foreground)
                .bg(app.theme.background),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(app.theme.border_type)
                .border_style(Style::default().fg(app.theme.highlight))
                .title(format!(" Event Details{} ", scroll_info))
                .title_bottom(
                    Line::from(" ↑↓:scroll  PgUp/Dn:page  y:copy  Esc:close ").centered(),
                ),
        )
        .scroll((v_start as u16, 0));

    f.render_widget(paragraph, area);

    // Render vertical scrollbar only
    render_scrollbar_raw(
        f,
        area,
        total_lines,
        viewport_height,
        v_start,
        ScrollbarStyle::Arrows,
    );
}

/// Render structured content with 2D scrolling (for JSON, code output)
fn render_structured_detail(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    content: &str,
    viewport_width: usize,
    viewport_height: usize,
) {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // Update detail panel scroll dimensions
    app.detail_panel
        .scroll_state_mut()
        .update_dimensions(total_lines, viewport_height);

    // Get scroll offsets
    let vertical_offset = app.detail_panel.scroll_state().offset();
    let horizontal_offset = app.detail_panel.horizontal_offset();

    // Calculate visible vertical range
    let v_start = vertical_offset.min(total_lines.saturating_sub(viewport_height));
    let v_end = (v_start + viewport_height).min(total_lines);

    // Find max line width for horizontal scrollbar
    let max_line_width = lines.iter().map(|line| line.len()).max().unwrap_or(0);

    // Clip lines horizontally and vertically
    let visible_lines: Vec<String> = lines[v_start..v_end]
        .iter()
        .map(|line| {
            // Skip to horizontal offset, then take viewport width
            line.chars()
                .skip(horizontal_offset)
                .take(viewport_width)
                .collect()
        })
        .collect();

    let visible_text = visible_lines.join("\n");

    // Scroll info shows both dimensions if needed
    let v_scroll_info = if total_lines > viewport_height {
        format!("V:{}/{} ", v_start + 1, total_lines)
    } else {
        String::new()
    };

    let h_scroll_info = if max_line_width > viewport_width {
        format!("H:{}/{} ", horizontal_offset + 1, max_line_width)
    } else {
        String::new()
    };

    let scroll_info = if !v_scroll_info.is_empty() || !h_scroll_info.is_empty() {
        format!(" ({}{}) ", v_scroll_info, h_scroll_info)
    } else {
        String::new()
    };

    let paragraph = Paragraph::new(visible_text)
        .style(
            Style::default()
                .fg(app.theme.foreground)
                .bg(app.theme.background),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(app.theme.border_type)
                .border_style(Style::default().fg(app.theme.highlight))
                .title(format!(" Event Details{} ", scroll_info))
                .title_bottom(
                    Line::from(" ↑↓←→:scroll  PgUp/Dn:page  y:copy  Esc:close ").centered(),
                ),
        );

    f.render_widget(paragraph, area);

    // Render vertical scrollbar (right edge)
    render_scrollbar_raw(
        f,
        area,
        total_lines,
        viewport_height,
        v_start,
        ScrollbarStyle::Arrows,
    );

    // Render horizontal scrollbar (bottom edge) if content wider than viewport
    if max_line_width > viewport_width {
        render_horizontal_scrollbar(
            f,
            area,
            max_line_width,
            viewport_width,
            horizontal_offset,
            &app.theme,
        );
    }
}

/// Render horizontal scrollbar at bottom of area
fn render_horizontal_scrollbar(
    f: &mut Frame,
    area: Rect,
    content_width: usize,
    viewport_width: usize,
    offset: usize,
    theme: &crate::theme::Theme,
) {
    if content_width <= viewport_width {
        return;
    }

    // Calculate scrollbar position (bottom row, inside borders)
    let scrollbar_y = area.y + area.height.saturating_sub(2);
    let scrollbar_x = area.x + 1; // Inside left border
    let scrollbar_width = area.width.saturating_sub(2); // Inside borders

    // Calculate thumb size and position
    let thumb_size =
        ((viewport_width as f64 / content_width as f64) * scrollbar_width as f64).max(1.0) as u16;
    let max_offset = content_width.saturating_sub(viewport_width);
    let thumb_pos = if max_offset > 0 {
        ((offset as f64 / max_offset as f64) * (scrollbar_width.saturating_sub(thumb_size)) as f64)
            as u16
    } else {
        0
    };

    // Build scrollbar string: ◄ ═══▓▓▓═══ ►
    let mut bar = String::new();
    bar.push('◄'); // Left arrow

    for i in 1..scrollbar_width.saturating_sub(1) {
        if i > thumb_pos && i <= thumb_pos + thumb_size {
            bar.push('▓'); // Thumb
        } else {
            bar.push('═'); // Track
        }
    }

    bar.push('►'); // Right arrow

    let scrollbar_area = Rect::new(scrollbar_x, scrollbar_y, scrollbar_width, 1);
    let scrollbar_widget = Paragraph::new(bar).style(Style::default().fg(theme.border));

    f.render_widget(scrollbar_widget, scrollbar_area);
}

/// Render the log detail modal overlay
/// Uses the already-cached content in detail_panel (set when Enter was pressed)
fn render_log_detail(f: &mut Frame, app: &mut App) {
    // Get cached content from detail_panel
    let content = app.detail_panel.copy_text().unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // Use nearly full screen (90% width, 85% height)
    let frame_area = f.area();
    let width = (frame_area.width * 90 / 100).max(60);
    let height = (frame_area.height * 85 / 100).max(20);
    let area = centered_rect(width, height, frame_area);

    // Clear area first
    f.render_widget(Clear, area);

    // Calculate viewport dimensions (subtract borders)
    let viewport_height = area.height.saturating_sub(2) as usize;
    let viewport_width = area.width.saturating_sub(2) as usize;

    // Update detail panel scroll dimensions
    app.detail_panel
        .scroll_state_mut()
        .update_dimensions(total_lines, viewport_height);

    // Get scroll offsets
    let vertical_offset = app.detail_panel.scroll_state().offset();
    let horizontal_offset = app.detail_panel.horizontal_offset();

    // Calculate visible vertical range
    let v_start = vertical_offset.min(total_lines.saturating_sub(viewport_height));
    let v_end = (v_start + viewport_height).min(total_lines);

    // Clip lines horizontally and vertically
    let visible_lines: Vec<String> = lines
        .get(v_start..v_end)
        .unwrap_or(&[])
        .iter()
        .map(|line| {
            line.chars()
                .skip(horizontal_offset)
                .take(viewport_width)
                .collect()
        })
        .collect();

    let visible_text = visible_lines.join("\n");

    // Scroll info
    let scroll_info = if total_lines > viewport_height {
        format!(" ({}/{}) ", v_start + 1, total_lines)
    } else {
        String::new()
    };

    let paragraph = Paragraph::new(visible_text)
        .style(
            Style::default()
                .fg(app.theme.foreground)
                .bg(app.theme.background),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(app.theme.border_type)
                .border_style(Style::default().fg(app.theme.panel_logs))
                .title(format!(" Log Details{} ", scroll_info))
                .title_bottom(Line::from(" ↑↓:scroll  y:copy  Esc:close ").centered()),
        );

    f.render_widget(paragraph, area);

    // Render vertical scrollbar (right edge)
    render_scrollbar_raw(
        f,
        area,
        total_lines,
        viewport_height,
        v_start,
        ScrollbarStyle::Arrows,
    );
}
