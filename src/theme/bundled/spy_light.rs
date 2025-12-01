//! Spy Light - Light variant of the flagship theme
//! Soft observatory, gentle workshop

pub const THEME: &str = r##"# Spy Light theme for anthropic-spy
# Soft observatory, gentle workshop
# "Pleasant to read, never straining"

[meta]
name = "Spy Light"
version = 1
author = "anthropic-spy"

[ui]
background = "#faf6f0"
foreground = "#5c5650"
border = "#cdc4b8"
border_focused = "#c4784a"
title = "#c4784a"
status_bar = "#c4784a"
selection_bg = "#ede6db"
selection_fg = "#3d3834"
muted = "#857c72"
border_type = "rounded"

[events]
tool_call = "#3d8a84"
tool_result_ok = "#6a8f4a"
tool_result_fail = "#b85a4a"
request = "#4a7a99"
response = "#8a6a8f"
error = "#b85a4a"
thinking = "#8a6a8f"
api_usage = "#7a7268"
headers = "#7a7268"
rate_limit = "#7a7268"
context_compact = "#c4944a"

[context_bar]
fill = "#3d8a84"
warn = "#c4944a"
danger = "#b85a4a"

[panels]
events = "#4a7a99"
thinking = "#8a6a8f"
logs = "#6a8f4a"

[code]
inline = "#c4784a"
block = "#6a7880"
"##;
