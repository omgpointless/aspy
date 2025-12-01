//! Catppuccin Mocha - Soothing pastel dark theme

pub const THEME: &str = r##"# Catppuccin Mocha theme for anthropic-spy
# Soothing pastel dark theme

[meta]
name = "Catppuccin Mocha"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#1e1e2e"
foreground = "#cdd6f4"
border = "#585b70"
border_focused = "#f9e2af"
title = "#89b4fa"
status_bar = "#cdd6f4"
selection_bg = "#585b70"
selection_fg = "#cdd6f4"

[events]
tool_call = "#89b4fa"
tool_result_ok = "#a6e3a1"
tool_result_fail = "#f38ba8"
request = "#89b4fa"
response = "#f5c2e7"
error = "#f38ba8"
thinking = "#f5c2e7"
api_usage = "#cdd6f4"
headers = "#cdd6f4"
rate_limit = "#cdd6f4"
context_compact = "#f9e2af"

[context_bar]
fill = "#a6e3a1"
warn = "#f9e2af"
danger = "#f38ba8"

[panels]
events = "#89b4fa"
thinking = "#f5c2e7"
logs = "#a6e3a1"

[vhs]
black = "#45475a"
red = "#f38ba8"
green = "#a6e3a1"
yellow = "#f9e2af"
blue = "#89b4fa"
purple = "#f5c2e7"
cyan = "#94e2d5"
white = "#a6adc8"
bright_black = "#585b70"
bright_red = "#f37799"
bright_green = "#89d88b"
bright_yellow = "#ebd391"
bright_blue = "#74a8fc"
bright_purple = "#f2aede"
bright_cyan = "#6bd7ca"
bright_white = "#bac2de"
cursor = "#f5e0dc"
"##;
