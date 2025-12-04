// Events view - main proxy event display
//
// Shows:
// - Event list (scrollable, with selection)
// - Detail panel (optional, toggled with Enter)
// - Thinking panel (rendered based on preset layout)
//
// This is the primary view of Aspy, showing all intercepted
// API traffic in real-time.

use crate::events::{ProxyEvent, TrackedEvent};
use crate::tui::app::App;
use crate::tui::layout::Breakpoint;
use crate::tui::preset::{LayoutDirection, Panel};
use crate::tui::scroll::FocusablePanel;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

// Import shared utilities from components
use super::super::components::format_number;

// ============================================================================
// Detail content types
// ============================================================================

/// Content for detail view rendering, indicating how it should be displayed.
///
/// Follows the "Data vs Rendering" principle: the formatter decides what kind
/// of content this is, the renderer decides how to display it.
pub(crate) enum RenderableContent {
    /// Markdown content - wrap text at word boundaries, render formatting
    /// (headings, inline code, code blocks, lists). Used for human-readable
    /// content like thinking, assistant responses, user prompts.
    Markdown(String),

    /// Structured content - preserve formatting exactly (JSON, code output).
    /// May require horizontal scrolling for wide content.
    #[allow(dead_code)] // Reserved for future use (raw logs, non-wrapped content)
    Structured(String),
}

impl RenderableContent {
    /// Get the raw string content for clipboard operations
    pub fn as_str(&self) -> &str {
        match self {
            RenderableContent::Markdown(s) => s,
            RenderableContent::Structured(s) => s,
        }
    }
}

/// Main render function for the Events view
pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    let bp = Breakpoint::from_width(area.width);

    // Get layout from preset
    let resolved = app.preset.events_view.layout.resolve(bp);
    let direction = match app.preset.events_view.layout.direction {
        LayoutDirection::Horizontal => Direction::Horizontal,
        LayoutDirection::Vertical => Direction::Vertical,
    };

    // Build constraints from resolved layout
    let constraints: Vec<Constraint> = resolved.iter().map(|(_, c)| *c).collect();

    // Split area based on preset layout
    let chunks = Layout::default()
        .direction(direction)
        .constraints(constraints)
        .split(area);

    // Render each panel by its position in the preset
    // Note: Detail view is now a modal, not a split panel
    for (i, (panel, _)) in resolved.iter().enumerate() {
        match panel {
            Panel::Events => render_list_view(f, chunks[i], app),
            Panel::Thinking => render_thinking_panel(f, chunks[i], app),
            _ => {} // Other panels not used in events_view
        }
    }
}

// ============================================================================
// Event list rendering
// ============================================================================

/// Render the main list view showing all events
fn render_list_view(f: &mut Frame, area: Rect, app: &App) {
    use super::super::components::events_panel;

    // Delegate to EventsPanel component
    events_panel::render(
        f,
        area,
        &app.events_panel,
        &app.events,
        &app.theme,
        app.is_focused(FocusablePanel::Events),
    );
}

// ============================================================================
// Thinking panel
// ============================================================================

/// Render the thinking panel using the ThinkingPanel component
fn render_thinking_panel(f: &mut Frame, area: Rect, app: &mut App) {
    super::super::components::thinking_panel::render(f, area, app);
}

// ============================================================================
// Event formatting
// ============================================================================

/// Sanitize content for single-line display
///
/// Replaces newlines, tabs, and control characters with spaces to prevent
/// rendering artifacts in list items. Collapses multiple whitespace to single space.
fn sanitize_preview(content: &str) -> String {
    content
        .chars()
        .map(|c| {
            if c.is_control() || c == '\n' || c == '\r' || c == '\t' {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Format a tracked event as a single line for the list view
///
/// Format: `[HH:MM:SS] @user_id 🔧 Event: details`
/// If user_id is None, the @prefix is omitted.
pub(crate) fn format_event_line(tracked: &TrackedEvent) -> String {
    // Build user prefix (e.g., "@foundry " or "" if none)
    let user_prefix = tracked
        .user_id
        .as_ref()
        .map(|id| format!("@{} ", id))
        .unwrap_or_default();

    let event = &tracked.event;
    match event {
        ProxyEvent::ToolCall {
            timestamp,
            tool_name,
            id,
            ..
        } => {
            format!(
                "[{}] {}🔧 Tool Call: {} ({})",
                timestamp.format("%H:%M:%S"),
                user_prefix,
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
                "[{}] {}{} Tool Result: {} ({:.2}s)",
                timestamp.format("%H:%M:%S"),
                user_prefix,
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
                "[{}] {}← Request: {} {}",
                timestamp.format("%H:%M:%S"),
                user_prefix,
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
                "[{}] {}→ Response: {} ({:.2}s)",
                timestamp.format("%H:%M:%S"),
                user_prefix,
                status,
                duration.as_secs_f64()
            )
        }
        ProxyEvent::Error {
            timestamp, message, ..
        } => {
            format!(
                "[{}] {}❌ Error: {}",
                timestamp.format("%H:%M:%S"),
                user_prefix,
                message
            )
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
                "[{}] {}📋 Headers Captured{}",
                timestamp.format("%H:%M:%S"),
                user_prefix,
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
                "[{}] {}⚖️  Rate Limits: Req={:?} Tok={:?}",
                timestamp.format("%H:%M:%S"),
                user_prefix,
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
                    "[{}] {}📊 Usage: {}in + {}out + {}cached",
                    timestamp.format("%H:%M:%S"),
                    user_prefix,
                    format_number(*input_tokens as u64),
                    format_number(*output_tokens as u64),
                    format_number(*cache_read_tokens as u64)
                )
            } else {
                format!(
                    "[{}] {}📊 Usage: {}in + {}out",
                    timestamp.format("%H:%M:%S"),
                    user_prefix,
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
            let preview: String = content
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(50)
                .collect();
            format!(
                "[{}] {}💭 Thinking: {}... (~{} tok)",
                timestamp.format("%H:%M:%S"),
                user_prefix,
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
                "[{}] {}📦 Context Compact: {}K → {}K",
                timestamp.format("%H:%M:%S"),
                user_prefix,
                previous_context / 1000,
                new_context / 1000
            )
        }
        ProxyEvent::ThinkingStarted { timestamp } => {
            format!(
                "[{}] {}💭 Thinking...",
                timestamp.format("%H:%M:%S"),
                user_prefix
            )
        }
        ProxyEvent::UserPrompt { timestamp, content } => {
            let sanitized = sanitize_preview(content);
            let preview = if sanitized.chars().count() > 60 {
                format!("{}...", sanitized.chars().take(60).collect::<String>())
            } else {
                sanitized
            };
            format!(
                "[{}] {}👤 User: {}",
                timestamp.format("%H:%M:%S"),
                user_prefix,
                preview
            )
        }
        ProxyEvent::AssistantResponse { timestamp, content } => {
            let sanitized = sanitize_preview(content);
            let preview = if sanitized.chars().count() > 60 {
                format!("{}...", sanitized.chars().take(60).collect::<String>())
            } else {
                sanitized
            };
            format!(
                "[{}] {}🤖 Assistant: {}",
                timestamp.format("%H:%M:%S"),
                user_prefix,
                preview
            )
        }
    }
}

/// Format tracking metadata as a small header section
///
/// Shows user_id and session_id when present, for observability.
fn format_tracking_header(tracked: &TrackedEvent) -> String {
    let mut parts = Vec::new();

    if let Some(ref user_id) = tracked.user_id {
        parts.push(format!("**User:** @{}", user_id));
    }

    if let Some(ref session_id) = tracked.session_id {
        // Show truncated session_id (first 8 chars) with full in parens
        let short = if session_id.len() > 8 {
            &session_id[..8]
        } else {
            session_id
        };
        parts.push(format!("**Session:** `{}`", short));
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!("{}\n\n", parts.join("  "))
    }
}

/// Format a tracked event as detailed content for the detail view
///
/// Returns `RenderableContent` to indicate how the content should be displayed:
/// - `Markdown`: Human-readable content (thinking, responses, prompts)
/// - `Structured`: Machine-formatted content (JSON, code output)
pub(crate) fn format_event_detail(tracked: &TrackedEvent) -> RenderableContent {
    let tracking_header = format_tracking_header(tracked);
    let event = &tracked.event;

    match event {
        ProxyEvent::ToolCall {
            id,
            timestamp,
            tool_name,
            input,
        } => RenderableContent::Markdown(format!(
            "{}## 🔧 Tool Call\n\n\
            **ID:** {}  \n\
            **Timestamp:** {}  \n\
            **Tool:** `{}`\n\n\
            ---\n\n\
            ```json\n{}\n```",
            tracking_header,
            id,
            timestamp.to_rfc3339(),
            tool_name,
            serde_json::to_string_pretty(input).unwrap_or_else(|_| "N/A".to_string())
        )),
        ProxyEvent::ToolResult {
            id,
            timestamp,
            tool_name,
            output,
            duration,
            success,
        } => {
            let status_icon = if *success { "✓" } else { "✗" };
            RenderableContent::Markdown(format!(
                "{}## {} Tool Result\n\n\
                **ID:** {}  \n\
                **Timestamp:** {}  \n\
                **Tool:** `{}`  \n\
                **Success:** {}  \n\
                **Duration:** {:.2}s\n\n\
                ---\n\n\
                ```json\n{}\n```",
                tracking_header,
                status_icon,
                id,
                timestamp.to_rfc3339(),
                tool_name,
                success,
                duration.as_secs_f64(),
                serde_json::to_string_pretty(output).unwrap_or_else(|_| "N/A".to_string())
            ))
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
                    "\n\n---\n\n```json\n{}\n```",
                    serde_json::to_string_pretty(json_body)
                        .unwrap_or_else(|_| "Failed to format".to_string())
                )
            } else {
                String::new()
            };

            RenderableContent::Markdown(format!(
                "{}## ← HTTP Request\n\n\
                **ID:** {}  \n\
                **Timestamp:** {}  \n\
                **Method:** {}  \n\
                **Path:** {}  \n\
                **Body Size:** {} bytes{}",
                tracking_header,
                id,
                timestamp.to_rfc3339(),
                method,
                path,
                body_size,
                body_content
            ))
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
                    "\n\n---\n\n```json\n{}\n```",
                    serde_json::to_string_pretty(json_body)
                        .unwrap_or_else(|_| "Failed to format".to_string())
                )
            } else {
                String::new()
            };

            RenderableContent::Markdown(format!(
                "{}## → HTTP Response\n\n\
                **Request ID:** {}  \n\
                **Timestamp:** {}  \n\
                **Status:** {}  \n\
                **Body Size:** {} bytes  \n\
                **TTFB:** {}ms  \n\
                **Total Duration:** {:.2}s{}",
                tracking_header,
                request_id,
                timestamp.to_rfc3339(),
                status,
                body_size,
                ttfb.as_millis(),
                duration.as_secs_f64(),
                body_content
            ))
        }
        ProxyEvent::Error {
            timestamp,
            message,
            context,
        } => RenderableContent::Markdown(format!(
            "{}## ❌ Error\n\n\
            **Timestamp:** {}  \n\
            **Message:** {}  \n\
            **Context:** {}",
            tracking_header,
            timestamp.to_rfc3339(),
            message,
            context.as_deref().unwrap_or("N/A")
        )),
        ProxyEvent::HeadersCaptured {
            timestamp, headers, ..
        } => {
            let beta_features = if !headers.anthropic_beta.is_empty() {
                headers.anthropic_beta.join(", ")
            } else {
                "None".to_string()
            };

            RenderableContent::Markdown(format!(
                "{}## 📋 Headers Captured\n\n\
                **Timestamp:** {}\n\n\
                ---\n\n\
                ### Request Headers\n\n\
                **API Version:** {}  \n\
                **Beta Features:** {}  \n\
                **API Key Hash:** `{}`\n\n\
                ### Response Headers\n\n\
                **Request ID:** {}  \n\
                **Org ID:** {}\n\n\
                ### Rate Limits\n\n\
                **Requests:** {}/{} ({}%)  \n\
                **Tokens:** {}/{} ({}%)  \n\
                **Reset:** {}",
                tracking_header,
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
            ))
        }
        ProxyEvent::RateLimitUpdate {
            timestamp,
            requests_remaining,
            requests_limit,
            tokens_remaining,
            tokens_limit,
            reset_time,
        } => {
            RenderableContent::Markdown(format!(
                "{}## ⚖️ Rate Limit Update\n\n\
                **Timestamp:** {}\n\n\
                ---\n\n\
                **Requests:** {}/{}  \n\
                **Tokens:** {}/{}  \n\
                **Reset:** {}",
                tracking_header,
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
            ))
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
                    "\n\n### Cache Statistics\n\n\
                    **Cache Creation:** {} tokens  \n\
                    **Cache Read:** {} tokens  \n\
                    **Cache Savings:** ${:.4} (vs regular input)",
                    format_number(*cache_creation_tokens as u64),
                    format_number(*cache_read_tokens as u64),
                    cache_savings
                )
            } else {
                String::new()
            };

            RenderableContent::Markdown(format!(
                "{}## 📊 API Usage\n\n\
                **Timestamp:** {}  \n\
                **Model:** `{}`\n\n\
                ---\n\n\
                ### Token Breakdown\n\n\
                **Input:** {} tokens  \n\
                **Output:** {} tokens  \n\
                **Total:** {} tokens\n\n\
                **Estimated Cost:** ${:.4}{}",
                tracking_header,
                timestamp.to_rfc3339(),
                model,
                format_number(*input_tokens as u64),
                format_number(*output_tokens as u64),
                format_number(total as u64),
                cost,
                cache_info
            ))
        }
        ProxyEvent::Thinking {
            timestamp,
            content,
            token_estimate,
        } => RenderableContent::Markdown(format!(
            "{}## 💭 Claude's Thinking\n\n**Timestamp:** {}  \n**Estimated Tokens:** ~{}\n\n---\n\n{}",
            tracking_header,
            timestamp.to_rfc3339(),
            token_estimate,
            content
        )),
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
            RenderableContent::Markdown(format!(
                "{}## 📦 Context Compaction Detected\n\n\
                **Timestamp:** {}\n\n\
                **Previous Context:** {} tokens ({:.1}K)  \n\
                **New Context:** {} tokens ({:.1}K)  \n\
                **Reduction:** {} tokens ({:.1}%)\n\n\
                Claude Code triggered a context window compaction to \
                reduce memory usage and stay within limits.",
                tracking_header,
                timestamp.to_rfc3339(),
                previous_context,
                *previous_context as f64 / 1000.0,
                new_context,
                *new_context as f64 / 1000.0,
                reduction,
                reduction_pct
            ))
        }
        ProxyEvent::ThinkingStarted { timestamp } => RenderableContent::Markdown(format!(
            "{}## 💭 Thinking Started\n\n**Timestamp:** {}\n\nClaude is processing your request...",
            tracking_header,
            timestamp.to_rfc3339()
        )),
        ProxyEvent::UserPrompt { timestamp, content } => RenderableContent::Markdown(format!(
            "{}## 👤 User Prompt\n\n**Timestamp:** {}\n\n---\n\n{}",
            tracking_header,
            timestamp.to_rfc3339(),
            content
        )),
        ProxyEvent::AssistantResponse { timestamp, content } => RenderableContent::Markdown(format!(
            "{}## 🤖 Assistant Response\n\n**Timestamp:** {}\n\n---\n\n{}",
            tracking_header,
            timestamp.to_rfc3339(),
            content
        )),
    }
}
