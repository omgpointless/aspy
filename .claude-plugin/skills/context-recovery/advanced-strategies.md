# Advanced Context Recovery Strategies

Read this when basic searches aren't finding what you need.

## Hybrid vs FTS-Only

**Hybrid search** (`aspy_recall`) should be your first choice:
- Combines semantic embeddings (understands meaning) with FTS5 (matches keywords)
- Finds results even when terminology differs from what you're searching
- Uses Reciprocal Rank Fusion (RRF) to merge both result sets

**Fall back to FTS-only** (`aspy_recall_thinking`, `aspy_recall_prompts`, `aspy_recall_responses`) when:
- Embeddings aren't available or not yet indexed
- You need exact phrase matching with specific operators
- Debugging why certain results aren't appearing

## The Banana Problem

If the user previously asked "search for banana", that meta-question is now in the logs. Searching for "banana" will match both:
1. The actual banana discussion (signal)
2. The "search for banana" request (noise)

**Mitigation:** Look for structural signals in results:
- Code references (`file.rs:123`, function names)
- Action language ("implemented", "fixed", "decided")
- Technical specifics (versions, configs, error messages)

Results with these markers are more likely to be substantive.

## Time-Based Filtering

Use `time_range` parameter when you know roughly when something happened:
- `"today"` - Current day only
- `"last_3_days"` - Recent work
- `"last_7_days"` - This week
- `"last_30_days"` - This month

## Multi-Keyword Strategies

**Phrase mode (default):** Exact phrase match
```
topic: "mouse scroll modal"  // Finds exact phrase
```

**Natural mode:** OR-style, any keyword
```
topic: "scroll OR mouse OR modal", mode: "natural"
```

**Raw FTS5 mode:** Full control
```
topic: "scroll NEAR/5 modal", mode: "raw"  // Within 5 words
```

## When Combined Search Is Noisy

Split into targeted searches:

1. **Search thinking first** - Claude's reasoning often has the most context
2. **Search prompts** - What did the user actually ask?
3. **Search responses** - What did Claude say/implement?

Cross-reference results to find the full picture.

## Session Filtering

If you know which session to search:
```
aspy_search(keyword: "topic", session: "partial-session-id")
```

Partial matches work - use first few characters of session ID.

## Recovery from Nothing

If searches return empty:
1. Ask user for ANY keyword they remember
2. Try broader terms (component names, file names)
3. Check if Aspy proxy was running during that session
4. Consider the work might predate cortex storage
