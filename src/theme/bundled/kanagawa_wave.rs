//! Kanagawa Wave - Inspired by Katsushika Hokusai's famous painting

pub const THEME: &str = r##"# Kanagawa Wave theme for anthropic-spy
# Inspired by Katsushika Hokusai's famous painting

[meta]
name = "Kanagawa Wave"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#1f1f28"
foreground = "#dcd7ba"
border = "#dcd7ba"
border_focused = "#c0a36e"
title = "#6a9589"
status_bar = "#dcd7ba"
selection_bg = "#2d4f67"
selection_fg = "#dcd7ba"

[events]
tool_call = "#6a9589"
tool_result_ok = "#76946a"
tool_result_fail = "#c34043"
request = "#7e9cd8"
response = "#957fb8"
error = "#c34043"
thinking = "#957fb8"
api_usage = "#dcd7ba"
headers = "#dcd7ba"
rate_limit = "#dcd7ba"
context_compact = "#c0a36e"

[context_bar]
fill = "#76946a"
warn = "#c0a36e"
danger = "#c34043"

[panels]
events = "#7e9cd8"
thinking = "#957fb8"
logs = "#98bb6c"

[vhs]
black = "#090618"
red = "#c34043"
green = "#76946a"
yellow = "#c0a36e"
blue = "#7e9cd8"
purple = "#957fb8"
cyan = "#6a9589"
white = "#c8c093"
bright_black = "#727169"
bright_red = "#e82424"
bright_green = "#98bb6c"
bright_yellow = "#e6c384"
bright_blue = "#7fb4ca"
bright_purple = "#938aa9"
bright_cyan = "#7aa89f"
bright_white = "#dcd7ba"
cursor = "#c8c093"
"##;
