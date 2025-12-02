---
layout: default
title: Getting Started
description: Get Aspy running in under 5 minutes
---

# Getting Started

Get Aspy running in under 5 minutes. No complex setup required.

---

## 1. Download

Grab the latest release for your platform:

<div class="hero-links" style="text-align: left; margin: 1.5rem 0;">
  <a href="https://github.com/omgpointless/aspy/releases" class="primary">Download from GitHub Releases</a>
</div>

Or build from source if you have Rust installed:

```bash
cargo install --git https://github.com/omgpointless/aspy
```

---

## 2. Run the Proxy

Open a terminal and start Aspy:

**Windows:**
```powershell
.\aspy.exe
```

**macOS / Linux:**
```bash
./aspy
```

You'll see the TUI appear with an empty event list. Leave this running.

---

## 3. Point Claude Code at the Proxy

Open a **new terminal** and set the environment variable before launching Claude:

**Windows PowerShell:**
```powershell
$env:ANTHROPIC_BASE_URL="http://127.0.0.1:8080"
claude
```

**macOS / Linux:**
```bash
export ANTHROPIC_BASE_URL=http://127.0.0.1:8080
claude
```

---

## 4. Start Using Claude Code

That's it! Use Claude Code normally. Every API call now flows through Aspy.

Ask Claude to do something that triggers tools:

```
You: "Read the README.md and summarize it"
```

Switch back to the Aspy terminal — you'll see events streaming in real-time.

---

## What You'll See

| Symbol | Event Type | What it means |
|--------|------------|---------------|
| 💭 | Thinking | Claude's reasoning (shown in dedicated panel) |
| 🔧 | Tool Call | Claude requesting to use a tool |
| ✓ | Tool Success | Tool executed successfully |
| ✗ | Tool Failure | Tool encountered an error |
| 📊 | API Usage | Token counts and model info |

### Navigation

- `↑`/`↓` or `j`/`k` — Navigate events
- `Enter` — Toggle detail view
- `1`/`2`/`3` — Switch views (Events, Stats, Settings)
- `t` — Cycle themes
- `q` — Quit

---

## Try Demo Mode

Want to see the TUI without setting up Claude Code? Run in demo mode:

**Windows:**
```powershell
$env:ASPY_DEMO="1"; .\aspy.exe
```

**macOS / Linux:**
```bash
ASPY_DEMO=1 ./aspy
```

This generates mock events so you can explore the interface.

---

## Troubleshooting

### "Connection refused"
Make sure Aspy is running **before** you start Claude Code.

### "Address already in use"
Another process is using port 8080. Use a different port:

```bash
ASPY_BIND="127.0.0.1:9000" ./aspy
```

Then point Claude Code at that port:
```bash
export ANTHROPIC_BASE_URL=http://127.0.0.1:9000
```

### No events appearing
- Double-check `ANTHROPIC_BASE_URL` is set in the Claude Code terminal
- Make sure Claude is doing something that triggers tool calls
- Check for errors in the Aspy terminal

---

## Next Steps

- **[Features]({{ '/features' | relative_url }})** — Everything Aspy can do
- **[Views]({{ '/views' | relative_url }})** — Navigate the TUI
- **[Themes]({{ '/themes' | relative_url }})** — Customize the look
- **[Sessions]({{ '/sessions' | relative_url }})** — Multi-client routing