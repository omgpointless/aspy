# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**anthropic-spy** is a Rust TUI (Terminal User Interface) application that acts as an observability proxy between Claude Code and the Anthropic API. It intercepts HTTP traffic, parses tool calls and responses, and displays them in real-time while logging to JSON Lines files.

**Key Purpose:** Understand Claude Code's behavior by visualizing all API interactions.

**Learning Context:** This is the maintainer's first serious Rust project, coming from a .NET/TypeScript background. Code is primarily AI-generated but must be understood and explainable.

---

## Architecture Philosophy

**Core Principle:** Composition over inheritance using Rust's trait system. New features MUST compose existing behaviors rather than modify core state containers.

**Layered Architecture:** Kernel (core system) → Userland (optional features) → User Space (custom extensions)

**Key Design Principles:**
1. **"Ah, Here It Is" Discoverability** - One concept, one place (e.g., scrolling code lives in `tui/traits/scrollable.rs`)
2. **Traits as Capabilities** - Components gain behavior by implementing traits, not inheritance
3. **Data vs Rendering** - Views produce layout trees, rendering is a separate pass
4. **Configuration Over Code Changes** - Userland features MUST be config-toggleable

**See [docs/architecture.md](docs/architecture.md) for detailed patterns, anti-patterns, and examples.**

---

## ⚠️ REQUIRED READING: Architecture Documentation

**CRITICAL:** Before adding features, refactoring, or modifying any code that touches component structure, traits, or app state, you **MUST** read [docs/architecture.md](docs/architecture.md) in full.

### Decision Tree: When to Read Sub-Documentation

```
┌─ Task involves...
│
├─ Adding a feature, refactoring, or modifying component/trait structure?
│  ├─ YES → READ docs/architecture.md (REQUIRED)
│  │        - Review patterns to follow (newtypes, traits, composition)
│  │        - Check anti-patterns to avoid (app.rs bloat, copy-paste)
│  │        - Understand kernel/userland/user space layers
│  └─ NO  → Continue
│
├─ Working on build setup, running commands, or dev environment?
│  ├─ YES → READ docs/commands.md
│  │        - Build commands, demo mode, code quality tools
│  └─ NO  → Continue
│
├─ Working on session tracking, API endpoints, or multi-client routing?
│  ├─ YES → READ docs/sessions.md
│  │        - Client/provider configuration, routing, MCP integration
│  └─ NO  → Continue
│
└─ Debugging, analyzing logs, or understanding event structure?
   ├─ YES → READ docs/log-analysis.md
   │        - jq queries, context recovery, session profiling
   └─ NO  → You're good to proceed
```

**Rule of thumb:** If unsure whether to read a doc, read it. Architectural violations are expensive to fix later.

---

## Quick Start

### Building & Running
```bash
# Development build
cargo build

# Run in release mode (recommended)
cargo run --release

# Run in demo mode (mock events for showcasing)
ANTHROPIC_SPY_DEMO=1 cargo run --release
```

### Testing with Claude Code
```bash
# Point Claude Code at the proxy with your client ID
export ANTHROPIC_BASE_URL=http://127.0.0.1:8080/dev-1
claude
```

**See [docs/commands.md](docs/commands.md) for complete build, run, and development commands.**

---

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

**Streaming Architecture:** For SSE responses, the proxy immediately streams chunks to Claude Code while accumulating a copy in a background task. This preserves low-latency token delivery. Parsing and event emission happen after the stream completes.

### Core Components

**1. main.rs** - Orchestration: sets up logging, creates channels, spawns tasks (proxy, storage), runs TUI

**2. proxy/** - HTTP Interception: Axum server forwarding requests to Anthropic API, handles streaming/buffered responses

**3. parser/** - Protocol Extraction: correlates tool calls with results, tracks pending calls, calculates durations

**4. events.rs** - Event System: `ProxyEvent` enum (ToolCall, ToolResult, ApiUsage, Thinking, etc.), `Stats` accumulation

**5. tui/** - Terminal Interface: event loop (tokio::select!), app state, rendering with ratatui

**6. storage/** - Persistence: writes events to JSON Lines format, session-based log rotation

**7. logging/** - Custom Tracing: `TuiLogLayer` captures logs to ring buffer, prevents stdout garbling

### Key Rust Patterns

**Async/Await with Tokio:**
- `#[tokio::main]` for async runtime
- `tokio::spawn()` for concurrent tasks
- `tokio::select!` for multiplexing events

**Shared State:**
- `Arc<Mutex<>>` for shared mutable state across tasks
- `Arc::clone()` for cheap references (refcount increment)

**Error Handling:**
- `anyhow::Result` for application-level errors
- `.context()` for error context
- `?` operator for propagation

---

## Key Systems

### Multi-Client Routing

anthropic-spy supports tracking multiple Claude Code instances through a single proxy using **named client routing**. Clients are configured in `~/.config/anthropic-spy/config.toml` and connect via URL paths like `http://localhost:8080/<client-id>`.

**Quick Setup:**
```toml
# ~/.config/anthropic-spy/config.toml
[clients.dev-1]
name = "Dev Laptop"
provider = "anthropic"

[providers.anthropic]
base_url = "https://api.anthropic.com"
```

Then connect: `ANTHROPIC_BASE_URL=http://127.0.0.1:8080/dev-1 claude`

**Key Benefits:**
- Same API key can track multiple separate sessions
- Route different clients to different providers (Anthropic, Foundry, Bedrock, etc.)
- Explicit naming instead of cryptic hashes

**API Endpoints:**
- `GET /api/stats` - Session statistics (supports `?client=<id>`)
- `GET /api/events` - Event buffer (supports `?client=<id>`)
- `GET /api/sessions` - All sessions
- `GET /api/clients` - Configured clients

**See [docs/sessions.md](docs/sessions.md) for detailed configuration, use cases, and MCP integration.**

### Event Correlation System

The parser maintains state to correlate tool calls with their results:

1. **Tool Call (from API response):** Extract `tool_use` block with `id`, store in `pending_calls` HashMap, emit event
2. **Tool Result (from API request):** Extract `tool_result` with `tool_use_id`, look up in `pending_calls`, calculate duration, emit event

This correlation allows measuring how long each tool call takes to execute.

### TUI Input Handling

State-based approach (no debouncing): tracks which keys are currently pressed via `HashSet<KeyCode>`. New presses trigger actions, repeated events while held are ignored.

### Logging Architecture

**Two Modes:**
1. **TUI Mode:** Custom `TuiLogLayer` captures logs to ring buffer (prevents stdout garbling)
2. **Headless Mode:** Standard `fmt::layer` outputs to stdout

---

## Session Log Analysis

Session logs are stored in JSON Lines format (`./logs/anthropic-spy-YYYYMMDD-HHMMSS-XXXX.jsonl`). Use `jq` for analysis.

**Quick queries:**
```bash
# Event type distribution
jq -s 'group_by(.type) | map({type: .[0].type, count: length})' logs/<session>.jsonl

# Cache efficiency
jq -s '[.[] | select(.type == "ApiUsage")] | {cache_ratio_pct: ((.total_cached / (.total_input + .total_cached)) * 100 | floor)}' logs/<session>.jsonl

# Tool call distribution
jq -s '[.[] | select(.type == "ToolCall") | .tool_name] | group_by(.) | map({tool: .[0], calls: length})' logs/<session>.jsonl
```

**See [docs/log-analysis.md](docs/log-analysis.md) for comprehensive queries, context recovery, and session profiling.**

---

## Development Guidelines

**Code Style:**
- Prefer `Result<T, E>` with `?` operator for error handling
- Use `async fn` for I/O operations, `.await` immediately
- Explain WHY in comments, not WHAT (code should be self-documenting)
- Each dependency must be purposeful and well-maintained

**Commit Conventions:**
Format: `<type>(<scope>): <description>`

Types: `feat`, `fix`, `refactor`, `chore`, `docs`, `test`, `perf`
Scopes: `proxy`, `tui`, `parser`, `storage`, `events`, `deps`

Examples:
- `feat(tui): add mouse scroll support for event list`
- `fix(proxy): implement SSE stream-through`

---

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

---

## Current Development Phase

**v0.1.0: Core Observability Complete**

**Completed Features:**
- Views system with Events, Stats, and Settings views
- Statistics view with 5-tab dashboard (Overview, Models, Tokens, Tools, Trends)
- Theme system with 32 bundled themes and TOML custom theme support
- CLI configuration tool (`anthropic-spy config --init/--show/--edit/--update/--reset`)
- REST API endpoints for programmatic access
- Multi-client routing with provider configuration
- Context warning augmentation

**See [ROADMAP.md](ROADMAP.md) and [CHANGELOG.md](CHANGELOG.md) for full details.**

---

## Documentation Index

**REQUIRED reading for architectural work:**
- **[docs/architecture.md](docs/architecture.md)** - Architecture patterns, design principles, anti-patterns, Rust idioms

**Reference documentation (read as needed):**
- **[docs/commands.md](docs/commands.md)** - Build, run, code quality commands
- **[docs/sessions.md](docs/sessions.md)** - Multi-client routing, provider configuration
- **[docs/api-reference.md](docs/api-reference.md)** - REST API endpoints and responses
- **[docs/themes.md](docs/themes.md)** - Theme system and custom themes
- **[docs/cli-reference.md](docs/cli-reference.md)** - CLI configuration commands
- **[docs/views.md](docs/views.md)** - TUI views and navigation
- **[docs/log-analysis.md](docs/log-analysis.md)** - Session log queries, context recovery

**Context preservation (not primary docs):**
- `.claude/PROJECT_STATUS.md` - Development phase notes
- `.claude/EXTENSIONS_VISION.md` - Future extensibility roadmap
- `.claude/impl-plans/` - RFC-style implementation plans

---

**Modular Development Note:** We aim to develop in a modular fashion, allowing flexibility and isolation of responsibility. We recognize that `app.rs` historically has "smelt" and we tend to regress. Long term, this is not maintainable for what this project has become.
