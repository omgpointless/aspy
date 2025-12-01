//! Tokyo Night - A clean, dark theme inspired by Tokyo at night

pub const THEME: &str = r##"# Tokyo Night theme for anthropic-spy
# A clean, dark theme inspired by Tokyo at night

[meta]
name = "Tokyo Night"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#1a1b26"
foreground = "#c0caf5"
border = "#c0caf5"
border_focused = "#e0af68"
title = "#7dcfff"
status_bar = "#c0caf5"
selection_bg = "#33467c"
selection_fg = "#c0caf5"

[events]
tool_call = "#7dcfff"
tool_result_ok = "#9ece6a"
tool_result_fail = "#f7768e"
request = "#7aa2f7"
response = "#bb9af7"
error = "#f7768e"
thinking = "#bb9af7"
api_usage = "#c0caf5"
headers = "#c0caf5"
rate_limit = "#c0caf5"
context_compact = "#e0af68"

[context_bar]
fill = "#9ece6a"
warn = "#e0af68"
danger = "#f7768e"

[panels]
events = "#7dcfff"
thinking = "#bb9af7"
logs = "#9ece6a"

[vhs]
black = "#15161e"
red = "#f7768e"
green = "#9ece6a"
yellow = "#e0af68"
blue = "#7aa2f7"
purple = "#bb9af7"
cyan = "#7dcfff"
white = "#a9b1d6"
bright_black = "#414868"
bright_red = "#f7768e"
bright_green = "#9ece6a"
bright_yellow = "#e0af68"
bright_blue = "#7aa2f7"
bright_purple = "#bb9af7"
bright_cyan = "#7dcfff"
bright_white = "#c0caf5"
cursor = "#c0caf5"
"##;
