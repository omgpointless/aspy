// Events view - main proxy event display
//
// Shows:
// - Event list (scrollable, with selection)
// - Detail panel (optional, toggled with Enter)
// - Thinking panel (rendered based on preset layout)
//
// This is the primary view of Aspy, showing all intercepted
// API traffic in real-time.

use crate::events::ProxyEvent;
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

/// Format an event as a single line for the list view
pub(crate) fn format_event_line(event: &ProxyEvent) -> String {
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
        ProxyEvent::UserPrompt { timestamp, content } => {
            let preview = if content.len() > 60 {
                format!("{}...", &content[..60])
            } else {
                content.clone()
            };
            format!("[{}] 👤 User: {}", timestamp.format("%H:%M:%S"), preview)
        }
        ProxyEvent::AssistantResponse { timestamp, content } => {
            let preview = if content.len() > 60 {
                format!("{}...", &content[..60])
            } else {
                content.clone()
            };
            format!("[{}] 🤖 Assistant: {}", timestamp.format("%H:%M:%S"), preview)
        }
    }
}

/// Format an event as detailed text for the detail view
pub(crate) fn format_event_detail(event: &ProxyEvent) -> String {
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
        ProxyEvent::UserPrompt { timestamp, content } => {
            format!(
                "👤 User Prompt\n\nTimestamp: {}\n\nContent:\n{}",
                timestamp.to_rfc3339(),
                content
            )
        }
        ProxyEvent::AssistantResponse { timestamp, content } => {
            format!(
                "🤖 Assistant Response\n\nTimestamp: {}\n\nContent:\n{}",
                timestamp.to_rfc3339(),
                content
            )
        }
    }
}
