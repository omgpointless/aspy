//! Material Palenight - Soft purple-tinted dark theme

pub const THEME: &str = r##"# Material Palenight theme for anthropic-spy
# Soft purple-tinted dark theme

[meta]
name = "Material Palenight"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#292D3E"
foreground = "#A6ACCD"
border = "#A6ACCD"
border_focused = "#FFCB6B"
title = "#89DDFF"
status_bar = "#A6ACCD"
selection_bg = "#717CB4"
selection_fg = "#EEFFFF"

[events]
tool_call = "#89DDFF"
tool_result_ok = "#C3E88D"
tool_result_fail = "#FF5370"
request = "#82AAFF"
response = "#C792EA"
error = "#FF5370"
thinking = "#C792EA"
api_usage = "#A6ACCD"
headers = "#A6ACCD"
rate_limit = "#A6ACCD"
context_compact = "#FFCB6B"

[context_bar]
fill = "#C3E88D"
warn = "#FFCB6B"
danger = "#FF5370"

[panels]
events = "#89DDFF"
thinking = "#C792EA"
logs = "#C3E88D"

[vhs]
black = "#676E95"
red = "#FF5370"
green = "#C3E88D"
yellow = "#FFCB6B"
blue = "#82AAFF"
purple = "#C792EA"
cyan = "#89DDFF"
white = "#EEFFFF"
bright_black = "#676E95"
bright_red = "#F07178"
bright_green = "#C3E88D"
bright_yellow = "#FFCB6B"
bright_blue = "#82AAFF"
bright_purple = "#C792EA"
bright_cyan = "#89DDFF"
bright_white = "#EEFFFF"
cursor = "#AB47BC"
"##;
