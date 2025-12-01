# aspy-mcp

MCP server for [aspy](https://github.com/omgpointless/anthropic-spy) — exposes session stats, events, and context window status to Claude Code.

## Installation

```bash
claude mcp add aspy -- npx -y aspy-mcp
```

Or with a custom proxy URL:

```bash
claude mcp add aspy -e ASPY_URL=http://192.168.1.100:8080 -- npx -y aspy-mcp
```

## Requirements

- [aspy](https://github.com/omgpointless/anthropic-spy) running locally (default: `http://127.0.0.1:8080`)
- Node.js 18+

## Available Tools

| Tool | Description |
|------|-------------|
| `aspy_stats` | Session statistics — tokens, costs, cache efficiency |
| `aspy_events` | Recent events — tool calls, API usage, thinking blocks |
| `aspy_context` | Context window status — usage percentage, warning level |
| `aspy_sessions` | All tracked sessions with `is_me` flag |
| `aspy_search` | Search session logs for past conversations |

## Usage

Once configured, Claude Code can query your session:

- "How many tokens have I used?" → `aspy_stats`
- "What tools have been called?" → `aspy_events`
- "Am I running low on context?" → `aspy_context`
- "Search for when we discussed authentication" → `aspy_search`

## License

MIT
