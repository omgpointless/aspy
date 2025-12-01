//! JetBrains Darcula - The classic IDE dark theme

pub const THEME: &str = r##"# JetBrains Darcula theme for anthropic-spy
# The classic IDE dark theme

[meta]
name = "JetBrains Darcula"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#202020"
foreground = "#adadad"
border = "#adadad"
border_focused = "#ffff00"
title = "#33c2c1"
status_bar = "#adadad"
selection_bg = "#1a3272"
selection_fg = "#eeeeee"

[events]
tool_call = "#33c2c1"
tool_result_ok = "#67ff4f"
tool_result_fail = "#fa5355"
request = "#4581eb"
response = "#fa54ff"
error = "#fa5355"
thinking = "#fa54ff"
api_usage = "#adadad"
headers = "#adadad"
rate_limit = "#adadad"
context_compact = "#ffff00"

[context_bar]
fill = "#126e00"
warn = "#c2c300"
danger = "#fa5355"

[panels]
events = "#33c2c1"
thinking = "#fa54ff"
logs = "#67ff4f"

[vhs]
black = "#000000"
red = "#fa5355"
green = "#126e00"
yellow = "#c2c300"
blue = "#4581eb"
purple = "#fa54ff"
cyan = "#33c2c1"
white = "#adadad"
bright_black = "#555555"
bright_red = "#fb7172"
bright_green = "#67ff4f"
bright_yellow = "#ffff00"
bright_blue = "#6d9df1"
bright_purple = "#fb82ff"
bright_cyan = "#60d3d1"
bright_white = "#eeeeee"
cursor = "#ffffff"
"##;
