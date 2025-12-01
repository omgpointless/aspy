![Aspy (Anthropic Spy)](docs/images/aspy-logo-v1-min-small-resized.jpg)

[![CI](https://github.com/omgpointless/anthropic-spy/actions/workflows/ci.yml/badge.svg)](https://github.com/omgpointless/anthropic-spy/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/omgpointless/anthropic-spy?include_prereleases)](https://github.com/omgpointless/anthropic-spy/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![npm](https://img.shields.io/npm/v/aspy-mcp)](https://www.npmjs.com/package/aspy-mcp)

**Observability proxy for Claude Code** — see what's happening between Claude and the Anthropic API.

![Demo](demo.gif)

## What is Aspy?

Aspy sits between Claude Code and the Anthropic API, intercepting all traffic and giving you visibility into tool calls, thinking blocks, token usage, and costs. Run it as a TUI proxy, integrate via MCP, or install the Claude Code plugin.

## What It Does

- **Real-time TUI** — Watch tool calls, thinking blocks, and API responses as they stream
- **Token & cost tracking** — Cumulative session statistics with per-model pricing
- **Thinking capture** — Dedicated panel showing Claude's reasoning in real-time
- **Structured logs** — JSON Lines format for analysis with `jq`
- **Multi-client routing** — Track multiple Claude Code instances through one proxy
- **REST API & MCP** — Programmatic access to session data

## Get Started

### Option 1: TUI Proxy (Binary)

Download from [GitHub Releases](https://github.com/omgpointless/anthropic-spy/releases) or build from source:

```bash
cargo install --git https://github.com/omgpointless/anthropic-spy
```

Run the proxy:
```bash
# Windows
.\aspy.exe

# macOS/Linux
./aspy
```

Point Claude Code at it:
```bash
# In a new terminal
export ANTHROPIC_BASE_URL=http://127.0.0.1:8080
claude
```

### Option 2: MCP Server

Add aspy to Claude Code's MCP servers:

```bash
claude mcp add aspy -- npx -y aspy-mcp
```

This gives Claude Code access to session stats, events, and context window status via tools like `aspy_stats`, `aspy_events`, and `aspy_context`.

> **Note**: Requires the TUI proxy running to collect data.

### Option 3: Claude Code Plugin

Install the plugin for slash commands:

```bash
/plugin marketplace add omgpointless/anthropic-spy
```

After restarting Claude Code, use `/aspy:stats` to query session metrics. The plugin also includes automatic `cargo fmt` hooks for Rust projects.

## Demo Mode

Try the TUI without Claude Code:

```bash
ASPY_DEMO=1 ./aspy
```

Generates mock events to showcase the interface.

## Documentation

| Topic | Description |
|-------|-------------|
| [Features](docs/features.md) | Deep dive into all capabilities |
| [Quick Start](QUICKSTART.md) | Step-by-step setup walkthrough |
| [CLI Reference](docs/cli-reference.md) | Commands, config options, env vars |
| [Log Analysis](docs/log-analysis.md) | jq queries for session profiling |
| [Themes](docs/themes.md) | 32 bundled themes + custom TOML |
| [Multi-Client Routing](docs/sessions.md) | Track multiple Claude instances |
| [REST API](docs/api-reference.md) | Programmatic endpoints |
| [Architecture](docs/architecture.md) | For contributors |

## Package Managers

Homebrew, Scoop, and Chocolatey packages will be available when the project stabilizes.

## About

Maintainer's first Rust project. Learning journey together with Claude Code which is the whole reason I built this tool to start with.

## License

MIT
