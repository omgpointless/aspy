//! Catppuccin Latte - Soothing pastel light theme

pub const THEME: &str = r##"# Catppuccin Latte theme for anthropic-spy
# Soothing pastel light theme

[meta]
name = "Catppuccin Latte"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#eff1f5"
foreground = "#4c4f69"
border = "#4c4f69"
border_focused = "#df8e1d"
title = "#179299"
status_bar = "#4c4f69"
selection_bg = "#acb0be"
selection_fg = "#4c4f69"

[events]
tool_call = "#179299"
tool_result_ok = "#40a02b"
tool_result_fail = "#d20f39"
request = "#1e66f5"
response = "#ea76cb"
error = "#d20f39"
thinking = "#ea76cb"
api_usage = "#4c4f69"
headers = "#4c4f69"
rate_limit = "#4c4f69"
context_compact = "#df8e1d"

[context_bar]
fill = "#40a02b"
warn = "#df8e1d"
danger = "#d20f39"

[panels]
events = "#1e66f5"
thinking = "#ea76cb"
logs = "#40a02b"

[vhs]
black = "#5c5f77"
red = "#d20f39"
green = "#40a02b"
yellow = "#df8e1d"
blue = "#1e66f5"
purple = "#ea76cb"
cyan = "#179299"
white = "#acb0be"
bright_black = "#6c6f85"
bright_red = "#de293e"
bright_green = "#49af3d"
bright_yellow = "#eea02d"
bright_blue = "#456eff"
bright_purple = "#fe85d8"
bright_cyan = "#2d9fa8"
bright_white = "#bcc0cc"
cursor = "#dc8a78"
"##;
