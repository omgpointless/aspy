//! Everforest Dark - Green nature-inspired dark theme, easy on the eyes

pub const THEME: &str = r##"# Everforest Dark theme for anthropic-spy
# Green nature-inspired dark theme, easy on the eyes

[meta]
name = "Everforest Dark"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#1e2326"
foreground = "#d3c6aa"
border = "#d3c6aa"
border_focused = "#dbbc7f"
title = "#83c092"
status_bar = "#d3c6aa"
selection_bg = "#4c3743"
selection_fg = "#fffbef"

[events]
tool_call = "#83c092"
tool_result_ok = "#a7c080"
tool_result_fail = "#e67e80"
request = "#7fbbb3"
response = "#d699b6"
error = "#e67e80"
thinking = "#d699b6"
api_usage = "#d3c6aa"
headers = "#d3c6aa"
rate_limit = "#d3c6aa"
context_compact = "#dbbc7f"

[context_bar]
fill = "#a7c080"
warn = "#dbbc7f"
danger = "#e67e80"

[panels]
events = "#83c092"
thinking = "#d699b6"
logs = "#a7c080"

[vhs]
black = "#7a8478"
red = "#e67e80"
green = "#a7c080"
yellow = "#dbbc7f"
blue = "#7fbbb3"
purple = "#d699b6"
cyan = "#83c092"
white = "#f2efdf"
bright_black = "#a6b0a0"
bright_red = "#f85552"
bright_green = "#8da101"
bright_yellow = "#dfa000"
bright_blue = "#3a94c5"
bright_purple = "#df69ba"
bright_cyan = "#35a77c"
bright_white = "#fffbef"
cursor = "#e69875"
"##;
