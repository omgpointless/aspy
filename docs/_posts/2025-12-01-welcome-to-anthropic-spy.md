---
layout: post
title: "Welcome to anthropic-spy"
date: 2025-12-01
tags: [docs, feature]
---

Welcome to the anthropic-spy blog! This is where we'll share updates, deep dives, and insights about observing Claude Code in action.

## What We're Building

anthropic-spy started as a simple question: *"What is Claude Code actually doing?"*

When you use Claude Code, there's a lot happening under the hood:

- Tool calls being made (Read, Write, Bash, Glob, Grep, etc.)
- API requests flying back and forth
- Tokens being consumed
- Thinking happening behind the scenes

All of this was invisible. Until now.

## The Spy Dark Theme

We've designed a custom theme called **Spy Dark** with a philosophy we call "workshop warmth with observatory clarity." It's meant to feel like a cozy workshop where you can focus for hours, while providing crystal-clear visibility into what's happening.

The color palette draws from these principles:

- **Warm amber** (`#c9a66b`) for titles and focus elements
- **Teal** (`#5da9a1`) for tool calls and primary actions
- **Muted grays** for secondary information
- **Distinct colors** for different event types

## Getting Started

Getting started with anthropic-spy is straightforward:

```bash
# Clone the repo
git clone https://github.com/omgpointless/anthropic-spy

# Build in release mode
cargo build --release

# Run the proxy
cargo run --release
```

Then point Claude Code at the proxy:

```bash
export ANTHROPIC_BASE_URL=http://127.0.0.1:8080/dev-1
claude
```

## What's Next

We're actively developing anthropic-spy with these goals in mind:

1. **Better filtering** - Find the events you care about
2. **Search** - Navigate through session history
3. **Analysis tools** - Understand patterns in tool usage
4. **Session replay** - Review past sessions

Stay tuned for more updates!
