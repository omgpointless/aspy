---
description: "View anthropic-spy session statistics"
---

Use the `aspy_stats` MCP tool to get current session statistics from anthropic-spy.

Display the results in a clear format showing:
- Session duration and event count
- Token usage (input, output, cached) with cache ratio
- Cost breakdown (total spent, savings from caching)
- Request success rate
- Top 5 most-used tools
- Thinking block statistics

**Note:** Requires anthropic-spy proxy to be running (default port 8080).
