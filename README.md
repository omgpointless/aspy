# Anthropic Spy 🔍

[![CI](https://github.com/omgpointless/anthropic-spy/actions/workflows/ci.yml/badge.svg)](https://github.com/omgpointless/anthropic-spy/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/omgpointless/anthropic-spy?include_prereleases)](https://github.com/omgpointless/anthropic-spy/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)

A Rust-based observability proxy for Claude Code that intercepts and logs all tool calls with a rich terminal UI.

![Demo](demo.gif)

## What It Does

Anthropic Spy sits between Claude Code and the Anthropic API as an HTTP proxy, providing:

- **Real-time visualization** of tool calls in a terminal UI (TUI)
- **Thinking block capture** with dedicated panel showing Claude's reasoning
- **Token & cost tracking** with cumulative session statistics
- **Structured logging** of all API traffic to JSON Lines files
- **Demo mode** for showcasing without a live Claude Code session

## Features

### TUI Display
- Live stream of tool calls as they happen
- **Thinking panel** - dedicated right panel showing Claude's reasoning in real-time
- Color-coded events (tool calls, results, thinking, requests, responses)
- Expandable detail view for full request/response inspection
- **Token tracking** - cumulative input/output/cache tokens with cost estimation
- Vim-style navigation (j/k or arrow keys)

### True Streaming Passthrough
- **SSE responses stream directly** to Claude Code with minimal latency
- Tokens appear incrementally as they're generated (not buffered)
- Proxy accumulates a copy for parsing without blocking the client
- Time-to-first-token preserved - no added latency for long responses

### Structured Logs
- JSON Lines format for easy parsing with `jq`, `grep`, etc.
- Daily log rotation (one file per day)
- Complete request/response bodies captured
- Timing information for every operation

### Architecture
```
Claude Code ←─── SSE Stream ───→ Anthropic Spy ←─── SSE Stream ───→ Anthropic API
                                       │
                                       │ (tee: accumulate while streaming)
                                       ▼
                              ┌────────┴────────┐
                              ▼                 ▼
                             TUI           JSON Logs
                        (live display)   (./logs/*.jsonl)
```

The proxy streams SSE responses directly to Claude Code while accumulating a copy for parsing and logging. This preserves the real-time token delivery experience.

### Demo Mode

Try the TUI without running Claude Code:

```bash
# Windows
$env:ANTHROPIC_SPY_DEMO="1"; .\anthropic-spy.exe

# macOS/Linux
ANTHROPIC_SPY_DEMO=1 ./anthropic-spy
```

Or if building from source: `ANTHROPIC_SPY_DEMO=1 cargo run --release`

Demo mode generates realistic mock events simulating a Claude Code session, including thinking blocks, tool calls, and token usage.

## Prerequisites

- **Claude Code** installed and configured
- Terminal with color support (Windows Terminal, iTerm2, etc.)
- **Rust** 1.75+ only if building from source ([Install Rust](https://rustup.rs/))

## Quick Start

### 1. Download

Get the latest release for your platform from [GitHub Releases](https://github.com/omgpointless/anthropic-spy/releases).

Or build from source:
```bash
cargo build --release
```

### 2. Run the Proxy

```bash
# Windows
.\anthropic-spy.exe

# macOS/Linux
./anthropic-spy
```

Or from source: `cargo run --release`

The TUI will display, showing an empty event list and status bar.

### 3. Configure Claude Code to Use the Proxy

In a **new terminal**, set the `ANTHROPIC_BASE_URL` environment variable to point to the proxy, then launch Claude Code:

#### Windows (PowerShell)
```powershell
$env:ANTHROPIC_BASE_URL="http://127.0.0.1:8080"
claude
```

#### Windows (CMD)
```cmd
set ANTHROPIC_BASE_URL=http://127.0.0.1:8080
claude
```

#### macOS/Linux
```bash
export ANTHROPIC_BASE_URL=http://127.0.0.1:8080
claude
```

> **💡 Why `ANTHROPIC_BASE_URL` instead of `HTTPS_PROXY`?**
>
> Using `ANTHROPIC_BASE_URL` is more targeted - it only redirects Anthropic API calls through the proxy, leaving other HTTPS traffic unaffected. The `HTTPS_PROXY` environment variable works too, but affects all HTTPS connections from Claude Code.

### 4. Watch the Magic Happen!

As Claude Code makes API calls, you'll see them appear in the TUI in real-time:
- 💭 **Thinking** - Claude's reasoning (dedicated panel + detail view)
- 🔧 **Tool Calls** - When Claude requests to use a tool (e.g., Read, Write, Bash)
- ✓ **Tool Results** - When tools complete successfully
- ✗ **Failed Results** - When tools encounter errors
- 📊 **API Usage** - Token counts and model information

## TUI Keyboard Controls

| Key | Action |
|-----|--------|
| `↑` or `k` | Move selection up (or scroll up in detail view) |
| `↓` or `j` | Move selection down (or scroll down in detail view) |
| `Enter` | Toggle detail view for selected event |
| `Home` | Jump to first event |
| `End` | Jump to last event |
| `q` | Quit the application |

## Log Files

Logs are written to `./logs/anthropic-spy-YYYY-MM-DD.jsonl` in JSON Lines format.

### Example Log Entry
```json
{
  "type": "tool_call",
  "id": "toolu_abc123",
  "timestamp": "2025-01-15T10:30:45Z",
  "tool_name": "Read",
  "input": {
    "file_path": "/path/to/file.txt"
  }
}
```

### Analyzing Logs

```bash
# Count tool calls by type
cat logs/*.jsonl | jq -r 'select(.type=="tool_call") | .tool_name' | sort | uniq -c

# Find all failed tool results
cat logs/*.jsonl | jq 'select(.type=="tool_result" and .success==false)'

# Calculate average tool duration
cat logs/*.jsonl | jq -r 'select(.type=="tool_result") | .duration.secs' | \
  awk '{sum+=$1; count++} END {print sum/count}'

# View tool calls in the last hour
cat logs/*.jsonl | jq -r 'select(.type=="tool_call" and (.timestamp | fromdate) > (now - 3600))'
```

## Configuration

Environment variables can customize the proxy behavior:

| Variable | Default | Description |
|----------|---------|-------------|
| `ANTHROPIC_SPY_BIND` | `127.0.0.1:8080` | Address to bind the proxy server |
| `ANTHROPIC_API_URL` | `https://api.anthropic.com` | Target API URL |
| `ANTHROPIC_SPY_LOG_DIR` | `./logs` | Directory for log files |
| `ANTHROPIC_SPY_NO_TUI` | `false` | Disable TUI (headless mode) |
| `ANTHROPIC_SPY_DEMO` | `false` | Enable demo mode (mock events) |

### Example: Custom Port
```bash
ANTHROPIC_SPY_BIND="127.0.0.1:9000" cargo run --release
```

Then configure Claude Code to use port 9000:
```bash
export ANTHROPIC_BASE_URL=http://127.0.0.1:9000
```

### Example: Headless Mode
```bash
ANTHROPIC_SPY_NO_TUI=1 cargo run --release
```
Logs will still be written, but no TUI will be displayed. Press Ctrl+C to stop.

## Project Structure

```
anthropic-spy/
├── src/
│   ├── main.rs          # Entry point, orchestrates components
│   ├── config.rs        # Configuration management
│   ├── events.rs        # Event types and statistics
│   ├── demo.rs          # Demo mode event generation
│   ├── pricing.rs       # Anthropic API pricing for cost estimation
│   ├── proxy/           # HTTP proxy server (axum)
│   ├── parser/          # API payload parsing, SSE handling
│   ├── tui/             # Terminal UI (ratatui)
│   ├── storage/         # JSON Lines file logging
│   └── logging/         # Custom tracing layer for TUI
├── Cargo.toml
└── logs/                # Generated log files (gitignored)
```

## Troubleshooting

### "Connection refused" Error

**Problem:** Claude Code shows connection errors.

**Solution:** Ensure the proxy is running before starting Claude Code. Check that the port (8080) is not already in use.

### No Events Appearing

**Problem:** TUI shows no events even though Claude Code is running.

**Solution:**
1. Verify `ANTHROPIC_BASE_URL` is set correctly in the Claude Code terminal
2. Look for errors in the proxy terminal
3. ?

### "Address already in use"

**Problem:** Cannot bind to port 8080.

**Solution:** Either:
- Stop the other process using port 8080
- Use a different port: `ANTHROPIC_SPY_BIND="127.0.0.1:9000" cargo run`

### TUI Not Displaying Correctly

**Problem:** Garbled or misaligned display.

**Solution:**
- Resize your terminal window
- Ensure your terminal supports color and Unicode
- Try a different terminal emulator (Windows Terminal, iTerm2, etc.)

## Development

### Running Tests
```bash
cargo test
```

### Checking Code
```bash
# Format code
cargo fmt

# Lint with Clippy
cargo clippy

# Check without building
cargo check
```

### Debug Logging
```bash
# Enable debug logging
RUST_LOG=debug cargo run
```

## About

Built with the help of [Claude Code](https://claude.ai/code). This is the maintainer's first Rust project.

## License

MIT
