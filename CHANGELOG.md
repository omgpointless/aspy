# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0] - 2025-12-01

Major feature release with views, statistics, themes, and configuration.

### Added
- **Views System** - Three main views (Events, Stats, Settings) with keyboard navigation
- **Statistics View** - 5-tab dashboard with gauges, charts, and sparklines
  - Overview tab: Session gauges and summary
  - Models tab: API call distribution by model
  - Tokens tab: Token usage breakdown
  - Tools tab: Tool call frequency analysis
  - Trends tab: Sparkline grid for metrics over time
- **Theme System** - 32 bundled themes with TOML-based custom theme support
  - Spy Dark/Light (flagship themes)
  - Popular themes: Dracula, Catppuccin, Nord, Gruvbox, Tokyo Night, etc.
  - Material family: Oceanic, Darker, Palenight, and more
  - Monokai family: Pro, Ristretto, Machine, Soda
  - Runtime theme switching via Settings view
- **Settings View** - Configuration UI for themes and layout presets
- **CLI Configuration Tool** - `anthropic-spy config` subcommand
  - `--init`: Interactive setup wizard
  - `--show`: Display effective configuration
  - `--edit`: Open config in editor
  - `--update`: Merge new defaults (with diff preview)
  - `--reset`: Reset to defaults
- **REST API** - Programmatic access to session data
  - `GET /api/stats` - Session statistics
  - `GET /api/events` - Event buffer with filtering
  - `GET /api/context` - Context window status
  - `GET /api/sessions` - Active session list
  - `POST /api/search` - Search past session logs
- **Multi-Client Routing** - Track multiple Claude Code instances
  - Named client configuration in config.toml
  - Provider backend routing (Anthropic, Foundry, Bedrock)
- **Context Warning Augmentation** - Inject usage alerts into responses
- **Layout Presets** - classic, reasoning, debug layouts

### Changed
- Configuration moved to `~/.config/anthropic-spy/config.toml`
- Themes extracted to `~/.config/anthropic-spy/themes/` on first run

### Documentation
- Added `docs/api-reference.md` - Complete REST API documentation
- Added `docs/themes.md` - Theme system guide
- Added `docs/cli-reference.md` - CLI tool reference
- Added `docs/views.md` - TUI views documentation
- Updated `docs/sessions.md` - Multi-client routing details
- Updated `ROADMAP.md` - Current progress and next steps

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
