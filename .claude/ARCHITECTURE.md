# anthropic-spy Architecture

**Purpose:** This document explains the architectural principles and patterns that make anthropic-spy maintainable, composable, and extensible. For implementation details, see CLAUDE.md. For future extensibility, see EXTENSIONS_VISION.md.

---

## Core Principle: Composition Over Inheritance

anthropic-spy uses **Rust's trait system** to compose behaviors instead of building inheritance hierarchies. Components gain capabilities by implementing traits, not by inheriting from base classes.

**Why this matters:**
- Features are independent and testable
- Adding behavior doesn't modify existing code
- Components opt into what they need
- No "god objects" accumulating responsibilities

**Example:**
```rust
// A panel gets scrolling by implementing Scrollable
impl Scrollable for EventsPanel {
    fn scroll_state(&self) -> &ScrollState { &self.scroll }
    fn scroll_state_mut(&mut self) -> &mut ScrollState { &mut self.scroll }
}

// It gets clipboard support by implementing Copyable
impl Copyable for EventsPanel {
    fn copy_content(&self) -> Option<String> {
        self.selected_event().map(|e| format!("{:?}", e))
    }
}

// Traits don't know about each other - pure composition
```

---

## Layered Architecture: Kernel, Userland, User Space

The system is organized in three conceptual layers:

### Kernel (Core Infrastructure)
**Cannot be disabled. App doesn't function without these.**

- `proxy/` - HTTP interception, SSE streaming
- `parser/` - Protocol extraction, tool correlation
- `events.rs` - Event system (mpsc channels)
- `storage/` - JSON Lines persistence
- `logging/` - Custom tracing layer
- `tui/mod.rs`, `tui/app.rs` - Event loop, state container

**Decision:** Is this fundamental to operation? → Kernel

### Userland (Optional Features)
**Config-toggleable. Enhance UX but aren't required.**

- `proxy/augmentation/` - Stream transformations (context warnings, future: loop detection)
- `tui/components/` - Specific panels (events, logs, thinking, themes, settings)
- `tui/views/` - Full-screen compositions (main, settings, stats)
- `theme/` - Theme system

**Decision:** Can the app function without this specific feature? → Userland (must have config toggle)

### User Space (Custom Extensions)
**Completely external. Users bring their own.**

- Custom theme TOML files
- Future: Hook scripts (see EXTENSIONS_VISION.md Phase 4)
- Future: Custom slash commands

**Decision:** Is this user-provided content? → User Space

---

## System Flow

```
Claude Code → Proxy → Parser → Events → [Consumers]
                ↓        ↓        ↓           ↓
           Forward   Extract  Broadcast    TUI + Storage
             SSE    Tool Calls   mpsc      Display  .jsonl
```

**Key Characteristics:**
1. **Streaming-first:** SSE responses tee to client immediately while accumulating for parsing
2. **Event-driven:** Loose coupling via mpsc channels, easy to add consumers
3. **Async throughout:** Tokio runtime, no blocking operations
4. **Correlation:** Parser tracks tool_use_id → tool_result to measure execution time

---

## Trait-Based Behaviors

Components compose capabilities by implementing traits. Each trait is self-contained and discoverable.

### Current Traits (`tui/traits/`)

**Scrollable** (`scrollable.rs`)
- Provides: `scroll_up()`, `scroll_down()`, `scroll_to_top()`, `scroll_to_bottom()`
- Used by: EventsPanel, LogsPanel, DetailPanel, ThinkingPanel

**Copyable** (`copyable.rs`)
- Provides: `copy_content()`, `copy_to_clipboard()`
- Used by: EventsPanel, DetailPanel, ThinkingPanel

**Focusable** (`focusable.rs`)
- Provides: `is_focused()`, `set_focus()`, `clear_focus()`
- Used by: All interactive panels

**Interactive** (`interactive.rs`)
- Provides: `handle_key_event()`, `handle_mouse_event()`
- Used by: EventsPanel, ThemeListPanel

**Pattern:** One file per trait, organized by behavior. When someone asks "where's the scrolling code?", the answer is obvious: `tui/traits/scrollable.rs`.

---

## Component Organization

### Components Own Their State

**Anti-pattern (old architecture):**
```rust
// ❌ Centralized state in app.rs
pub struct App {
    events: Vec<Event>,
    events_scroll: usize,
    theme_list_scroll: usize,
    settings_selected_index: usize,
    // ... app.rs becomes a god object
}
```

**Correct pattern (current architecture):**
```rust
// ✅ Components own their state
pub struct EventsPanel {
    events: Vec<Event>,
    scroll: ScrollState,        // Component owns scroll state
    selected: Option<usize>,
}

pub struct ThemeListPanel {
    themes: Vec<Theme>,
    scroll: ScrollState,        // Independent state
    selected: Option<usize>,
}

pub struct App {
    events: Vec<Event>,         // Cross-cutting data only
    active_view: View,
    stats: Stats,
    // Components manage themselves
}
```

**Why:** Components are self-contained, reusable, and testable. No tight coupling to App.rs.

### Views Compose Components

```rust
// tui/views/settings.rs
pub fn render_settings_view(frame: &mut Frame, app: &App) {
    let theme_list = ThemeListPanel::new(app.themes());
    let settings_form = SettingsPanel::new(app.config());

    // Layout logic
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    theme_list.render(frame, chunks[0]);
    settings_form.render(frame, chunks[1]);
}
```

**Pattern:** Views are compositions. Components are building blocks. Dependencies flow inward (views depend on components, not vice versa).

---

## Data Flow Examples

### Tool Call Correlation

```
1. API Response arrives with tool_use block
   → Parser extracts: { id: "toolu_123", name: "Read", input: {...} }
   → Stores in pending_calls: HashMap<id, (name, timestamp)>
   → Emits ProxyEvent::ToolCall

2. Claude Code executes tool, sends result in next Request
   → Parser extracts: { tool_use_id: "toolu_123", content: "..." }
   → Looks up in pending_calls by tool_use_id
   → Calculates duration: now - stored_timestamp
   → Removes from pending_calls
   → Emits ProxyEvent::ToolResult { duration, success }

3. TUI receives both events
   → Displays tool call with execution time
   → Updates stats (total tool calls, avg duration)
```

**Why:** Correlation across multiple HTTP requests requires stateful tracking. Parser owns this state via `Arc<Mutex<HashMap>>`.

### SSE Streaming

```
1. Claude Code sends POST /v1/messages with stream=true
2. Proxy forwards request to Anthropic API
3. Response arrives as Server-Sent Events stream
4. Proxy tees stream:
   → Forwards chunks to Claude Code immediately (low latency)
   → Accumulates copy in background task for parsing
5. Stream completes
6. Parser extracts tool_use blocks from accumulated JSON
7. Events emitted to TUI/Storage

Why: Streaming preserves real-time token delivery while still capturing full context for analysis.
```

---

## Helper Organization

Complex single-purpose logic lives in feature-specific helper modules:

```
proxy/helpers/    # Proxy-specific utilities
  └── sse.rs      # SSE parsing, chunk accumulation

parser/helpers/   # Parser-specific utilities
  └── correlation.rs  # Tool call correlation tracking

tui/helpers/      # TUI-specific utilities
  └── format.rs   # Token/duration formatting
```

**Why:** Feature-based organization prevents flat directories with 60 helpers. Clear namespace: `tui/helpers/format.rs` is obviously TUI formatting, not proxy formatting.

---

## Extension Points

### Adding a New Behavior (Trait)

**When:** You need a capability that multiple components will use

**Steps:**
1. Create `tui/traits/your_behavior.rs`
2. Define trait with required methods + default implementations
3. Implement for components that need it
4. Input handling checks for trait, not concrete type

**Example:** Adding `Filterable` behavior
```rust
// tui/traits/filterable.rs
pub trait Filterable {
    fn apply_filter(&mut self, filter: Filter);
    fn clear_filter(&mut self);
    fn is_filtered(&self) -> bool { /* default */ }
}

// tui/components/events_panel.rs
impl Filterable for EventsPanel {
    fn apply_filter(&mut self, filter: Filter) {
        self.filter = Some(filter);
        self.refresh_visible_events();
    }
    // ...
}
```

### Adding a New Component

**When:** You need a new UI panel or widget

**Steps:**
1. Create `tui/components/your_component.rs`
2. Define struct with component state
3. Implement needed traits (Scrollable, Copyable, etc.)
4. Add to view that uses it

**Key:** Don't add component state to `app.rs`. Let the component own it.

### Adding a New Augmentor

**When:** You want to transform/annotate the API stream

**Steps:**
1. Create `proxy/augmentation/your_augmentor.rs`
2. Implement `Augmentor` trait
3. Manage own state (don't use global state)
4. Add config flag to enable/disable
5. Register in augmentation pipeline

**Example:** See `proxy/augmentation/context_warning.rs` for pattern

### Adding a New Event Type

**When:** You need to capture new data from API interactions

**Steps:**
1. Add variant to `ProxyEvent` enum in `events.rs`
2. Update parser to extract and emit it
3. Update TUI match arms to display it
4. Storage automatically logs it (JSON Lines)

**Why this works:** Event system is the central nervous system. New events flow through existing infrastructure.

---

## Performance Characteristics

**Memory:**
- Log buffer: ~200 KB (1000 entries × ~200 bytes)
- Event channels: Bounded at 1000, drained by consumers
- Parser state: HashMap of pending tool calls (~50 bytes per entry)

**Latency:**
- Proxy overhead: 1-2ms (header extraction + body buffering)
- TUI rendering: 100ms (10 FPS)
- Event processing: Async, non-blocking

**Throughput:**
- Limited by Anthropic API rate limits (not proxy)
- Proxy can theoretically handle ~1000 req/s
- Actual: ~1-10 req/s (Claude Code's natural rate)

---

## Design Patterns in Use

### Event-Driven Architecture
Loose coupling via mpsc channels. Easy to add new consumers without modifying producers.

### State-Based Input Handling
```rust
pressed_keys: HashSet<KeyCode>

fn handle_key_press(&mut self, key: KeyCode) -> bool {
    if self.pressed_keys.contains(&key) {
        return false;  // Already pressed, ignore
    }
    self.pressed_keys.insert(key);
    true  // New press, trigger action
}
```
**Why:** Prevents double-triggering regardless of poll rate. No debouncing needed.

### Ring Buffers
```rust
if log_entries.len() >= MAX_ENTRIES {
    log_entries.pop_front();  // Drop oldest
}
log_entries.push_back(new_entry);
```
**Why:** Bounded memory usage for long-running processes.

### Graceful Shutdown
```rust
let (shutdown_tx, shutdown_rx) = oneshot::channel();

// Proxy listens
axum::serve(listener, app)
    .with_graceful_shutdown(async { shutdown_rx.await.ok(); })
    .await?;

// TUI signals when quitting
shutdown_tx.send(());
```
**Why:** Clean termination, no force-close needed.

---

## Alignment with EXTENSIONS_VISION.md

This architecture enables future extensions without modifications to core systems:

**HTTP API (Phase 1):**
- Queries existing event storage
- No new state containers
- Kernel layer (always available)

**MCP Server (Phase 3):**
- Wraps HTTP API calls
- Userland layer (optional npm package)
- No coupling to proxy internals

**Hook Scripts (Phase 4):**
- Receive events via webhooks
- User space (completely external)
- Interact via public APIs only

**Pattern:** Extensions compose existing capabilities. Kernel provides data, userland consumes, user space customizes.

---

## Migration Status

The codebase is evolving toward these patterns:

**Current State:**
- ✅ Trait system in place (Scrollable, Copyable, Focusable, Interactive)
- ✅ Components own their state (EventsPanel, ThemeListPanel, etc.)
- ✅ Feature-based helpers (proxy/helpers/, tui/helpers/)
- ⚠️ Some legacy state still in app.rs (being migrated)
- ⚠️ Not all panels fully implement all traits yet

**Migration Strategy:**
1. New features MUST follow these patterns
2. Bug fixes align with patterns when touching related code
3. Refactoring brings modules fully into compliance
4. Don't break working code unless actively improving that area

**Note:** When you see code that doesn't match these patterns, that's tech debt from before the architectural pivot (Nov 29-30, 2025). Improve it when you touch it, but don't rewrite the world.

---

## Key Takeaways

1. **Composition over inheritance** - Traits enable capability mixing without coupling
2. **Components own state** - No god objects, better encapsulation
3. **Kernel/userland separation** - Core vs. optional features
4. **Event-driven flow** - Loose coupling, easy extensibility
5. **Feature-based organization** - "Ah, here it is" discoverability

**For implementation details:** See CLAUDE.md
**For extension roadmap:** See EXTENSIONS_VISION.md
**For project status:** See PROJECT_STATUS.md
