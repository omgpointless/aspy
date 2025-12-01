# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0] - 2025-12-01

Initial stable release with full observability features.

### Core Observability
- **Proxy Server** - Intercepts Claude Code ↔ Anthropic API traffic
- **Tool Call Tracking** - Captures tool invocations with timing and correlation
- **Thinking Blocks** - Dedicated panel showing Claude's extended thinking in real-time
- **Token & Cost Tracking** - Cumulative session statistics with cost estimation
- **SSE Streaming** - Proper delta accumulation for streaming responses
- **JSON Lines Logging** - Session logs for post-hoc analysis

### Views & Statistics
- **Views System** - Three main views (Events, Stats, Settings) with keyboard navigation
- **Statistics View** - 5-tab dashboard with gauges, charts, and sparklines
  - Overview tab: Session gauges and summary
  - Models tab: API call distribution by model
  - Tokens tab: Token usage breakdown
  - Tools tab: Tool call frequency analysis
  - Trends tab: Sparkline grid for metrics over time

### Theme System
- **32 Bundled Themes** - Spy Dark/Light, Dracula, Catppuccin, Nord, Gruvbox, etc.
- **TOML Custom Themes** - Create your own themes with full semantic color control
- **Runtime Switching** - Change themes via Settings view or config

### Configuration
- **Config File** - `~/.config/aspy/config.toml`
- **CLI Tool** - `aspy config` with --init, --show, --edit, --update, --reset
- **Multi-Client Routing** - Track multiple Claude Code instances with named clients
- **Provider Backends** - Route to Anthropic, Foundry, Bedrock, etc.

### REST API
- `GET /api/stats` - Session statistics
- `GET /api/events` - Event buffer with filtering
- `GET /api/context` - Context window status
- `GET /api/sessions` - Active session list
- `POST /api/search` - Search past session logs

### Documentation
- `docs/api-reference.md` - Complete REST API documentation
- `docs/themes.md` - Theme system guide
- `docs/cli-reference.md` - CLI tool reference
- `docs/views.md` - TUI views documentation
- `docs/sessions.md` - Multi-client routing details

## [0.1.0-alpha] - 2025-11-26

Pre-release alpha.

### Added
- **Thinking block capture** - Dedicated panel showing Claude's reasoning in real-time
- **Token & cost tracking** - Cumulative session statistics in status bar
- **Demo mode** - Generate mock events for showcasing (`ASPY_DEMO=1`)
- **SSE delta accumulation** - Proper handling of streaming API responses
- Real-time TUI with tool calls, results, and API usage
- JSON Lines logging with daily rotation
- Vim-style navigation (j/k, arrow keys)
- Multi-platform binaries (Windows, macOS, Linux)

### Fixed
- Tool calls showing empty `{}` inputs (SSE delta accumulation)
- Demo mode graceful shutdown
