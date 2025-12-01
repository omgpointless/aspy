# Agent Guidelines

This doc mirrors the expectations in `CLAUDE.md` for any AI agent contributing here. Internalize these before changing code.

## Architecture Philosophy
- **Composition over inheritance:** new behaviors come from traits with default impls; avoid god objects and deep type hierarchies.
- **Three layers:** Kernel (proxy, SSE, parser, events, TUI loop) is non-optional and cannot depend on userland; Userland (augmentors, panels, themes, analytics) must be config-toggleable and only depend on kernel; User Space is user-provided extensions via public APIs.
- **Discoverability:** one concept, one place (e.g., scrolling belongs in `tui/traits/scrollable.rs` and implementors). Keep hierarchies flat (max ~3 levels).
- **Data vs rendering:** views build data structures; rendering happens in dedicated passes/traits. Avoid mixing rendering into state structs.
- **Configuration over forks:** all userland features get toggles (config/env/feature flags) instead of hard-coded changes.

## Quick Decision Tree

**Adding a new feature? Ask:**
1. Does the app work without it? No → Kernel (`proxy/`, `parser/`, `events.rs`) | Yes → Userland
2. Is it a UI capability multiple components need? → Trait in `tui/traits/[name].rs`
3. Is it a reusable UI widget? → Component in `tui/components/[name]_panel.rs`
4. Is it a full-screen layout? → View in `tui/views/[name].rs`
5. Does it transform the proxy stream? → Augmentor in `proxy/augmentation/[name].rs` + config toggle
6. Is it pure logic used by one feature? → Helper in `[feature]/helpers/[name].rs`

**Where does state live?**
- Component-specific state (scroll offset, selection) → inside the component struct
- Cross-cutting state (event list, global stats) → `app.rs`
- Augmentor state → inside the augmentor struct, NOT global

**When to modify vs compose?**
- Behavior needed by multiple components → new trait
- Behavior specific to one component → method on that component
- Existing component needs new capability → implement existing trait or add new trait
- Never modify `app.rs` to add component-specific state

## Project Structure (target)
- `src/main.rs` orchestrates config, events, storage, logging, proxy, TUI (and demo mode).
- `src/config.rs`, `src/events.rs` define configuration, event types, and stats helpers.
- `src/proxy` (kernel) handles HTTP interception; `proxy/augmentation` (userland) holds toggleable augmentors; helpers stay proxy-local.
- `src/parser` parses Anthropic payloads and correlates tool calls/results; helpers stay parser-local.
- `src/storage` writes JSONL logs under `logs/` (rotated daily `aspy-*.jsonl`, gitignored).
- `src/logging` owns tracing layer for the TUI log pane; `src/pricing.rs` handles token cost math; `src/theme.rs` defines color palettes; `src/demo.rs` drives mock events.
- `src/tui` owns app state, rendering orchestration, behaviors/traits, components, views, layout helpers, theme loader, modal/markdown/preset systems, and TUI-only helpers. Components own their state; views compose components; behaviors stay isolated per trait file.
- Bundled themes live in `themes/*.json`; binary target is `aspy`.

## Current State vs Target

**Aligned with target:**
- ✅ Augmentation system is fully modular and config-toggleable
- ✅ Event system, parser, proxy follow kernel/userland separation
- ✅ Most components in `tui/components/` follow trait composition

**In progress (migrate as touched):**
- ⚠️ Some component state still lives in `app.rs` → move to component structs
- ⚠️ Not all panels implement full trait set → add traits as features enhance
- ⚠️ Some helpers not yet feature-scoped → organize as you work on those features

**Semantic notes:**
- `tui/traits/` vs `tui/behaviors/` are equivalent terms (folder name is secondary)
- When refactoring, prioritize alignment with target patterns
- Don't break working code for perfection; align when actively touching that area

## Patterns to Follow
- Behaviors as traits with default methods; components opt in by implementing them. Add new behaviors in their own files.
- Newtype wrappers for domain concepts; builder pattern for complex setup; exhaustive `match` on enums; `Box<dyn Trait>` for hetero pipelines.
- Helpers are feature-scoped (`proxy/helpers`, `tui/helpers`, `parser/helpers`) and as pure as possible.
- Stateless or self-contained augmentors registered via a builder; always guard with config.

## Anti-Patterns (avoid)

**❌ Adding state to `app.rs`:**
```rust
// Wrong: app.rs knows about ThemeList details
pub struct App {
    theme_list_scroll: usize,  // ❌
}

// Right: component owns its state
pub struct ThemeListPanel {
    themes: Vec<Theme>,
    scroll: ScrollState,  // ✅
}
impl Scrollable for ThemeListPanel { ... }
```

**❌ Duplicating behavior:**
```rust
// Wrong: each panel duplicates scroll logic
impl EventsPanel {
    fn scroll_up(&mut self) { self.offset = self.offset.saturating_sub(1); }  // ❌
}
impl LogsPanel {
    fn scroll_up(&mut self) { self.offset = self.offset.saturating_sub(1); }  // ❌
}

// Right: trait provides default implementation
trait Scrollable {
    fn scroll_state_mut(&mut self) -> &mut ScrollState;
    fn scroll_up(&mut self) { /* impl once */ }  // ✅
}
```

**❌ Coupling component to view:**
```rust
// Wrong: EventsPanel knows about MainView
impl EventsPanel {
    fn notify_main_view(&self) { ... }  // ❌
}

// Right: return data, let caller decide
impl EventsPanel {
    fn selected_event(&self) -> Option<&Event> { ... }  // ✅
}
```

**Other anti-patterns:**
- Deep nesting: `tui::components::panels::events::state` ❌ → `tui::components::events_panel.rs` ✅
- Mixing rendering with state: `impl Panel { fn draw(...) }` ❌ → `impl Renderable for Panel` ✅
- Modifying components instead of composing: edit `EventsPanel` ❌ → add `impl Filterable for EventsPanel` ✅

## Code Review Checklist

**Architecture:**
- [ ] No feature-specific state added to `app.rs`
- [ ] Components own their state; views compose components
- [ ] Userland features have config toggles
- [ ] Traits are isolated (don't depend on other traits)
- [ ] Helpers are feature-scoped, not in flat `helpers/` folder
- [ ] Module nesting ≤3 levels

**Patterns:**
- [ ] Duplicated behavior extracted to trait with default impl
- [ ] New behaviors in dedicated `tui/traits/[name].rs` file
- [ ] Data vs rendering separated (no `draw()` methods on state structs)
- [ ] Newtype wrappers for domain concepts (not bare `u64`, `String`)
- [ ] Exhaustive `match` on enums (no `_ =>` unless justified)

**Code Quality:**
- [ ] `cargo fmt` applied
- [ ] `cargo clippy` warnings addressed
- [ ] Tests added for new behaviors
- [ ] Tracing spans for debugging (not `println!`)
- [ ] No unwraps in production paths (use `?` or `unwrap_or`)

**Userland Features:**
- [ ] Config flag added to enable/disable
- [ ] Augmentors registered in pipeline builder
- [ ] Documented in CLAUDE.md if significant

## Coding Style
- Rust 2021; format with `cargo fmt` (4-space indents, no trailing whitespace).
- Naming: modules/files `snake_case`; types/traits `PascalCase`; fns/vars `snake_case`; constants `SCREAMING_SNAKE_CASE`.
- Prefer `anyhow::Result`; use `thiserror` for domain errors; add `tracing` spans where useful.
- Keep TUI rendering functions pure; side effects live in proxy/storage layers.

## Build & Run
- `cargo build --release` for optimized builds; `cargo run --release` to start proxy + TUI (honors env like `ASPY_BIND`).
- Demo: `ASPY_DEMO=1 cargo run --release` (mock events); headless: `ASPY_NO_TUI=1 cargo run --release`.
- Diagnostics: `RUST_LOG=debug cargo run`; quick checks: `cargo fmt`, `cargo clippy`, `cargo check`; tests: `cargo test`.

## Testing
- Keep tests near code in `#[cfg(test)]` modules; use `tokio::test` for async.
- Name by behavior (`handles_invalid_payload`, `renders_empty_state`).
- For log-writing code, write to a temp dir to avoid polluting `logs/`.
- Add parsing fixtures for new Anthropic payload shapes; run `cargo test` before PRs.

## Commit & PR Expectations
- Commits: concise, imperative subjects (e.g., `Add parser validation`), narrow scope, reference issues when relevant. Conventional commit style is preferred (`feat`, `fix`, `refactor`, `chore`, `docs`, `test`, `perf` with scopes like `proxy`, `tui`, `parser`, `storage`, `events`, `deps`).
- PRs: short summary, test evidence (`cargo test`, `cargo clippy`), screenshots/GIFs for TUI changes, and call out new env vars or breaking config changes.

## Security & Configuration Notes
- Proxy binds to `127.0.0.1:8080` by default; override with `ASPY_BIND`. Prefer `ANTHROPIC_BASE_URL` for Claude Code (`HTTPS_PROXY` redirects all HTTPS).
- Do not commit logs or secrets. Validate `ANTHROPIC_API_URL` for non-default endpoints.
- Use `ASPY_NO_TUI=1` for headless logging in CI/remote environments. Demo mode via `ASPY_DEMO=1`.

## Common Scenarios

**Adding scrolling to a panel:**
1. Add `scroll: ScrollState` field to panel struct
2. `impl Scrollable for YourPanel` (just return `&self.scroll`)
3. Handle key events in `tui/mod.rs`: `if let Some(scrollable) = panel.as_scrollable_mut()`
4. Reference: `tui/traits/scrollable.rs`, `tui/components/events_panel.rs`

**Adding a new augmentor:**
1. Create `proxy/augmentation/your_augmentor.rs`
2. `impl Augmentor` trait with state in struct
3. Register in `proxy/augmentation/mod.rs` pipeline builder
4. Add config flag in `config.rs`
5. Reference: `proxy/augmentation/context_warning.rs`

**Adding a new panel:**
1. Create `tui/components/your_panel.rs`
2. Implement traits for needed behaviors (Scrollable, Copyable, etc.)
3. Add to relevant view in `tui/views/[name].rs`
4. Panel owns its state, view just composes it
5. Reference: `tui/components/events_panel.rs`

**Adding a new view:**
1. Create `tui/views/your_view.rs`
2. Build layout from components (don't create components inline)
3. Return layout data, let renderer handle drawing
4. Register in `tui/mod.rs` view routing
5. Reference: `tui/views/stats.rs`

## References
- Detailed philosophy, architecture, and tone live in `CLAUDE.md` and `.claude/*` (especially `DEVELOPMENT_PHILOSOPHY.md`, `ARCHITECTURE.md`, `PROJECT_STATUS.md`, `TONE_GUIDE.md`). Align new work with those documents; if existing code drifts, prefer bringing touched areas back toward the target patterns.
