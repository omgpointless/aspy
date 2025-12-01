---
layout: default
title: Log Analysis
---

# Analyzing Session Logs

Session logs are stored in JSON Lines format (`./logs/anthropic-spy-YYYYMMDD-HHMMSS-XXXX.jsonl`). Each session creates a new file. Use `jq` to query and analyze them.

## Quick Session Profile

Get a complete session overview:

```bash
# Event type distribution
jq -s 'group_by(.type) | map({type: .[0].type, count: length}) | sort_by(-.count)' logs/<session>.jsonl

# Model distribution (Haiku vs Opus vs Sonnet)
jq -s '[.[] | select(.type == "ApiUsage") | .model] | group_by(.) | map({model: .[0], count: length}) | sort_by(-.count)' logs/<session>.jsonl

# Tool call distribution
jq -s '[.[] | select(.type == "ToolCall") | .tool_name] | group_by(.) | map({tool: .[0], count: length}) | sort_by(-.count)' logs/<session>.jsonl
```

## Token & Cost Analysis

```bash
# Token breakdown by model
jq -s '[.[] | select(.type == "ApiUsage")] | group_by(.model) | map({model: .[0].model, input: (map(.input_tokens) | add), output: (map(.output_tokens) | add), cached: (map(.cache_read_tokens) | add)})' logs/<session>.jsonl

# Cache efficiency (expect 90%+ for typical sessions)
jq -s '[.[] | select(.type == "ApiUsage")] | {total_input: (map(.input_tokens) | add), total_cached: (map(.cache_read_tokens) | add), total_output: (map(.output_tokens) | add)} | . + {cache_ratio_pct: ((.total_cached / (.total_input + .total_cached)) * 100 | floor)}' logs/<session>.jsonl

# Session time range
jq -s '[.[] | select(.type == "ApiUsage")] | {first: .[0].timestamp, last: .[-1].timestamp, count: length}' logs/<session>.jsonl
```

## Debugging Queries

```bash
# Find failed tool results
jq 'select(.type == "ToolResult" and .success == false)' logs/<session>.jsonl

# Get specific event by ID
jq 'select(.id == "<event-id>")' logs/<session>.jsonl

# Last N events (most recent activity)
jq -cs '.[-10:][] | {type, timestamp, tool_name}' logs/<session>.jsonl

# Errors only
jq 'select(.type == "Error")' logs/<session>.jsonl
```

## Context Recovery (Conversation Archaeology)

Recover lost conversation context from session logs. Useful after `/compact`, session expiry, or when resuming work.

**Decision Tree:**
```
Need to recover context?
├── Do you know the session file?
│   ├── YES → Use that file directly
│   └── NO → `ls -lt logs/ | head -5` to find recent sessions
│
├── Do you have a specific keyword?
│   ├── YES → Use grep + jq pipeline below
│   └── NO → Browse with schema discovery queries first
│
└── Is the log file huge (>50MB)?
    ├── YES → Always grep FIRST to filter, then pipe to jq
    └── NO → Can use jq directly (but grep is still faster)
```

**Key Insight:** Session logs contain full API request bodies, including the complete `messages` array sent to Claude. Your prompts are preserved verbatim.

```bash
# Find what you said about a topic
grep -i "keyword" logs/<session>.jsonl | \
  jq -r 'select(.type == "Request") | .body.messages[]? |
    select(.role == "user") | .content[]? |
    select(.type == "text") | .text' 2>/dev/null | \
  grep -i "keyword" | head -10

# Example: Recover theme design direction
grep -i "solarized" logs/<session>.jsonl | \
  jq -r 'select(.type == "Request") | .body.messages[]? |
    select(.role == "user") | .content[]? |
    select(.type == "text") | .text' 2>/dev/null | \
  grep -i "solarized"

# Find Claude's responses on a topic (assistant messages)
grep -i "keyword" logs/<session>.jsonl | \
  jq -r 'select(.type == "Response") | .body.content[]? |
    select(.type == "text") | .text' 2>/dev/null | \
  grep -i "keyword" | head -10
```

**Why grep first?** A single API request can be 500KB+ (system prompts, full context). Grep filters to relevant lines before jq parses JSON.

**Structure reference:**
- `Request.body.messages[]` → Array of conversation turns
- `.role` → "user" or "assistant"
- `.content[]` → Array of content blocks (text, tool_use, tool_result, etc.)
- `.content[].type` → "text", "tool_use", "tool_result", "image", etc.

## Schema Discovery

Understand the structure of logged events:

```bash
# Get all event type schemas (field names per type)
jq -s 'group_by(.type) | map({type: .[0].type, fields: (.[0] | keys)})' logs/<session>.jsonl

# Files read during session (most accessed first)
jq -r 'select(.type == "ToolCall" and .tool_name == "Read") | .input.file_path' logs/<session>.jsonl | sort | uniq -c | sort -rn

# Tool execution times (reveals human-in-the-loop delays for Edit/Write)
jq -s '[.[] | select(.type == "ToolResult")] | group_by(.tool_name) | map({tool: .[0].tool_name, avg_ms: ((map(.duration.secs * 1000 + .duration.nanos / 1000000) | add) / length | floor), count: length})' logs/<session>.jsonl

# Thinking block stats
jq -s '[.[] | select(.type == "Thinking")] | {count: length, total_tokens: (map(.token_estimate) | add), avg_tokens: ((map(.token_estimate) | add) / length | floor)}' logs/<session>.jsonl
```

## Comprehensive Session Summary

The power query - full session profile in one command:

```bash
jq -s '
{
  session: {
    first: ([.[] | select(.type == "Request")][0].timestamp),
    last: ([.[] | select(.type == "Response")][-1].timestamp),
    events: length
  },
  models: ([.[] | select(.type == "ApiUsage") | .model] | group_by(.) | map({model: .[0], calls: length})),
  tokens: {
    input: ([.[] | select(.type == "ApiUsage") | .input_tokens] | add),
    output: ([.[] | select(.type == "ApiUsage") | .output_tokens] | add),
    cached: ([.[] | select(.type == "ApiUsage") | .cache_read_tokens] | add),
    cache_pct: ((([.[] | select(.type == "ApiUsage") | .cache_read_tokens] | add) / (([.[] | select(.type == "ApiUsage") | .input_tokens] | add) + ([.[] | select(.type == "ApiUsage") | .cache_read_tokens] | add))) * 100 | floor)
  },
  tools: ([.[] | select(.type == "ToolCall") | .tool_name] | group_by(.) | map({tool: .[0], calls: length})),
  thinking: {
    blocks: ([.[] | select(.type == "Thinking")] | length),
    tokens: ([.[] | select(.type == "Thinking") | .token_estimate] | add)
  },
  health: {
    requests: ([.[] | select(.type == "Request")] | length),
    responses: ([.[] | select(.type == "Response")] | length),
    failures: ([.[] | select(.type == "ToolResult" and .success == false)] | length),
    errors: ([.[] | select(.type == "Error")] | length)
  }
}
' logs/<session>.jsonl
```

## Typical Session Profile

A healthy Claude Code session looks like:
- **Cache ratio:** 95-99% (context is heavily cached)
- **Model split:** ~50/50 Haiku/Opus (Haiku for quick tasks, Opus for reasoning)
- **Tool distribution:** Read-heavy for research, Edit-heavy for implementation

Example output from a 3.5 hour research session:
```
Duration:     ~3.5 hours
API calls:    79
Model split:  53% Haiku, 47% Opus
Cache ratio:  98.2%
Total tokens: ~1.56M (1.5M cached)
Tool calls:   29 (Read-heavy)
```
