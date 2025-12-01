//! Nord - Arctic, bluish color palette

pub const THEME: &str = r##"# Nord theme for anthropic-spy
# Arctic, bluish color palette

[meta]
name = "Nord"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#2e3440"
foreground = "#d8dee9"
border = "#d8dee9"
border_focused = "#ebcb8b"
title = "#88c0d0"
status_bar = "#d8dee9"
selection_bg = "#eceff4"
selection_fg = "#2e3440"

[events]
tool_call = "#88c0d0"
tool_result_ok = "#a3be8c"
tool_result_fail = "#bf616a"
request = "#81a1c1"
response = "#b48ead"
error = "#bf616a"
thinking = "#b48ead"
api_usage = "#d8dee9"
headers = "#d8dee9"
rate_limit = "#d8dee9"
context_compact = "#ebcb8b"

[context_bar]
fill = "#a3be8c"
warn = "#ebcb8b"
danger = "#bf616a"

[panels]
events = "#88c0d0"
thinking = "#b48ead"
logs = "#a3be8c"

[vhs]
black = "#3b4252"
red = "#bf616a"
green = "#a3be8c"
yellow = "#ebcb8b"
blue = "#81a1c1"
purple = "#b48ead"
cyan = "#88c0d0"
white = "#e5e9f0"
bright_black = "#596377"
bright_red = "#bf616a"
bright_green = "#a3be8c"
bright_yellow = "#ebcb8b"
bright_blue = "#81a1c1"
bright_purple = "#b48ead"
bright_cyan = "#8fbcbb"
bright_white = "#eceff4"
cursor = "#eceff4"
"##;
