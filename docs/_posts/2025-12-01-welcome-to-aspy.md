---
layout: post
title: "Welcome to Aspy"
date: 2025-12-01
tags: [announcement, story]
---

Welcome to the Aspy blog! This is where I'll share updates, deep dives, and the occasional rambling about observing Claude Code in action.

## How It Started (Not How It's Going)

It actually started as something completely different.

I was working on a proxy to translate between OpenAI and Anthropic v1 APIs. And while it worked—I had it running, streaming SSE and everything—the conversion was *really* tricky due to my limited understanding of the protocols at the time.

So I thought to myself: "Hey, I should just create a proxy that can perfectly log and deserialize the communication between Claude Code and Anthropic. That way, when I know *everything* that's happening, I can easier approach the translation part. I'll know exactly what's breaking and why."

And here I... well, embrace perhaps not being rational.

*"You know what would be fun? To also learn Rust."*

And so off I went.

## The Learning Curve

I dug into Rust documentation. I enabled learning mode on Claude Code and started asking about how to approach what I was trying to do. At the time, I couldn't really read the code. As a veteran C# developer, I must admit—it was daunting. The ownership model, the borrow checker, lifetimes... it felt like learning to program all over again.

Yet, I found myself more and more fascinated by the ecosystem. The tooling. The community. And eventually, I suppose, it clicked.

Now, mind you, I'm no expert at Rust. But it has been an educational journey. The result of that journey is Aspy.

## Scope Creep? Passion Project.

Aspy has become something much bigger than my initial narrow scope.

We all scope creep a little at times, don't we?

But I guess this became something of a passion project. One feature led to the next. One new insight planted a seed for future iterations. Eventually, I started to realize that what I was working on wasn't just a logging proxy—it was a complementary suite for Claude Code.

The core foundation is built on **observability**. But to that core, layers emerged with concepts such as **augmentation**. Now Aspy is really trying to be a developer tool that extends and enhances the already awesome Claude Code.

By providing rich ways to manage and inspect your sessions, you'll further your understanding of how to efficiently work with AI models:

- Make your prompt engineering more efficient
- Reduce waiting-time anxiety with easier overview of the reasoning chain
- Understand the cost of your MCP calls (use aspy-mcp carefully while you develop with it... or don't—and check the stats page as you do)

## Where We Are Now

I've set the foundations for the **multi-session system** to allow you to run multiple Claude instances through the proxy while keeping track of each individually. The horizon goal for this is to enable headless/CI integrations and even organization-wide use.

The focus has been on an **event pipeline** to allow consumption of data by different actors. My immediate short-term goal is to introduce **SQLite as a context management solution** locally. This will allow the MCP server to more efficiently serve and find data. This will be an ongoing task—there's a balance between fetching not enough and too much.

Of course, users will have full agency and ownership of this database. It's simply there to provide more efficient tooling to Claude. JSONL files will remain; the SQLite storage module is simply a feature of the suite.

## A Note on Bugs

Do mind, while generally free of critical bugs, there can be discrepancies or minor issues. Should any of them happen to you, feel free to post an issue on the [GitHub repository](https://github.com/omgpointless/anthropic-spy). Don't worry too much about the formalia of the issue templates should they look daunting.

## What's Next

I'm excited for what's to come for Aspy.

The roadmap is larger than what I'm capable of delivering for a long time. But I hope somebody finds enjoyment in their own spying sessions on Claude.

And if you do try it out—I'd love to hear about the many "meta" observations Claude makes when you use this tool. There's something delightfully recursive about watching an AI observe itself being observed.

Happy spying! 🔍
