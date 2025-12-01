//! Material Darker - Deep charcoal dark theme

pub const THEME: &str = r##"# Material Darker theme for anthropic-spy
# Deep charcoal dark theme

[meta]
name = "Material Darker"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#212121"
foreground = "#B0BEC5"
border = "#B0BEC5"
border_focused = "#FFCB6B"
title = "#89DDFF"
status_bar = "#B0BEC5"
selection_bg = "#404040"
selection_fg = "#EEFFFF"

[events]
tool_call = "#89DDFF"
tool_result_ok = "#C3E88D"
tool_result_fail = "#FF5370"
request = "#82AAFF"
response = "#C792EA"
error = "#FF5370"
thinking = "#C792EA"
api_usage = "#B0BEC5"
headers = "#B0BEC5"
rate_limit = "#B0BEC5"
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
black = "#616161"
red = "#FF5370"
green = "#C3E88D"
yellow = "#FFCB6B"
blue = "#82AAFF"
purple = "#C792EA"
cyan = "#89DDFF"
white = "#EEFFFF"
bright_black = "#616161"
bright_red = "#F07178"
bright_green = "#C3E88D"
bright_yellow = "#FFCB6B"
bright_blue = "#82AAFF"
bright_purple = "#C792EA"
bright_cyan = "#89DDFF"
bright_white = "#EEFFFF"
cursor = "#FF9800"
"##;
