---
description: "Generate context file before /compact to ensure smooth transition after compaction. Use this template to structure your file and provide a recommended compact message to the user."
---

# Temporal Context Export for Compact Survival

Generate a compact survival document using real session data from Aspy.

## Phase 1: Gather Session Data

**FIRST**, spawn the `summarize` agent (Haiku) to collect current session data:

```
Agent: summarize
```

This agent will return structured data including:
- Current todo state (pending/in-progress/completed tasks)
- Key thinking blocks from this session
- Recent tool activity (files being modified)
- Session metrics (tokens, costs, duration)

**Wait for the agent to return before proceeding to Phase 2.**

## Phase 2: Generate Context File

Using the data from Phase 1 **combined with your own conversation context**, generate a context file at:
```
.claude/temporal-context/YYYY-MM-DD-HH-MM-<slug>.md
```

Where `<slug>` is a 2-4 word kebab-case summary (e.g., `plugin-summarize-agent`)

### Template Structure

```markdown
# Temporal Context: <Topic>

**Generated:** YYYY-MM-DD HH:MM
**Purpose:** Compact survival - resume without context loss

---

## Active Work Thread

What is actively being worked on RIGHT NOW. Be specific:
- Current task/feature (from todo state)
- Where we left off (file:line if applicable)
- Immediate next step

## Recent Decisions

Key decisions made this session that inform ongoing work:
- Decision → Rationale (extract from thinking blocks)

## User Direction & Intent

What the user is trying to achieve (may be broader than current task):
- Stated goals
- Implicit preferences observed
- "Vibe" of the session (exploratory? focused? debugging?)

## Files in Play

Files actively being modified or referenced (from tool activity):
- `path` - what's happening there

## Do NOT Forget

Critical items that must survive compact:
- Unfinished work (from pending todos)
- Promised follow-ups
- User-stated priorities

---

## Resume Prompt

[One paragraph: exactly what a post-compact Claude needs to know to continue seamlessly]

## Recovery Keywords

[3-5 searchable terms for `aspy_recall` post-compact: feature names, file paths, concepts]
```

## Phase 3: Provide Compact Recommendation

Reply with a recommended `/compact` message:

```
Ready for compact. Recommended message:

/compact Continue the session documented in .claude/temporal-context/<generated-filename>.md - we were [brief current state]. Key context: [1-2 critical points that must survive].
```

## Style Guidelines

- **Ground in Aspy data** - Use todos, thinking blocks, and tool activity as the factual backbone
- **Augment with conversation context** - Fill gaps the data misses: user intent, promises, session vibe
- The thinking blocks contain Claude's reasoning; interpret them to extract decisions
- Prioritize RECENT context over session history
- Be terse—this survives compact, not archives
- Capture intent, not just actions
- Include recovery keywords for post-compact `aspy_recall` searches

## What Goes Where

| Source | Best For |
|--------|----------|
| `aspy_todos` | Active Work Thread, Do NOT Forget |
| `aspy_events(Thinking)` | Recent Decisions, reasoning behind choices |
| `aspy_events(ToolCall)` | Files in Play |
| Conversation analysis | User Direction & Intent, Tangents That Matter, session vibe |
