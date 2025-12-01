---
layout: default
title: Home
---

<div class="hero">
  <img src="{{ '/images/aspy-logo-v1-min-small-resized.jpg' | relative_url }}" alt="Aspy logo" class="hero-logo">
  <p class="hero-tagline">Stop guessing. Start seeing.</p>
  <p class="hero-subtitle">Real-time observability proxy for Claude Code</p>
  <div class="hero-links">
    <a href="https://github.com/omgpointless/anthropic-spy" class="primary">Get Started</a>
    <a href="{{ '/blog/' | relative_url }}">Read the Blog</a>
  </div>
</div>

## What is Aspy?

**Aspy** sits between Claude Code and the Anthropic API, giving you a live view of everything happening under the hood. Every tool call. Every response. Every token. All in a terminal interface that stays out of your way until you need it.

<div class="terminal-block">
<span class="comment"># Point Claude Code at the proxy</span>
<span class="prompt">$</span> export ANTHROPIC_BASE_URL=http://127.0.0.1:8080
<span class="prompt">$</span> claude
</div>

<div class="feature-grid">
  <div class="feature-card">
    <h3>Real-time Visibility</h3>
    <p>Watch tool calls, API requests, and responses the moment they happen. No more wondering what Claude is doing or thinking.</p>
  </div>
  <div class="feature-card">
    <h3>Detailed Statistics</h3>
    <p>Token usage with sparklines, model breakdowns, tool call counts, and execution times. A full stats dashboard at your fingertips.</p>
  </div>
  <div class="feature-card">
    <h3>Multi-Session Support</h3>
    <p>Run multiple Claude Code instances through a single proxy. Each session tracked independently.</p>
  </div>
  <div class="feature-card">
    <h3>Thinking Blocks</h3>
    <p>Peek into Claude's reasoning process. Understand the "why" behind the actions.</p>
  </div>
  <div class="feature-card">
    <h3>32 Themes & Custom</h3>
    <p>Make your terminal beautiful. Built-in themes from Dracula to Solarized, plus full TOML custom theme support.</p>
  </div>
  <div class="feature-card">
    <h3>Context Recall (MCP)</h3>
    <p>Search past sessions. Let new Claude instances recall what previous ones discovered—context that survives compaction.</p>
  </div>
</div>

## Why Use It?

When I use Claude Code without Aspy now, I feel blind. Genuinely.

Once you see the full picture—which tools get called, how tokens flow, when cache hits happen—you start making better prompts. You debug faster. You understand *why* something worked (or didn't).

**What you gain:**

- **Prompt engineering intuition** — See exactly how your words translate to actions
- **Cost awareness** — No more surprise token bills; watch consumption in real-time
- **Debugging superpowers** — When something breaks, you'll know where and why
- **Deeper understanding** — Learn how Claude Code actually operates

<p class="cta-subtle"><a href="{% post_url 2025-12-01-welcome-to-aspy %}">Read the story behind Aspy →</a></p>

## Documentation

<div class="docs-grid">
  <div class="docs-section">
    <h4>Getting Started</h4>
    <ul>
      <li><a href="{{ '/features' | relative_url }}">Features</a> — Everything Aspy can do</li>
      <li><a href="{{ '/commands' | relative_url }}">Commands</a> — Build, run, and develop</li>
      <li><a href="{{ '/sessions' | relative_url }}">Sessions</a> — Multi-client routing setup</li>
    </ul>
  </div>
  <div class="docs-section">
    <h4>Using the TUI</h4>
    <ul>
      <li><a href="{{ '/views' | relative_url }}">Views</a> — Events, Stats, and Settings</li>
      <li><a href="{{ '/themes' | relative_url }}">Themes</a> — 32 built-in + custom TOML themes</li>
      <li><a href="{{ '/cli-reference' | relative_url }}">CLI Reference</a> — Configuration commands</li>
    </ul>
  </div>
  <div class="docs-section">
    <h4>Advanced</h4>
    <ul>
      <li><a href="{{ '/api-reference' | relative_url }}">API Reference</a> — REST endpoints</li>
      <li><a href="{{ '/log-analysis' | relative_url }}">Log Analysis</a> — jq queries and session recovery</li>
      <li><a href="{{ '/architecture' | relative_url }}">Architecture</a> — Design patterns and internals</li>
    </ul>
  </div>
</div>

## Latest Posts

<ul class="post-list">
{% for post in site.posts limit:3 %}
  <li class="post-list-item">
    <h3 class="post-list-title">
      <a href="{{ post.url | relative_url }}">{{ post.title }}</a>
    </h3>
    <p class="post-list-meta">{{ post.date | date: "%B %-d, %Y" }}</p>
    {% if post.excerpt %}
      <p class="post-list-excerpt">{{ post.excerpt | strip_html | truncate: 160 }}</p>
    {% endif %}
  </li>
{% endfor %}
</ul>

<p><a href="{{ '/blog/' | relative_url }}">View all posts &rarr;</a></p>
