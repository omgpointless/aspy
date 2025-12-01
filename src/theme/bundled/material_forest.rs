//! Material Forest - Deep green nature theme

pub const THEME: &str = r##"# Material Forest theme for anthropic-spy
# Deep green nature theme

[meta]
name = "Material Forest"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#002626"
foreground = "#B2C2B0"
border = "#B2C2B0"
border_focused = "#FFCB6B"
title = "#89DDFF"
status_bar = "#B2C2B0"
selection_bg = "#1E611E"
selection_fg = "#EEFFFF"

[events]
tool_call = "#89DDFF"
tool_result_ok = "#C3E88D"
tool_result_fail = "#FF5370"
request = "#82AAFF"
response = "#C792EA"
error = "#FF5370"
thinking = "#C792EA"
api_usage = "#B2C2B0"
headers = "#B2C2B0"
rate_limit = "#B2C2B0"
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
black = "#005454"
red = "#FF5370"
green = "#C3E88D"
yellow = "#FFCB6B"
blue = "#82AAFF"
purple = "#C792EA"
cyan = "#89DDFF"
white = "#EEFFFF"
bright_black = "#005454"
bright_red = "#F07178"
bright_green = "#C3E88D"
bright_yellow = "#FFCB6B"
bright_blue = "#82AAFF"
bright_purple = "#C792EA"
bright_cyan = "#89DDFF"
bright_white = "#EEFFFF"
cursor = "#FFCC80"
"##;
