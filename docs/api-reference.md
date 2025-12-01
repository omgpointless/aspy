# REST API Reference

anthropic-spy exposes a local HTTP API for programmatic access to session data. All endpoints return JSON and are designed for local consumption only.

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
- `ThinkingStarted` - Thinking block started

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

**Query Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `user` | string | Filter by user ID (API key hash) |

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

**Example:**

```bash
curl http://127.0.0.1:8080/api/context
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
      "session": "anthropic-spy-20251127-103000-abc1.jsonl",
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
CONTEXT=$(curl -s http://127.0.0.1:8080/api/context)
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
