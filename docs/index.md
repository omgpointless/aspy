---
layout: default
title: Home
---

<div class="hero">
  <img src="{{ '/images/aspy-logo-v1-min-small-resized.jpg' | relative_url }}" alt="Aspy - the anthropic-spy mascot" class="hero-logo">
  <p class="hero-tagline">See what Claude sees. Observability proxy for Claude Code.</p>
  <div class="hero-links">
    <a href="https://github.com/omgpointless/anthropic-spy" class="primary">Get Started</a>
    <a href="{{ '/blog/' | relative_url }}">Read the Blog</a>
  </div>
</div>

## What is anthropic-spy?

**anthropic-spy** is a Rust TUI application that acts as an observability proxy between Claude Code and the Anthropic API. It intercepts HTTP traffic, parses tool calls and responses, and displays them in real-time.

<div class="terminal-block">
<span class="comment"># Point Claude Code at the proxy</span>
<span class="prompt">$</span> export ANTHROPIC_BASE_URL=http://127.0.0.1:8080/dev-1
<span class="prompt">$</span> claude
</div>

<div class="feature-grid">
  <div class="feature-card">
    <h3>Real-time Visibility</h3>
    <p>Watch every tool call, API request, and response as it happens.</p>
  </div>
  <div class="feature-card">
    <h3>Token Tracking</h3>
    <p>Monitor token usage and costs with live statistics.</p>
  </div>
  <div class="feature-card">
    <h3>Multi-Client</h3>
    <p>Track multiple Claude Code instances through a single proxy.</p>
  </div>
  <div class="feature-card">
    <h3>Thinking Blocks</h3>
    <p>See Claude's reasoning process in a dedicated panel.</p>
  </div>
</div>

## Why?

Understanding how Claude Code works is key to using it effectively. anthropic-spy gives you:

- **Insight** into which tools Claude chooses and why
- **Visibility** into token consumption and cache efficiency
- **Debugging** capability when things don't work as expected
- **Learning** opportunity to improve your prompting

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
