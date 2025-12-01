//! One Half Dark - A clean, modern dark theme

pub const THEME: &str = r##"# One Half Dark theme for anthropic-spy
# A clean, modern dark theme (default)

[meta]
name = "One Half Dark"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#282c34"
foreground = "#dcdfe4"
border = "#dcdfe4"
border_focused = "#e5c07b"
title = "#56b6c2"
status_bar = "#dcdfe4"
selection_bg = "#474e5d"
selection_fg = "#dcdfe4"

[events]
tool_call = "#56b6c2"
tool_result_ok = "#98c379"
tool_result_fail = "#e06c75"
request = "#61afef"
response = "#c678dd"
error = "#e06c75"
thinking = "#c678dd"
api_usage = "#dcdfe4"
headers = "#dcdfe4"
rate_limit = "#dcdfe4"
context_compact = "#e5c07b"

[context_bar]
fill = "#98c379"
warn = "#e5c07b"
danger = "#e06c75"

[panels]
events = "#56b6c2"
thinking = "#c678dd"
logs = "#98c379"

[vhs]
black = "#282c34"
red = "#e06c75"
green = "#98c379"
yellow = "#e5c07b"
blue = "#61afef"
purple = "#c678dd"
cyan = "#56b6c2"
white = "#dcdfe4"
bright_black = "#5d677a"
bright_red = "#e06c75"
bright_green = "#98c379"
bright_yellow = "#e5c07b"
bright_blue = "#61afef"
bright_purple = "#c678dd"
bright_cyan = "#56b6c2"
bright_white = "#dcdfe4"
cursor = "#a3b3cc"
"##;
