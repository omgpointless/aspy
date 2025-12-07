---
name: context-recovery
description: >
  Recover lost context after session compaction or when information from
  previous sessions is needed. Use when: user mentions "what were we working on",
  "I lost context", "before the compact", "previous session", or asks about
  decisions/implementations/discussions that aren't in current context.
  Also use proactively when you notice references to prior work you lack context for.
allowed-tools: Read, Grep, mcp__plugin_aspy_aspy__aspy_recall, mcp__plugin_aspy_aspy__aspy_recall_thinking, mcp__plugin_aspy_aspy__aspy_recall_prompts, mcp__plugin_aspy_aspy__aspy_recall_responses
---

# Context Recovery

You've been activated to recover context that was lost to compaction or exists in a previous session.

## Quick Start

1. **Identify the topic** - What specific context is needed?
   - If the user's request is vague, ask: "What topic should I search for?"

2. **Use aspy_recall** (primary tool):
   ```
   aspy_recall(query="<keywords>", limit=10)
   ```
   This combines semantic search (if embeddings enabled) with keyword matching.
   Searches thinking blocks, user prompts, AND assistant responses simultaneously.
   Handles both exact queries and fuzzy queries like "that golf thing?"

3. **Synthesize, don't dump** - Summarize findings:
   - What was decided or implemented
   - Key file paths and line numbers mentioned
   - Any unfinished work or next steps discussed

4. **Offer continuity** - "Would you like me to continue where we left off?"

## Search Strategy

### Start with aspy_recall (Primary)
- Combines semantic + keyword search automatically
- Finds conceptually related content even with different wording
- Default limit of 10 results is usually sufficient

### Targeted Searches (If Combined Is Noisy)
- `aspy_recall_thinking` - Claude's reasoning and analysis (WHY decisions were made)
- `aspy_recall_prompts` - What the user asked
- `aspy_recall_responses` - Claude's answers and code

## What Makes Good Context Recovery

**Good synthesis:**
> "On Dec 2nd, we implemented mouse scroll support for the detail modal.
> The fix was in `src/tui/mod.rs:299-322` - checking if modal is open
> before dispatching scroll events. You mentioned wanting to test it
> before merging."

**Bad synthesis:**
> "Found 5 results mentioning 'scroll'. Here they are: [dumps raw results]"

## Common Patterns

| User Says | Search For |
|-----------|------------|
| "that bug we fixed" | error keywords, "fix", file names |
| "the refactor" | "refactor", component names |
| "what we decided" | "decided", "approach", "pattern" |
| "before compact" | recent topics from today |
| "something about golf?" | just search it - semantic will handle fuzzy |
