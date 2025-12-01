//! GitHub Dark - GitHub's official dark theme

pub const THEME: &str = r##"# GitHub Dark theme for anthropic-spy
# GitHub's official dark theme

[meta]
name = "GitHub Dark"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#101216"
foreground = "#8b949e"
border = "#8b949e"
border_focused = "#e3b341"
title = "#2b7489"
status_bar = "#8b949e"
selection_bg = "#3b5070"
selection_fg = "#ffffff"

[events]
tool_call = "#2b7489"
tool_result_ok = "#56d364"
tool_result_fail = "#f78166"
request = "#6ca4f8"
response = "#db61a2"
error = "#f78166"
thinking = "#db61a2"
api_usage = "#8b949e"
headers = "#8b949e"
rate_limit = "#8b949e"
context_compact = "#e3b341"

[context_bar]
fill = "#56d364"
warn = "#e3b341"
danger = "#f78166"

[panels]
events = "#2b7489"
thinking = "#db61a2"
logs = "#56d364"

[vhs]
black = "#000000"
red = "#f78166"
green = "#56d364"
yellow = "#e3b341"
blue = "#6ca4f8"
purple = "#db61a2"
cyan = "#2b7489"
white = "#ffffff"
bright_black = "#4d4d4d"
bright_red = "#f78166"
bright_green = "#56d364"
bright_yellow = "#e3b341"
bright_blue = "#6ca4f8"
bright_purple = "#db61a2"
bright_cyan = "#2b7489"
bright_white = "#ffffff"
cursor = "#c9d1d9"
"##;
