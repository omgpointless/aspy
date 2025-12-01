# Tone & Messaging Guide - anthropic-spy

Quick reference for maintaining consistent voice across all project communications.

---

## The Golden Rule

**Be honest, educational, and technically grounded.**

If you can't back it up with code or explain the Rust concept, don't say it.

---

## Voice Characteristics

### ✅ We Sound Like:
- **A competent developer** learning Rust through building
- **A helpful tool creator** solving an observability need
- **A thoughtful engineer** making deliberate technical choices
- **A transparent learner** honest about the journey

### ❌ We Don't Sound Like:
- An expert Rust developer (not yet!)
- A startup pitch deck (no hype)
- A production-ready product (it's a learning tool)
- Someone pretending they didn't use AI

---

## Word Choice Guide

### Use → Instead Of

| ✅ Use This | ❌ Not This |
|------------|------------|
| "Event-driven architecture with Tokio" | "Revolutionary async design" |
| "Learning project, first Rust implementation" | "Expert-level Rust mastery" |
| "Demonstrates ownership patterns" | "Cutting-edge memory management" |
| "Built to understand Claude Code behavior" | "Production-ready enterprise monitoring" |
| "Phase 3 in progress" | "Feature-complete and battle-tested" |
| "Custom tracing layer for TUI compatibility" | "Revolutionary logging system" |
| "AI-generated code, human-understood" | "Expertly hand-crafted by Rust masters" |
| "Clean implementation" | "World-class engineering" |

---

## Sentence Templates

### For Claims
```
❌ "This is the most advanced Rust TUI ever built"
✅ "This TUI demonstrates Rust async patterns with ratatui"

❌ "Production-ready enterprise observability platform"
✅ "Learning project for Claude Code observability"

❌ "Cutting-edge Rust implementation"
✅ "Uses Tokio for async, ratatui for TUI, and demonstrates event-driven architecture"
```

### For Limitations
```
✅ "SSE streaming support planned for Phase 4"
✅ "First Rust project - learning through building"
✅ "Manual testing only so far - automated tests planned"

❌ "Streaming coming soon™"
❌ "Will support everything eventually"
❌ "No limitations worth mentioning"
```

### For Technical Decisions
```
✅ "Uses Arc<Mutex<>> for shared mutable state across async tasks"
✅ "Custom tracing layer intercepts logs before stdout to prevent TUI breakage"
✅ "Event-driven with mpsc channels for loose coupling between components"

❌ "Revolutionary memory-safe concurrent architecture"
❌ "Enterprise-grade logging infrastructure"
❌ "Advanced async patterns that push Rust to its limits"
```

### For Learning Journey
```
✅ "First serious Rust project - coming from .NET/TypeScript background"
✅ "AI-generated code, but every piece is understood and explainable"
✅ "Learning Rust concepts through building something useful"

❌ "Showcasing expert Rust knowledge"
❌ "Built entirely from scratch by hand"
❌ "Demonstrating advanced Rust mastery"
```

---

## Writing Checklist

Before publishing any text (README, docs, commits, issues):

- [ ] Is this claim backed by code or explanation?
- [ ] Am I being honest about the learning journey?
- [ ] Would I roll my eyes reading this?
- [ ] Does this sound like a real person or marketing?
- [ ] Am I explaining Rust concepts where relevant?
- [ ] Can I remove adjectives and still make my point?

---

## README Section Templates

### Hero / Overview
```markdown
# Anthropic Spy 🔍

A Rust TUI application for observing Claude Code's interactions with the Anthropic API.

**What it does:**
- Intercepts HTTP requests between Claude Code and Anthropic API
- Visualizes tool calls in real-time
- Logs all interactions to JSON Lines files
- Displays performance metrics and timing

**Status:** Active development - Phase 3 (token tracking) in progress

**Learning Project:** First serious Rust implementation, demonstrates async/await,
custom traits, event-driven architecture, and TUI development with ratatui.
```

### Features Section
```markdown
## Features

### Real-Time Visualization
See tool calls, results, requests, and responses as they happen in a clean TUI.
Navigate with vim-style keybindings, expand events for detailed inspection.

**Rust concepts:** State-based input handling, ratatui rendering, async event processing.

### Structured Logging
All events written to JSON Lines format for analysis with jq, grep, or other tools.
Daily log rotation, complete request/response bodies captured.

**Rust concepts:** Async file I/O, serde serialization, structured data handling.

### Header Capture
Captures API version, beta features, rate limits, and request IDs for comprehensive
observability. Security: API keys are hashed (SHA-256), never logged in full.

**Rust concepts:** HTTP header extraction, cryptographic hashing, type-safe parsing.
```

### Status Section
```markdown
## Status & Roadmap

**Current Phase:** Phase 3 - Token Tracking & Cost Estimation

**Completed:**
- ✅ HTTP proxy with request forwarding
- ✅ Tool call and result parsing
- ✅ TUI with proper log handling
- ✅ Header capture and rate limit tracking
- ✅ JSON Lines structured logging

**In Progress:**
- 🚧 Token usage tracking and cost calculation

**Planned:**
- 📋 SSE streaming support (Phase 4)
- 📋 Enhanced dashboard layout
- 📋 Filtering and search functionality
```

---

## Commit Message Style

### ✅ Good Commits
```
feat: add token tracking to status bar

Tracks cumulative input/output tokens from API responses. Calculates
estimated cost based on Anthropic pricing (different rates for input,
output, cache writes, cache reads).

Demonstrates: State management in Rust, number formatting, integration
with existing event system.
```

### ❌ Bad Commits
```
feat: revolutionary token tracking system

Implemented the most advanced token tracking using cutting-edge Rust
patterns. This will handle everything forever.
```

---

## Issue Response Style

### ✅ Good Response
```markdown
Thanks for reporting! I'll investigate this.

Quick questions:
- What action in Claude Code triggered this?
- Can you check the logs in ./logs/ for relevant entries?
- Did the TUI recover or freeze completely?

This is a learning project, so I appreciate the feedback. I'll respond
within 24-48 hours with findings.
```

### ❌ Bad Response
```markdown
This is a known limitation of the ratatui framework, not our implementation.
Our code is correct per the specification. You may want to try [complex workaround].
```

---

## Documentation Tone

### Code Comments
```rust
// ✅ Good: Explains Rust pattern and WHY
// Use Arc<Mutex<>> for shared mutable state across async tasks.
// Arc provides shared ownership (ref counted), Mutex allows mutation.
let app = Arc::new(Mutex::new(App::new(log_buffer)));

// Clone Arc for each task (cheap - just increments ref count)
let app_tui = Arc::clone(&app);
let app_input = Arc::clone(&app);

// ❌ Bad: States the obvious
// This creates an Arc around a Mutex
let app = Arc::new(Mutex::new(App::new(log_buffer)));

// ❌ Bad: Over-explains with hype
// REVOLUTIONARY memory-safe concurrent state management using
// advanced Rust patterns for maximum performance and reliability
let app = Arc::new(Mutex::new(App::new(log_buffer)));
```

### Architecture Docs
```markdown
✅ "The proxy uses axum for HTTP handling and Tokio for async runtime.
Events are passed through mpsc channels to decouple components (proxy,
TUI, storage). This makes it easy to add new event consumers without
modifying existing code."

❌ "Leveraging cutting-edge async Rust paradigms, our revolutionary
architecture employs advanced patterns for maximum scalability and
enterprise-grade reliability."
```

---

## Rust Learning Disclosure

When discussing Rust learning in development:

### ✅ Honest and Measured
```markdown
This is a learning project - my first serious Rust implementation coming
from a .NET/TypeScript background. AI (Claude) generated most of the code,
but every piece is reviewed, understood, and explainable.

**What I learned:**
- Ownership and borrowing (Arc<Mutex<>> patterns)
- Async/await with Tokio (concurrent tasks, channels)
- Custom trait implementation (tracing layer)
- TUI development with ratatui
- Event-driven architecture in Rust

The goal: build something useful (Claude Code observability) while learning
Rust concepts through practical application.
```

### ❌ Defensive or Hiding
```markdown
Built entirely by hand with expert Rust knowledge. No AI assistance.
```

---

## Handling Questions

### When Someone Asks "Why Rust?"

✅ **Do:**
```markdown
Good question! Three main reasons:

1. **Performance** - Proxy needs minimal overhead
2. **Type Safety** - Catch bugs at compile time
3. **Learning** - Wanted to learn Rust through a practical project

Coming from .NET/TypeScript, Rust's ownership model was fascinating
to explore. This project lets me learn while building something useful.
```

❌ **Don't:**
```markdown
Because Rust is clearly superior to all other languages. If you're not
using Rust, you're doing it wrong.
```

### When Someone Points Out Issues

✅ **Do:**
```markdown
Thanks for the detailed feedback! You're right about [specific issue].

I'll fix [issue] in the next update. This is a learning project, so
constructive feedback like this is really helpful.

Appreciate you taking the time to look at this carefully.
```

❌ **Don't:**
```markdown
Actually, if you understood Rust's ownership model, you'd see this is
the correct approach. Your analysis is incomplete.
```

---

## Version Messaging

### Current (Phase 3)
```markdown
**Message:** "Learning project demonstrating Rust patterns through Claude Code
observability. Phase 3 (token tracking) in progress."

**Tone:** Honest and educational
- "Works reliably for completed features"
- "Learning Rust through practical application"
- "Phase-by-phase development with clear goals"
```

### Future (Phases 4-5)
```markdown
**Message:** "Enhanced with streaming support and dashboard features.
Continuing to learn Rust while building useful observability tools."

**Tone:** Building on foundation
- "Added features based on real usage"
- "Deeper understanding of Rust patterns"
- "Still a learning project, now with more features"
```

---

## Quick Reference: Tone Spectrum

```
Too Timid          Just Right              Too Confident
    ↓                  ↓                         ↓
"Maybe works?"    "Demonstrates patterns"    "EXPERT-LEVEL!"
"Trying to..."    "Learning through..."      "REVOLUTIONARY!"
"Hope this..."    "Built for observability"  "BEST EVER!"
"Just a demo"     "Phase 3 in progress"      "PRODUCTION READY!"
```

**Aim for the middle - honest and competent.**

---

## Final Check: The Eye-Roll Test

**Before publishing anything, read it out loud.**

Questions to ask:
- Would I cringe if someone else said this?
- Am I hiding that it's a learning project?
- Am I using buzzwords instead of explanations?
- Does this sound like a real person?

If you'd roll your eyes, revise.

---

*Use this guide for all project communications: README, docs, commits, issues, PRs, and AI sessions.*

*Last updated: 2025-11-24*
