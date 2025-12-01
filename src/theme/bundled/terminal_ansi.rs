//! Terminal ANSI - Uses your terminal's native ANSI colors
//! Perfect for users who have carefully crafted their terminal theme

pub const THEME: &str = r##"# Terminal ANSI theme for anthropic-spy
# Uses your terminal's native ANSI colors - adapts to your terminal theme!
#
# This theme uses "ansi:X" syntax instead of hex colors:
# - ansi:0-7 = standard colors (black, red, green, yellow, blue, magenta, cyan, white)
# - ansi:8-15 = bright variants
# - ansi:fg = terminal's default foreground
# - ansi:bg = terminal's default background (transparent)
#
# Perfect for users who have carefully crafted their terminal theme and want
# anthropic-spy to inherit those colors automatically.

[meta]
name = "Terminal ANSI"
version = 1
author = "anthropic-spy"

[ui]
background = "ansi:bg"
foreground = "ansi:fg"
border = "ansi:fg"
border_focused = "ansi:3"
title = "ansi:6"
status_bar = "ansi:fg"
selection_bg = "ansi:8"
selection_fg = "ansi:fg"

[events]
tool_call = "ansi:6"
tool_result_ok = "ansi:2"
tool_result_fail = "ansi:1"
request = "ansi:4"
response = "ansi:5"
error = "ansi:1"
thinking = "ansi:5"
api_usage = "ansi:fg"
headers = "ansi:fg"
rate_limit = "ansi:fg"
context_compact = "ansi:3"

[context_bar]
fill = "ansi:2"
warn = "ansi:3"
danger = "ansi:1"

[panels]
events = "ansi:6"
thinking = "ansi:5"
logs = "ansi:2"

[vhs]
black = "ansi:0"
red = "ansi:1"
green = "ansi:2"
yellow = "ansi:3"
blue = "ansi:4"
purple = "ansi:5"
cyan = "ansi:6"
white = "ansi:7"
bright_black = "ansi:8"
bright_red = "ansi:9"
bright_green = "ansi:10"
bright_yellow = "ansi:11"
bright_blue = "ansi:12"
bright_purple = "ansi:13"
bright_cyan = "ansi:14"
bright_white = "ansi:15"
cursor = "ansi:fg"
"##;
