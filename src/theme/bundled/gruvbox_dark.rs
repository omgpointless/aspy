//! Gruvbox Dark - Retro groove color scheme

pub const THEME: &str = r##"# Gruvbox Dark theme for anthropic-spy
# Retro groove color scheme

[meta]
name = "Gruvbox Dark"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#282828"
foreground = "#ebdbb2"
border = "#ebdbb2"
border_focused = "#fabd2f"
title = "#689d6a"
status_bar = "#ebdbb2"
selection_bg = "#665c54"
selection_fg = "#ebdbb2"

[events]
tool_call = "#689d6a"
tool_result_ok = "#b8bb26"
tool_result_fail = "#fb4934"
request = "#83a598"
response = "#d3869b"
error = "#fb4934"
thinking = "#d3869b"
api_usage = "#ebdbb2"
headers = "#ebdbb2"
rate_limit = "#ebdbb2"
context_compact = "#fabd2f"

[context_bar]
fill = "#b8bb26"
warn = "#fabd2f"
danger = "#fb4934"

[panels]
events = "#689d6a"
thinking = "#d3869b"
logs = "#b8bb26"

[vhs]
black = "#282828"
red = "#cc241d"
green = "#98971a"
yellow = "#d79921"
blue = "#458588"
purple = "#b16286"
cyan = "#689d6a"
white = "#a89984"
bright_black = "#928374"
bright_red = "#fb4934"
bright_green = "#b8bb26"
bright_yellow = "#fabd2f"
bright_blue = "#83a598"
bright_purple = "#d3869b"
bright_cyan = "#8ec07c"
bright_white = "#ebdbb2"
cursor = "#ebdbb2"
"##;
