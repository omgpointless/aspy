# Anthropic Spy - Project Status

**Current State:** v0.1.0-alpha Ready
**Last Updated:** 2025-11-26

## Project Vision

A professional observability proxy for Claude Code that captures and visualizes all API interactions including thinking blocks, tool calls, and token usage in real-time.

## ✅ Completed Phases

### Phase 1: Visual Bug Fix (COMPLETE)
**Problem Solved:** Logs breaking through TUI and garbling the status bar

**Implementation:**
- Created custom `TuiLogLayer` that captures logs to in-memory buffer
- Conditional logging: TUI mode uses buffer, headless uses stdout
- Added system logs panel at bottom of TUI (20% height)
- 4-section layout: Title → Main (60%) → Logs (20%) → Status

**Files Modified:**
- `src/logging/mod.rs` - New custom tracing layer
- `src/main.rs:30-60` - Conditional logging setup
- `src/tui/app.rs` - Added log_buffer field
- `src/tui/ui.rs` - Added render_logs_panel()

**Result:** Clean, professional TUI with no visual glitches

### Phase 2: Enhanced Data Capture (COMPLETE)
**Goal:** Capture ALL data from Anthropic API - nothing should be missed

**What We Now Capture:**

1. **Extended API Request Fields** (`src/parser/models.rs:12-43`)
   - system: System prompt
   - temperature, top_p, top_k: Sampling parameters
   - stop_sequences: Custom stops
   - stream: Streaming flag (CRITICAL!)
   - metadata: User metadata

2. **Extended API Response Fields** (`src/parser/models.rs:45-57`)
   - stop_sequence: Which stop was hit

3. **Token Usage with Caching** (`src/parser/models.rs:110-121`)
   - cache_creation_input_tokens
   - cache_read_input_tokens
   - **This shows prompt caching savings!**

4. **Complete Header Capture** (`src/parser/models.rs:166-217`)
   - Request: anthropic-version, anthropic-beta, API key hash (SHA-256)
   - Response: request-id, organization-id
   - Rate limits: requests/tokens remaining/limit/reset (when available)

5. **New Event Types** (`src/events.rs:59-74`)
   - HeadersCaptured: Shows headers and beta features
   - RateLimitUpdate: Real-time quota monitoring

**Files Modified:**
- `src/parser/models.rs` - Expanded all data models
- `src/proxy/mod.rs:225-292` - Header extraction functions
- `src/proxy/mod.rs:186-227` - Header capture and emission
- `src/events.rs` - New event types
- `src/tui/ui.rs` - Display new events
- `Cargo.toml` - Added sha2 for key hashing

**Result:** Comprehensive API observability - we see everything!

**User Testing Findings:**
- Works smoothly, professional feel
- Successfully captured Claude Code using beta features:
  - interleaved-thinking
  - context-management
  - tool-examples
- API version visible
- Rate limit headers not present (likely OAuth/subscription mode)
- Token usage available in API response `usage` field

### Phase 3: Token Tracking & Cost Estimation (COMPLETE)
**Goal:** Visualize token usage and cost in status bar

**Implementation:**
- Track cumulative tokens from API response `usage` fields
- Calculate cost based on official Anthropic pricing (`src/pricing.rs`)
- Display in status bar with input/output/cache breakdown
- Support multiple models with different pricing tiers

### Phase 4: SSE Streaming & Thinking Blocks (COMPLETE)
**Goal:** Proper handling of Server-Sent Events and thinking block capture

**Implementation:**
- SSE delta accumulation pattern: accumulate `content_block_delta` events before emitting
- Thinking block capture from `thinking_delta` events
- Tool input capture from `input_json_delta` events (fixes empty `{}` bug)
- Dedicated thinking panel (35% width) when thinking content exists

**Key Code:** `src/parser/mod.rs` - `PartialContentBlock` enum and `parse_sse_response`

### Phase 5: Demo Mode (COMPLETE)
**Goal:** Showcase TUI without live Claude Code session

**Implementation:**
- `ANTHROPIC_SPY_DEMO=1` environment variable
- Generates realistic mock events: thinking, tool calls, API usage
- Graceful shutdown support
- Used for creating demo GIFs

## 📋 Future Phases

### Phase 6: Enhanced TUI Dashboard
- Filtering by tool/status/time
- Search functionality
- Real-time graphs
- Export capabilities

## 🏗️ Architecture Overview

**Component Structure:**
```
main.rs
  ├─> config (environment variables)
  ├─> logging (custom tracing layer)
  ├─> proxy (HTTP interceptor)
  │     ├─> parser (extract tool calls/results)
  │     └─> emit events
  ├─> storage (write events to .jsonl)
  └─> tui (ratatui UI)
        ├─> app (state management)
        └─> ui (rendering)

Event Flow:
HTTP Request → Proxy → Parser → Events → [TUI, Storage]
```

**Key Design Patterns:**
- Event-driven architecture (mpsc channels)
- Custom tracing layer for log capture
- State-based input handling (no debouncing!)
- Ring buffers for bounded memory
- Graceful shutdown with oneshot channels

## 🧪 Testing Methodology

**User's Test Case:**
1. `cargo build --release`
2. `cargo run --release`
3. New terminal: run Claude Code (configured to localhost:8080)
4. Inspect bootup requests
5. Use `/usage` command in Claude Code to verify proxy working
6. Send simple message: "hi"
7. Inspect complete event chain

**What to Verify:**
- No visual glitches (logs stay in logs panel)
- All events appear in order
- Headers captured (beta features visible)
- Detail view scrollable
- Token usage tracked
- Smooth navigation (arrow keys work properly)

## 📝 Key Learnings

1. **State-based input > debouncing**: Track key press/release state instead of timing
2. **Ring buffers prevent memory growth**: VecDeque with max size
3. **Custom tracing layers**: Intercept logs before stdout for TUI compatibility
4. **Type conversion hell**: Different crate versions = different types (http v0.2 vs v1.0)
5. **Headers are optional**: Don't rely on rate limit headers - use API response data
6. **Security**: Never log API keys - only SHA-256 hash prefix
7. **Anthropic beta features**: Visible in headers, explains Claude Code intelligence

## 🎯 Success Metrics

**v0.1.0-alpha Validation (2025-11-26):**
- ✅ Tool call inputs captured correctly (SSE delta accumulation)
- ✅ Thinking blocks captured and displayed in dedicated panel
- ✅ Token/cost tracking in status bar
- ✅ Demo mode generates realistic events
- ✅ Graceful shutdown in all modes
- ✅ Real-world testing with Claude Code traffic

**Meta Moment:** During testing, the observed Claude instance realized it was looking at demo.rs (which generates mock spy events) while being observed by the spy itself. Captured in logs.

## 🔧 Development Commands

```bash
# Build
cargo build --release

# Run
cargo run --release

# Run with debug logs (headless mode)
ANTHROPIC_SPY_NO_TUI=1 RUST_LOG=debug cargo run

# Test compilation
cargo check

# Format code
cargo fmt

# Lint
cargo clippy
```

## 🌐 Configuration

**Environment Variables:**
- `ANTHROPIC_SPY_BIND`: Proxy bind address (default: 127.0.0.1:8080)
- `ANTHROPIC_API_URL`: Target API URL (default: https://api.anthropic.com)
- `ANTHROPIC_SPY_LOG_DIR`: Log directory (default: ./logs)
- `ANTHROPIC_SPY_NO_TUI`: Disable TUI (default: false)

**Claude Code Configuration:**
```bash
export ANTHROPIC_BASE_URL=http://127.0.0.1:8080
claude-code
```

## 📚 Important Files

**Core:**
- `src/main.rs` - Entry point, orchestration
- `src/config.rs` - Configuration from env vars
- `src/events.rs` - Event types and statistics
- `src/proxy/mod.rs` - HTTP proxy implementation
- `src/parser/mod.rs` - API payload parsing
- `src/parser/models.rs` - Anthropic API data structures

**TUI:**
- `src/tui/mod.rs` - Event loop and input handling
- `src/tui/app.rs` - Application state
- `src/tui/ui.rs` - Rendering logic

**Observability:**
- `src/logging/mod.rs` - Custom tracing layer
- `src/storage/mod.rs` - JSON Lines file writing

**Config:**
- `Cargo.toml` - Dependencies
- `.claude/` - Context preservation
