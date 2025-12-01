# Temporal Context Export for Compact Survival

Gather recent relevant tangents and context to ensure a smooth transition through `/compact` without losing continuity or the user's current direction.

## Instructions

1. **Analyze the recent conversation** for:
   - Active work threads (what's being built/fixed right now)
   - Recent decisions and their rationale
   - Tangents that inform current direction (even if not directly code)
   - User preferences/patterns expressed this session
   - Any "we should..." or "next we'll..." statements

2. **Generate a context file** at:
   ```
   .claude/temporal-context/YYYY-MM-DD-HH-MM-<slug>.md
   ```
   Where `<slug>` is a 2-4 word kebab-case summary (e.g., `theme-light-mode-fixes`)

3. **Use this structure**:

```markdown
# Temporal Context: <Topic>

**Generated:** YYYY-MM-DD HH:MM
**Purpose:** Compact survival - resume without context loss

---

## Active Work Thread

What is actively being worked on RIGHT NOW. Be specific:
- Current task/feature
- Where we left off (file:line if applicable)
- Immediate next step

## Recent Decisions

Key decisions made this session that inform ongoing work:
- Decision → Rationale (brief)

## User Direction & Intent

What the user is trying to achieve (may be broader than current task):
- Stated goals
- Implicit preferences observed
- "Vibe" of the session (exploratory? focused? debugging?)

## Tangents That Matter

Context from side discussions that's relevant:
- Topic → Why it matters for continuation

## Files in Play

Files actively being modified or referenced:
- `path` - what's happening there

## Do NOT Forget

Critical items that must survive compact:
- Unfinished work
- Promised follow-ups
- User-stated priorities

---

## Resume Prompt

[One paragraph: exactly what a post-compact Claude needs to know to continue seamlessly]
```

## After Generating the File

Reply with a recommended `/compact` message in this format:

```
Ready for compact. Recommended message:

/compact Continue the session documented in .claude/temporal-context/<generated-filename>.md - we were [brief current state]. Key context: [1-2 critical points that must survive].
```

## Style Guidelines

- Prioritize RECENT context over session history
- Be terse—this survives compact, not archives
- Capture intent, not just actions
- Include emotional/directional context ("user wants polish, not features")
- Reference the generated file path explicitly in your compact recommendation
