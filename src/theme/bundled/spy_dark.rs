//! Spy Dark - The flagship theme for anthropic-spy
//! Workshop warmth with Observatory clarity

pub const THEME: &str = r##"# Spy Dark theme for anthropic-spy
# Workshop warmth with Observatory clarity
# "I can see clearly, and I'm comfortable here"

[meta]
name = "Spy Dark"
version = 1
author = "anthropic-spy"

[ui]
background = "#28292d"
foreground = "#d4cfc9"
border = "#3a3b40"
border_focused = "#c9a66b"
title = "#c9a66b"
status_bar = "#c9a66b"
selection_bg = "#3d3834"
selection_fg = "#e8e4df"
muted = "#8a8279"
border_type = "rounded"

[events]
tool_call = "#5da9a1"
tool_result_ok = "#8fad5c"
tool_result_fail = "#c75f4a"
request = "#6b98b8"
response = "#a88fad"
error = "#c75f4a"
thinking = "#a88fad"
api_usage = "#8a8279"
headers = "#8a8279"
rate_limit = "#8a8279"
context_compact = "#d4a54a"

[context_bar]
fill = "#5da9a1"
warn = "#d4a54a"
danger = "#c75f4a"

[panels]
events = "#6b98b8"
thinking = "#a88fad"
logs = "#8fad5c"

[code]
inline = "#e8b87a"
block = "#9ca8b4"
"##;
