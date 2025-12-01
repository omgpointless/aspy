//! Monokai Pro Ristretto - Warm, coffee-inspired variant

pub const THEME: &str = r##"# Monokai Pro Ristretto theme for anthropic-spy
# Warm, coffee-inspired variant

[meta]
name = "Monokai Pro Ristretto"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#2c2525"
foreground = "#fff1f3"
border = "#fff1f3"
border_focused = "#f9cc6c"
title = "#85dacc"
status_bar = "#fff1f3"
selection_bg = "#5b5353"
selection_fg = "#fff1f3"

[events]
tool_call = "#85dacc"
tool_result_ok = "#adda78"
tool_result_fail = "#fd6883"
request = "#f38d70"
response = "#a8a9eb"
error = "#fd6883"
thinking = "#a8a9eb"
api_usage = "#fff1f3"
headers = "#fff1f3"
rate_limit = "#fff1f3"
context_compact = "#f9cc6c"

[context_bar]
fill = "#adda78"
warn = "#f9cc6c"
danger = "#fd6883"

[panels]
events = "#85dacc"
thinking = "#a8a9eb"
logs = "#adda78"

[vhs]
black = "#2c2525"
red = "#fd6883"
green = "#adda78"
yellow = "#f9cc6c"
blue = "#f38d70"
purple = "#a8a9eb"
cyan = "#85dacc"
white = "#fff1f3"
bright_black = "#72696a"
bright_red = "#fd6883"
bright_green = "#adda78"
bright_yellow = "#f9cc6c"
bright_blue = "#f38d70"
bright_purple = "#a8a9eb"
bright_cyan = "#85dacc"
bright_white = "#fff1f3"
cursor = "#c3b7b8"
"##;
