//! Material Deep Ocean - Ultra-dark blue theme

pub const THEME: &str = r##"# Material Deep Ocean theme for anthropic-spy
# Ultra-dark blue theme

[meta]
name = "Material Deep Ocean"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#0F111A"
foreground = "#8F93A2"
border = "#8F93A2"
border_focused = "#FFCB6B"
title = "#89DDFF"
status_bar = "#8F93A2"
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
api_usage = "#8F93A2"
headers = "#8F93A2"
rate_limit = "#8F93A2"
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
black = "#717CB4"
red = "#FF5370"
green = "#C3E88D"
yellow = "#FFCB6B"
blue = "#82AAFF"
purple = "#C792EA"
cyan = "#89DDFF"
white = "#EEFFFF"
bright_black = "#717CB4"
bright_red = "#F07178"
bright_green = "#C3E88D"
bright_yellow = "#FFCB6B"
bright_blue = "#82AAFF"
bright_purple = "#C792EA"
bright_cyan = "#89DDFF"
bright_white = "#EEFFFF"
cursor = "#84FFFF"
"##;
