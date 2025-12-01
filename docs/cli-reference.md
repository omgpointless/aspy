# CLI Reference

aspy provides a command-line interface for configuration management.

## Basic Usage

```bash
# Run the proxy (normal operation)
aspy

# Run in demo mode (mock events for testing)
ASPY_DEMO=1 aspy

# Configuration management
aspy config [OPTIONS]
```

## Configuration Commands

### Interactive Setup

```bash
aspy config --init
```

Launches an interactive setup wizard that guides you through:
1. **Theme selection** - Choose from popular themes or view all 32 options
2. **Proxy settings** - Configure bind address
3. **Features** - Enable/disable storage, thinking panel, stats

The wizard creates `~/.config/aspy/config.toml` with your choices.

### Show Configuration

```bash
aspy config --show
```

Displays the effective configuration after merging:
1. Environment variables (highest priority)
2. Config file
3. Built-in defaults (lowest priority)

**Example output:**
```
# Effective configuration (env > file > defaults)

theme = "Spy Dark"
use_theme_background = true
context_limit = 147000
bind_addr = "127.0.0.1:8080"
log_dir = "./logs"

[features]
storage = true
thinking_panel = true
stats = true

[augmentation]
context_warning = true
context_warning_thresholds = [60, 80, 85, 90, 95]

# Source: /home/user/.config/aspy/config.toml
```

### Edit Configuration

```bash
aspy config --edit
```

Opens the config file in your preferred editor (detected from `$EDITOR` or `$VISUAL`, falls back to `nano` on Unix, `notepad` on Windows).

If no config file exists, creates one with defaults first.

### Update Configuration

```bash
aspy config --update
```

Merges new defaults into your existing config file while preserving your customizations. Useful after upgrading anthropic-spy to get new configuration options.

Shows a colored diff preview before applying:
- Green lines = additions
- Red lines = removals

Creates a backup at `config.toml.bak` before making changes.

### Reset Configuration

```bash
aspy config --reset
```

Overwrites the config file with defaults. Prompts for confirmation if the file exists.

### Show Config Path

```bash
aspy config --path
```

Prints the config file path:
```
/home/user/.config/aspy/config.toml
```

## Configuration File Format

Location: `~/.config/aspy/config.toml`

```toml
# Theme (use 't' in TUI to see all options)
theme = "Spy Dark"
use_theme_background = true

# Layout preset: classic, reasoning, debug
preset = "classic"

# Context window limit for the gauge
context_limit = 147000

# Proxy bind address
bind_addr = "127.0.0.1:8080"

# Log directory for session files
log_dir = "./logs"

# Feature flags
[features]
storage = true          # Write events to JSONL files
thinking_panel = true   # Show Claude's extended thinking
stats = true            # Track tokens, costs, tool distribution

# Augmentation (response modifications)
[augmentation]
context_warning = true  # Inject usage alerts when context fills up
context_warning_thresholds = [60, 80, 85, 90, 95]

# Logging configuration
[logging]
level = "info"          # trace, debug, info, warn, error

# Multi-client routing (optional)
[clients.dev-1]
name = "Dev Laptop"
provider = "anthropic"

[providers.anthropic]
base_url = "https://api.anthropic.com"
```

## Environment Variables

Environment variables override config file values:

| Variable | Description | Default |
|----------|-------------|---------|
| `ASPY_BIND` | Proxy bind address | `127.0.0.1:8080` |
| `ANTHROPIC_API_URL` | Upstream API URL | `https://api.anthropic.com` |
| `ASPY_LOG_DIR` | Log directory | `./logs` |
| `ASPY_CONTEXT_LIMIT` | Context window limit | `147000` |
| `ASPY_THEME` | Theme name | `Spy Dark` |
| `ASPY_NO_TUI` | Disable TUI (headless) | `false` |
| `ASPY_DEMO` | Enable demo mode | `false` |
| `RUST_LOG` | Log level filter | `info` |

**Examples:**

```bash
# Run with different bind address
ASPY_BIND=0.0.0.0:9090 aspy

# Run headless (no TUI, just proxy)
ASPY_NO_TUI=1 aspy

# Debug logging
RUST_LOG=debug aspy

# Demo mode with custom theme
ASPY_DEMO=1 ASPY_THEME="Spy Dark" aspy
```

## Configuration Options Reference

### General Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `theme` | string | `"Spy Dark"` | Color theme name |
| `use_theme_background` | bool | `true` | Use theme's background vs terminal's |
| `preset` | string | `"classic"` | Layout preset |
| `context_limit` | integer | `147000` | Context window size for gauge |
| `bind_addr` | string | `"127.0.0.1:8080"` | Proxy server address |
| `log_dir` | string | `"./logs"` | Session log directory |

### Feature Flags

All features are enabled by default (opt-out pattern):

| Option | Default | Description |
|--------|---------|-------------|
| `features.storage` | `true` | Write events to JSONL files |
| `features.thinking_panel` | `true` | Display Claude's extended thinking |
| `features.stats` | `true` | Track token usage and costs |

### Augmentation

| Option | Default | Description |
|--------|---------|-------------|
| `augmentation.context_warning` | `true` | Inject context usage warnings |
| `augmentation.context_warning_thresholds` | `[60, 80, 85, 90, 95]` | Warning percentages |

### Logging

| Option | Default | Description |
|--------|---------|-------------|
| `logging.level` | `"info"` | Log level: trace, debug, info, warn, error |

### Multi-Client Configuration

See [docs/sessions.md](sessions.md) for complete multi-client routing documentation.

```toml
[clients.<client-id>]
name = "Display Name"     # Required: human-readable name
provider = "provider-id"  # Required: references [providers.X]
tags = ["optional", "tags"]

[providers.<provider-id>]
base_url = "https://api.anthropic.com"  # Required: upstream API URL
name = "Optional Display Name"
```

## Layout Presets

| Preset | Description |
|--------|-------------|
| `classic` | Events list with detail panel, thinking panel on right |
| `reasoning` | Larger thinking panel, optimized for extended thinking |
| `debug` | Includes logs panel, useful for development |

## Version Information

```bash
aspy --version
```

Displays the current version number.
