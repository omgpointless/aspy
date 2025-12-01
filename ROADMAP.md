# Anthropic Spy Roadmap

A living document capturing the vision and planned iterations for anthropic-spy.

**Philosophy:** Build fundamentals that enable feature iterations. Small batches, frequent releases, no grand slams.

---

## Current: v0.2.0 - Views & Statistics

**Status:** Complete

### Completed Features

**Views System:**
- [x] `View` enum: Events | Stats | Settings
- [x] Keyboard navigation between views (1, 2, s)
- [x] Each view has dedicated render function
- [x] Settings view with theme/preset selection

**Statistics View (5 tabs):**
- [x] Overview tab: Session gauges + summary
- [x] Models tab: API call distribution with BarChart and sparklines
- [x] Tokens tab: Token usage breakdown with grouped bars
- [x] Tools tab: Tool call frequency and duration analysis
- [x] Trends tab: Sparklines grid showing trends over time

**Theme System:**
- [x] 32 bundled themes (Spy Dark, Dracula, Nord, Catppuccin, etc.)
- [x] TOML-based custom theme support
- [x] Theme picker in Settings view
- [x] Runtime theme switching

**Configuration:**
- [x] Config file support (`~/.config/anthropic-spy/config.toml`)
- [x] CLI config management (`--init`, `--show`, `--edit`, `--update`, `--reset`)
- [x] Multi-client routing with named clients
- [x] Provider backend configuration

**REST API:**
- [x] `/api/stats` - Session statistics
- [x] `/api/events` - Event buffer with filtering
- [x] `/api/context` - Context window status
- [x] `/api/sessions` - Active session list
- [x] `/api/search` - Search past session logs

---

## Released: v0.1.0 - Core Observability

**Status:** Released 2025-11-26

The spy observes and logs all Claude Code ↔ Anthropic API traffic:
- [x] Tool calls and results with timing
- [x] Thinking blocks with dedicated panel
- [x] Token usage and cost estimation
- [x] Request/response headers
- [x] Real-time TUI display
- [x] SSE streaming with delta accumulation
- [x] Demo mode for showcasing
- [x] JSON Lines logging

---

## Next: v0.3.0 - Analysis & Polish

**Theme:** "Observation + Analysis in Terminal"

Building on 0.2.0, this release demonstrates that anthropic-spy isn't just a proxy - it's an analysis tool.

### Potential Features
- Session profile summary (on exit or dedicated view)
- CLI query mode (`anthropic-spy analyze <session>`)
- More chart types (token timeline, tool sequence)
- Export capabilities (CSV, JSON reports)
- Context warning augmentation improvements

### UX Iteration
- Refine navigation based on 0.2.x usage
- Mouse support improvements
- Keyboard shortcut help (`?`)
- Improved modal dialogs

---

## Horizon: The Suite Vision

Long-term, anthropic-spy evolves from observer to augmenter:

```
PHASE 1 (Current):  Claude Code ←→ Anthropic API
                    (spy observes)

PHASE 2 (Future):   Claude Code ←→ anthropic-spy ←→ Anthropic API
                    (spy PARTICIPATES)
```

### Augmentation Ideas (Not Committed)
- Context budget tracking with warnings
- Custom `/spy:` commands intercepted locally
- Response injection (add context warnings to stream)
- Session continuity (handoff/resume across compactions)
- Breakpoints and request inspection

### Platform Ideas (Aspirational)
- Gamification (XP, achievements, character classes)
- CLI streaming for model-as-observer patterns
- API translation layer (Anthropic → OpenAI format)
- Web UI mirror
- Plugin/theme system

---

## Versioning Philosophy

- **0.x.y** - Active development, things may change
- **Patch (0.1.x)** - Bug fixes, small polish
- **Minor (0.x.0)** - New features, views, capabilities
- **Major (1.0.0)** - When the fundamentals are stable and battle-tested

Go as slow as needed. This is a learning project. Quality over velocity.

---

## Session Insights

Data from actual usage (research session, 2025-11-27):

```
Duration:     ~3.5 hours
API calls:    79
Model split:  53% Haiku, 47% Opus
Cache ratio:  98.2%
Total tokens: ~1.56M (1.5M cached)
Tool calls:   29 (Read-heavy)
```

Claude Code's caching is remarkably efficient. This is the kind of insight the Statistics view will surface.

---

## References

- Brainstorms: `.claude/roadmap-brainstorms/`
- Project status: `.claude/PROJECT_STATUS.md`
- Log analysis: See "Analyzing Session Logs" in `CLAUDE.md`
