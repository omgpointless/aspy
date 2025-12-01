# Theme System

anthropic-spy includes a comprehensive theme system with 32 bundled themes and support for custom themes.

## Quick Start

Switch themes in the TUI by pressing `t` to open the theme picker, then use arrow keys or `j`/`k` to navigate and `Enter` to select.

To set a default theme, edit your config file:

```bash
aspy config --edit
```

```toml
theme = "Spy Dark"
use_theme_background = true
```

## Bundled Themes

### Featured Themes

| Theme | Description |
|-------|-------------|
| **Spy Dark** | The flagship theme - workshop warmth with observatory clarity |
| **Spy Light** | Light variant of Spy Dark |

### Popular Editor Themes

| Theme | Origin |
|-------|--------|
| Dracula | Classic purple-tinted dark theme |
| Catppuccin Mocha | Warm pastel dark theme |
| Catppuccin Latte | Light variant of Catppuccin |
| Gruvbox Dark | Retro groove with warm colors |
| Nord | Arctic, bluish color palette |
| Tokyo Night | Dark theme inspired by Tokyo lights |
| One Half Dark | Atom editor theme |
| Solarized Light | Classic light theme with reduced eye strain |

### JetBrains & VS Code Themes

| Theme | Origin |
|-------|--------|
| JetBrains Darcula | IntelliJ IDEA default dark |
| GitHub Dark | GitHub's dark mode |

### Monokai Family

| Theme | Variant |
|-------|---------|
| Monokai Pro | Original Monokai Pro |
| Monokai Pro Ristretto | Warm brown tones |
| Monokai Pro Machine | Industrial grays |
| Monokai Soda | Classic Sublime Text Monokai |

### Material Family

| Theme | Variant |
|-------|---------|
| Material Oceanic | Deep ocean blues |
| Material Darker | Pure dark variant |
| Material Lighter | Light variant |
| Material Palenight | Purple-tinted dark |
| Material Deep Ocean | Darker ocean variant |
| Material Forest | Green nature tones |
| Material Sky Blue | Bright blue accents |
| Material Sandy Beach | Warm beach tones |
| Material Volcano | Red/orange fire tones |
| Material Space | Deep space purples |

### Other Themes

| Theme | Description |
|-------|-------------|
| Synthwave 84 | Retro synthwave neon |
| Rosé Pine | Elegant rose-tinted dark theme |
| Everforest Dark | Comfortable green-tinted theme |
| Ayu Mirage | Soft dark theme |
| Kanagawa Wave | Japanese-inspired dark theme |
| Terminal ANSI | Uses your terminal's native colors |

## Configuration Options

### Theme Selection

```toml
# config.toml
theme = "Spy Dark"
```

Theme names are case-insensitive and spaces are converted to underscores when looking up files.

### Background Mode

```toml
# Use theme's background color
use_theme_background = true

# Use terminal's default background (transparent terminals, etc.)
use_theme_background = false
```

## Creating Custom Themes

Themes are defined in TOML format. Custom themes live in `~/.config/aspy/themes/`.

### Theme File Structure

```toml
# ~/.config/aspy/themes/My_Custom_Theme.toml

[meta]
name = "My Custom Theme"
version = 1
author = "Your Name"

[ui]
background = "#1e1e2e"      # Main background
foreground = "#cdd6f4"      # Default text color
border = "#45475a"          # Unfocused panel borders
border_focused = "#f5c2e7"  # Focused panel border
title = "#cdd6f4"           # Title bar text
status_bar = "#cdd6f4"      # Status bar text
selection_bg = "#45475a"    # Selection background
selection_fg = "#cdd6f4"    # Selection foreground
muted = "#6c7086"           # Secondary/muted text (optional)
border_type = "rounded"     # Border style: plain, rounded, double, thick

[events]
tool_call = "#89b4fa"       # Tool call events (cyan/blue)
tool_result_ok = "#a6e3a1"  # Successful tool results (green)
tool_result_fail = "#f38ba8" # Failed tool results (red)
request = "#9399b2"         # API request events
response = "#f5c2e7"        # API response events
error = "#f38ba8"           # Error events
thinking = "#f5c2e7"        # Thinking blocks (purple/pink)
api_usage = "#9399b2"       # Token usage info
headers = "#9399b2"         # Header info
rate_limit = "#9399b2"      # Rate limit warnings
context_compact = "#f9e2af" # Context compaction events (yellow)

[context_bar]
fill = "#89b4fa"            # Normal usage (under 70%)
warn = "#f9e2af"            # Warning level (70-85%)
danger = "#f38ba8"          # Danger level (85%+)

[panels]
events = "#89b4fa"          # Events panel border when focused
thinking = "#f5c2e7"        # Thinking panel border when focused
logs = "#a6e3a1"            # Logs panel border when focused

[code]                      # Optional: code highlighting
inline = "#f9e2af"          # `inline code` color
block = "#9399b2"           # Fenced code block color
```

### Color Formats

anthropic-spy supports two color formats:

**Hex Colors:**
```toml
background = "#1e1e2e"
foreground = "#cdd6f4"
```

**ANSI Colors** (for terminal-native themes):
```toml
background = "ansi:bg"      # Use terminal default background
foreground = "ansi:fg"      # Use terminal default foreground
tool_call = "ansi:6"        # ANSI cyan
error = "ansi:1"            # ANSI red
```

ANSI color codes:
| Code | Color |
|------|-------|
| 0 | Black |
| 1 | Red |
| 2 | Green |
| 3 | Yellow |
| 4 | Blue |
| 5 | Magenta |
| 6 | Cyan |
| 7 | White |
| 8-15 | Bright variants |
| fg | Terminal default foreground |
| bg | Terminal default background |

### Border Styles

The `border_type` option accepts:
- `plain` - Simple line borders (default)
- `rounded` - Rounded corners
- `double` - Double-line borders
- `thick` - Thick line borders

## Theme Loading Priority

When loading a theme, anthropic-spy checks in order:

1. **External TOML** - `~/.config/aspy/themes/{theme_name}.toml`
2. **Bundled TOML** - Compiled into the binary
3. **Legacy JSON** - For backwards compatibility
4. **Hardcoded fallback** - One Half Dark

## Theme Extraction

Bundled themes are automatically extracted to `~/.config/aspy/themes/` on first run. This allows you to:

- Modify bundled themes to your preference
- Use them as templates for custom themes
- Share themes with others

The extraction only happens once (tracked by `.extracted_v2` marker file).

## Tips

### Choosing a Theme

- **Terminal transparency:** Set `use_theme_background = false` to let your terminal's background show through
- **Eye strain:** Light themes (Solarized Light, Catppuccin Latte, Material Lighter) can reduce eye strain in bright environments
- **Color blindness:** Themes with high contrast between success (green) and error (red) colors work best

### Customizing Existing Themes

1. Find the theme in `~/.config/aspy/themes/`
2. Edit the TOML file
3. Restart anthropic-spy or press `t` to reload themes

### Creating from Scratch

1. Copy an existing theme file as a template
2. Update the `[meta]` section with your theme name
3. Adjust colors using a color picker or your editor's palette
4. Save as `Theme_Name.toml` (use underscores for spaces)

### Testing Themes

Run in demo mode to see how your theme handles various event types:

```bash
ASPY_DEMO=1 aspy
```

## Programmatic Access

The theme system exposes these functions in Rust:

```rust
use anthropic_spy::theme::Theme;

// Load by name
let theme = Theme::by_name("Spy Dark");

// List all available themes
let themes: Vec<String> = Theme::list_available();

// Load with custom config
use anthropic_spy::theme::ThemeConfig;
let config = ThemeConfig { use_theme_background: false };
let theme = Theme::by_name_with_config("Spy Dark", &config);
```
