//! Ayu Mirage - Modern dark theme with soft colors

pub const THEME: &str = r##"# Ayu Mirage theme for anthropic-spy
# Modern dark theme with soft colors

[meta]
name = "Ayu Mirage"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#1f2430"
foreground = "#cccac2"
border = "#cccac2"
border_focused = "#facc6e"
title = "#90e1c6"
status_bar = "#cccac2"
selection_bg = "#409fff"
selection_fg = "#ffffff"

[events]
tool_call = "#90e1c6"
tool_result_ok = "#87d96c"
tool_result_fail = "#ed8274"
request = "#6dcbfa"
response = "#dabafa"
error = "#ed8274"
thinking = "#dabafa"
api_usage = "#cccac2"
headers = "#cccac2"
rate_limit = "#cccac2"
context_compact = "#facc6e"

[context_bar]
fill = "#87d96c"
warn = "#facc6e"
danger = "#ed8274"

[panels]
events = "#6dcbfa"
thinking = "#dabafa"
logs = "#87d96c"

[vhs]
black = "#171b24"
red = "#ed8274"
green = "#87d96c"
yellow = "#facc6e"
blue = "#6dcbfa"
purple = "#dabafa"
cyan = "#90e1c6"
white = "#c7c7c7"
bright_black = "#686868"
bright_red = "#f28779"
bright_green = "#d5ff80"
bright_yellow = "#ffd173"
bright_blue = "#73d0ff"
bright_purple = "#dfbfff"
bright_cyan = "#95e6cb"
bright_white = "#ffffff"
cursor = "#ffcc66"
"##;
