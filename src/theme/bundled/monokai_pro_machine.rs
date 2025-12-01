//! Monokai Pro Machine - Industrial, teal-accented variant

pub const THEME: &str = r##"# Monokai Pro Machine theme for anthropic-spy
# Industrial, teal-accented variant

[meta]
name = "Monokai Pro Machine"
version = 1
author = "iTerm2-Color-Schemes"

[ui]
background = "#273136"
foreground = "#f2fffc"
border = "#f2fffc"
border_focused = "#ffed72"
title = "#7cd5f1"
status_bar = "#f2fffc"
selection_bg = "#545f62"
selection_fg = "#f2fffc"

[events]
tool_call = "#7cd5f1"
tool_result_ok = "#a2e57b"
tool_result_fail = "#ff6d7e"
request = "#ffb270"
response = "#baa0f8"
error = "#ff6d7e"
thinking = "#baa0f8"
api_usage = "#f2fffc"
headers = "#f2fffc"
rate_limit = "#f2fffc"
context_compact = "#ffed72"

[context_bar]
fill = "#a2e57b"
warn = "#ffed72"
danger = "#ff6d7e"

[panels]
events = "#7cd5f1"
thinking = "#baa0f8"
logs = "#a2e57b"

[vhs]
black = "#273136"
red = "#ff6d7e"
green = "#a2e57b"
yellow = "#ffed72"
blue = "#ffb270"
purple = "#baa0f8"
cyan = "#7cd5f1"
white = "#f2fffc"
bright_black = "#6b7678"
bright_red = "#ff6d7e"
bright_green = "#a2e57b"
bright_yellow = "#ffed72"
bright_blue = "#ffb270"
bright_purple = "#baa0f8"
bright_cyan = "#7cd5f1"
bright_white = "#f2fffc"
cursor = "#b8c4c3"
"##;
