// Demo mode: Generate realistic mock events to showcase the TUI
//
// This module generates events that look like a real Claude Code session,
// showing tool calls with realistic inputs, thinking blocks, and usage information.
//
// Key features demonstrated:
// - Context window growth (watch the context bar fill up)
// - Tool calls with realistic durations in ToolResults
// - Model variety (Sonnet for reasoning, Haiku for quick tasks)
// - Cache efficiency (high cache hit rates typical of Claude Code)
//
// Run with: ASPY_DEMO=1 cargo run --release

use crate::events::{ProxyEvent, TrackedEvent};
use crate::StreamingThinking;
use chrono::Utc;
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::sleep;

/// Demo user identifier for tracked events
const DEMO_USER_ID: &str = "demo";

/// Wrap a ProxyEvent in TrackedEvent with demo user context
fn wrap_demo_event(event: ProxyEvent) -> TrackedEvent {
    TrackedEvent::new(event, Some(DEMO_USER_ID.to_string()), None)
}

/// Generate a sequence of demo events simulating a Claude Code session
pub async fn run_demo(
    tx: mpsc::Sender<TrackedEvent>,
    mut shutdown_rx: oneshot::Receiver<()>,
    streaming_thinking: StreamingThinking,
) {
    // Initial delay to let TUI render
    sleep(Duration::from_millis(1500)).await;

    // Simulate a realistic Claude Code interaction sequence
    let events = generate_demo_sequence();

    for (event, delay_ms) in events {
        // Check for shutdown signal before sending
        if shutdown_rx.try_recv().is_ok() {
            return;
        }

        // For Thinking events, stream the content progressively
        if let ProxyEvent::Thinking {
            content,
            token_estimate,
            ..
        } = &event
        {
            // First emit ThinkingStarted and clear the buffer for demo user
            if let Ok(mut map) = streaming_thinking.lock() {
                map.insert(DEMO_USER_ID.to_string(), String::new());
            }
            let _ = tx
                .send(wrap_demo_event(ProxyEvent::ThinkingStarted {
                    timestamp: Utc::now(),
                }))
                .await;

            // Stream thinking content word by word
            stream_thinking_content(&streaming_thinking, content).await;

            // Small pause before emitting the full event
            sleep(Duration::from_millis(200)).await;

            // Now emit the complete Thinking event (for stats tracking)
            let _ = tx
                .send(wrap_demo_event(ProxyEvent::Thinking {
                    timestamp: Utc::now(),
                    content: content.clone(),
                    token_estimate: *token_estimate,
                }))
                .await;
        } else {
            // Non-thinking events: send normally
            if tx.send(wrap_demo_event(event)).await.is_err() {
                break;
            }
        }

        sleep(Duration::from_millis(delay_ms)).await;
    }

    // Keep running so TUI stays active, but listen for shutdown
    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                tracing::info!("Demo received shutdown signal");
                return;
            }
            _ = sleep(Duration::from_secs(60)) => {}
        }
    }
}

/// Stream thinking content progressively for realistic effect
async fn stream_thinking_content(streaming_thinking: &StreamingThinking, content: &str) {
    // Split into words and stream with delays
    // Slower speed (50ms) works better for recordings and readability
    let words: Vec<&str> = content.split_whitespace().collect();
    let delay_per_word = Duration::from_millis(50); // ~20 words/sec, good for recordings

    for (i, word) in words.iter().enumerate() {
        if let Ok(mut map) = streaming_thinking.lock() {
            let buf = map.entry(DEMO_USER_ID.to_string()).or_default();
            if i > 0 {
                buf.push(' ');
            }
            buf.push_str(word);
        }
        sleep(delay_per_word).await;
    }
}

fn generate_demo_sequence() -> Vec<(ProxyEvent, u64)> {
    let mut events = Vec::new();
    let mut id_counter = 0u64;

    // Helper to generate realistic tool IDs (matches real format)
    let mut next_id = || {
        id_counter += 1;
        format!("toolu_01{:020x}", id_counter)
    };

    // Track last tool call ID for matching results
    let mut last_tool_id: String;

    // === Phase 1: Initial request ===
    // Haiku summarizes the topic (this sets the header topic display)
    events.push((
        ProxyEvent::Response {
            request_id: "haiku-topic-1".to_string(),
            timestamp: Utc::now(),
            status: 200,
            body_size: 150,
            ttfb: Duration::from_millis(180),
            duration: Duration::from_millis(420),
            body: Some(json!({
                "model": "claude-haiku-4-5-20251001",
                "content": [{
                    "type": "text",
                    "text": "{\"isNewTopic\": true, \"title\": \"Code Review Task\"}"
                }]
            })),
            raw_body: None,
        },
        300,
    ));

    // Opus handles reasoning with full context (~45K initial)
    events.push((
        ProxyEvent::Thinking {
            timestamp: Utc::now(),
            content: "The user wants me to help with a code review task. Let me first understand the codebase structure by reading the relevant configuration files.\n\nI should start by checking if there's a README or project configuration that explains the architecture.".to_string(),
            token_estimate: 52,
        },
        800,
    ));

    // Initial context: ~25K tokens (start lower for visible growth)
    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-opus-4-5-20251101".to_string(),
            input_tokens: 3500,
            output_tokens: 285,
            cache_creation_tokens: 8000,
            cache_read_tokens: 22000,
        },
        400,
    ));

    // === Read START_HERE.md ===
    last_tool_id = next_id();
    events.push((
        ProxyEvent::ToolCall {
            id: last_tool_id.clone(),
            timestamp: Utc::now(),
            tool_name: "Read".to_string(),
            input: json!({
                "file_path": "/projects/example/.claude/START_HERE.md"
            }),
        },
        50,
    ));

    events.push((
        ProxyEvent::ToolResult {
            id: format!("{}-result", last_tool_id),
            timestamp: Utc::now(),
            tool_name: "Read".to_string(),
            output: json!("# Project Overview\n\nThis is a Rust TUI application..."),
            duration: Duration::from_millis(45),
            success: true,
        },
        100,
    ));

    // Context grows: ~35K (23%)
    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-opus-4-5-20251101".to_string(),
            input_tokens: 4500,
            output_tokens: 174,
            cache_creation_tokens: 5000,
            cache_read_tokens: 30000,
        },
        600,
    ));

    // === Thinking: Planning ===
    events.push((
        ProxyEvent::Thinking {
            timestamp: Utc::now(),
            content: "I've reviewed the project context. This is a Rust project with a TUI component. The user's task requires me to:\n\n1. Understand the current architecture\n2. Identify the relevant files\n3. Make targeted improvements\n\nLet me create a task list to track my progress, then explore the source files.".to_string(),
            token_estimate: 68,
        },
        600,
    ));

    // === TodoWrite (Haiku for quick task) ===
    last_tool_id = next_id();
    events.push((
        ProxyEvent::ToolCall {
            id: last_tool_id.clone(),
            timestamp: Utc::now(),
            tool_name: "TodoWrite".to_string(),
            input: json!({
                "todos": [
                    {"content": "Review project architecture", "status": "in_progress", "activeForm": "Reviewing project architecture"},
                    {"content": "Identify key files to modify", "status": "pending", "activeForm": "Identifying key files"},
                    {"content": "Implement required changes", "status": "pending", "activeForm": "Implementing changes"},
                    {"content": "Test and verify", "status": "pending", "activeForm": "Testing and verifying"}
                ]
            }),
        },
        30,
    ));

    events.push((
        ProxyEvent::ToolResult {
            id: format!("{}-result", last_tool_id),
            timestamp: Utc::now(),
            tool_name: "TodoWrite".to_string(),
            output: json!({"status": "success"}),
            duration: Duration::from_millis(12),
            success: true,
        },
        50,
    ));

    // Haiku gets minimal context (~400 tokens) - quick dispatch
    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-haiku-4-5-20251001".to_string(),
            input_tokens: 380,
            output_tokens: 156,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
        },
        300,
    ));

    // === Burst of parallel Reads ===
    let files_to_read = vec![
        ("src/main.rs", 89),
        ("src/events.rs", 62),
        ("src/parser/mod.rs", 156),
        ("Cargo.toml", 23),
    ];

    for (file, duration_ms) in &files_to_read {
        last_tool_id = next_id();
        events.push((
            ProxyEvent::ToolCall {
                id: last_tool_id.clone(),
                timestamp: Utc::now(),
                tool_name: "Read".to_string(),
                input: json!({
                    "file_path": format!("/projects/example/{}", file)
                }),
            },
            15,
        ));
        events.push((
            ProxyEvent::ToolResult {
                id: format!("{}-result", last_tool_id),
                timestamp: Utc::now(),
                tool_name: "Read".to_string(),
                output: json!(format!("// Contents of {}...", file)),
                duration: Duration::from_millis(*duration_ms),
                success: true,
            },
            30,
        ));
    }

    // Context growing with file contents: ~50K (33%)
    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-opus-4-5-20251101".to_string(),
            input_tokens: 6000,
            output_tokens: 891,
            cache_creation_tokens: 8000,
            cache_read_tokens: 44000,
        },
        800,
    ));

    // === Thinking: Analyzing code ===
    events.push((
        ProxyEvent::Thinking {
            timestamp: Utc::now(),
            content: "After reviewing the codebase, I can see the architecture:\n\n- main.rs orchestrates the application\n- events.rs defines the event types\n- parser/mod.rs handles SSE parsing\n\nThe key insight is that the SSE parser needs to accumulate deltas before emitting events. Currently it emits on content_block_start which is too early.\n\nI'll need to:\n1. Add a PartialContentBlock enum to track in-progress blocks\n2. Accumulate input_json_delta events\n3. Emit complete events on content_block_stop\n\nThis is a clean fix that follows the existing patterns.".to_string(),
            token_estimate: 142,
        },
        1000,
    ));

    // === Update todos (Haiku) ===
    last_tool_id = next_id();
    events.push((
        ProxyEvent::ToolCall {
            id: last_tool_id.clone(),
            timestamp: Utc::now(),
            tool_name: "TodoWrite".to_string(),
            input: json!({
                "todos": [
                    {"content": "Review project architecture", "status": "completed", "activeForm": "Reviewing project architecture"},
                    {"content": "Identify key files to modify", "status": "completed", "activeForm": "Identifying key files"},
                    {"content": "Implement required changes", "status": "in_progress", "activeForm": "Implementing changes"},
                    {"content": "Test and verify", "status": "pending", "activeForm": "Testing and verifying"}
                ]
            }),
        },
        15,
    ));

    events.push((
        ProxyEvent::ToolResult {
            id: format!("{}-result", last_tool_id),
            timestamp: Utc::now(),
            tool_name: "TodoWrite".to_string(),
            output: json!({"status": "success"}),
            duration: Duration::from_millis(8),
            success: true,
        },
        50,
    ));

    // Haiku - tiny context for quick task
    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-haiku-4-5-20251001".to_string(),
            input_tokens: 420,
            output_tokens: 89,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
        },
        200,
    ));

    // === Glob for test files ===
    last_tool_id = next_id();
    events.push((
        ProxyEvent::ToolCall {
            id: last_tool_id.clone(),
            timestamp: Utc::now(),
            tool_name: "Glob".to_string(),
            input: json!({
                "pattern": "**/*test*.rs"
            }),
        },
        10,
    ));

    events.push((
        ProxyEvent::ToolResult {
            id: format!("{}-result", last_tool_id),
            timestamp: Utc::now(),
            tool_name: "Glob".to_string(),
            output: json!(["src/parser/tests.rs", "tests/integration.rs"]),
            duration: Duration::from_millis(34),
            success: true,
        },
        50,
    ));

    // Context: ~65K (43%)
    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-opus-4-5-20251101".to_string(),
            input_tokens: 5000,
            output_tokens: 567,
            cache_creation_tokens: 0,
            cache_read_tokens: 60000,
        },
        400,
    ));

    // === Edit file (longer duration - user approval) ===
    last_tool_id = next_id();
    events.push((
        ProxyEvent::ToolCall {
            id: last_tool_id.clone(),
            timestamp: Utc::now(),
            tool_name: "Edit".to_string(),
            input: json!({
                "file_path": "/projects/example/src/parser/mod.rs",
                "old_string": "events.push(ProxyEvent::ToolCall {",
                "new_string": "// Accumulate input in partial_blocks\npartial_blocks.insert(index, PartialContentBlock::ToolUse {"
            }),
        },
        100,
    ));

    events.push((
        ProxyEvent::ToolResult {
            id: format!("{}-result", last_tool_id),
            timestamp: Utc::now(),
            tool_name: "Edit".to_string(),
            output: json!({"status": "success", "lines_changed": 2}),
            duration: Duration::from_millis(5847), // User reviewed the edit
            success: true,
        },
        200,
    ));

    // === Thinking: Verifying changes ===
    events.push((
        ProxyEvent::Thinking {
            timestamp: Utc::now(),
            content: "Good, I've made the core change. Now I need to verify it compiles and works correctly.\n\nThe fix follows the existing code patterns:\n- Uses the same timestamp handling\n- Matches the existing event structure\n- Properly accumulates data before emission\n\nLet me run the build to check for any compilation errors.".to_string(),
            token_estimate: 78,
        },
        500,
    ));

    // Context: ~80K (53%)
    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-opus-4-5-20251101".to_string(),
            input_tokens: 5000,
            output_tokens: 234,
            cache_creation_tokens: 0,
            cache_read_tokens: 75000,
        },
        400,
    ));

    // === Bash: Run build ===
    last_tool_id = next_id();
    events.push((
        ProxyEvent::ToolCall {
            id: last_tool_id.clone(),
            timestamp: Utc::now(),
            tool_name: "Bash".to_string(),
            input: json!({
                "command": "cargo build --release",
                "description": "Build project in release mode"
            }),
        },
        50,
    ));

    events.push((
        ProxyEvent::ToolResult {
            id: format!("{}-result", last_tool_id),
            timestamp: Utc::now(),
            tool_name: "Bash".to_string(),
            output: json!(
                "   Compiling aspy v0.1.0\n    Finished release [optimized] target(s) in 12.34s"
            ),
            duration: Duration::from_millis(3240), // cargo build
            success: true,
        },
        300,
    ));

    // Context: ~95K (63%)
    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-opus-4-5-20251101".to_string(),
            input_tokens: 5000,
            output_tokens: 156,
            cache_creation_tokens: 0,
            cache_read_tokens: 90000,
        },
        600,
    ));

    // === Grep for validation ===
    last_tool_id = next_id();
    events.push((
        ProxyEvent::ToolCall {
            id: last_tool_id.clone(),
            timestamp: Utc::now(),
            tool_name: "Grep".to_string(),
            input: json!({
                "pattern": "PartialContentBlock",
                "path": "src/"
            }),
        },
        10,
    ));

    events.push((
        ProxyEvent::ToolResult {
            id: format!("{}-result", last_tool_id),
            timestamp: Utc::now(),
            tool_name: "Grep".to_string(),
            output: json!(["src/parser/mod.rs:45", "src/parser/mod.rs:112"]),
            duration: Duration::from_millis(67),
            success: true,
        },
        100,
    ));

    // === Topic update: Implementation complete ===
    events.push((
        ProxyEvent::Response {
            request_id: "haiku-topic-2".to_string(),
            timestamp: Utc::now(),
            status: 200,
            body_size: 160,
            ttfb: Duration::from_millis(165),
            duration: Duration::from_millis(380),
            body: Some(json!({
                "model": "claude-haiku-4-5-20251001",
                "content": [{
                    "type": "text",
                    "text": "{\"isNewTopic\": false, \"title\": \"Implementation Complete\"}"
                }]
            })),
            raw_body: None,
        },
        200,
    ));

    // === Final thinking ===
    events.push((
        ProxyEvent::Thinking {
            timestamp: Utc::now(),
            content: "The build succeeded. The changes are complete:\n\n✓ SSE delta accumulation implemented\n✓ Tool inputs now captured correctly\n✓ Thinking blocks supported\n✓ All tests passing\n\nThe fix resolves the empty tool input bug by properly accumulating input_json_delta events before emitting the ToolCall event.".to_string(),
            token_estimate: 72,
        },
        600,
    ));

    // === Mark tasks complete (Haiku) ===
    last_tool_id = next_id();
    events.push((
        ProxyEvent::ToolCall {
            id: last_tool_id.clone(),
            timestamp: Utc::now(),
            tool_name: "TodoWrite".to_string(),
            input: json!({
                "todos": [
                    {"content": "Review project architecture", "status": "completed", "activeForm": "Reviewing project architecture"},
                    {"content": "Identify key files to modify", "status": "completed", "activeForm": "Identifying key files"},
                    {"content": "Implement required changes", "status": "completed", "activeForm": "Implementing changes"},
                    {"content": "Test and verify", "status": "completed", "activeForm": "Testing and verifying"}
                ]
            }),
        },
        30,
    ));

    events.push((
        ProxyEvent::ToolResult {
            id: format!("{}-result", last_tool_id),
            timestamp: Utc::now(),
            tool_name: "TodoWrite".to_string(),
            output: json!({"status": "success"}),
            duration: Duration::from_millis(6),
            success: true,
        },
        50,
    ));

    // Haiku - tiny context for final todo update
    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-haiku-4-5-20251001".to_string(),
            input_tokens: 450,
            output_tokens: 234,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
        },
        500,
    ));

    // === Bonus: Add more events to show higher context usage ===

    // Simulate continued conversation growing context
    events.push((
        ProxyEvent::Thinking {
            timestamp: Utc::now(),
            content: "The user is asking follow-up questions. The context is growing as we discuss more implementation details. This is typical of a longer Claude Code session where context accumulates with each interaction.".to_string(),
            token_estimate: 45,
        },
        800,
    ));

    // Context: ~110K (73%)
    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-opus-4-5-20251101".to_string(),
            input_tokens: 8000,
            output_tokens: 1823,
            cache_creation_tokens: 0,
            cache_read_tokens: 102000,
        },
        1000,
    ));

    // More reads to show continued activity
    last_tool_id = next_id();
    events.push((
        ProxyEvent::ToolCall {
            id: last_tool_id.clone(),
            timestamp: Utc::now(),
            tool_name: "Read".to_string(),
            input: json!({
                "file_path": "/projects/example/README.md"
            }),
        },
        20,
    ));

    events.push((
        ProxyEvent::ToolResult {
            id: format!("{}-result", last_tool_id),
            timestamp: Utc::now(),
            tool_name: "Read".to_string(),
            output: json!("# aspy\n\nA TUI observability proxy..."),
            duration: Duration::from_millis(38),
            success: true,
        },
        100,
    ));

    // Context: ~120K (80%) - session nearing compact threshold
    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-opus-4-5-20251101".to_string(),
            input_tokens: 8000,
            output_tokens: 456,
            cache_creation_tokens: 0,
            cache_read_tokens: 112000,
        },
        2000,
    ));

    events
}
