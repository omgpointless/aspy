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
         SystemEditor (system prompt modification)
         CompactEnhancer (compaction guidance)
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

## System Editor

Modifies the `system` field in Claude API requests. Use this to append, prepend, or replace content in system prompts—useful for adding global context, branding, or modifying Claude Code's base behavior.

### Rule Types

| Type | Purpose | Example |
|------|---------|---------|
| `append` | Add text to the end of the last system block | Augmentation notices |
| `prepend` | Add text to the beginning of the first system block | Priority instructions |
| `replace` | Find and replace text in all system blocks | Update references |

Rules are applied in order as defined in config.

### Configuration

```toml
[transformers]
enabled = true

[transformers.system-editor]
enabled = true

# Append a notice to the system prompt
[[transformers.system-editor.rules]]
type = "append"
content = "\n\nYou are augmented by Aspy observability."

# Prepend priority instructions
[[transformers.system-editor.rules]]
type = "prepend"
content = "[ENHANCED MODE] "

# Replace references (regex supported)
[[transformers.system-editor.rules]]
type = "replace"
pattern = "Claude Code"
replacement = "Claude Code (Aspy-enhanced)"
```

### Append and Prepend

**Append** adds content to the **last** text block in the system array:

```toml
[[transformers.system-editor.rules]]
type = "append"
content = """

## Aspy Active
Recovery tools available via aspy_recall.
"""
```

**Prepend** adds content to the **first** text block:

```toml
[[transformers.system-editor.rules]]
type = "prepend"
content = "[Session tracked by Aspy] "
```

### Replace with Regex

The `replace` rule uses regex patterns and applies to **all** system blocks:

```toml
# Update version references
[[transformers.system-editor.rules]]
type = "replace"
pattern = "v\\d+\\.\\d+\\.\\d+"
replacement = "v2.0.0"

# Capture groups work
[[transformers.system-editor.rules]]
type = "replace"
pattern = "(Claude) (Code)"
replacement = "$1 $2 (enhanced)"
```

### String vs Array System Format

The System Editor handles both system prompt formats:

- **String format:** `"system": "You are Claude."`
- **Array format:** `"system": [{"type": "text", "text": "You are Claude."}]`

String format is automatically converted to array format when modified, then the rules are applied.

### Use Cases

**Add global context:**
```toml
[[transformers.system-editor.rules]]
type = "append"
content = "\n\nThis project uses Rust 2021 edition with async/await patterns."
```

**Branding/identification:**
```toml
[[transformers.system-editor.rules]]
type = "prepend"
content = "[MyCompany Dev Environment] "
```

**Update outdated references:**
```toml
[[transformers.system-editor.rules]]
type = "replace"
pattern = "api\\.old-domain\\.com"
replacement = "api.new-domain.com"
```

---

## Compact Enhancer

Detects Anthropic's compaction prompts and enhances them with continuity guidance. When Claude Code's context window fills up, Anthropic sends a special prompt asking Claude to summarize the conversation. This transformer appends instructions to improve what gets preserved.

### How It Works

1. **Detection** - Multi-signal detection identifies compaction requests:
   - Primary signal: "summary of the conversation" phrase (required)
   - Structural markers: Section headers like "Primary Request", "Pending Tasks" (2+ required)

2. **Injection** - Appends continuity guidance to the compaction prompt:
   - Prompts for active work tracks, key decisions, current mental model
   - Suggests searchable keywords for post-compaction recovery
   - Mentions `aspy_recall` for context lookup

### Configuration

```toml
[transformers]
enabled = true

[transformers.compact-enhancer]
enabled = true
```

### What Gets Injected

When a compaction request is detected, this text is appended:

```markdown
## Aspy Continuity Enhancement

**For the summary:** To help the continuing Claude maintain flow, please include:
- **Active Work Tracks:** What features/bugs/tasks are in progress (with file paths if relevant)
- **Key Decisions Made:** Important choices that shouldn't be revisited
- **Current Mental Model:** The user's goals and approach being taken

**Post-compaction recovery:** The continuing Claude has `aspy_recall` to search
the full pre-compaction conversation. Include 3-5 searchable keywords (feature names, concepts,
file paths) that would help locate detailed context.
```

### Why This Matters

Without guidance, compaction summaries often lose:
- File paths and specific locations
- The "why" behind decisions
- Current work direction and momentum

The CompactEnhancer nudges the summarizing Claude to preserve what the continuing Claude actually needs.

---

## Future Transformers

Planned additions:
- **ContextEnricher** - Inject RAG context from embeddings
- **ModelRouter** - Route requests based on content/model
- **ContentFilter** - Block requests matching policy rules
- **CostEstimator** - Detect expensive operations, inject warnings

## Future Conditions

Planned `when` conditions:
- `context_percent` - Trigger based on context window usage (e.g., `">90"`)
- `session_duration` - Time-based triggers

## Implementation Notes

- Transformers run synchronously (async prep in handler if needed)
- Uses `Cow<'a, Value>` for zero-copy passthrough when unchanged
- Implements `RequestTransformer` trait for extensibility
- Remove/Replace rules scan ALL text blocks; Inject only applies to the last text block
