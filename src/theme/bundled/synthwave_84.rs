//! Synthwave 84 - Retro neon synthwave theme

pub const THEME: &str = r##"# Synthwave 84 theme for anthropic-spy
# Retro neon synthwave theme

[meta]
name = "Synthwave 84"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#2A2139"
foreground = "#FFFFFF"
border = "#FFFFFF"
border_focused = "#FEDE5D"
title = "#36F9F6"
status_bar = "#FFFFFF"
selection_bg = "#463465"
selection_fg = "#FFFFFF"

[events]
tool_call = "#36F9F6"
tool_result_ok = "#72F1B8"
tool_result_fail = "#FE4450"
request = "#34D3FB"
response = "#FF7EDB"
error = "#FE4450"
thinking = "#FF7EDB"
api_usage = "#FFFFFF"
headers = "#FFFFFF"
rate_limit = "#FFFFFF"
context_compact = "#FEDE5D"

[context_bar]
fill = "#72F1B8"
warn = "#FEDE5D"
danger = "#FE4450"

[panels]
events = "#36F9F6"
thinking = "#FF7EDB"
logs = "#72F1B8"

[vhs]
black = "#848BBD"
red = "#FE4450"
green = "#72F1B8"
yellow = "#FEDE5D"
blue = "#34D3FB"
purple = "#FF7EDB"
cyan = "#36F9F6"
white = "#B6B1B1"
bright_black = "#848BBD"
bright_red = "#FE4450"
bright_green = "#72F1B8"
bright_yellow = "#FEDE5D"
bright_blue = "#34D3FB"
bright_purple = "#FF7EDB"
bright_cyan = "#36F9F6"
bright_white = "#FFFFFF"
cursor = "#F92AAD"
"##;
