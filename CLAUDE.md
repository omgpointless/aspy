# CLAUDE.md

Guidance for Claude Code working in this repository.

## What This Is

**Aspy** — Rust TUI observability proxy for Claude Code. Intercepts HTTP traffic between Claude Code and Anthropic API, displays tool calls/results in real-time, logs to JSONL.

**Tech:** Rust 2021, Tokio async, Axum (HTTP), Ratatui (TUI), anyhow/thiserror (errors).

## IMPORTANT Constraints from User

The steps below are hard constraints in your assistance of the User.

- ALWAYS execute `cargo check` over `cargo build`
- ALWAYS execute `cargo fmt` after completion of an implementation.
- ALWAYS execute `cargo clippy` and resolve all warnings.
  - Exception: `dead_code` warnings for future features → add a detailed doc comment explaining the future purpose, then suppress with `#[allow(dead_code)]`
- ALWAYS check for existing patterns in the codebase and CLAUDE.md before introducing new ones
- ALWAYS validate that there are 0 warnings from `clippy` BEFORE giving summary of your work to the User.
- NEVER commit changes without user approval
- NEVER start OR stop the application for the user
- NEVER apply Claude attribution to commits, user likes CLEAN history.

## The Mental Model

Inspired by Linux's kernel/userland separation — adapted for our domain:

```
┌─────────────────────────────────────────────────────────────┐
│ CORE (non-optional, never depends on extensions)           │
│   proxy/mod.rs, parser/, events.rs, tui/mod.rs, storage/   │
├─────────────────────────────────────────────────────────────┤
│ EXTENSIONS (config-toggleable, depend only on core)        │
│   proxy/augmentation/, tui/components/*_panel.rs, themes   │
├─────────────────────────────────────────────────────────────┤
│ CUSTOM (external: config.toml, user themes)                │
└─────────────────────────────────────────────────────────────┘
```

**Rule:** If the app works without it → Extension. If user-provided → Custom. Otherwise → Core.

## Where Does New Code Go?

| Adding...                        | Location                                    |
|----------------------------------|---------------------------------------------|
| UI behavior (scroll, select)     | `tui/traits/[name].rs` + impl on components |
| UI widget                        | `tui/components/[name]_panel.rs`            |
| Full-screen layout               | `tui/views/[name].rs`                       |
| Stream modification (response)   | `proxy/augmentation/[name].rs` + config     |
| Stream modification (request)    | `proxy/transformation/[name].rs` + config   |
| Feature-local helper             | `[feature]/helpers/[name].rs`               |

## Where Does State Live?

| State type                             | Location                      |
|----------------------------------------|-------------------------------|
| Component-specific (scroll, selection) | Inside the component struct   |
| Cross-cutting (event list, stats)      | `app.rs`                      |
| Augmentor state                        | Inside augmentor struct, NOT global |

**❌ NEVER** add component-specific state to `app.rs`. Components own their state.

## Critical Anti-Patterns

1. **app.rs bloat** — Don't add `theme_list_scroll`, `panel_selected_index` to App. Put in component.
2. **Duplicating behavior** — Extract to trait with default impl, not copy-paste.
3. **Coupling component→view** — Components return data; views decide what to do.
4. **Mixing render+state** — State structs return data; `impl Renderable` handles drawing.

See `AGENTS.md` for quick-reference scenarios and checklists.

## Data Flow

```
Claude Code → HTTP → Proxy (axum) → tee stream → Parser → Events → TUI/Storage
                         ↓
              SSE immediately forwarded (low latency)
```

Parsing happens after stream completes. Tool calls correlated with results via `pending_calls` HashMap.

## Quick Commands

```bash
cargo run --release              # Run proxy + TUI
ASPY_DEMO=1 cargo run --release  # Demo mode (mock events)
ASPY_NO_TUI=1 cargo run          # Headless mode

# Connect Claude Code
export ANTHROPIC_BASE_URL=http://127.0.0.1:8080/dev-1
```

## Before Modifying Architecture

**READ:** [docs/architecture.md](docs/architecture.md) — patterns, anti-patterns, examples.

Only read these if working on that area:
- [docs/sessions.md](docs/sessions.md) — multi-client routing
- [docs/commands.md](docs/commands.md) — build/run commands
- [docs/log-analysis.md](docs/log-analysis.md) — jq queries, debugging

## Current State

**v0.1.0 complete:** Views, Stats dashboard, 32 themes, CLI config, REST API, multi-client routing, context warning augmentation.

**Known gaps (fix as touched):**
- Some component state still in `app.rs`
- Not all panels implement full trait set
- `tui/traits/` vs `tui/behaviors/` naming (equivalent)

## Guardrails

- API keys: SHA-256 hash prefix only (never logged in full)
- Proxy overhead: ~1-2ms
- TUI: 10 FPS
- Before SQLite migrations: verify proxy isn't running
