---
description: "View context window status"
---

Use the `aspy_context` MCP tool to check current context window usage.

Display results with:
- Status indicator based on warning level:
  - 🟢 **NORMAL** - Under 70% (safe)
  - 🟡 **WARNING** - 70-85% (approaching limit)
  - 🟠 **HIGH** - 85-95% (high risk)
  - 🔴 **CRITICAL** - Over 95% (imminent compact)
- Usage percentage and token counts
- Number of compacts this session

**Note:** Requires anthropic-spy proxy to be running (default port 8080).
