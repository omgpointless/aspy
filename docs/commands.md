# Build & Development Commands

## Building

```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Check compilation without building
cargo check
```

## Running

```bash
# Run in development mode
cargo run

# Run in release mode (recommended for actual use)
cargo run --release

# Run in headless mode (no TUI, logs to stdout)
ANTHROPIC_SPY_NO_TUI=1 cargo run --release

# Run in demo mode (generate mock events for showcasing)
ANTHROPIC_SPY_DEMO=1 cargo run --release

# Run with debug logging
RUST_LOG=debug cargo run
```

## Demo Mode

Demo mode generates realistic mock events to showcase the TUI without needing a live Claude Code session. This is useful for:
- Recording GIFs/videos for documentation
- Testing UI changes without API costs
- Demonstrating features to others

The demo simulates a typical Claude Code session including:
- Thinking blocks showing Claude's reasoning
- Tool calls (Read, Edit, Bash, Glob, TodoWrite) with realistic inputs
- API usage events with token counts
- Progressive task completion

```bash
# Windows PowerShell
$env:ANTHROPIC_SPY_DEMO="1"; cargo run --release

# macOS/Linux
ANTHROPIC_SPY_DEMO=1 cargo run --release
```

## Code Quality

```bash
# Format code
cargo fmt

# Lint with clippy
cargo clippy

# Run with all clippy warnings
cargo clippy -- -W clippy::all
```

## Automatic Formatting

Install the `aspy` Claude Code plugin to enable automatic `cargo fmt` on Rust files after Write or Edit operations.

```bash
# Install the plugin from the project directory
/plugin marketplace add /mnt/c/Projects/anthropic-spy

# Restart Claude Code to load the plugin
# The plugin provides /aspy:stats and /aspy:tempcontext commands
# Plus automatic formatting hooks
```

See `.claude-plugin/` for plugin manifest and `hooks/spy/` for hook implementation.

## Testing with Claude Code

After starting the proxy, configure Claude Code in a separate terminal. Include your client ID in the URL path:

```powershell
# Windows PowerShell
$env:ANTHROPIC_BASE_URL="http://127.0.0.1:8080/dev-1"
claude
```

```bash
# macOS/Linux
export ANTHROPIC_BASE_URL=http://127.0.0.1:8080/dev-1
claude
```

The client ID (`dev-1` in these examples) must be configured in `~/.config/anthropic-spy/config.toml`. See [sessions.md](sessions.md) for configuration details.

## Configuration

### Config File

The primary configuration is in `~/.config/anthropic-spy/config.toml`:

```toml
# Proxy settings
bind_addr = "127.0.0.1:8080"
log_dir = "./logs"

# Define clients (each gets a unique URL path)
[clients.dev-1]
name = "Dev Laptop"
provider = "anthropic"

[clients.ci]
name = "CI Runner"
provider = "foundry"

# Define providers (upstream API endpoints)
[providers.anthropic]
base_url = "https://api.anthropic.com"

[providers.foundry]
base_url = "https://your-instance.services.ai.azure.com/anthropic"
```

See [sessions.md](sessions.md) for full configuration reference.

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ANTHROPIC_SPY_BIND` | `127.0.0.1:8080` | Proxy bind address (overrides config) |
| `ANTHROPIC_SPY_LOG_DIR` | `./logs` | Log file directory (overrides config) |
| `ANTHROPIC_SPY_NO_TUI` | `false` | Disable TUI (headless mode) |
| `ANTHROPIC_SPY_DEMO` | `false` | Enable demo mode (mock events) |
| `RUST_LOG` | `anthropic_spy=info` | Logging level |

## Code Style Guidelines

**Error Handling:**
- Prefer `Result<T, E>` with `?` operator
- Use `.context()` to add error context
- Avoid `.unwrap()` except where proven safe (rare)

**Async Code:**
- Use `async fn` for I/O operations
- `.await` immediately, don't store futures
- Spawn tasks with `tokio::spawn()` for concurrency

**Comments:**
- Explain WHY, not WHAT (code should be self-documenting)
- Document Rust patterns (Arc, Mutex, async boundaries)
- Include context that can't be inferred from code

**Dependencies:**
- Each dependency must be purposeful and well-maintained
- Document why each crate is used (see Cargo.toml comments)

## Commit Conventions

This project uses [Conventional Commits](https://www.conventionalcommits.org/).

Format: `<type>(<scope>): <description>`

Types: `feat`, `fix`, `refactor`, `chore`, `docs`, `test`, `perf`
Scopes: `proxy`, `tui`, `parser`, `storage`, `events`, `deps`

Examples:
- `feat(tui): add mouse scroll support for event list`
- `fix(proxy): implement SSE stream-through`
- `chore(deps): remove unused dependencies`
