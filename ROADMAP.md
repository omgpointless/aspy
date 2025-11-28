# Anthropic Spy Roadmap

A living document capturing the vision and planned iterations for anthropic-spy.

**Philosophy:** Build fundamentals that enable feature iterations. Small batches, frequent releases, no grand slams.

---

## Current: v0.1.0 - Core Observability

**Status:** Polishing, near release

The spy observes and logs all Claude Code ↔ Anthropic API traffic:
- Tool calls and results with timing
- Thinking blocks
- Token usage and cost estimation
- Request/response headers
- Real-time TUI display

**Remaining for 0.1.0:**
- [ ] Session-based log rotation (not daily)
- [ ] UI polish (in progress in separate session)

---

## Next: v0.2.0 - Views & Statistics

**Theme:** Multiple views, initial analytics

### Views System
- `View` enum: Events | Statistics | Thinking | Logs
- Tab bar in header for navigation
- Each view has dedicated render function
- Foundation for future views (Achievements, Settings, etc.)

### Statistics View
- Model distribution chart (Haiku vs Opus vs Sonnet)
- Token breakdown (input / output / cached)
- Cache efficiency ratio (expect 95-99%)
- Session cost tracking
- Tool usage distribution

### Infrastructure
- Config file support (`~/.config/anthropic-spy/config.toml`)
- Structured stats data for charting

---

## Future: v0.3.0 - Analysis Milestone

**Theme:** "Observation + Analysis in Terminal"

Building on 0.2.0, this release demonstrates that anthropic-spy isn't just a proxy - it's an analysis tool.

### Potential Features
- Session profile summary (on exit or dedicated view)
- CLI query mode (`anthropic-spy analyze <session>`)
- More chart types (token timeline, tool sequence)
- Export capabilities

### UX Iteration
- Refine navigation based on 0.2.x usage
- Mouse support improvements
- Keyboard shortcut help (`?`)

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
