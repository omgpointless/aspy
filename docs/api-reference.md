---
layout: default
title: API Reference
---

# REST API Reference

Aspy exposes a local HTTP API for programmatic access to session data. All endpoints return JSON and are designed for local consumption only.

**Security:** The API binds to `127.0.0.1` by default (localhost only).

## Base URL

```
http://127.0.0.1:8080/api
```

## Endpoints

### GET /api/stats

Returns session statistics including token usage, costs, and tool call metrics.

**Query Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `user` | string | Filter by user ID (API key hash, e.g., `b0acf41e12907b7b`) |

**Response:**

```json
{
  "session": {
    "started": "2025-11-27T10:30:00Z",
    "duration_secs": 3600,
    "events_count": 150
  },
  "tokens": {
    "input": 50000,
    "output": 15000,
    "cached": 45000,
    "cache_created": 5000,
    "cache_ratio_pct": 90
  },
  "cost": {
    "total_usd": 0.0234,
    "savings_usd": 0.0180,
    "by_model": {
      "claude-sonnet-4-20250514": 0.0150,
      "claude-haiku-3-5-20241022": 0.0084
    }
  },
  "requests": {
    "total": 25,
    "failed": 0,
    "success_rate_pct": 100.0,
    "avg_ttfb_ms": 450
  },
  "tools": {
    "total_calls": 58,
    "failed_calls": 2,
    "by_tool": {
      "Read": 32,
      "Edit": 15,
      "Bash": 8,
      "Glob": 3
    }
  },
  "thinking": {
    "blocks": 12,
    "total_tokens": 8500
  }
}
```

**Example:**

```bash
# Global stats
curl http://127.0.0.1:8080/api/stats

# Stats for specific user
curl "http://127.0.0.1:8080/api/stats?user=b0acf41e12907b7b"
```

---

### GET /api/events

Returns recent events with optional filtering. Events are returned most recent first.

**Query Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `limit` | integer | 50 | Maximum events to return (max: 500) |
| `type` | string | - | Filter by event type |
| `user` | string | - | Filter by user ID (API key hash) |

**Event Types:**

- `ToolCall` - Tool invocation from Claude
- `ToolResult` - Result returned to Claude
- `Request` - API request sent
- `Response` - API response received
- `Error` - Error occurred
- `HeadersCaptured` - Request headers captured
- `RateLimitUpdate` - Rate limit information
- `ApiUsage` - Token usage report
- `Thinking` - Thinking block content
- `ContextCompact` - Context window compaction detected
- `ContextRecovery` - Automatic context recovery (tool_result crunching)
- `ThinkingStarted` - Thinking block started
- `UserPrompt` - User's prompt extracted from request
- `AssistantResponse` - Claude's text response
- `RequestTransformed` - Request was modified by a transformer
- `ResponseAugmented` - Response was augmented with injected content
- `PreCompactHook` - PreCompact hook was triggered
- `TodoSnapshot` - Todo list snapshot from TodoWrite

**Response:**

```json
{
  "total_in_buffer": 500,
  "returned": 50,
  "events": [
    {
      "type": "ToolCall",
      "timestamp": "2025-11-27T10:35:00Z",
      "tool_name": "Read",
      "tool_id": "toolu_01ABC123",
      "input": "{\"file_path\": \"/home/user/project/src/main.rs\"}"
    },
    {
      "type": "ToolResult",
      "timestamp": "2025-11-27T10:35:01Z",
      "tool_name": "Read",
      "tool_id": "toolu_01ABC123",
      "duration_ms": 45,
      "is_error": false,
      "output_preview": "// Main entry point..."
    }
  ]
}
```

**Examples:**

```bash
# Recent 50 events
curl http://127.0.0.1:8080/api/events

# Last 10 tool calls only
curl "http://127.0.0.1:8080/api/events?type=ToolCall&limit=10"

# Thinking blocks from specific user
curl "http://127.0.0.1:8080/api/events?type=Thinking&user=b0acf41e12907b7b"
```

---

### GET /api/context

Returns context window status including current usage and warning level.

> **Note:** Context is inherently per-session, so the `user` parameter is required.

**Query Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `user` | string | **Yes** | User ID (API key hash, e.g., `b0acf41e12907b7b`) |

**Response:**

```json
{
  "current_tokens": 85000,
  "limit_tokens": 200000,
  "usage_pct": 42.5,
  "warning_level": "normal",
  "compacts": 0,
  "breakdown": {
    "input": 40000,
    "cached": 45000
  }
}
```

**Warning Levels:**

| Level | Usage % | Description |
|-------|---------|-------------|
| `normal` | < 70% | Safe operating range |
| `warning` | 70-85% | Approaching limit |
| `high` | 85-95% | High risk of compaction |
| `critical` | > 95% | Compaction imminent |

**Error Response (missing user):**

```json
{
  "error": "Context is per-session. Please provide ?user=<api_key_hash> parameter. Use /api/sessions to list active user sessions and their IDs."
}
```

**Example:**

```bash
# Get context for specific user (required)
curl "http://127.0.0.1:8080/api/context?user=b0acf41e12907b7b"

# Find your user ID first
curl http://127.0.0.1:8080/api/sessions | jq '.sessions[].user_id'
```

---

### GET /api/sessions

Returns information about all tracked sessions.

**Response:**

```json
{
  "active_count": 2,
  "sessions": [
    {
      "key": "session:abc123",
      "user_id": "b0acf41e12907b7b",
      "claude_session_id": "session-xyz-789",
      "source": "hook",
      "started": "2025-11-27T10:30:00Z",
      "status": "active",
      "event_count": 150,
      "stats": {
        "requests": 25,
        "tool_calls": 58,
        "input_tokens": 50000,
        "output_tokens": 15000,
        "cost_usd": 0.0234
      }
    }
  ]
}
```

**Session Status Values:**

- `active` - Session is currently active
- `idle` - No recent activity
- `ended` - Session has been closed

**Example:**

```bash
curl http://127.0.0.1:8080/api/sessions
```

---

### GET /api/whoami

Returns the current user's identity and session information. Useful for discovering your user ID and verifying session detection is working.

**Identification Priority:**
1. `?user=` query param (supports ASPY_CLIENT_ID / URL path routing)
2. `x-api-key` header (hashed)
3. `Authorization: Bearer` header (hashed)

**Query Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `user` | string | User identifier (client_id or api_key_hash) |

**Response:**

```json
{
  "user_id": "b0acf41e12907b7b",
  "session_id": "session:abc123",
  "claude_session_id": "session-xyz-789",
  "session_started": "2025-12-01T10:30:00Z",
  "session_source": "hook",
  "session_status": "active",
  "transcript_path": "/home/user/.claude/projects/.../session.jsonl"
}
```

| Field | Description |
|-------|-------------|
| `user_id` | First 16 chars of SHA-256 hash of API key, or client_id if using URL routing |
| `session_id` | Aspy's internal session key |
| `claude_session_id` | Claude Code's session ID (from hook) |
| `session_started` | When the session started (ISO 8601) |
| `session_source` | How session was detected: `hook`, `implicit`, `reconnected` |
| `session_status` | Current status: `active`, `idle`, `ended` |
| `transcript_path` | Path to Claude Code's transcript file (if available) |

**Example:**

```bash
# Discover your user ID and session
curl http://127.0.0.1:8080/api/whoami -H "x-api-key: $ANTHROPIC_API_KEY"

# Using client_id
curl "http://127.0.0.1:8080/api/whoami?user=dev-1"
```

---

### GET /api/session-history

Returns session history for the current user, including both in-memory history and persisted sessions from the database.

**Query Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `user` | string | - | User identifier (required via param or header) |
| `limit` | integer | 20 | Max sessions to return (max: 100) |
| `offset` | integer | 0 | Skip first N sessions |
| `after` | string | - | Only sessions after this timestamp (ISO 8601) |
| `before` | string | - | Only sessions before this timestamp (ISO 8601) |

**Response:**

```json
{
  "user_id": "b0acf41e12907b7b",
  "count": 5,
  "has_more": true,
  "sessions": [
    {
      "session_id": "session:abc123",
      "user_id": "b0acf41e12907b7b",
      "claude_session_id": "session-xyz-789",
      "started": "2025-12-01T10:30:00Z",
      "ended": "2025-12-01T12:45:00Z",
      "source": "hook",
      "end_reason": "prompt_input_exit",
      "transcript_path": "/home/user/.claude/projects/.../session.jsonl",
      "stats": {
        "requests": 25,
        "tool_calls": 58,
        "input_tokens": 50000,
        "output_tokens": 15000,
        "cost_usd": 0.0234
      }
    }
  ]
}
```

**Example:**

```bash
# Get last 10 sessions
curl "http://127.0.0.1:8080/api/session-history?user=b0acf41e12907b7b&limit=10"

# Sessions from last week
curl "http://127.0.0.1:8080/api/session-history?user=dev-1&after=2025-11-24T00:00:00Z"
```

---

### POST /api/session/start

Register a new session. Called by the SessionStart hook when Claude Code starts.

**Request Body:**

```json
{
  "session_id": "session-xyz-789",
  "user_id": "b0acf41e12907b7b",
  "source": "hook"
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `session_id` | Yes | Claude Code's session ID |
| `user_id` | Yes | API key hash (first 16 chars of SHA-256) |
| `source` | No | How started: `"hook"` or `"warmup"` |

**Response:**

```json
{
  "success": true,
  "message": "Session started",
  "session_key": "session:abc123"
}
```

---

### POST /api/session/end

End a session. Called by the SessionEnd hook when Claude Code exits.

**Request Body:**

```json
{
  "session_id": "session-xyz-789",
  "user_id": "b0acf41e12907b7b",
  "reason": "prompt_input_exit"
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `session_id` | Yes | Claude Code's session ID |
| `user_id` | Yes | API key hash |
| `reason` | No | End reason: `"clear"`, `"logout"`, `"prompt_input_exit"` |

**Response:**

```json
{
  "success": true,
  "message": "Session ended"
}
```

---

### POST /api/search

Search session logs for past conversations. Useful for recovering context lost to compaction.

**Request Body:**

```json
{
  "keyword": "authentication",
  "role": "user",
  "session": "20251127",
  "limit": 10,
  "time_range": "last_7_days"
}
```

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `keyword` | Yes | - | Search term (case-insensitive) |
| `role` | No | - | Filter by message role: `"user"` or `"assistant"` |
| `session` | No | - | Filter by session filename (partial match) |
| `limit` | No | 10 | Max results (max: 100) |
| `time_range` | No | - | Time filter (see below) |

**Time Range Values:**

- `today` - Today only
- `before_today` - Before today
- `last_3_days` - Last 3 days
- `last_7_days` - Last 7 days
- `last_30_days` - Last 30 days

**Response:**

```json
{
  "query": "authentication",
  "sessions_searched": 5,
  "total_matches": 3,
  "results": [
    {
      "session": "aspy-20251127-103000-abc1.jsonl",
      "timestamp": "2025-11-27T10:35:00Z",
      "role": "user",
      "text": "...implement JWT authentication for the API..."
    }
  ]
}
```

**Example:**

```bash
curl -X POST http://127.0.0.1:8080/api/search \
  -H "Content-Type: application/json" \
  -d '{"keyword": "refactor", "time_range": "today"}'
```

---

## Cortex API

The Cortex API provides access to historical data across all sessions, stored in SQLite with FTS5 indexing.

### GET /api/cortex/stats

Returns lifetime statistics across all sessions.

**Response:**

```json
{
  "total_sessions": 150,
  "total_tokens": {
    "input": 5000000,
    "output": 1500000,
    "cached": 4500000
  },
  "total_cost_usd": 12.34,
  "by_model": {
    "claude-sonnet-4-20250514": {
      "tokens": 4000000,
      "cost_usd": 8.50
    },
    "claude-haiku-3-5-20241022": {
      "tokens": 2500000,
      "cost_usd": 3.84
    }
  },
  "tool_usage": {
    "Read": { "calls": 5000, "avg_duration_ms": 45 },
    "Edit": { "calls": 2000, "avg_duration_ms": 120 }
  },
  "time_range": {
    "first_session": "2025-11-01T10:00:00Z",
    "last_session": "2025-12-03T15:30:00Z"
  }
}
```

---

### GET /api/cortex/context/hybrid/user/:user_id

**Best quality** — Hybrid search combining semantic embeddings with FTS5 keyword matching using Reciprocal Rank Fusion (RRF).

**Path Parameters:**

| Parameter | Description |
|-----------|-------------|
| `user_id` | API key hash (e.g., `b0acf41e12907b7b`) |

**Query Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `topic` | string | required | Search query |
| `limit` | integer | 10 | Max results (max: 50) |
| `mode` | string | `phrase` | FTS mode: `phrase`, `natural`, `raw` |

**Response:**

```json
{
  "search_type": "hybrid",
  "query": "authentication",
  "results": [
    {
      "match_type": "thinking",
      "content": "For the auth system, we should use JWT tokens with...",
      "session_id": "session-abc123",
      "timestamp": "2025-12-01T14:30:00Z",
      "rank_score": -12.34
    },
    {
      "match_type": "user_prompt",
      "content": "How should we implement login?",
      "session_id": "session-abc123",
      "timestamp": "2025-12-01T14:29:00Z",
      "rank_score": -11.89
    }
  ]
}
```

**Notes:**
- `search_type` will be `"fts_only"` if embeddings aren't available
- Lower `rank_score` = more relevant (BM25 algorithm)
- `match_type`: `thinking`, `user_prompt`, or `assistant_response`

---

### GET /api/cortex/context/user/:user_id

FTS-only context search (fallback when embeddings unavailable).

Same parameters and response as hybrid, but `search_type` is always `"fts_only"`.

---

### GET /api/cortex/search/thinking/user/:user_id

Search only Claude's thinking blocks.

**Query Parameters:** Same as hybrid endpoint.

**Response:**

```json
{
  "query": "refactor",
  "results": [
    {
      "content": "I need to refactor this function because...",
      "session_id": "session-abc123",
      "timestamp": "2025-12-01T14:30:00Z",
      "rank_score": -10.5
    }
  ]
}
```

---

### GET /api/cortex/search/prompts/user/:user_id

Search only user prompts.

---

### GET /api/cortex/search/responses/user/:user_id

Search only assistant responses.

---

### GET /api/cortex/embeddings/status

Check embedding indexer status.

**Response:**

```json
{
  "enabled": true,
  "running": true,
  "provider": "remote",
  "model": "text-embedding-3-small",
  "dimensions": 1536,
  "documents_indexed": 5000,
  "documents_pending": 200,
  "index_progress_pct": 96.2
}
```

---

### POST /api/cortex/embeddings/reindex

Trigger a full reindex of embeddings. Use after changing embedding providers/models.

**Response:**

```json
{
  "success": true,
  "message": "Reindex started"
}
```

---

### POST /api/cortex/embeddings/poll

Force the indexer to check for new content immediately (instead of waiting for poll interval).

---

### GET /api/cortex/todos

Search todo snapshots captured from Claude's TodoWrite tool calls. Useful for recalling task lists from past sessions.

**Query Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `q` | string | - | Search query (FTS on todo content) |
| `limit` | integer | 10 | Max results (max: 100) |
| `days` | integer | - | Days to look back (default: all time) |
| `mode` | string | `phrase` | FTS mode: `phrase`, `natural`, `raw` |

**Response:**

```json
{
  "query": "authentication",
  "count": 3,
  "results": [
    {
      "timestamp": "2025-12-01T14:30:00Z",
      "session_id": "session-abc123",
      "pending_count": 2,
      "in_progress_count": 1,
      "completed_count": 5,
      "todos": [
        {
          "content": "Implement JWT authentication",
          "status": "completed"
        },
        {
          "content": "Add refresh token support",
          "status": "in_progress"
        }
      ],
      "rank_score": -12.34
    }
  ]
}
```

**Example:**

```bash
# Search for todos mentioning "refactor"
curl "http://127.0.0.1:8080/api/cortex/todos?q=refactor"

# Last 7 days, limit 20
curl "http://127.0.0.1:8080/api/cortex/todos?q=test&days=7&limit=20"
```

---

## Error Responses

All endpoints may return error responses:

**500 Internal Server Error:**
```json
{
  "error": "Failed to lock sessions: ..."
}
```

**404 Not Found:**
```json
{
  "error": "Session not found"
}
```

---

## Integration Examples

### Shell Script: Check Context Usage

```bash
#!/bin/bash
# Note: Replace USER_ID with your API key hash (use /api/sessions to find it)
USER_ID="b0acf41e12907b7b"

CONTEXT=$(curl -s "http://127.0.0.1:8080/api/context?user=${USER_ID}")
USAGE=$(echo "$CONTEXT" | jq -r '.usage_pct')
WARNING=$(echo "$CONTEXT" | jq -r '.warning_level')

if [ "$WARNING" != "normal" ]; then
  echo "Context warning: ${USAGE}% used (${WARNING})"
fi
```

### MCP Server Integration

The `aspy` MCP server uses these endpoints to expose data to Claude Code:

```typescript
// Get session stats
const stats = await fetch('http://127.0.0.1:8080/api/stats').then(r => r.json());

// Format for Claude
return `Session: ${stats.session.duration_secs}s, Cost: $${stats.cost.total_usd.toFixed(4)}`;
```

### Claude Code Hook

```bash
# In .claude/hooks/session_start.sh
curl -X POST http://127.0.0.1:8080/api/session/start \
  -H "Content-Type: application/json" \
  -d "{\"session_id\": \"$CLAUDE_SESSION_ID\", \"user_id\": \"$CLAUDE_USER_ID\"}"
```

---

## Rate Limits

The local API has no rate limits. However, be mindful that:

- Event buffer holds max 500 events
- Search scans log files on disk (may be slow for many large files)
- Stats are computed on each request (not cached)

For high-frequency polling, consider caching responses client-side.
