# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**anthropic-spy** is a Rust TUI (Terminal User Interface) application that acts as an observability proxy between Claude Code and the Anthropic API. It intercepts HTTP traffic, parses tool calls and responses, and displays them in real-time while logging to JSON Lines files.

**Key Purpose:** Understand Claude Code's behavior by visualizing all API interactions.

**Learning Context:** This is the maintainer's first serious Rust project, coming from a .NET/TypeScript background. Code is primarily AI-generated but must be understood and explainable.

## Build & Development Commands

### Building
```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Check compilation without building
cargo check
```

### Running
```bash
# Run in development mode
cargo run

# Run in release mode (recommended for actual use)
cargo run --release

# Run in headless mode (no TUI, logs to stdout)
ANTHROPIC_SPY_NO_TUI=1 cargo run --release

# Run in demo mode (generate mock events for showcasing)
ANTHROPIC_SPY_DEMO=1 cargo run --release

# Run with debug logging
RUST_LOG=debug cargo run
```

### Demo Mode

Demo mode generates realistic mock events to showcase the TUI without needing a live Claude Code session. This is useful for:
- Recording GIFs/videos for documentation
- Testing UI changes without API costs
- Demonstrating features to others

The demo simulates a typical Claude Code session including:
- Thinking blocks showing Claude's reasoning
- Tool calls (Read, Edit, Bash, Glob, TodoWrite) with realistic inputs
- API usage events with token counts
- Progressive task completion

```bash
# Windows PowerShell
$env:ANTHROPIC_SPY_DEMO="1"; cargo run --release

# macOS/Linux
ANTHROPIC_SPY_DEMO=1 cargo run --release
```

### Code Quality
```bash
# Format code
cargo fmt

# Lint with clippy
cargo clippy

# Run with all clippy warnings
cargo clippy -- -W clippy::all
```

### Testing with Claude Code
After starting the proxy, configure Claude Code in a separate terminal:

```powershell
# Windows PowerShell
$env:ANTHROPIC_BASE_URL="http://127.0.0.1:8080"
claude-code
```

```bash
# macOS/Linux
export ANTHROPIC_BASE_URL=http://127.0.0.1:8080
claude-code
```

## Architecture Overview

### Component Flow

```
Claude Code (HTTP Client)
    ↓ HTTP Requests
    ↓ ↑ (SSE streams passed through immediately)
Proxy Server (axum)
    ↓ Tee stream: forward to client + accumulate for parsing
Parser (extract tool calls/results after stream completes)
    ↓ Emit events
Event Channels (mpsc)
    ↓ Broadcast
┌─────────┬──────────┐
TUI       Storage    (Future consumers)
```

**Streaming Architecture:** For SSE responses, the proxy immediately streams chunks
to Claude Code while accumulating a copy in a background task. This preserves
low-latency token delivery. Parsing and event emission happen after the stream
completes, ensuring Claude Code receives tokens without waiting for parsing.

### Core Components

**1. main.rs - Orchestration**
- Sets up conditional logging (TUI mode vs headless)
- Creates mpsc channels for event distribution
- Spawns background tasks: proxy, storage
- Runs TUI in main task (or waits for Ctrl+C in headless mode)
- Handles graceful shutdown via oneshot channel

**2. proxy/ - HTTP Interception**
- `mod.rs`: Axum server that forwards all requests to Anthropic API
- `proxy_handler()`: Routes to streaming or buffered handler based on content-type
- `handle_streaming_response()`: Tees SSE stream to client + accumulator, parses after completion
- `handle_buffered_response()`: Buffers small JSON responses for parsing
- Uses `reqwest` with `bytes_stream()` for SSE streaming
- 50MB request body limit prevents DoS
- Headers are extracted and API keys are hashed (SHA-256) for security

**3. parser/ - Protocol Extraction**
- `Parser` struct maintains state to correlate tool calls with results
- `parse_request()`: Extracts tool_result blocks from requests
- `parse_response()`: Extracts tool_use blocks from responses
- Uses `Arc<Mutex<HashMap>>` to track pending calls across async tasks
- Calculates duration by matching tool_use_id with timestamps

**4. events.rs - Event System**
- `ProxyEvent` enum: Tagged union of all event types
- Types: ToolCall, ToolResult, Request, Response, Error, HeadersCaptured, RateLimitUpdate, ApiUsage, Thinking
- `Stats` struct: Accumulates metrics (requests, tokens, thinking blocks, success rate, cost)
- All events are `Clone + Serialize` for broadcasting and logging

**5. tui/ - Terminal Interface**
- `mod.rs`: Event loop using tokio::select! (keyboard + events + periodic redraw)
- `app.rs`: Application state (events list, selection, stats, scroll state)
- `ui.rs`: Rendering with ratatui (4-section layout: title, main, logs, status)
- `input.rs`: State-based keyboard handling (tracks pressed keys, no debouncing)

**6. storage/ - Persistence**
- Writes events to JSON Lines format (`./logs/anthropic-spy-YYYY-MM-DD.jsonl`)
- Daily log rotation
- Runs in background task, receives events via mpsc channel

**7. logging/ - Custom Tracing**
- `TuiLogLayer`: Custom tracing::Layer implementation
- Intercepts log events before they reach stdout
- Stores in ring buffer (max 1000 entries) shared via `Arc<Mutex<>>`
- Prevents logs from breaking through TUI alternate screen buffer

### Key Rust Patterns Used

**Async/Await with Tokio:**
- `#[tokio::main]` for async runtime
- `tokio::spawn()` for concurrent background tasks
- `tokio::select!` for multiplexing events in TUI loop
- `tokio::sync::mpsc` for event channels
- `tokio::sync::oneshot` for shutdown signal

**Shared State:**
- `Arc<Mutex<>>` for shared mutable state across tasks (parser's pending_calls, log buffer)
- `Arc::clone()` to create cheap references (just increments refcount)
- `.lock().await` for async mutex access

**Error Handling:**
- `anyhow::Result` for application-level errors
- `thiserror` for custom error types (if added)
- `.context()` for adding error context
- `?` operator for error propagation

**Pattern Matching:**
- Exhaustive `match` on `ProxyEvent` enum
- `if let` for optional extraction

**Traits:**
- Custom `Layer<S>` implementation for TuiLogLayer
- Serde's `Serialize`/`Deserialize` traits

## Configuration

Environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `ANTHROPIC_SPY_BIND` | `127.0.0.1:8080` | Proxy bind address |
| `ANTHROPIC_API_URL` | `https://api.anthropic.com` | Target API URL |
| `ANTHROPIC_SPY_LOG_DIR` | `./logs` | Log file directory |
| `ANTHROPIC_SPY_NO_TUI` | `false` | Disable TUI (headless mode) |
| `ANTHROPIC_SPY_DEMO` | `false` | Enable demo mode (mock events) |
| `RUST_LOG` | `anthropic_spy=info` | Logging level |

## Event Correlation System

**Critical Architecture Detail:**

The parser maintains state to correlate tool calls with their results:

1. **Tool Call (from API response):**
   - Extract `tool_use` block with `id`, `name`, `input`
   - Store in `pending_calls: HashMap<id, (name, timestamp)>`
   - Emit `ProxyEvent::ToolCall`

2. **Tool Result (from API request):**
   - Extract `tool_result` block with `tool_use_id`, `content`
   - Look up in `pending_calls` by `tool_use_id`
   - Calculate duration: `now - stored_timestamp`
   - Remove from `pending_calls`
   - Emit `ProxyEvent::ToolResult`

This correlation allows measuring how long each tool call takes to execute.

## TUI Input Handling

**State-Based Approach (No Debouncing):**

Instead of debouncing key events, the TUI tracks which keys are currently pressed:

```rust
// InputHandler maintains: pressed_keys: HashSet<KeyCode>

fn handle_key_press(&mut self, key: KeyCode) -> bool {
    if self.pressed_keys.contains(&key) {
        return false;  // Already pressed, ignore
    }
    self.pressed_keys.insert(key);
    true  // New press, trigger action
}

fn handle_key_release(&mut self, key: KeyCode) {
    self.pressed_keys.remove(&key);
}
```

This prevents double-triggering regardless of poll rate.

## Logging Architecture

**Two Modes:**

1. **TUI Mode:** Custom `TuiLogLayer` captures logs to `LogBuffer` (ring buffer)
   - Prevents stdout logs from garbling the TUI
   - Logs displayed in dedicated panel (bottom 20% of screen)

2. **Headless Mode:** Standard `fmt::layer` outputs to stdout
   - Normal terminal logging behavior

**Implementation:**
- `TuiLogLayer` implements `tracing_subscriber::Layer<S>`
- `.on_event()` intercepts log events and stores in buffer
- Buffer is `Arc<Mutex<VecDeque<LogEntry>>>` for thread-safe access
- Max 1000 entries (oldest dropped when full)

## Code Style Guidelines

**Error Handling:**
- Prefer `Result<T, E>` with `?` operator
- Use `.context()` to add error context
- Avoid `.unwrap()` except where proven safe (rare)

**Async Code:**
- Use `async fn` for I/O operations
- `.await` immediately, don't store futures
- Spawn tasks with `tokio::spawn()` for concurrency

**Comments:**
- Explain WHY, not WHAT (code should be self-documenting)
- Document Rust patterns (Arc, Mutex, async boundaries)
- Include context that can't be inferred from code

**Dependencies:**
- Each dependency must be purposeful and well-maintained
- Document why each crate is used (see Cargo.toml comments)

## Current Development Phase

**v0.1.0-alpha: Core Observability Complete**

**Completed:**
- Phase 3: Token tracking with cost estimation in status bar
- Phase 4: SSE streaming support with delta accumulation
- Thinking block capture and dedicated panel display

**Implementation Highlights:**
- SSE parser accumulates `content_block_delta` events before emitting complete events
- Thinking blocks displayed in dedicated right panel (35% width when active)
- Tool call inputs properly captured from `input_json_delta` events
- Demo mode for showcasing TUI without live API

**Next Phases:**
- Phase 5: Enhanced dashboard layout
- Future: Filtering, search, analysis tools

## Important Notes

**Security:**
- API keys are never logged in full - only SHA-256 hash prefix (16 chars)
- See `proxy/mod.rs::extract_request_headers()`

**Performance:**
- Proxy overhead is minimal (~1-2ms per request)
- TUI renders at 10 FPS (100ms intervals)
- Event channels have buffer of 1000 (backpressure if full)

**Compatibility:**
- Tested with Claude Code on Windows (PowerShell)
- Should work on macOS/Linux (standard ANTHROPIC_BASE_URL)

## Commit Conventions

This project uses [Conventional Commits](https://www.conventionalcommits.org/).

Format: `<type>(<scope>): <description>`

Types: `feat`, `fix`, `refactor`, `chore`, `docs`, `test`, `perf`
Scopes: `proxy`, `tui`, `parser`, `storage`, `events`, `deps`

Examples:
- `feat(tui): add mouse scroll support for event list`
- `fix(proxy): implement SSE stream-through`
- `chore(deps): remove unused dependencies`

## References

- **Rust Concepts:** See `.claude/DEVELOPMENT_PHILOSOPHY.md`
- **Architecture Deep Dive:** See `.claude/ARCHITECTURE.md`
- **Project Status:** See `.claude/PROJECT_STATUS.md`
- **Messaging Guidelines:** See `.claude/TONE_GUIDE.md`