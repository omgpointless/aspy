# Rust Learning Path with Anthropic Spy

Use this project as a structured path for learning Rust concepts hands-on.

## Phase 1: Understanding the Basics (Week 1)

### Goal: Get the project running and understand the high-level architecture

**Tasks:**
1. ✅ Build and run the project following QUICKSTART.md
2. ✅ Use it with Claude Code and observe tool calls
3. ✅ Read `src/main.rs` - understand the entry point and task spawning
4. ✅ Read `src/events.rs` - understand enums and data structures

**Key Concepts to Learn:**
- **Cargo**: Rust's build tool and package manager
- **Modules**: How Rust code is organized
- **Enums**: Algebraic data types (like TypeScript's discriminated unions)
- **Structs**: Custom data structures
- **Result<T, E>**: Error handling pattern

**Exercises:**
1. Add a new event type to `ProxyEvent` enum
2. Add a new field to `Stats` struct and update the display
3. Change the proxy port in the config
4. Modify the TUI colors in `ui.rs`

## Phase 2: Ownership and Borrowing (Week 2)

### Goal: Understand Rust's ownership system

**Files to Study:**
- `src/parser/mod.rs` - See `Arc<Mutex<>>` for shared state
- `src/proxy/mod.rs` - See `.clone()` and ownership transfer
- `src/tui/mod.rs` - See mutable and immutable references

**Key Concepts:**
- **Ownership**: Every value has one owner
- **Borrowing**: References that don't transfer ownership
- **Move semantics**: Values move by default
- **Copy vs Clone**: Different ways to duplicate data
- **Lifetimes**: How long references are valid

**Exercises:**
1. Try removing `.clone()` calls and understand the compile errors
2. Add a method to `ProxyState` that borrows data instead of taking ownership
3. Create a function that takes a reference to an event instead of moving it
4. Experiment with `&str` vs `String` in function signatures

**Reading:**
- [The Rust Book - Chapter 4: Ownership](https://doc.rust-lang.org/book/ch04-00-understanding-ownership.html)
- [The Rust Book - Chapter 10.3: Lifetimes](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html)

## Phase 3: Error Handling (Week 3)

### Goal: Master Rust's error handling patterns

**Files to Study:**
- `src/storage/mod.rs` - Using `?` operator and `Context`
- `src/proxy/mod.rs` - Custom error types with `IntoResponse`
- `src/parser/mod.rs` - Handling JSON parsing errors

**Key Concepts:**
- **Result<T, E>**: Success or error
- **Option<T>**: Presence or absence
- **? operator**: Early return on error
- **anyhow::Context**: Adding error context
- **thiserror**: Deriving custom errors

**Exercises:**
1. Create a custom error type for the parser
2. Add error context to all `.context()` calls with meaningful messages
3. Handle a specific error case differently (e.g., file not found vs permission denied)
4. Add retry logic for transient network errors

**Reading:**
- [The Rust Book - Chapter 9: Error Handling](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
- [anyhow documentation](https://docs.rs/anyhow/)
- [thiserror documentation](https://docs.rs/thiserror/)

## Phase 4: Async Rust with Tokio (Week 4)

### Goal: Understand asynchronous programming in Rust

**Files to Study:**
- `src/main.rs` - `#[tokio::main]` and `tokio::spawn`
- `src/tui/mod.rs` - `tokio::select!` for multiplexing
- `src/proxy/mod.rs` - Async HTTP handling with axum

**Key Concepts:**
- **async fn**: Functions that return Futures
- **await**: Waiting for async operations
- **tokio::spawn**: Running tasks concurrently
- **tokio::select!**: Multiplexing async operations
- **mpsc channels**: Message passing between tasks

**Exercises:**
1. Add a periodic task that runs every 10 seconds
2. Implement a timeout for API requests
3. Add a graceful shutdown mechanism with `tokio::signal`
4. Create a new async task that monitors file system changes

**Reading:**
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Async Book](https://rust-lang.github.io/async-book/)
- [The Rust Book - Chapter 16: Concurrency](https://doc.rust-lang.org/book/ch16-00-concurrency.html)

## Phase 5: Traits and Generics (Week 5)

### Goal: Understand Rust's type system and polymorphism

**Files to Study:**
- `src/events.rs` - Deriving traits (`Clone`, `Debug`, `Serialize`)
- `src/parser/models.rs` - `#[serde(tag)]` for tagged enums
- `src/proxy/mod.rs` - Implementing `IntoResponse` trait

**Key Concepts:**
- **Traits**: Similar to interfaces in other languages
- **Derive macros**: Automatic trait implementation
- **Generic functions**: Functions that work with multiple types
- **Trait bounds**: Constraining generic types
- **Associated types**: Types within traits

**Exercises:**
1. Create a custom trait for all event types
2. Implement `Display` trait for `ProxyEvent`
3. Write a generic function that works with any serializable type
4. Create a trait for formatting events in different ways

**Reading:**
- [The Rust Book - Chapter 10: Generics](https://doc.rust-lang.org/book/ch10-00-generics.html)
- [The Rust Book - Chapter 10.2: Traits](https://doc.rust-lang.org/book/ch10-02-traits.html)

## Phase 6: Real-World Features (Week 6+)

### Goal: Add significant features to the project

**Enhancement Ideas:**

### 6.1: Add Filtering
**Skills:** Pattern matching, state management
- Add a filter text box to the TUI
- Filter events by tool name, status, or text search
- Learn: `String` methods, pattern matching, TUI input handling

### 6.2: Request Replay
**Skills:** HTTP clients, file I/O, async programming
- Save complete requests to replay later
- Add a "replay" mode that resends saved requests
- Learn: File serialization, HTTP client usage

### 6.3: Configuration File
**Skills:** Serde, file I/O, error handling
- Support a `config.toml` file for settings
- Learn: TOML parsing with serde, merging config sources

### 6.4: Web Dashboard
**Skills:** Web development, shared state
- Add a web server alongside the TUI
- Display events in a browser UI
- Learn: Axum routing, HTML templating, WebSockets

### 6.5: Database Storage
**Skills:** SQL, connection pooling
- Replace JSON files with SQLite
- Add querying capabilities
- Learn: sqlx or rusqlite, database design

### 6.6: Testing
**Skills:** Unit tests, integration tests, mocking
- Add comprehensive tests for each module
- Mock the Anthropic API for testing
- Learn: `#[cfg(test)]`, `#[test]`, test organization

## Rust Resources by Topic

### General Learning
- 📚 [The Rust Book](https://doc.rust-lang.org/book/) - Start here!
- 📚 [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- 📚 [Rustlings](https://github.com/rust-lang/rustlings) - Interactive exercises

### Async Rust
- 📚 [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- 📚 [Async Book](https://rust-lang.github.io/async-book/)
- 📹 [Jon Gjengset - Tokio Video](https://www.youtube.com/watch?v=o2ob8zkeq2s)

### Web Development
- 📚 [Axum Examples](https://github.com/tokio-rs/axum/tree/main/examples)
- 📚 [Actix Web Book](https://actix.rs/docs/)

### TUI Development
- 📚 [Ratatui Book](https://ratatui.rs/)
- 📚 [Ratatui Examples](https://github.com/ratatui-org/ratatui/tree/main/examples)

### Advanced Topics
- 📚 [Rust Atomics and Locks](https://marabos.nl/atomics/) - Concurrency deep dive
- 📚 [Rust for Rustaceans](https://rust-for-rustaceans.com/) - Intermediate/advanced book
- 📹 [Jon Gjengset's YouTube](https://www.youtube.com/c/JonGjengset) - Excellent tutorials

## Tips for Learning Rust

### 1. Embrace the Compiler
The Rust compiler is your teacher. Read error messages carefully - they often explain exactly what's wrong and how to fix it.

```rust
error[E0382]: use of moved value: `data`
  --> src/main.rs:10:5
   |
8  |     let other = data;
   |                 ---- value moved here
9  |
10 |     println!("{}", data);
   |                    ^^^^ value used here after move
```

### 2. Fight the Borrow Checker (and Lose)
The borrow checker will frustrate you at first. This is normal! It's teaching you to think about memory safety. When you hit a wall:
1. Read the error message completely
2. Draw out ownership flow on paper
3. Ask "who owns this data and when?"
4. Consider using `.clone()` to unblock yourself (optimize later)

### 3. Start Small, Iterate
Don't try to write perfect Rust code immediately. Start with:
- Using `.clone()` liberally
- Using `Box`, `Rc`, or `Arc` to bypass borrow checker initially
- Converting everything to `String` instead of dealing with `&str`

Then refactor as you learn more efficient patterns.

### 4. Read Lots of Code
- Browse [lib.rs](https://lib.rs) for well-written crates
- Read Rust Standard Library source code
- Study examples in popular projects

### 5. Build Things
This project is your playground! Don't just read about Rust - modify this code, break things, fix them, and experiment.

## Common Gotchas Coming from JS/TS

| Concept | JavaScript/TypeScript | Rust |
|---------|----------------------|------|
| **Variables** | `let x = 5; x = 6;` works | Variables are immutable by default, use `let mut` |
| **Strings** | One string type | `String` (owned) vs `&str` (borrowed) |
| **Arrays** | Dynamic by default | `Vec<T>` is dynamic, `[T; N]` is fixed-size |
| **null** | `null` or `undefined` | `Option<T>` - explicit handling required |
| **Errors** | Try/catch | `Result<T, E>` and `?` operator |
| **async** | Promises, single runtime | Futures, explicit runtime (tokio) |
| **Copying** | Values copied/referenced automatically | Explicit `Clone` or `Copy` |
| **Memory** | Garbage collected | Ownership system, no GC |

## Tracking Your Progress

As you work through this learning path:

- ✅ Mark off completed exercises
- 📝 Keep notes on concepts that confuse you
- 💡 Document "aha!" moments
- 🐛 When you get stuck, paste the error into GitHub issues or forums
- 🎯 Set weekly goals for features to add

## Getting Help

### Where to Ask Questions
- [Rust Users Forum](https://users.rust-lang.org/)
- [Rust Discord](https://discord.gg/rust-lang)
- [r/rust](https://reddit.com/r/rust) subreddit
- [Stack Overflow - rust tag](https://stackoverflow.com/questions/tagged/rust)

### How to Ask
1. Share your code (use [Rust Playground](https://play.rust-lang.org/))
2. Include the full error message
3. Explain what you tried
4. Mention your background ("coming from TypeScript")

---

**Remember:** Everyone struggles with Rust at first. The learning curve is real, but once it clicks, you'll write more confident and correct code than ever before. Stick with it! 🦀
