# Quick Start Guide

Get up and running with Anthropic Spy in 5 minutes!

## Step 0: Try Demo Mode (Optional)

Want to see the TUI first? Try demo mode - no Claude Code needed:

```bash
# Windows
$env:ASPY_DEMO="1"; .\aspy.exe

# macOS/Linux
ASPY_DEMO=1 ./aspy
```

This generates mock events showing thinking blocks, tool calls, and token tracking.

## Step 1: Download or Build

**Option A: Download** (recommended)

Get the latest release from [GitHub Releases](https://github.com/omgpointless/anthropic-spy/releases).

**Option B: Build from source**
```bash
cargo build --release
```

## Step 2: Start the Proxy

```bash
# Windows
.\aspy.exe

# macOS/Linux
./aspy

# Or from source
cargo run --release
```

The TUI will appear with an empty event list and status bar showing token counts.

## Step 3: Launch Claude Code

Open a **NEW terminal** (keep the proxy running!) and configure Claude Code to use the proxy:

**Windows PowerShell:**
```powershell
$env:ANTHROPIC_BASE_URL="http://127.0.0.1:8080"
claude
```

**macOS/Linux:**
```bash
export ANTHROPIC_BASE_URL=http://127.0.0.1:8080
claude
```

## Step 4: Use Claude Code Normally

Ask Claude Code to do something that requires tools. For example:

```
You: "Read the README.md file and summarize it"
```

## Step 5: Watch the TUI!

Switch back to the proxy terminal. You'll see events appearing, and if Claude is thinking, a dedicated thinking panel appears on the right:

- **Left panel**: Event stream with tool calls, results, usage
- **Right panel**: Claude's current thinking/reasoning (when active)
- **Status bar**: Token counts, costs, uptime

### Navigation

- `↑`/`↓` or `j`/`k` - Navigate events
- `Enter` - Toggle detail view
- `q` - Quit

## What You're Seeing

| Symbol | Event Type | Description |
|--------|------------|-------------|
| 💭 | Thinking | Claude's reasoning (shown in dedicated panel) |
| 🔧 | Tool Call | Claude requesting to use a tool |
| ✓ | Tool Success | Tool executed successfully |
| ✗ | Tool Failure | Tool encountered an error |
| 📊 | API Usage | Token counts and model info |

## Check the Logs

All events are also saved to `logs/aspy-*.jsonl`:

```bash
# View today's log file
cat logs/aspy-*.jsonl

# Pretty-print with jq
cat logs/aspy-*.jsonl | jq

# Count tool calls
cat logs/aspy-*.jsonl | grep tool_call | wc -l
```

## Common Issues

### "Connection refused"
Make sure the proxy is running BEFORE starting Claude Code.

### "Address already in use"
Use a different port:
```bash
ASPY_BIND="127.0.0.1:9000" ./aspy
```
Then: `export ANTHROPIC_BASE_URL=http://127.0.0.1:9000`

### No events appearing
- Verify `ANTHROPIC_BASE_URL` is set in the Claude Code terminal
- Make sure Claude is doing something that triggers tool calls
- Check for errors in the proxy terminal

---

See [README.md](README.md) for full documentation.
