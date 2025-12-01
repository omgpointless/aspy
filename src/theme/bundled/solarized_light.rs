//! Solarized Light - Ethan Schoonover's precision color palette (light variant)

pub const THEME: &str = r##"# Solarized Light theme for anthropic-spy
# Ethan Schoonover's precision color palette (light variant)
#
# NOTE: context_bar.fill uses CYAN (#2aa198) instead of the olive green (#859900)
# because the olive works well for text but looks muddy as a gauge fill.
# This is exactly the kind of fix the new theme format enables!

[meta]
name = "Solarized Light"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#fdf6e3"
foreground = "#657b83"
border = "#93a1a1"
border_focused = "#657b83"
title = "#657b83"
status_bar = "#657b83"
selection_bg = "#eee8d5"
selection_fg = "#657b83"

[events]
tool_call = "#2aa198"
tool_result_ok = "#859900"
tool_result_fail = "#dc322f"
request = "#268bd2"
response = "#d33682"
error = "#dc322f"
thinking = "#d33682"
api_usage = "#657b83"
headers = "#657b83"
rate_limit = "#657b83"
context_compact = "#dc322f"

[context_bar]
# KEY FIX: Using cyan instead of olive green for gauge fill
# The olive (#859900) looks great for text but muddy as a solid fill
fill = "#2aa198"
warn = "#b58900"
danger = "#dc322f"

[panels]
events = "#268bd2"
thinking = "#d33682"
logs = "#859900"

[vhs]
black = "#073642"
red = "#dc322f"
green = "#859900"
yellow = "#b58900"
blue = "#268bd2"
purple = "#d33682"
cyan = "#2aa198"
white = "#bbb5a2"
bright_black = "#002b36"
bright_red = "#cb4b16"
bright_green = "#586e75"
bright_yellow = "#657b83"
bright_blue = "#839496"
bright_purple = "#6c71c4"
bright_cyan = "#93a1a1"
bright_white = "#fdf6e3"
cursor = "#657b83"
"##;
