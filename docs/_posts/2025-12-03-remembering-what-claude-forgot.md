---
layout: post
title: "Remembering What Claude Forgot"
date: 2025-12-03
tags: [architecture, feature]
---

Long Claude Code sessions are wonderful until they aren't.

You're deep into a complex refactor, decisions stacking on decisions, and suddenly Claude starts asking questions you answered two hours ago. *"What theme system did we decide on?"* The one we spent 45 minutes discussing. The one you chose specifically because it matched your existing workflow. Gone.

Context compaction happened.

## The jq Archaeology Workflow

For a while, my solution was decidedly manual. Every time Claude forgot something critical, I'd drop into the terminal:

```bash
jq 'select(.type == "Thinking") | select(.content | contains("solarized"))' \
  logs/session-xyz.jsonl
```

Find the thinking block where we discussed themes. Copy the relevant bits. Paste them back into the conversation. Claude would read its own past reasoning and suddenly everything clicked again.

It worked. It was also tedious.

But here's the thing—it *validated* that the solution existed in the data. Every insight, every decision, every nuance of our collaboration was sitting right there in those JSONL files. The problem wasn't missing information. The problem was retrieval.

## Enter the Event Pipeline

The new architecture introduces an extensible event processing pipeline. Every event that flows through Aspy—tool calls, thinking blocks, API usage, user prompts—now passes through a chain of processors before reaching its destinations.

```
ProxyEvent → EventPipeline → [Processor₁, Processor₂...] → send_event()
```

Processors can:
- **Transform**: Modify events (redaction, enrichment)
- **Filter**: Drop events (by type, content, or condition)
- **Side-effect**: React without modification (storage, metrics, webhooks)

The pipeline itself is kernel-level infrastructure. But the interesting processors—that's userland. Toggleable via config. Non-invasive to the core.

## Lifestats: The Memory Layer

The first processor to ship is `LifestatsProcessor`. It writes events to a SQLite database optimized for one thing: context recovery queries.

Not just storage. *Queryable* storage.

The schema includes:
- **Thinking blocks** with full-text search (FTS5)
- **User prompts** with full-text search
- **Tool calls** with duration tracking
- **API usage** with cost calculation
- **Session boundaries** with aggregated statistics

All indexed. All searchable. All sitting in `./data/lifestats.db` ready to be queried.

```sql
SELECT content, timestamp
FROM thinking_fts
WHERE thinking_fts MATCH '"theme" AND "solarized"'
ORDER BY bm25(thinking_fts)
LIMIT 5;
```

That jq archaeology workflow? Now it's a millisecond query.

## The Two-Agent Pattern

Here's where it gets interesting.

The lifestats database exposes itself through MCP tools. Claude can query its own past context directly—no terminal diving, no copy-paste, no human in the middle.

But there's a subtlety to how this works best.

Searching through past conversations is a *different* task than synthesizing meaning from those conversations. Search is fast, cheap, and tolerates multiple attempts. Synthesis requires deep reasoning. Mixing them is inefficient.

So we split the work:

```
User Query → Haiku Agent (search + filter)
                 ↓ structured results
             Opus Agent (read full + synthesize)
```

Haiku handles the retrieval. It takes your fuzzy query—*"what did we decide about error handling?"*—and executes parallel searches with different keywords. It filters, ranks, and returns structured matches: session IDs, timestamps, content previews.

Then Opus reads the full context and does what it does best: understand.

The result is context recovery that feels almost magical. You ask Claude about something from three sessions ago, and it *remembers*—not because the context window held that information, but because it queried its own past.

## Non-Blocking by Design

One architectural decision worth highlighting: the storage processor uses a dedicated OS thread, not a tokio task.

SQLite doesn't play well with async runtimes. Blocking I/O in an async context is a recipe for latency spikes. So the processor sends events to a bounded channel, and a separate thread handles batched writes with WAL mode enabled.

The pipeline never blocks. Events continue flowing. If the writer falls behind, we track backpressure metrics rather than silently dropping data.

```rust
fn process(&self, event: &ProxyEvent, ctx: &ProcessContext) -> ProcessResult {
    match self.tx.try_send(WriterCommand::Store(event.clone(), ctx.clone())) {
        Ok(()) => { /* queued */ }
        Err(TrySendError::Full(_)) => {
            self.metrics.events_dropped.fetch_add(1, Ordering::Relaxed);
            tracing::warn!("Lifestats backpressure: dropped event");
        }
        // ...
    }
    ProcessResult::Continue  // Always pass through
}
```

Observability tools should be invisible. They shouldn't introduce latency. They definitely shouldn't lose your data without telling you.

## The Bigger Picture

This isn't just about recovering from context compaction—though that's the immediate motivation.

It's about building a *memory layer* for AI-assisted development. Your coding sessions leave traces. Decisions accumulate. Patterns emerge. With lifestats, that history becomes queryable infrastructure.

Want to know which tools are slowest across all your sessions? Query it.

Want to track your API costs over the past month? Query it.

Want to find every time you discussed "authentication" in any session ever? Query it.

The JSONL files remain—they're the source of truth, the raw observability layer. The SQLite storage is an optimization layer on top, designed for the queries that matter most.

## What's Next

The lifestats system ships in phases:

1. **Core pipeline** - Event processing infrastructure
2. **Storage foundation** - SQLite with FTS5 and connection pooling
3. **Query interface** - HTTP API and MCP tools
4. **Agent layer** - Recovery and analysis workflows

The groundwork is laid. The architecture is solid. Now comes the implementation.

I'm particularly excited about the agent workflows. There's something poetic about Claude querying its own past reasoning to inform its current decisions. A kind of artificial introspection.

The spy remembers everything. Now Claude can too.
