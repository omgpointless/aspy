// Demo mode: Generate realistic mock events to showcase the TUI
//
// This module generates events that look like a real Claude Code session,
// showing tool calls with realistic inputs, thinking blocks, and usage information.
//
// Run with: ANTHROPIC_SPY_DEMO=1 cargo run --release

use crate::events::ProxyEvent;
use chrono::Utc;
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::sleep;

/// Generate a sequence of demo events simulating a Claude Code session
pub async fn run_demo(tx: mpsc::Sender<ProxyEvent>, mut shutdown_rx: oneshot::Receiver<()>) {
    // Initial delay to let TUI render
    sleep(Duration::from_millis(1500)).await;

    // Simulate a realistic Claude Code interaction sequence
    let events = generate_demo_sequence();

    for (event, delay_ms) in events {
        // Check for shutdown signal before sending
        if shutdown_rx.try_recv().is_ok() {
            return;
        }
        if tx.send(event).await.is_err() {
            break;
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

fn generate_demo_sequence() -> Vec<(ProxyEvent, u64)> {
    let mut events = Vec::new();
    let mut id_counter = 0u64;

    // Helper to generate realistic tool IDs (matches real format)
    let mut next_id = || {
        id_counter += 1;
        format!("toolu_01{:020x}", id_counter)
    };

    // === Thinking: Initial analysis ===
    events.push((
        ProxyEvent::Thinking {
            timestamp: Utc::now(),
            content: "The user wants me to help with a code review task. Let me first understand the codebase structure by reading the relevant configuration files.\n\nI should start by checking if there's a README or project configuration that explains the architecture.".to_string(),
            token_estimate: 52,
        },
        800,
    ));

    // === Initial response - model warming up ===
    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            input_tokens: 3774,
            output_tokens: 285,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
        },
        400,
    ));

    // === First tool call: Read START_HERE.md ===
    events.push((
        ProxyEvent::ToolCall {
            id: next_id(),
            timestamp: Utc::now(),
            tool_name: "Read".to_string(),
            input: json!({
                "file_path": "/projects/example/.claude/START_HERE.md"
            }),
        },
        50,
    ));

    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            input_tokens: 10,
            output_tokens: 174,
            cache_creation_tokens: 9218,
            cache_read_tokens: 15082,
        },
        1200,
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

    // === TodoWrite: Planning the work ===
    events.push((
        ProxyEvent::ToolCall {
            id: next_id(),
            timestamp: Utc::now(),
            tool_name: "TodoWrite".to_string(),
            input: json!({
                "todos": [
                    {"content": "Review project architecture", "status": "in_progress"},
                    {"content": "Identify key files to modify", "status": "pending"},
                    {"content": "Implement required changes", "status": "pending"},
                    {"content": "Test and verify", "status": "pending"}
                ]
            }),
        },
        80,
    ));

    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            input_tokens: 156,
            output_tokens: 423,
            cache_creation_tokens: 0,
            cache_read_tokens: 24300,
        },
        500,
    ));

    // === Burst of parallel Reads ===
    let files_to_read = vec![
        "src/main.rs",
        "src/lib.rs",
        "src/config.rs",
        "Cargo.toml",
        "src/events.rs",
        "src/parser/mod.rs",
    ];

    for file in &files_to_read {
        events.push((
            ProxyEvent::ToolCall {
                id: next_id(),
                timestamp: Utc::now(),
                tool_name: "Read".to_string(),
                input: json!({
                    "file_path": format!("/projects/example/{}", file)
                }),
            },
            8,
        ));
    }

    // === Thinking: Analyzing code ===
    events.push((
        ProxyEvent::Thinking {
            timestamp: Utc::now(),
            content: "After reviewing the codebase, I can see the architecture:\n\n- main.rs orchestrates the application\n- events.rs defines the event types\n- parser/mod.rs handles SSE parsing\n\nThe key insight is that the SSE parser needs to accumulate deltas before emitting events. Currently it emits on content_block_start which is too early.\n\nI'll need to:\n1. Add a PartialContentBlock enum to track in-progress blocks\n2. Accumulate input_json_delta events\n3. Emit complete events on content_block_stop\n\nThis is a clean fix that follows the existing patterns.".to_string(),
            token_estimate: 142,
        },
        1000,
    ));

    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            input_tokens: 2847,
            output_tokens: 891,
            cache_creation_tokens: 4181,
            cache_read_tokens: 24300,
        },
        800,
    ));

    // === Update todos ===
    events.push((
        ProxyEvent::ToolCall {
            id: next_id(),
            timestamp: Utc::now(),
            tool_name: "TodoWrite".to_string(),
            input: json!({
                "todos": [
                    {"content": "Review project architecture", "status": "completed"},
                    {"content": "Identify key files to modify", "status": "completed"},
                    {"content": "Implement required changes", "status": "in_progress"},
                    {"content": "Test and verify", "status": "pending"}
                ]
            }),
        },
        15,
    ));

    // === Glob for test files ===
    events.push((
        ProxyEvent::ToolCall {
            id: next_id(),
            timestamp: Utc::now(),
            tool_name: "Glob".to_string(),
            input: json!({
                "pattern": "**/*test*.rs"
            }),
        },
        5,
    ));

    events.push((
        ProxyEvent::ToolCall {
            id: next_id(),
            timestamp: Utc::now(),
            tool_name: "Glob".to_string(),
            input: json!({
                "pattern": "src/**/*.rs"
            }),
        },
        5,
    ));

    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            input_tokens: 1823,
            output_tokens: 567,
            cache_creation_tokens: 0,
            cache_read_tokens: 28481,
        },
        600,
    ));

    // === Edit file ===
    events.push((
        ProxyEvent::ToolCall {
            id: next_id(),
            timestamp: Utc::now(),
            tool_name: "Edit".to_string(),
            input: json!({
                "file_path": "/projects/example/src/parser/mod.rs",
                "old_string": "events.push(ProxyEvent::ToolCall {\n    id,\n    timestamp,\n    tool_name: name,\n    input: serde_json::Value::Object(serde_json::Map::new()),\n});",
                "new_string": "// Accumulate input in partial_blocks instead of emitting immediately\npartial_blocks.insert(index, PartialContentBlock::ToolUse {\n    id,\n    name,\n    input_json: String::new(),\n    timestamp,\n});"
            }),
        },
        100,
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

    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            input_tokens: 3421,
            output_tokens: 234,
            cache_creation_tokens: 0,
            cache_read_tokens: 28481,
        },
        400,
    ));

    // === Bash: Run build ===
    events.push((
        ProxyEvent::ToolCall {
            id: next_id(),
            timestamp: Utc::now(),
            tool_name: "Bash".to_string(),
            input: json!({
                "command": "cargo build --release"
            }),
        },
        50,
    ));

    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            input_tokens: 987,
            output_tokens: 156,
            cache_creation_tokens: 0,
            cache_read_tokens: 28481,
        },
        800,
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

    // === Mark tasks complete ===
    events.push((
        ProxyEvent::ToolCall {
            id: next_id(),
            timestamp: Utc::now(),
            tool_name: "TodoWrite".to_string(),
            input: json!({
                "todos": [
                    {"content": "Review project architecture", "status": "completed"},
                    {"content": "Identify key files to modify", "status": "completed"},
                    {"content": "Implement required changes", "status": "completed"},
                    {"content": "Test and verify", "status": "completed"}
                ]
            }),
        },
        30,
    ));

    events.push((
        ProxyEvent::ApiUsage {
            timestamp: Utc::now(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            input_tokens: 456,
            output_tokens: 1823,
            cache_creation_tokens: 0,
            cache_read_tokens: 28481,
        },
        2000,
    ));

    events
}
