//! Monokai Soda - Vibrant variant with darker background

pub const THEME: &str = r##"# Monokai Soda theme for anthropic-spy
# Vibrant variant with darker background

[meta]
name = "Monokai Soda"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#1a1a1a"
foreground = "#c4c5b5"
border = "#c4c5b5"
border_focused = "#e0d561"
title = "#58d1eb"
status_bar = "#c4c5b5"
selection_bg = "#343434"
selection_fg = "#f6f6ef"

[events]
tool_call = "#58d1eb"
tool_result_ok = "#98e024"
tool_result_fail = "#f4005f"
request = "#9d65ff"
response = "#f4005f"
error = "#f4005f"
thinking = "#9d65ff"
api_usage = "#c4c5b5"
headers = "#c4c5b5"
rate_limit = "#c4c5b5"
context_compact = "#e0d561"

[context_bar]
fill = "#98e024"
warn = "#fa8419"
danger = "#f4005f"

[panels]
events = "#58d1eb"
thinking = "#9d65ff"
logs = "#98e024"

[vhs]
black = "#1a1a1a"
red = "#f4005f"
green = "#98e024"
yellow = "#fa8419"
blue = "#9d65ff"
purple = "#f4005f"
cyan = "#58d1eb"
white = "#c4c5b5"
bright_black = "#625e4c"
bright_red = "#f4005f"
bright_green = "#98e024"
bright_yellow = "#e0d561"
bright_blue = "#9d65ff"
bright_purple = "#f4005f"
bright_cyan = "#58d1eb"
bright_white = "#f6f6ef"
cursor = "#c4c5b5"
"##;
