# Development Philosophy

**This document provides AI agents insight into the development philosophy of anthropic-spy, with important distinctions and rules to adhere to**

---

## Origin: Why This Project Exists

**Primary Goal:** Understand Claude Code's behavior by intercepting and visualizing all API interactions

**Secondary Goal:** Learn Rust through building something practical and useful

**Context:** This is the maintainer's first serious Rust project, coming from a .NET/TypeScript background

---

## The Learning Journey: .NET/TypeScript Dev → Rust

### Background Context

**Developer profile:**
- Primary background: C# / .NET, TypeScript
- Appreciates: Strong typing, explicitness, SOLID principles, clean architecture
- Current journey: Learning Rust through this practical project
- Opinion: Rust concepts are fascinating (ownership, borrowing, lifetimes) but take time to internalize
- npm ecosystem view: Skeptical (dependency hell), now appreciates Cargo's approach

**Mindset carried forward:**
The discipline from .NET development applies to Rust:
- Understand what dependencies you're bringing in
- Weight convenience vs. complexity vs. maintenance burden
- Type safety catches bugs at compile time
- Architecture matters even in smaller projects

**Learning approach:**
- Build something real, not just tutorials
- AI writes code, but human must understand it
- Iterate, experiment, break things to learn
- Ask "why" frequently

---

## Rust-Specific Philosophy

### Why Rust for This Project?

**Reasons:**
1. **Performance** - Proxy needs to be fast, minimal overhead
2. **Type Safety** - Strong typing prevents runtime errors
3. **Modern ecosystem** - Tokio for async, ratatui for TUI, serde for JSON
4. **Learning opportunity** - Practical project to understand Rust concepts
5. **Curiosity** - Coming from garbage-collected languages, wanted to experience manual memory management (done safely)

**Not because:**
- ❌ "Rust is cool" (it is, but that's not enough reason)
- ❌ "Everyone should use Rust" (wrong tool for many jobs)
- ❌ "To prove something" (just learning and building)

### Dependency Philosophy for Rust

**Different from zero-dependency TypeScript approach:**

In TypeScript, we avoided dependencies because of npm's issues. In Rust, the landscape is different:

**Use battle-tested crates when appropriate:**
- `tokio` - Async runtime (de facto standard)
- `axum` - Web framework (clean, well-maintained)
- `ratatui` - TUI framework (active, excellent API)
- `serde` - Serialization (industry standard)
- `anyhow` / `thiserror` - Error handling (idiomatic)

**Criteria for adding dependencies:**
1. **Is this the idiomatic Rust way?** (e.g., tokio for async)
2. **Is the crate well-maintained?** (check recent commits, issue responses)
3. **Does it solve a real problem?** (not just convenience)
4. **Is the API clean and learnable?** (good documentation)
5. **What's the dependency tree like?** (cargo tree to check)

**Current philosophy:**
Use quality crates for complex functionality (async runtime, TUI rendering), but keep the count reasonable. Don't reinvent Tokio, but don't add a crate for every small utility either.

---

## Code Quality Standards for Rust

### Type Safety and Explicitness

**Rust's type system is a strength, use it:**

```rust
// ✅ Good: Explicit types, clear intent
pub struct ProxyEvent {
    pub event_type: EventType,
    pub timestamp: SystemTime,
    pub data: EventData,
}

pub enum EventType {
    ToolCall,
    ToolResult,
    Request,
    Response,
}

// ❌ Bad: Using String for everything
pub struct ProxyEvent {
    pub event_type: String,  // What values are valid?
    pub data: String,        // What's the structure?
}
```

**Why explicit types matter:**
- Compiler catches invalid states at build time
- Self-documenting code
- Easier refactoring
- Pattern matching exhaustiveness

### Error Handling Pattern

**Rust forces you to handle errors, embrace it:**

```rust
// ✅ Good: Result with context
pub async fn proxy_handler(req: Request<Body>) -> Result<Response<Body>, ProxyError> {
    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|e| ProxyError::BodyReadError(e.to_string()))?;

    // Continue processing...
}

// ✅ Good: anyhow for main and simple cases
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    start_proxy(config).await?;
    Ok(())
}

// ❌ Bad: Unwrapping everywhere
let value = risky_operation().unwrap();  // Will panic!
```

**Error handling philosophy:**
- Use `Result<T, E>` for recoverable errors
- Use `?` operator for propagation
- Create custom error types with `thiserror` when needed
- Use `anyhow` for application-level errors (main, CLI)
- Only `unwrap()` when you've proven it can't fail (rare!)

### Ownership and Borrowing

**Learn by understanding patterns in this codebase:**

```rust
// Arc for shared ownership across threads
let event_tx_tui = event_tx.clone();
let event_tx_storage = event_tx.clone();

tokio::spawn(async move {
    // event_tx_tui moved into this task
    proxy::start_proxy(config, event_tx_tui).await
});

tokio::spawn(async move {
    // event_tx_storage moved into this task
    storage::run(event_tx_storage).await
});
```

**Key concepts to understand:**
- **Ownership** - Each value has one owner
- **Borrowing** - References (`&T`) let you use without owning
- **Move semantics** - Values move by default, clone when needed
- **Arc<T>** - Shared ownership with reference counting
- **Mutex<T>** - Interior mutability with locking

**Learning approach:**
When you see `Arc<Mutex<T>>`, understand why:
- `Arc` - Multiple tasks need to access the same data
- `Mutex` - Data needs to be mutated from multiple places

### Async/Await with Tokio

**This project is heavily async:**

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Tokio provides the runtime

    // Spawn concurrent tasks
    let proxy_handle = tokio::spawn(proxy::start_proxy(config));
    let storage_handle = tokio::spawn(storage::run(rx));

    // Wait for completion
    tokio::join!(proxy_handle, storage_handle);

    Ok(())
}
```

**Why async:**
- HTTP proxy needs to handle multiple concurrent requests
- TUI needs to respond to user input while processing events
- File I/O (logging) shouldn't block the TUI
- Efficient with system resources

**Key patterns:**
- `async fn` - Function returns a Future
- `.await` - Wait for a Future to complete
- `tokio::spawn` - Run a task concurrently
- `tokio::select!` - Wait on multiple async operations

---

## Code Comments: Educational Focus

### Inspiration: Quality Rust Codebases

**Study these for comment style:**
- Tokio source code
- Ratatui examples
- Serde documentation

**What they do well:**
- Explain WHY, not WHAT
- Call out non-obvious Rust patterns
- Document safety considerations
- Minimal but valuable comments

### ❌ BAD: AI-Generated Clutter

```rust
// NEW
// This is a new function
// This function processes events
fn process_event(event: ProxyEvent) {
    // We create a variable
    let event_type = event.event_type;

    // We match on the event type
    match event_type {
        // ...
    }
}
```

**Problems:**
- "NEW" markers (why is this here?)
- Obvious comments (code is self-documenting)
- States WHAT instead of WHY

### ✅ GOOD: Purposeful Rust Comments

```rust
/// Captures HTTP headers from Anthropic API requests/responses.
///
/// Security: Never logs actual API keys - only stores SHA-256 hash
/// for request correlation.
pub fn extract_request_headers(headers: &HeaderMap) -> CapturedHeaders {
    let mut captured = CapturedHeaders::default();

    if let Some(api_key) = headers.get("x-api-key") {
        // Hash API key for security - full key never stored or logged
        let mut hasher = Sha256::new();
        hasher.update(api_key.as_bytes());
        let hash = hasher.finalize();
        captured.api_key_hash = Some(format!("{:x}", hash)[..16].to_string());
    }

    captured
}
```

**Why this is good:**
- Doc comment explains purpose
- Security rationale is clear
- Comment explains WHY we hash (not just that we do)
- Code structure is self-documenting

### ✅ GOOD: Explaining Rust Patterns

```rust
// Use Arc<Mutex<>> for shared mutable state across async tasks.
// Arc allows multiple ownership, Mutex provides interior mutability.
let app = Arc::new(Mutex::new(App::new(log_buffer)));

// Clone the Arc (cheap - just increments ref count) for the render loop
let app_render = Arc::clone(&app);
let app_input = Arc::clone(&app);

// Each task gets its own Arc clone, all point to same App
tokio::spawn(async move {
    // app_render moved into this task
    render_loop(app_render).await
});
```

**Why this is good:**
- Explains the Rust pattern (Arc<Mutex<>>)
- Clarifies why it's needed (shared mutable state)
- Shows that Arc::clone is cheap
- Educational for someone learning Rust

### Comment Guidelines

**DO comment:**
- Non-obvious Rust patterns (Arc, Mutex, lifetimes)
- Why certain approaches were chosen
- Safety considerations
- Performance implications
- Ownership transfers that aren't obvious
- Protocol/API quirks (Anthropic API specifics)

**DON'T comment:**
- Obvious operations (`// Create variable`)
- What the code does (code should be self-evident)
- "NEW" or AI generation markers
- Every single line

**The test:**
> "If I removed this comment, would someone learning Rust understand?"
> - Yes → Maybe the comment is redundant
> - No → Good comment, keep it

---

## Architecture: Learning Through Building

### Event-Driven Design

**Why this architecture:**

```
HTTP Request → Proxy → Parser → Events → [TUI, Storage]
```

**Benefits:**
- **Loose coupling** - TUI and Storage don't know about each other
- **Easy to extend** - Add new event consumers
- **Learning** - Demonstrates Rust's channel patterns

**Rust concepts demonstrated:**
- `mpsc::channel()` - Multi-producer, single-consumer
- `async` message passing
- Event-driven architecture

### State Management in TUI

**Pattern used:**

```rust
pub struct App {
    events: Vec<ProxyEvent>,
    selected: usize,
    show_detail: bool,
    stats: Stats,
    // ... more state
}

impl App {
    pub fn handle_key(&mut self, key: KeyCode) {
        // Mutable self - state changes in place
        match key {
            KeyCode::Down => self.next(),
            KeyCode::Up => self.previous(),
            // ...
        }
    }
}
```

**Why this pattern:**
- **Centralized state** - All TUI state in one struct
- **Mutation** - `&mut self` for state changes
- **Clear ownership** - App owns all TUI state

**Learning point:**
This is how you manage mutable state in Rust - centralize it and use `&mut` methods.

### Custom Tracing Layer

**Challenge:** TUI runs in alternate screen buffer, stdout logs break the display

**Solution:** Custom tracing layer that captures logs to memory

```rust
pub struct TuiLogLayer {
    buffer: LogBuffer,  // Shared via Arc<Mutex<>>
}

impl<S> Layer<S> for TuiLogLayer {
    fn on_event(&self, event: &Event, _ctx: Context) {
        // Intercept log events before they hit stdout
        let entry = LogEntry::from(event);
        self.buffer.add(entry);
    }
}
```

**Rust concepts:**
- **Traits** - Implementing tracing's `Layer` trait
- **Generics** - `Layer<S>` where S is subscriber
- **Shared state** - LogBuffer via Arc<Mutex<>>

**Learning point:**
Traits let you extend behavior (here, custom logging) in a type-safe way.

---

## AI-Assisted Development: The Discipline

### How We Work With AI for Rust Learning

**1. AI Generates, Human Learns**
- AI writes Rust code quickly
- Human reviews and asks "why this pattern?"
- Iterate until human understands the code
- Experiment with changes to verify understanding

**2. Explain Rust Concepts**
- Don't just provide code, explain the Rust-specific parts
- Why Arc here? Why Mutex? Why clone?
- What ownership rules apply?
- What does the compiler check?

**3. Reference Documentation**
- Link to Rust Book sections
- Reference official crate docs (docs.rs)
- Explain with analogies to C#/TypeScript where helpful

**4. Embrace Compiler Errors**
- Rust compiler errors are educational
- Don't just fix, explain what the error means
- Understand the borrow checker's reasoning

**5. Iterate to Learn**
- Try different approaches
- See what compiles, what doesn't
- Understand why Rust prevents certain patterns

**Result:**
Rapid development without sacrificing learning. AI generates correct Rust code, but human understands and can maintain it.

---

## Presenting Quality Without Hype

### Technical Decisions: Explain, Don't Proclaim

**❌ Proclamation:**
```markdown
REVOLUTIONARY Rust implementation using CUTTING-EDGE async paradigms
```

**✅ Explanation:**
```markdown
## Technical Decisions

**Rust**
Uses Rust for type safety and performance. As a learning project, demonstrates
async/await with Tokio, custom trait implementations, and event-driven architecture.

**Async Runtime**
Tokio is the de facto standard for async Rust. Enables efficient handling of
concurrent HTTP requests, file I/O, and TUI updates without blocking.

**Minimal Dependencies**
Uses well-maintained crates for complex functionality (Tokio, ratatui, axum) but
keeps the total count reasonable. Each dependency is intentional.
```

### Learning Journey: Be Honest

**✅ Good:**
```markdown
## About This Project

This is a learning project - my first serious Rust implementation coming from
a .NET/TypeScript background. The goal is to build something useful (observability
for Claude Code) while learning Rust concepts through practical application.

Most code is AI-generated, but every piece is reviewed and understood. The
architecture demonstrates key Rust patterns: ownership, async/await, traits,
error handling, and channels.
```

**❌ Bad:**
```markdown
EXPERT-LEVEL Rust implementation showcasing ADVANCED patterns
```

---

## Messaging Guidelines

### Internal Voice (Development Sessions)

**Use for:** Working with AI, discussing decisions

**Characteristics:**
- "Let's understand why this uses Arc<Mutex<>>"
- "Explain the lifetime annotations here"
- "What would the compiler error be if we tried X?"
- Learning-focused, questioning

### External Voice (Documentation, Users)

**Use for:** README, docs, issues

**Characteristics:**
- Honest about learning journey
- Explains decisions clearly
- Shows code examples
- Acknowledges limitations

**Example:**
> "This project uses Rust for observability. As a learning project, it demonstrates
> practical Rust patterns like async/await, custom traits, and event-driven
> architecture. Feedback and contributions welcome!"

---

## Quality Metrics for Rust

### How We Know We're On Track

**Code Quality:**
- ✅ Compiles without warnings (except dead code during development)
- ✅ Minimal use of `unwrap()` (only where proven safe)
- ✅ Error handling with `Result<T, E>`
- ✅ Clear ownership patterns (no confusing Arc chains)
- ✅ Reasonable dependency count

**Architecture:**
- ✅ Event-driven design is clear
- ✅ Concerns are separated (proxy, parser, TUI, storage)
- ✅ Async boundaries are explicit
- ✅ State management is centralized

**Documentation:**
- ✅ Explains Rust patterns used
- ✅ Honest about learning journey
- ✅ Code examples compile and work
- ✅ README helps users get started quickly

**Learning:**
- ✅ Can explain why Arc<Mutex<>> is used
- ✅ Understands ownership in the codebase
- ✅ Knows what each dependency provides
- ✅ Can modify code with confidence

---

## For AI Agents: What This Means

### When Working On This Codebase

**Your role:**
- Write idiomatic Rust code
- Explain Rust-specific patterns
- Help developer learn through building
- Provide educational context for decisions

**Your outputs should:**
- Compile with Rust stable
- Use appropriate error handling
- Include WHY comments for Rust patterns
- Explain ownership/borrowing when relevant
- Reference Rust documentation when helpful

**When suggesting code:**
- Explain the Rust concepts used
- Compare to C#/TypeScript patterns where helpful
- Show why certain approaches don't work in Rust
- Demonstrate idiomatic patterns

**When explaining:**
- Use analogies to garbage-collected languages
- Explain compiler reasoning
- Show what errors would occur and why
- Build understanding incrementally

---

## Summary: The Philosophy in One Paragraph

We're building an observability tool for Claude Code using Rust as a learning journey. Coming from .NET/TypeScript, we bring discipline around type safety and clean architecture. We use AI to generate code quickly, but every piece must be understood and explained in Rust terms. We use quality Rust crates appropriately (Tokio, ratatui, axum) rather than avoiding all dependencies. We document Rust patterns for learning purposes. We're honest about this being a learning project and present quality through clean code, not hype. The goal is to build something useful while genuinely learning Rust concepts through practical application.

---

*Share this document with all AI agents working on this project to ensure consistent standards, philosophy, and learning-focused development.*

*Last updated: 2025-11-24*
