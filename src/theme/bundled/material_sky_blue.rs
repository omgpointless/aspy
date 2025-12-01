//! Material Sky Blue - Bright light theme with blue accents

pub const THEME: &str = r##"# Material Sky Blue theme for anthropic-spy
# Bright light theme with blue accents

[meta]
name = "Material Sky Blue"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#F5F5F5"
foreground = "#005761"
border = "#005761"
border_focused = "#F6A434"
title = "#39ADB5"
status_bar = "#005761"
selection_bg = "#ADE2EB"
selection_fg = "#272727"

[events]
tool_call = "#39ADB5"
tool_result_ok = "#91B859"
tool_result_fail = "#E53935"
request = "#6182B8"
response = "#7C4DFF"
error = "#E53935"
thinking = "#7C4DFF"
api_usage = "#005761"
headers = "#005761"
rate_limit = "#005761"
context_compact = "#F6A434"

[context_bar]
fill = "#91B859"
warn = "#F6A434"
danger = "#E53935"

[panels]
events = "#39ADB5"
thinking = "#7C4DFF"
logs = "#91B859"

[vhs]
black = "#01579B"
red = "#E53935"
green = "#91B859"
yellow = "#F6A434"
blue = "#6182B8"
purple = "#7C4DFF"
cyan = "#39ADB5"
white = "#272727"
bright_black = "#01579B"
bright_red = "#E53935"
bright_green = "#91B859"
bright_yellow = "#F6A434"
bright_blue = "#6182B8"
bright_purple = "#7C4DFF"
bright_cyan = "#39ADB5"
bright_white = "#272727"
cursor = "#00C6E0"
"##;
