# aspy-mcp

MCP server for [Aspy](https://github.com/omgpointless/aspy) — exposes session stats, events, and context window status to Claude Code.

## Installation

```bash
claude mcp add aspy -- npx -y aspy-mcp
```

Or with a custom proxy URL:

```bash
claude mcp add aspy -e ASPY_URL=http://192.168.1.100:8080 -- npx -y aspy-mcp
```

## Requirements

- [Aspy](https://github.com/omgpointless/aspy) running locally (default: `http://127.0.0.1:8080`)
- Node.js 18+

## Available Tools

| Tool | Description |
|------|-------------|
| `aspy_stats` | Session statistics — tokens, costs, cache efficiency |
| `aspy_events` | Recent events — tool calls, API usage, thinking blocks |
| `aspy_context` | Context window status — usage percentage, warning level |
| `aspy_sessions` | All tracked sessions with `is_me` flag |
| `aspy_search` | Search session logs for past conversations |
| `aspy_lifestats_context` | **RECOMMENDED:** Combined FTS5 context recovery |
| `aspy_lifestats_context_hybrid` | **BEST QUALITY:** Semantic + FTS hybrid search |
| `aspy_lifestats_search_thinking` | Search past thinking blocks |
| `aspy_lifestats_search_prompts` | Search past user prompts |
| `aspy_lifestats_search_responses` | Search past assistant responses |
| `aspy_lifestats_stats` | Lifetime usage statistics |
| `aspy_lifestats_embeddings_status` | Check semantic embedding indexer status |

## Usage

Once configured, Claude Code can query your session:

- "How many tokens have I used?" → `aspy_stats`
- "What tools have been called?" → `aspy_events`
- "Am I running low on context?" → `aspy_context`
- "Search for when we discussed authentication" → `aspy_search`
- "Recover context about the refactor we did" → `aspy_lifestats_context`

---

## Claude Usage Guide

> This section is for you, Claude. Follow these guidelines when using Aspy MCP tools.

### Session Identity

The MCP server automatically determines your identity via:
1. `ASPY_CLIENT_ID` environment variable (if set)
2. SHA-256 hash of `ANTHROPIC_API_KEY` (first 16 hex chars)

Most tools automatically scope to your session. If identity cannot be determined, you'll see an error.

### Tool Selection Guide

| Need | Use This | Notes |
|------|----------|-------|
| Check context window % | `aspy_context` | Shows warning level, compacts count |
| Session token/cost summary | `aspy_stats` | Current session only |
| Recent tool calls | `aspy_events` | Filter by type if needed |
| **Recover lost context** | `aspy_lifestats_context` | Searches thinking, prompts, responses |
| Semantic + keyword search | `aspy_lifestats_context_hybrid` | Best quality if embeddings enabled |
| Lifetime usage summary | `aspy_lifestats_stats` | All sessions, all time |

### Context Recovery After Compaction

When you receive a compacted context or lose memory:

```
1. Use aspy_lifestats_context with the topic you need
2. Try mode="natural" with AND/OR for complex queries
3. Fall back to specific searches if needed:
   - aspy_lifestats_search_thinking (your reasoning)
   - aspy_lifestats_search_prompts (what user asked)
   - aspy_lifestats_search_responses (what you answered)
```

### Error Handling

| Error | Cause | Fix |
|-------|-------|-----|
| "Context is per-session" | `/api/context` called without user ID | MCP auto-handles; check `ANTHROPIC_API_KEY` is set |
| "Cannot determine user identity" | No API key or client ID available | Ensure Claude Code has `ANTHROPIC_API_KEY` |
| "No active session for user" | Session not found in proxy | Wait for first API call to register session |

### Best Practices

1. **Don't poll context constantly** — Check when user mentions context or before large operations
2. **Use lifestats for cross-session memory** — Current session tools won't find past conversations
3. **Trust the warning level** — `normal` (ok), `warning` (>70%), `high` (>85%), `critical` (>95%)
4. **Proactively offer `/compact` at 90%+** — Don't wait for critical

---

## Multi-Client Architecture

Aspy supports multiple Claude Code instances via URL-path routing:

```
Client A → http://localhost:8080/dev-1/v1/messages
Client B → http://localhost:8080/dev-2/v1/messages
```

Each gets isolated stats and context tracking. The MCP server uses `ASPY_CLIENT_ID` to identify which session is "you."

### For Multi-Client Setups

Set `ASPY_CLIENT_ID` to match your URL path prefix:

```bash
claude mcp add aspy -e ASPY_CLIENT_ID=dev-1 -- npx -y aspy-mcp
```

## License

MIT
