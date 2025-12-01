---
layout: default
title: Views
---

# TUI Views

anthropic-spy's TUI consists of three main views that you can switch between using keyboard shortcuts.

## View Navigation

| Key | Action |
|-----|--------|
| `1` | Switch to Events view |
| `2` | Switch to Stats view |
| `s` | Switch to Settings view |
| `Escape` | Return to Events view |

---

## Events View (Main)

The default view showing real-time proxy events.

### Layout

```
┌─────────────────────────────────────┐┌──────────────────────┐
│ Events Panel                        ││ Thinking Panel       │
│                                     ││                      │
│ [10:30:15] 🔧 Tool Call: Read (...) ││ Claude is reasoning  │
│ [10:30:16] ✓ Tool Result: Read (0.  ││ about the request... │
│ [10:30:17] 📊 Usage: 1.5Kin + 200out││                      │
│ [10:30:18] 💭 Thinking: Let me...   ││                      │
│                                     ││                      │
└─────────────────────────────────────┘└──────────────────────┘
```

### Event Types

| Icon | Event | Description |
|------|-------|-------------|
| 🔧 | Tool Call | Claude invoked a tool (Read, Edit, Bash, etc.) |
| ✓ | Tool Result (success) | Tool completed successfully |
| ✗ | Tool Result (failure) | Tool execution failed |
| ← | Request | API request sent to Anthropic |
| → | Response | API response received |
| ❌ | Error | Error occurred during processing |
| 📋 | Headers | Request/response headers captured |
| ⚖️ | Rate Limits | Rate limit information updated |
| 📊 | Usage | Token usage for a request |
| 💭 | Thinking | Claude's extended thinking content |
| 📦 | Context Compact | Context window compaction detected |

### Keyboard Controls

| Key | Action |
|-----|--------|
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `Enter` | Open detail modal for selected event |
| `c` | Copy selected event details to clipboard |
| `Tab` | Cycle focus between panels |
| `g` | Jump to top of list |
| `G` | Jump to bottom of list |
| `Page Up` / `Page Down` | Scroll by page |

### Panels

**Events Panel** (left)
- Scrollable list of all captured events
- Events color-coded by type
- Shows timestamp, type, and summary

**Thinking Panel** (right)
- Displays Claude's extended thinking in real-time
- Shows the current thinking content as it streams
- Only visible when thinking blocks are present

**Detail Modal** (press `Enter`)
- Full details of selected event
- Tool inputs/outputs, headers, token breakdown
- Scrollable for long content

---

## Stats View

Session analytics with tabbed dashboard.

### Accessing

Press `2` from any view to open Stats.

### Tabs

Navigate tabs with number keys `1`-`5` or use `Tab`:

#### 1. Overview Tab

```
┌─────────────────────┐┌──────────────────────────────┐
│ Session Gauges      ││ Session Summary              │
│                     ││                              │
│ Cost: ████░░░░ $0.02││   Requests:     25 (0 failed)│
│ Input: ██████░░ 50K ││   Tool Calls:   58 (2 failed)│
│ Output: ██░░░░░ 15K ││                              │
│ Cache: █████████ 98%││   Total Tokens: 65,000       │
│                     ││     Input:      50K          │
└─────────────────────┘│     Output:     15K          │
                       │     Cached:     45K          │
                       │                              │
                       │   Est. Cost:    $0.0234      │
                       │   Cache Savings:$0.0180      │
                       └──────────────────────────────┘
```

**Gauges:**
- Cost progress toward session estimate
- Input token usage
- Output token usage
- Cache hit ratio

**Summary:**
- Request/tool call counts
- Token breakdown
- Estimated cost and cache savings
- Thinking block statistics

#### 2. Models Tab

Shows API call distribution across models:
- Bar chart of model usage (Haiku vs Sonnet vs Opus)
- Sparklines showing model usage over time
- Request count per model

#### 3. Tokens Tab

Token usage breakdown:
- Grouped bar chart: Input / Output / Cached per model
- Running totals
- Cache efficiency metrics

#### 4. Tools Tab

Tool call analysis:
- Tool frequency distribution
- Success/failure rates
- Average duration per tool

#### 5. Trends Tab

Sparkline grid showing trends over time:
- Tokens per request
- Tool calls per request
- Cache hit ratio
- Request latency

### Keyboard Controls

| Key | Action |
|-----|--------|
| `1`-`5` | Switch to specific tab |
| `Tab` | Cycle to next tab |
| `Shift+Tab` | Cycle to previous tab |
| `Escape` / `1` | Return to Events view |

---

## Settings View

Configuration interface for themes and layout presets.

### Accessing

Press `s` or `t` (theme shortcut) from any view.

### Layout

```
┌────────────────────┐┌──────────────────────────────────┐
│ Categories         ││ Appearance                       │
│                    ││                                  │
│  ▸ Appearance      ││ ● Spy Dark (current)             │
│    Layout          ││   Spy Light                      │
│                    ││   Dracula                        │
│                    ││   Catppuccin Mocha               │
│                    ││   Nord                           │
│                    ││   ...                            │
│                    ││                                  │
│                    ││ [ ] Use theme background         │
└────────────────────┘└──────────────────────────────────┘
```

### Categories

**Appearance**
- Theme selection from 32+ bundled themes
- Toggle for using theme's background color

**Layout**
- Preset selection:
  - `classic` - Side-by-side events and thinking
  - `reasoning` - Thinking-first, larger reasoning panel
  - `debug` - Expanded logs for debugging

### Keyboard Controls

| Key | Action |
|-----|--------|
| `Tab` | Switch between Categories and Options |
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `Enter` | Apply selected option |
| `Space` | Toggle checkbox options |
| `Escape` | Return to previous view |

---

## Common Controls

These work across all views:

| Key | Action |
|-----|--------|
| `q` | Quit application |
| `?` | Show help (when available) |
| `t` | Quick access to theme picker |
| `c` | Copy current content to clipboard |
| `r` | Refresh/redraw screen |

---

## Shell Components

These components appear in all views:

### Title Bar
Shows application name, version, and current view indicator.

### Context Bar
Visual gauge showing context window usage:
- Green (normal): Under 70% usage
- Yellow (warning): 70-85% usage
- Red (danger): Over 85% usage

### Status Bar
Shows:
- Session duration
- Total tokens used
- Estimated cost
- Cache savings
- Current model

### Logs Panel (debug preset only)
Displays internal application logs for debugging.

---

## Layout Presets

### Classic (default)
```
[Title Bar]
[Context Bar]
┌─────────────────┐┌──────────────┐
│ Events          ││ Thinking     │
│                 ││              │
│                 ││              │
└─────────────────┘└──────────────┘
[Status Bar]
```

### Reasoning
```
[Title Bar]
[Context Bar]
┌──────────────┐┌─────────────────┐
│ Thinking     ││ Events          │
│ (60%)        ││ (40%)           │
│              ││                 │
└──────────────┘└─────────────────┘
[Status Bar]
```

### Debug
```
[Title Bar]
[Context Bar]
┌─────────────────┐┌──────────────┐
│ Events          ││ Thinking     │
│                 ││              │
└─────────────────┘└──────────────┘
[Logs Panel]
[Status Bar]
```

---

## Responsive Layout

The TUI adapts to terminal width:

- **Wide (>120 cols)**: Full layout with all panels
- **Medium (80-120 cols)**: Reduced thinking panel width
- **Narrow (<80 cols)**: Stacked layout, thinking hidden

Resize your terminal to see the layout adapt.
