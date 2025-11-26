# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0-alpha] - 2025-11-26

Initial alpha release.

### Added
- **Thinking block capture** - Dedicated panel showing Claude's reasoning in real-time
- **Token & cost tracking** - Cumulative session statistics in status bar
- **Demo mode** - Generate mock events for showcasing (`ANTHROPIC_SPY_DEMO=1`)
- **SSE delta accumulation** - Proper handling of streaming API responses
- Real-time TUI with tool calls, results, and API usage
- JSON Lines logging with daily rotation
- Vim-style navigation (j/k, arrow keys)
- Multi-platform binaries (Windows, macOS, Linux)

### Fixed
- Tool calls showing empty `{}` inputs (SSE delta accumulation)
- Demo mode graceful shutdown
