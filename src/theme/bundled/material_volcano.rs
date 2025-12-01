//! Material Volcano - Deep red dark theme

pub const THEME: &str = r##"# Material Volcano theme for anthropic-spy
# Deep red dark theme

[meta]
name = "Material Volcano"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#390000"
foreground = "#FFEAEA"
border = "#FFEAEA"
border_focused = "#FFCB6B"
title = "#89DDFF"
status_bar = "#FFEAEA"
selection_bg = "#750000"
selection_fg = "#EEFFFF"

[events]
tool_call = "#89DDFF"
tool_result_ok = "#C3E88D"
tool_result_fail = "#FF5370"
request = "#82AAFF"
response = "#C792EA"
error = "#FF5370"
thinking = "#C792EA"
api_usage = "#FFEAEA"
headers = "#FFEAEA"
rate_limit = "#FFEAEA"
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
black = "#7F6451"
red = "#FF5370"
green = "#C3E88D"
yellow = "#FFCB6B"
blue = "#82AAFF"
purple = "#C792EA"
cyan = "#89DDFF"
white = "#EEFFFF"
bright_black = "#7F6451"
bright_red = "#F07178"
bright_green = "#C3E88D"
bright_yellow = "#FFCB6B"
bright_blue = "#82AAFF"
bright_purple = "#C792EA"
bright_cyan = "#89DDFF"
bright_white = "#EEFFFF"
cursor = "#00BCD4"
"##;
