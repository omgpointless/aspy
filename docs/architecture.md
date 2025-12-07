---
layout: default
title: Architecture
---

# Architecture Patterns & Design Principles

> **⚠️ REQUIRED READING**
>
> This document is **MANDATORY** reading before:
> - Adding any new features (components, traits, augmentors, views)
> - Refactoring existing code
> - Modifying component structure, app state, or trait implementations
>
> **Why:** Architectural violations compound over time. Understanding these patterns upfront prevents expensive rewrites later. Both AI agents and human contributors must internalize these principles before making changes.

**What this document defines:** The architectural patterns and principles that MUST be followed when adding features or modifying code. These patterns ensure maintainability, composability, and scalability.

## Core Principle: Composition Over Inheritance

This project follows a **composition-over-inheritance** model using Rust's trait system. New features MUST compose existing behaviors rather than modify core state containers or create deep inheritance hierarchies.

**Why:** Composition allows features to be independent, testable, and discoverable. A component gains capabilities by implementing traits, not by inheriting from a base class. This prevents "god objects" like `app.rs` from accumulating unrelated responsibilities.

## Layered Architecture: Core, Extensions, Custom

The codebase is organized into three conceptual layers (inspired by Linux's kernel/userland separation, adapted for our domain):

**1. Core**
- **What:** Proxy server, SSE handling, event system, TUI framework, parser, storage pipeline
- **Characteristics:** Cannot be disabled, app doesn't function without these components
- **Config Toggle:** No (fundamental infrastructure)
- **Dependencies:** Core components may depend on each other, but NEVER on extensions
- **Examples:** `proxy/mod.rs`, `events.rs`, `parser/mod.rs`, `tui/mod.rs`, `pipeline/mod.rs`

**2. Extensions**
- **What:** Augmentors, specific panels, themes, analytics
- **Characteristics:** Enhance user experience, but system functions without any specific one
- **Config Toggle:** Yes (MUST be config-toggleable)
- **Dependencies:** Can depend on core, NEVER on other extensions
- **Examples:** `proxy/augmentation/context_warning.rs`, `tui/components/theme_list_panel.rs`

**3. Custom**
- **What:** User-provided themes, custom configurations, future plugins
- **Characteristics:** Completely external, user brings their own
- **Config Toggle:** Yes (enable/disable at will)
- **Dependencies:** Interacts via public APIs only
- **Examples:** Custom theme TOML files, `.config/aspy/config.toml`

**Decision Tree:**
- Can the app function without this feature? **No** → Core | **Yes** → Extension
- Is this user-provided content? **Yes** → Custom
- Does this augment the stream or UI? **Yes** → Extension (must be toggleable)

## Project Structure

The following structure reflects our target organization. Current code may not fully align yet, but **new code MUST follow this structure**:

```
src/
├── main.rs                  # Orchestration (core)
├── events.rs                # Event system (core)
├── config.rs                # Configuration loading (core)
│
├── proxy/                   # HTTP interception (core)
│   ├── mod.rs
│   ├── augmentation/        # Stream transformations (extension)
│   │   ├── mod.rs
│   │   ├── context_warning.rs    # Context % tracking augmentor
│   │   └── [future augmentors]
│   └── helpers/             # Proxy-specific utilities
│       └── sse.rs           # SSE parsing logic
│
├── parser/                  # Protocol extraction (core)
│   ├── mod.rs
│   └── helpers/             # Parser-specific utilities
│       └── correlation.rs   # Tool call correlation
│
├── storage/                 # Event persistence (core)
│   └── mod.rs
│
├── pipeline/                # Data processing pipeline (core)
│   ├── mod.rs               # Pipeline orchestration
│   ├── cortex.rs            # SQLite storage (sessions, thinking, todos)
│   ├── cortex_query/        # Query interface (FTS5, stats, semantic)
│   ├── embeddings.rs        # Embedding provider abstraction
│   ├── embedding_indexer.rs # Background embedding indexer
│   └── transformation/      # Request transformations (extension)
│       ├── mod.rs
│       └── [future transformers]
│
├── logging/                 # Custom tracing layer (core)
│   └── mod.rs
│
├── tui/                     # Terminal interface (core + extensions)
│   ├── mod.rs               # Event loop, input handling
│   ├── app.rs               # Application state container
│   ├── ui.rs                # Rendering orchestration
│   │
│   ├── traits/              # [NOTE: Migrating to behaviors/]
│   │   ├── mod.rs           # Trait definitions (behaviors)
│   │   ├── scrollable.rs    # "Ah, here's scrolling code"
│   │   ├── copyable.rs      # Clipboard integration behavior
│   │   ├── focusable.rs     # Focus management behavior
│   │   └── interactive.rs   # Input handling behavior
│   │
│   ├── components/          # Reusable UI building blocks (extension)
│   │   ├── mod.rs
│   │   ├── events_panel.rs
│   │   ├── detail_panel.rs
│   │   ├── logs_panel.rs
│   │   ├── thinking_panel.rs
│   │   ├── context_bar.rs
│   │   └── theme_list_panel.rs
│   │
│   ├── views/               # Full-screen compositions (extension)
│   │   ├── mod.rs
│   │   ├── main.rs          # Main dashboard view
│   │   ├── settings.rs      # Settings view
│   │   └── stats.rs         # Statistics view
│   │
│   ├── layout/              # Constraint-based positioning
│   │   ├── mod.rs
│   │   └── constraints.rs   # Layout DSL
│   │
│   ├── theme/               # Theme system
│   │   └── mod.rs
│   │
│   ├── modal.rs             # Modal dialog system
│   ├── markdown.rs          # Markdown rendering
│   ├── preset.rs            # Layout presets
│   │
│   └── helpers/             # TUI-specific utilities
│       └── format.rs        # Token/duration formatting
│
└── themes/                  # Bundled themes (custom)
    └── *.toml
```

**Note on Current State:** The codebase currently uses `tui/traits/` but the target is conceptually `tui/behaviors/`. Both terms refer to the same concept (traits that define capabilities). The folder name is less important than the pattern: **one file per trait/behavior, organized by feature**.

## Data Pipeline Architecture

The pipeline layer handles persistent storage and search across sessions. Data flows through multiple stages:

```
Claude Code Request
        ↓
┌───────────────────┐
│  Transformation   │  ← Request transformers (extension)
│     Pipeline      │    Modify requests before forwarding
└───────────────────┘
        ↓
┌───────────────────┐
│   Proxy (Axum)    │  ← Core HTTP handling
│   SSE Streaming   │    Forward immediately, tee for parsing
└───────────────────┘
        ↓
┌───────────────────┐
│   Parser          │  ← Extract events from SSE stream
│   Event System    │    Emit to TUI + Storage
└───────────────────┘
        ↓
┌───────────────────┐     ┌───────────────────┐
│  JSONL Storage    │     │  Cortex           │
│  (real-time)      │     │  (SQLite + FTS5)  │
└───────────────────┘     └───────────────────┘
                                  ↓
                          ┌───────────────────┐
                          │ Embedding Indexer │  ← Background task
                          │ (vectors for      │    Polls for new content
                          │  semantic search) │    Batches to provider
                          └───────────────────┘
```

### Storage Layers

| Layer | Purpose | Query Tool |
|-------|---------|------------|
| **JSONL** | Real-time logs, `jq` analysis, portability | `aspy_search` |
| **Cortex (SQLite)** | Sessions, thinking, todos, FTS5 search | `aspy_recall_*` |
| **Embeddings (SQLite)** | Vector storage for semantic similarity | `aspy_recall` |

### Embedding Indexer

The `EmbeddingIndexer` runs as a background task with these characteristics:

- **Async poll loop** — Checks for unembedded documents every N seconds (configurable)
- **Batch processing** — Groups documents to reduce API calls
- **Provider abstraction** — Supports local (MiniLM) and remote (OpenAI-compatible) providers
- **Graceful degradation** — If embeddings unavailable, hybrid search falls back to FTS-only

**Provider selection:**
```toml
[embeddings]
provider = "remote"                    # "none" | "local" | "remote"
model = "text-embedding-3-small"
api_base = "https://api.openai.com/v1"
```

### Transformation Pipeline

Request transformers modify outgoing requests before forwarding. Unlike augmentors (which modify responses), transformers operate on the request path:

```rust
// Example transformer (conceptual)
pub trait RequestTransformer: Send + Sync {
    fn transform(&self, request: &mut Request) -> Result<()>;
    fn is_enabled(&self, config: &Config) -> bool;
}
```

**Use cases:**
- Header injection (add custom headers for tracking)
- Request logging (capture request bodies)
- Rate limit enforcement (client-side throttling)

## Design Principles

### 1. "Ah, Here It Is" Discoverability

Code should be immediately discoverable. When someone asks "where's the scrolling code?", the answer should be obvious: `tui/traits/scrollable.rs`. One concept, one place.

**Anti-pattern:** Scrolling logic scattered across `app.rs`, `ui.rs`, and individual components.

**Correct pattern:**
- Behavior defined in: `tui/traits/scrollable.rs`
- Components implement it: `impl Scrollable for EventsPanel`
- Behavior is self-contained and reusable

### 2. Traits as Capabilities

When adding a new behavior, define it as a trait with default implementations where sensible. Components gain behavior by implementing the trait, NOT by inheriting from a base class or adding state to `app.rs`.

**Key insight:** Traits don't know about each other. `Scrollable` doesn't know `Copyable` exists. A component opts into what it needs. This is the opposite of deep inheritance hierarchies—features compose without modifying existing code (Open/Closed Principle enforced by the type system).

### 3. Data vs Rendering

Views produce layout trees (data). Rendering is a separate pass. This keeps view logic testable without needing a terminal.

**Example:**
```rust
// View returns layout data (what to render)
fn build_layout(&self) -> LayoutTree {
    LayoutTree::vertical([
        Panel::new("Events").constraint(Percentage(60)),
        Panel::new("Logs").constraint(Percentage(40)),
    ])
}

// Renderer walks the tree (how to render it)
fn render(layout: LayoutTree, frame: &mut Frame) {
    for node in layout.iter() {
        frame.render_widget(node.widget, node.rect);
    }
}
```

### 4. Delegation for Specifics

Complex single-purpose logic (SSE parsing, token formatting, correlation tracking) lives in focused helper modules organized by feature. Keep helpers pure (no side effects) when possible.

**Feature-based helpers prevent:**
- 60 helpers in one flat directory (cognitive overload)
- Unclear namespace: is `format.rs` for TUI, SSE, or tokens?
- Wading through proxy helpers when working on TUI

**Structure:**
```
proxy/helpers/     # Proxy-specific utilities
tui/helpers/       # TUI-specific utilities
parser/helpers/    # Parser-specific utilities
```

### 5. Configuration Over Code Changes

User-facing features MUST be toggleable when they fall into the "userland" layer. If someone doesn't want an augmentor or specific panel, they disable it in config—NOT by forking the codebase.

**Config toggle requirement:**
1. **Is this fundamental kernel functionality?** → No config toggle needed (e.g., proxy server, event system)
2. **Is this a feature that enhances UX but isn't required for system operation?** → MUST have config toggle (e.g., augmentors, specific panels)

**Future consideration:** Compilation opt-outs for advanced users (feature flags in `Cargo.toml`).

## Adding New Features

### Adding a New Behavior (Trait)

**Example: Adding `Selectable` behavior**

```rust
// 1. Create tui/traits/selectable.rs
pub trait Selectable {
    fn selection(&self) -> Option<usize>;
    fn select(&mut self, index: usize);
    fn clear_selection(&mut self);

    // Default implementations for common patterns
    fn has_selection(&self) -> bool {
        self.selection().is_some()
    }
}

// 2. Implement for components that need it
// In tui/components/events_panel.rs
impl Selectable for EventsPanel {
    fn selection(&self) -> Option<usize> {
        self.selected_index
    }

    fn select(&mut self, index: usize) {
        self.selected_index = Some(index);
    }

    fn clear_selection(&mut self) {
        self.selected_index = None;
    }
}

// 3. Input handling checks for trait, not concrete type
// In tui/mod.rs event loop
if let Some(selectable) = focused_component.as_selectable_mut() {
    selectable.select(index);
}
```

**Why this works:** `Selectable` is isolated, reusable, and discoverable. Any component can opt in. Input handling is generic.

### Adding a New Component

**Example: Adding a `MetricsPanel`**

```rust
// 1. Create tui/components/metrics_panel.rs
pub struct MetricsPanel {
    metrics: Vec<Metric>,
    scroll: ScrollState,  // Compose behavior state
}

// 2. Implement needed behaviors by composition
impl Scrollable for MetricsPanel {
    fn scroll_state(&self) -> &ScrollState { &self.scroll }
    fn scroll_state_mut(&mut self) -> &mut ScrollState { &mut self.scroll }
}

impl Renderable for MetricsPanel {
    fn render(&self, frame: &mut Frame, area: Rect) {
        // Rendering logic here
    }
}

// 3. Register in view that uses it
// In tui/views/stats.rs
let metrics = MetricsPanel::new(app.metrics.clone());
```

**Anti-pattern to avoid:** Don't add `metrics_panel_state`, `metrics_scroll_offset`, `metrics_selected_index` to `app.rs`. Let the component own its state.

### Adding a New Augmentor

Augmentors implement the `Augmentor` trait and are registered in the proxy pipeline. They MUST:
- Be stateless or manage their own state (NOT shared global state)
- Have clear single responsibility
- Include a config flag to disable them
- Log at debug level, not info (unless user-facing alerts)

**Example: Adding a `LoopDetector` augmentor**

```rust
// 1. Create proxy/augmentation/loop_detector.rs
pub struct LoopDetector {
    recent_requests: VecDeque<String>,  // Owns its state
}

impl Augmentor for LoopDetector {
    fn augment(&mut self, request: &Request) -> Option<Annotation> {
        // Detect repetitive patterns
        if self.is_loop_detected(&request.body) {
            Some(Annotation::warning("Possible loop detected"))
        } else {
            None
        }
    }

    fn is_enabled(&self, config: &Config) -> bool {
        config.augmentors.loop_detection_enabled
    }
}

// 2. Register in proxy/augmentation/mod.rs
pub fn build_pipeline(config: &Config) -> Vec<Box<dyn Augmentor>> {
    vec![
        Box::new(ContextWarning::new()),
        Box::new(LoopDetector::new()),  // Add here
    ]
}

// 3. Add config flag in config.rs
pub struct AugmentorConfig {
    pub context_warning_enabled: bool,
    pub loop_detection_enabled: bool,  // New flag
}
```

## Patterns to Follow

These patterns are proven and MUST be used consistently:

- **Newtype wrappers** for domain concepts: `Tokens(u64)`, `PanelId(usize)` - prevents accidentally passing wrong types
- **Builder pattern** for complex construction: `Panel::new("Events").scrollable().focusable().build()`
- **`impl Iterator`** returns over concrete collection types - more flexible for callers
- **Exhaustive matching** on enums to catch new variants at compile time - compiler enforces correctness
- **Feature-based module organization** - helpers live with the feature they support
- **Default trait implementations** - reduce boilerplate in implementors

## Patterns to Avoid

These anti-patterns degrade maintainability and MUST NOT be used:

### ❌ Adding state to `app.rs` for feature-specific concerns

**Why wrong:** `app.rs` should be a lightweight state container for cross-cutting concerns (active view, global stats, event list). Adding `theme_list_scroll_offset` or `metrics_panel_selected_index` creates a god object.

**Do instead:** Let components own their state. If a component needs scroll state, it has a `ScrollState` field and implements `Scrollable`.

```rust
// ❌ Wrong
pub struct App {
    events: Vec<Event>,
    metrics_scroll: usize,        // Feature-specific state
    theme_list_selection: Option<usize>,  // Feature-specific state
}

// ✅ Correct
pub struct App {
    events: Vec<Event>,
    // Components manage their own state
}

pub struct MetricsPanel {
    metrics: Vec<Metric>,
    scroll: ScrollState,  // Component owns its scroll state
}
```

### ❌ Adding rendering logic to state structs

**Why wrong:** State and rendering are separate concerns. Mixing them makes testing harder and violates single responsibility.

**Do instead:** State structs return data. Separate rendering functions or `Renderable` trait implementations handle drawing.

```rust
// ❌ Wrong
impl EventsPanel {
    pub fn draw(&self, frame: &mut Frame) {
        // Rendering logic mixed with state
    }
}

// ✅ Correct
impl Renderable for EventsPanel {
    fn render(&self, frame: &mut Frame, area: Rect) {
        // Rendering is delegated to trait
    }
}
```

### ❌ Copy-pasting code instead of using traits

**Why wrong:** Duplicating scroll logic across 5 panels means bugs get fixed 5 times (or 4 times, then you find the 5th later).

**Do instead:** Define behavior once as a trait, provide default implementations, components implement the trait.

```rust
// ❌ Wrong: Each panel duplicates scroll logic
impl EventsPanel {
    fn scroll_up(&mut self) {
        self.offset = self.offset.saturating_sub(1);
    }
}
impl LogsPanel {
    fn scroll_up(&mut self) {
        self.offset = self.offset.saturating_sub(1);  // Duplicated
    }
}

// ✅ Correct: Trait provides default implementation
trait Scrollable {
    fn scroll_state_mut(&mut self) -> &mut ScrollState;

    fn scroll_up(&mut self) {
        self.scroll_state_mut().offset =
            self.scroll_state_mut().offset.saturating_sub(1);
    }
}
```

### ❌ Deep module nesting (>3 levels)

**Why wrong:** `tui::components::panels::events::scroll::state` is unreadable and hard to navigate.

**Do instead:** Keep hierarchies flat. Max 2-3 levels. Use descriptive file names instead of folders.

```rust
// ❌ Wrong
src/tui/components/panels/events/rendering/layout.rs

// ✅ Correct
src/tui/components/events_panel.rs
src/tui/helpers/layout.rs
```

### ❌ Components knowing about specific views

**Why wrong:** Creates tight coupling. If `EventsPanel` knows about `MainView`, it can't be reused in `DebugView`.

**Do instead:** Components are generic. Views compose components. Dependencies flow inward (views depend on components, not vice versa).

```rust
// ❌ Wrong
impl EventsPanel {
    fn notify_main_view(&self) {
        // Component shouldn't know about parent view
    }
}

// ✅ Correct
impl EventsPanel {
    fn on_selection_changed(&self) -> Option<Event> {
        // Return data, let parent decide what to do
        self.selected_event()
    }
}
```

### ❌ Modifying existing components instead of composing behaviors

**Why wrong:** If you modify `EventsPanel` to add a new behavior, you risk breaking existing functionality.

**Do instead:** Add a new trait, implement it for the component. Existing code is unchanged.

```rust
// ❌ Wrong: Modifying EventsPanel internals
impl EventsPanel {
    pub fn add_filtering(&mut self) {
        // Changing existing component
    }
}

// ✅ Correct: Compose new behavior
trait Filterable {
    fn apply_filter(&mut self, filter: Filter);
}

impl Filterable for EventsPanel {
    fn apply_filter(&mut self, filter: Filter) {
        // New behavior, old code untouched
    }
}
```

## Rust Idioms in This Codebase

These Rust-specific patterns are used throughout:

- **`Arc<Mutex<T>>`** for shared mutable state across async tasks (e.g., parser's pending calls, log buffer)
- **Newtype pattern** for type safety (`Tokens(u64)` can't be confused with `Duration`)
- **Trait objects (`Box<dyn Trait>`)** for heterogeneous collections (e.g., augmentor pipeline)
- **`Option<T>` and `Result<T, E>`** for explicit error handling (no null pointers, no unchecked exceptions)
- **Exhaustive `match`** on enums - compiler forces you to handle all cases

## Migration Strategy

The codebase is evolving toward these patterns. When working on existing code:

1. **New features:** MUST follow these patterns from day one
2. **Bug fixes:** Prefer minimal changes, but if touching related code, align with patterns
3. **Refactoring:** When refactoring a module, bring it fully into alignment
4. **Don't break working code:** If a component works but isn't "perfect", leave it unless you're actively improving that area

**Current known gaps:**
- `tui/traits/` → target name is conceptually `tui/behaviors/` (semantically equivalent, folder name is secondary concern)
- Some components still have state in `app.rs` - migrate as those components are touched
- Not all panels fully implement behavior traits yet - add as features are enhanced
- Transformation pipeline is scaffolded but no transformers implemented yet

**AI Agent Note:** When planning refactors, prioritize alignment with these patterns. If existing code violates a pattern, call it out and propose the correct structure. Push toward the target architecture, not just "making it work."

## MCP Server Integration

The MCP server (`mcp-server/`) is a **separate Node.js process** that queries the Rust proxy's REST API. It does NOT share state directly.

```
┌─────────────────┐     HTTP      ┌─────────────────┐
│  Claude Code    │  ←─────────→  │   MCP Server    │
│  (MCP Client)   │               │   (Node.js)     │
└─────────────────┘               └─────────────────┘
                                          │
                                    HTTP  │  REST API
                                          ↓
                                  ┌─────────────────┐
                                  │  Aspy Proxy     │
                                  │  (Rust)         │
                                  └─────────────────┘
```

**Key points:**
- MCP server is stateless — all data comes from REST API calls
- Proxy must be running for MCP tools to work
- MCP tools map 1:1 to REST endpoints (e.g., `aspy_stats` → `GET /api/stats`)
- Cortex tools query SQLite directly via proxy API

**MCP tool categories:**
| Category | Tools | Data Source |
|----------|-------|-------------|
| Current session | `aspy_stats`, `aspy_events`, `aspy_context` | Proxy memory |
| Lifetime search | `aspy_recall_*`, `aspy_lifetime` | SQLite (FTS5 + embeddings) |
| Real-time logs | `aspy_search` | JSONL files |
