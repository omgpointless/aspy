---
layout: default
title: Request Transformers
nav_order: 6
description: "Modify API requests before forwarding - edit XML tags, inject context conditionally"
---

# Request Transformers

Transform API requests **before** they are forwarded to the provider. Use transformers to edit XML-style tags (`<system-reminder>`, `<aspy-context>`, custom tags), inject context conditionally, or modify message content.

## Quick Start

Add to your `~/.config/aspy/config.toml`:

```toml
[transformers]
enabled = true  # Master switch - MUST be true for any transformer to run

[transformers.tag-editor]
enabled = true

# Remove reminders containing "debug" or "noisy"
[[transformers.tag-editor.rules]]
type = "remove"
tag = "system-reminder"
pattern = "debug|noisy"

# Add custom context (only on conversational turns, not during tool calls)
[[transformers.tag-editor.rules]]
type = "inject"
tag = "aspy-context"
content = "Custom context for Claude."
position = "end"
when = { has_tool_results = "=0" }
```

## Architecture

```
Request → [Transformation Pipeline] → [Translation Pipeline] → Provider
              ↓
         TagEditor (XML tag manipulation)
         (more transformers planned)
```

Transformers run **before** translation, when the request is in known Anthropic format. This ensures consistent behavior regardless of target provider format.

## Tag Editor

Edits any XML-style tags in user messages. Each rule explicitly specifies which tag it targets, allowing fine-grained control over different tag types.

### Rule Types

| Type | Purpose | Example |
|------|---------|---------|
| `inject` | Add new tagged content | Custom instructions, context |
| `remove` | Delete tags matching a pattern | Filter noise |
| `replace` | Modify content within tags | Update URLs |

Rules are applied in order: **Remove → Replace → Inject**

### The `tag` Field

Every rule must specify which XML tag it targets:

```toml
[[transformers.tag-editor.rules]]
type = "inject"
tag = "system-reminder"      # Creates <system-reminder>...</system-reminder>
content = "Your content here"

[[transformers.tag-editor.rules]]
type = "inject"
tag = "aspy-context"         # Creates <aspy-context>...</aspy-context>
content = "Different content"

[[transformers.tag-editor.rules]]
type = "remove"
tag = "noisy-tag"            # Only removes <noisy-tag> blocks
pattern = ".*"
```

### Conditional Execution (`when`)

Rules can have conditions that must be met for the rule to apply:

```toml
[[transformers.tag-editor.rules]]
type = "inject"
tag = "aspy-context"
content = "Context info"
when = { has_tool_results = "=0", turn_number = ">1" }
```

#### Available Conditions

| Condition | Syntax | Description |
|-----------|--------|-------------|
| `turn_number` | `"=1"`, `">5"`, `"<10"`, `">=3"`, `"<=5"`, `"every:3"` | Match conversation turn |
| `has_tool_results` | `"=0"`, `">0"`, `">3"` | Count of tool_result blocks in message |
| `client_id` | `"dev-1"`, `"foundry\|local"` | Match client ID (pipe = OR) |

#### Compound Conditions

Multiple conditions in the same `when` clause are **ANDed** together:

```toml
# Only fires on turn 1, with no tool results, for dev-1 client
when = { turn_number = "=1", has_tool_results = "=0", client_id = "dev-1" }
```

#### Frequency Control

Use `every:N` for periodic injection:

```toml
# Inject every 5th turn (5, 10, 15...)
when = { turn_number = "every:5" }

# Inject every 3rd conversational turn (no tool results)
when = { turn_number = "every:3", has_tool_results = "=0" }
```

### Inject Rules

Add new tagged blocks:

```toml
[[transformers.tag-editor.rules]]
type = "inject"
tag = "system-reminder"
content = "Remember to use TypeScript for all code examples."
position = "end"  # Where to insert (default: end)
```

**Position options:**
- `"start"` - Before all content in the last user message
- `"end"` - After all content (default)
- `{ before = { pattern = "regex" } }` - Before first block matching pattern
- `{ after = { pattern = "regex" } }` - After last block matching pattern

**Multiline content:**
```toml
[[transformers.tag-editor.rules]]
type = "inject"
tag = "aspy-context"
content = """
Important context:
- Use TypeScript
- Follow project conventions
"""
position = "start"
when = { turn_number = "=1" }  # Only on first turn
```

### Remove Rules

Delete tags whose content matches a regex pattern:

```toml
[[transformers.tag-editor.rules]]
type = "remove"
tag = "system-reminder"
pattern = "debug|verbose|noisy"  # Regex pattern
```

**Conditional removal:**
```toml
# Remove "Learning output style" reminder after turn 2
[[transformers.tag-editor.rules]]
type = "remove"
tag = "system-reminder"
pattern = "Learning output style"
when = { turn_number = ">2" }
```

### Replace Rules

Modify content within matching tags:

```toml
[[transformers.tag-editor.rules]]
type = "replace"
tag = "system-reminder"
pattern = "old-api\\.example\\.com"  # Note: escape dots in regex
replacement = "new-api.example.com"
```

Supports regex capture groups:
```toml
[[transformers.tag-editor.rules]]
type = "replace"
tag = "config-tag"
pattern = "version: (\\d+)"
replacement = "version: 2 (was $1)"
```

## Configuration Reference

### Master Switch

```toml
[transformers]
enabled = true  # Required for any transformer to run
```

When `enabled = false`, the entire transformation pipeline is bypassed (zero overhead).

### Tag Editor

```toml
[transformers.tag-editor]
enabled = true  # Enable this specific transformer

[[transformers.tag-editor.rules]]
type = "inject"
tag = "system-reminder"
content = "Your content"
position = "end"
when = { has_tool_results = "=0" }  # Optional conditions
```

## Fail-Safe Guarantee

The transformation pipeline is designed to **never break your requests**:

1. **Errors don't fail requests** - If a transformer errors, the request continues with the original body
2. **One transformer failing ≠ pipeline fails** - Other transformers still run
3. **Worst case = passthrough** - Original unmodified request goes through

## Startup Verification

Check if transformers are active in the startup output:

```
─ Pipeline ─
  ✓ transformers    Request editing
```

If disabled:
```
─ Pipeline ─
  ○ transformers    Request editing (disabled)
```

## Observability

Transformation events are logged via tracing (not ProxyEvents):

```
DEBUG Request transformed by pipeline (pre-translation)
INFO  Request blocked by transformation pipeline: Content policy violation
WARN  Transformation error (continuing with original): ...
```

Set `RUST_LOG=aspy::proxy::transformation=debug` for detailed logs.

## Use Cases

### Inject Context Only During Conversation

```toml
[[transformers.tag-editor.rules]]
type = "inject"
tag = "aspy-context"
content = """
Aspy session active. Recovery tools available after /compact.
"""
when = { has_tool_results = "=0" }  # Skip during tool-heavy turns
```

### First-Turn Orientation

```toml
[[transformers.tag-editor.rules]]
type = "inject"
tag = "system-reminder"
content = "This project uses Rust 2021 edition with async/await patterns."
when = { turn_number = "=1" }  # Only on first turn
```

### Periodic Reminders

```toml
[[transformers.tag-editor.rules]]
type = "inject"
tag = "system-reminder"
content = "Remember to run tests before committing."
when = { turn_number = "every:10", has_tool_results = "=0" }
```

### Filter Noisy Reminders

```toml
[[transformers.tag-editor.rules]]
type = "remove"
tag = "system-reminder"
pattern = "git status|codebase structure"
```

### Client-Specific Rules

```toml
[[transformers.tag-editor.rules]]
type = "inject"
tag = "system-reminder"
content = "You are connected to the development environment."
when = { client_id = "dev-1|dev-2" }  # Pipe = OR
```

## Future Transformers

Planned additions:
- **ContextEnricher** - Inject RAG context from embeddings
- **ModelRouter** - Route requests based on content/model
- **ContentFilter** - Block requests matching policy rules

## Future Conditions

Planned `when` conditions:
- `context_percent` - Trigger based on context window usage (e.g., `">90"`)
- `session_duration` - Time-based triggers

## Implementation Notes

- Transformers run synchronously (async prep in handler if needed)
- Uses `Cow<'a, Value>` for zero-copy passthrough when unchanged
- Implements `RequestTransformer` trait for extensibility
- Remove/Replace rules scan ALL text blocks; Inject only applies to the last text block
