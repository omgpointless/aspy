---
layout: default
title: Request Transformers
nav_order: 6
description: "Modify API requests before forwarding - edit system-reminders, inject context"
---

# Request Transformers

Transform API requests **before** they are forwarded to the provider. Use transformers to edit `<system-reminder>` tags, inject custom context, or modify message content.

## Quick Start

Add to your `~/.config/aspy/config.toml`:

```toml
[transformers]
enabled = true  # Master switch - MUST be true for any transformer to run

[transformers.system-reminder-editor]
enabled = true

# Remove reminders containing "debug" or "noisy"
[[transformers.system-reminder-editor.rules]]
type = "remove"
pattern = "debug|noisy"

# Add custom context at the end of the last user message
[[transformers.system-reminder-editor.rules]]
type = "inject"
content = "Always respond in formal English."
position = "end"
```

## Architecture

```
Request → [Transformation Pipeline] → [Translation Pipeline] → Provider
              ↓
         SystemReminderEditor
         (more transformers planned)
```

Transformers run **before** translation, when the request is in known Anthropic format. This ensures consistent behavior regardless of target provider format.

## System Reminder Editor

Edits `<system-reminder>` tags in user messages. Claude Code uses these tags to inject contextual information.

### Rule Types

| Type | Purpose | Example |
|------|---------|---------|
| `inject` | Add new `<system-reminder>` content | Custom instructions |
| `remove` | Delete reminders matching a pattern | Filter noise |
| `replace` | Modify content within reminders | Update URLs |

Rules are applied in order: **Remove → Replace → Inject**

### Inject Rules

Add new `<system-reminder>` blocks:

```toml
[[transformers.system-reminder-editor.rules]]
type = "inject"
content = "Remember to use TypeScript for all code examples."
position = "end"  # Where to insert (default: end)
```

**Position options:**
- `"start"` - Before all content in the last user message
- `"end"` - After all content (default)
- `{ before = { pattern = "regex" } }` - Before first reminder matching pattern
- `{ after = { pattern = "regex" } }` - After last reminder matching pattern

**Multiline content:**
```toml
[[transformers.system-reminder-editor.rules]]
type = "inject"
content = """
Important context:
- Use TypeScript
- Follow project conventions
"""
position = "start"
```

### Remove Rules

Delete reminders whose content matches a regex pattern:

```toml
[[transformers.system-reminder-editor.rules]]
type = "remove"
pattern = "debug|verbose|noisy"  # Regex pattern
```

**What does `pattern = "debug|noisy"` mean?**

It's a regular expression. This removes any `<system-reminder>` block whose content contains "debug" OR "noisy".

Examples of content that would be removed:
- `<system-reminder>Debug mode enabled</system-reminder>`
- `<system-reminder>This is noisy output</system-reminder>`
- `<system-reminder>Some debugging information here</system-reminder>`

### Replace Rules

Modify content within matching reminders:

```toml
[[transformers.system-reminder-editor.rules]]
type = "replace"
pattern = "old-api\\.example\\.com"  # Note: escape dots in regex
replacement = "new-api.example.com"
```

Supports regex capture groups:
```toml
[[transformers.system-reminder-editor.rules]]
type = "replace"
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

### System Reminder Editor

```toml
[transformers.system-reminder-editor]
enabled = true  # Enable this specific transformer

[[transformers.system-reminder-editor.rules]]
# Rule configuration (see above)
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

### Filter Noisy Claude Code Reminders

```toml
[[transformers.system-reminder-editor.rules]]
type = "remove"
pattern = "git status|codebase structure"
```

### Inject Project-Specific Context

```toml
[[transformers.system-reminder-editor.rules]]
type = "inject"
content = "This project uses Rust 2021 edition with async/await patterns."
position = "start"
```

### Standardize URLs

```toml
[[transformers.system-reminder-editor.rules]]
type = "replace"
pattern = "docs\\.old\\.com"
replacement = "docs.new.com"
```

## Future Transformers

Planned additions:
- **ContextEnricher** - Inject RAG context from embeddings
- **ModelRouter** - Route requests based on content/model
- **ContentFilter** - Block requests matching policy rules

## Implementation Notes

- Transformers run synchronously (async prep in handler if needed)
- Uses `Cow<'a, Value>` for zero-copy passthrough when unchanged
- Implements `RequestTransformer` trait for extensibility
