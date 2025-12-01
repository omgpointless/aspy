---
description: "View anthropic-spy session events"
argument-hint: "[limit] [type]"
---

Use the `aspy_events` MCP tool to query recent events from the current session.

**Arguments:**
- First argument: `limit` - Max events to return (default: 10, max: 500)
- Second argument: `type` - Filter by event type (optional)

**Event types:** `ToolCall`, `ToolResult`, `ApiUsage`, `Thinking`, `Request`, `Response`

**Usage examples:**
- `/aspy:events` - Last 10 events (any type)
- `/aspy:events 20` - Last 20 events
- `/aspy:events 5 ToolCall` - Last 5 tool calls only
- `/aspy:events 100 Thinking` - Last 100 thinking blocks

Display results showing timestamp, type, and relevant details for each event.

**Note:** Requires anthropic-spy proxy to be running (default port 8080).
