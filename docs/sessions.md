# Multi-User Session Tracking

anthropic-spy supports tracking multiple Claude Code instances through a single proxy. Each user is identified by a hash of their API key or OAuth token, enabling per-user statistics, events, and session management.

## Privacy Model

**Identity Derivation:**
- User identity = SHA-256 hash of API key or OAuth token, truncated to first 16 hex characters
- Example: API key `sk-ant-xxx...` → User ID `b0acf41e12907b7b`
- The actual API key is **never stored or logged** - only the hash
- Hash is computed identically in Rust proxy and TypeScript MCP server

**What's Tracked Per Session:**
- Session key (explicit from hook or implicit from user hash)
- User ID (the 16-char hash)
- Session start time and status (active/idle/ended)
- Per-session statistics (requests, tokens, costs, tool calls)
- Per-session event buffer (last 500 events)

**What's NOT Tracked:**
- Actual API keys or OAuth tokens
- Request/response bodies (beyond what's in events)
- Any personally identifiable information

## Session Lifecycle

Sessions are tracked via Claude Code's hook system:

```
SessionStart hook fires → POST /api/session/start → Session created
   ↓
First request arrives → User ID backfilled from headers
   ↓
Events flow → Recorded to user's session + global buffer
   ↓
SessionEnd hook fires → POST /api/session/end → Session archived
```

**Supersession:** When a user starts a new session (e.g., restarts Claude Code), their previous session is automatically archived with status `superseded`.

## API Endpoints

All endpoints support optional `?user=<hash>` filtering:

| Endpoint | Without `?user=` | With `?user=<hash>` |
|----------|------------------|---------------------|
| `GET /api/stats` | Global aggregate stats | User's session stats |
| `GET /api/events` | Global event buffer | User's session events |
| `GET /api/context` | Global context status | User's context status |
| `GET /api/sessions` | All sessions | All sessions (no filter) |

**Session Management:**
- `POST /api/session/start` - Register new session (called by hook)
- `POST /api/session/end` - End session (called by hook)

## MCP Integration

The `aspy` MCP server automatically scopes queries to the current user:

```typescript
// MCP tools auto-detect user from ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN
const userId = getUserId();  // Computes same hash as Rust proxy
const endpoint = `/api/stats?user=${userId}`;
```

**Available MCP Tools:**
- `aspy_stats` - Your session statistics (auto-scoped)
- `aspy_events` - Your session events (auto-scoped)
- `aspy_context` - Your context window status (auto-scoped)
- `aspy_sessions` - All sessions with `is_me` flag

## Hook Setup

Session hooks are configured in `hooks/hooks.json`:

```json
{
  "hooks": {
    "SessionStart": [{
      "matcher": "startup|resume|clear|compact",
      "command": "hooks/spy/session-start.sh"
    }],
    "SessionEnd": [{
      "matcher": "clear|logout|prompt_input_exit|other",
      "command": "hooks/spy/session-end.sh"
    }]
  }
}
```

**Hook Scripts:**
- `session-start.sh` - Calls `/api/session/start` with session_id
- `session-end.sh` - Calls `/api/session/end` (fire-and-forget)

**Note:** Hooks cannot access `ANTHROPIC_API_KEY` due to Claude Code's security sandboxing. The proxy backfills user identity from the first proxied request's headers.

## Multi-User Scenarios

**Running multiple Claude Code instances:**
1. Start anthropic-spy proxy
2. Point multiple Claude Code instances at the proxy (`ANTHROPIC_BASE_URL`)
3. Each instance gets its own session, identified by API key hash
4. Use `/api/sessions` to see all active sessions
5. Each Claude sees only its own data via MCP tools

**Querying another user's data (admin use):**
```bash
# See all sessions
curl http://127.0.0.1:8080/api/sessions

# Get specific user's stats
curl "http://127.0.0.1:8080/api/stats?user=a1b2c3d4e5f6g7h8"
```
