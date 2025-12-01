//! Material Lighter - Clean light theme

pub const THEME: &str = r##"# Material Lighter theme for anthropic-spy
# Clean light theme

[meta]
name = "Material Lighter"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#FAFAFA"
foreground = "#546E7A"
border = "#546E7A"
border_focused = "#F6A434"
title = "#39ADB5"
status_bar = "#546E7A"
selection_bg = "#80CBC4"
selection_fg = "#272727"

[events]
tool_call = "#39ADB5"
tool_result_ok = "#91B859"
tool_result_fail = "#E53935"
request = "#6182B8"
response = "#7C4DFF"
error = "#E53935"
thinking = "#7C4DFF"
api_usage = "#546E7A"
headers = "#546E7A"
rate_limit = "#546E7A"
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
black = "#AABFC9"
red = "#E53935"
green = "#91B859"
yellow = "#F6A434"
blue = "#6182B8"
purple = "#7C4DFF"
cyan = "#39ADB5"
white = "#272727"
bright_black = "#AABFC9"
bright_red = "#E53935"
bright_green = "#91B859"
bright_yellow = "#F6A434"
bright_blue = "#6182B8"
bright_purple = "#7C4DFF"
bright_cyan = "#39ADB5"
bright_white = "#272727"
cursor = "#00BCD4"
"##;
