# POC: Context Recovery Agent with Haiku

**Date:** 2025-12-02
**Status:** ✅ PASSED
**Session:** anthropic-spy-20251202-*

---

## Summary

Successfully validated the two-agent pattern for context recovery using Haiku for search orchestration and Opus for synthesis. The agent (`/aspy:recover`) successfully found historical context ("creamy yummy white" theme description from Nov 29) but revealed UX challenges with keyword-based queries.

---

## What Worked

### 1. Two-Agent Pattern
- **Haiku (search)** → fast, cheap, parallel queries
- **Opus (synthesis)** → reads full context, understands nuance
- Division of labor validated: "find vs understand"

### 2. Signal Strength Ranking
- Agent successfully classified results as HIGH/MODERATE/LOW signal
- Correctly identified meta-discussions (documentation examples) as LOW signal
- Prioritized technical implementations (code commits, config changes) as HIGH signal

### 3. Progressive Search Strategy
- Phase 1: Narrow (5 results, role filter, recent time range)
- Phase 2: Expand (10 results, broader keywords, longer time range)
- Successfully found target with temporal expansion

### 4. Real Recovery Success
**Test case:** "Find theme discussion about solarized and creamy vibe from Nov 29"

**Result found:**
> "That **creamy yummy white**. Soft text. Pleasant pastel for accents... 90s mIRC vibes... it's actually fine... we did it!"

Agent correctly surfaced the original user description including:
- Key aesthetic phrase ("creamy yummy white")
- Design goals (soft text, pleasant pastels)
- Anti-patterns (hated "vomit green")
- Emotional context (nostalgic, satisfied)

---

## What Didn't Work

### 1. Keyword-Mode UX Issue

**User's first attempt:**
`"Spy Dark theme, solarized, cream or creamy"`

**Agent interpreted as:** Multiple separate recent topics
**User intended:** Keywords from a single historical discussion

**Problem:** Users invoke agents like CLI commands (keyword dump), not semantic queries. The agent is optimized for semantic/conversational input but receives terse keyword lists.

**Impact:** First attempt returned:
- Spy Dark implementation commits (recent, correct for that keyword)
- Material Sandy Beach "cream" theme (recent, different conversation)
- Solarized in documentation examples (meta-discussion, noise)

Missed the actual Nov 29 conversation because:
- No temporal hint provided
- Keywords treated as separate concepts
- Recent time range (last_7_days) excluded target date

### 2. Temporal Scope Heuristics

**Issue:** Agent defaults to `last_7_days` without semantic context clues.

**Failure case:** User provides keywords without "recently", "last week", etc. Agent should try broader time ranges in Phase 2, but only did so after explicit clarification.

**Potential fix:** If Phase 1 results are weak/meta-heavy, automatically expand to `last_30_days` in Phase 2.

### 3. Noise Levels in Current JSONL Search

**Reality check:** JSONL keyword search with 500-char context windows hits pollution easily:
- Recursive discussions about searching
- Documentation examples using same keywords
- Meta-references to past conversations

**Mitigation (current):** Signal strength ranking helps but isn't perfect
**Mitigation (future):** SQLite with FTS5 + semantic indexing will dramatically improve

---

## UX Discovery: Command Mode vs Conversational Mode

When users invoke `/aspy:recover`, they mentally switch to **command mode**:

```bash
# What user is thinking:
$ aspy-search "keyword1" "keyword2" "keyword3"

# Not thinking:
"Hey agent, find the conversation where I described..."
```

**This is critical for agent design:**
- Users will keyword dump (terse, multiple terms)
- Agent must interpret as OR search across concepts
- Agent should try multiple temporal scopes if initial results weak
- Agent needs to detect "no good matches" and expand automatically

---

## Architecture Validation

### Component: `.claude-plugin/agents/recover.md`
- **Model:** Haiku (fast, cheap)
- **Tools:** `mcp__plugin_aspy_aspy__aspy_search`, `aspy_events`
- **Workflow:** Two-phase progressive search with signal ranking
- **Status:** ✅ Working, needs UX refinement

### Integration with Existing MCP Tools
- Used `aspy_search` with parameters: `keyword`, `role`, `time_range`, `limit`
- Tool returned structured results (session, timestamp, role, text)
- Agent successfully filtered and ranked results

### Cost Efficiency (Theoretical)
- Haiku tokens: ~2K-5K per recovery attempt (search + filter + rank)
- Opus tokens: Only reads filtered results (~1K-3K for synthesis)
- **vs naive approach:** Opus reading all search results = 10K+ tokens

**Note:** Actual cost measurement pending (see Next Steps)

---

## Next Steps

### Immediate (Next Session)

1. **Economic Impact Measurement**
   - **IMPORTANT:** Run next session through Aspy proxy
   - Measure actual token usage for recovery queries
   - Compare Haiku search cost vs Opus direct search cost
   - Track context window impact of agent invocations
   - Validate cost optimization hypothesis (Haiku << Opus for search)

2. **UX Improvements for Keyword Mode**
   - Detect terse keyword input (3-5 words, no sentence structure)
   - Treat as OR search across concepts, not AND search
   - Automatically try broader time ranges if Phase 1 weak
   - Add explicit feedback: "Searched recent (last 7 days), found X. Try older?"

### Short-Term (Phase 1-2 Complete)

3. **Skills System Integration**
   - Explore https://code.claude.com/docs/en/skills for agent capabilities
   - More targeted search strategies (e.g., "find decisions", "find implementations")
   - Compositional skills for complex recovery workflows

4. **Storage Pipeline Impact**
   - Once SQLite backend deployed (Phase 1-2), re-test agent
   - SQLite + FTS5 = better relevance, less pollution
   - Semantic indexing possibilities (Phase 3+)
   - Direct event retrieval by ID (no 500-char truncation)

### Long-Term (Phase 3+)

5. **Additional Agents**
   - `profile.md` - Session profiler (Haiku, numeric analysis)
   - `cost.md` - Cost analyzer (Haiku, budget tracking)
   - Skills-based specialized agents (TBD based on storage capabilities)

6. **Enhanced MCP Tools**
   - `aspy_get_events` - Direct event fetch by ID
   - `aspy_recall_context` - Combined thinking + prompt search
   - `aspy_session_history` - Timeline view

---

## Key Learnings

### 1. Base Hygiene (80% Solution)
- ✅ Role-based filtering (user vs assistant intent)
- ✅ Time-range scoping (recency bias reduces pollution)
- ✅ Conservative limits (5→10, not 10→50)
- ⚠️ Signal ranking (works, but not foolproof with current noise)
- ❌ Phrase blacklists (too brittle, avoided)

### 2. The "Banana Problem"
**User insight:** "If I ask you to search for 'banana', that question itself gets logged. Future searches for 'banana' now match the question about searching for 'banana'."

**Implication:** Pollution is recursive and unavoidable with JSONL keyword search. The solution isn't better filtering—it's better indexing (SQLite + semantic analysis in Phase 1-3).

### 3. Structural Signals > Phrase Blacklists
**What works:** Detect presence of:
- Code references (file:line, function names)
- Action words ("implemented X", "decided on X")
- Technical specifics (versions, configs, errors)

**What doesn't work:** Blacklisting phrases like "for example", "you can", etc. (too brittle, misses nuance)

### 4. Agent Invocation ≠ Conversation
Users treat agent invocation as a command, not dialogue. This is a UX paradigm shift that wasn't obvious until testing.

---

## Test Case Archive

### Test 1: Multi-keyword Dump (Failed → Passed with refinement)

**Input 1 (keyword mode):**
`"Spy Dark theme, solarized, cream or creamy"`

**Results:**
- Found: Recent Spy Dark commits, Material Sandy Beach "cream", solarized in docs
- Missed: Nov 29 theme discussion
- Reason: No temporal context, treated as separate topics

**Input 2 (semantic mode):**
`"A few days ago, the 29th, creating a theme, solarized, creamy vibe, how I described it"`

**Results:**
- Found: Original Nov 29 conversation
- Key phrase: "creamy yummy white"
- Full context: Design goals, anti-patterns, emotional reaction
- Success: Proper temporal scoping + intent clarity

**Lesson:** Agent needs to handle both modes or guide users toward semantic queries.

---

## Files Modified

- `.claude-plugin/agents/recover.md` - Created Haiku context recovery agent
- `.claude/impl-plans/event-pipeline-and-storage.md` - Updated Phase 3 with agent layer
- `.claude/impl-plans/agent-context-recovery-poc.md` - This document

---

## Success Criteria Met

- ✅ POC demonstrates viability of two-agent pattern
- ✅ Agent successfully recovered historical context
- ✅ Signal ranking reduces (but doesn't eliminate) pollution
- ✅ Cost optimization hypothesis validated conceptually (needs measurement)
- ✅ Identified clear path forward (storage pipeline + skills)

**Status:** Ready to proceed with Phase 1 implementation (event pipeline + SQLite storage). Current agent provides MVP UX for context recovery with known limitations.

---

## Open Questions

1. **Economic validation:** What's the actual Haiku vs Opus cost differential in production?
2. **Context impact:** How much context window does agent orchestration consume?
3. **Skills integration:** Can skills improve targeted search beyond current implementation?
4. **Semantic indexing:** Will SQLite + plugin enrichment eliminate need for signal ranking?

**Next session should answer #1 and #2 by running through proxy.**
