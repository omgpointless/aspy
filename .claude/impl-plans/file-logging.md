# File Logging Feature

**Status**: Planned
**Priority**: Medium
**Context**: Discussed during PR #5 merge prep

## Problem

Currently logs are ephemeral:
- TUI mode: Ring buffer lost on exit
- Headless: stdout only

Errors and system logs disappear — no persistent record for debugging.

## Proposed Config

```toml
[logging]
level = "info"
file = "./logs/anthropic-spy.log"      # All logs
error_file = "./logs/errors.log"       # Errors only
rotate = "daily"                        # or "size:10mb"
```

## Implementation Notes

Use `tracing-appender` for rolling file logs:

```rust
use tracing_appender::rolling::{RollingFileAppender, Rotation};

let file_appender = RollingFileAppender::new(Rotation::DAILY, "./logs", "system.log");
let error_appender = RollingFileAppender::new(Rotation::DAILY, "./logs", "errors.log");

tracing_subscriber::registry()
    .with(filter)
    .with(TuiLogLayer::new(log_buffer.clone()))
    .with(fmt::layer().with_writer(file_appender))
    .with(fmt::layer().with_writer(error_appender).with_filter(LevelFilter::ERROR))
    .init();
```

## Scope

- Add `tracing-appender` dependency
- Extend `LoggingConfig` struct with file options
- Update subscriber initialization in `main.rs`
- Add commented examples to config template
- Consider non-blocking file writes for performance
