// Theme support for the TUI
//
// Provides color palettes that can be configured via config file.
// "auto" uses terminal's ANSI palette, named themes use true color (RGB).

use ratatui::style::Color;

/// Color palette for the TUI
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,

    // Event colors
    pub tool_call: Color,
    pub tool_result_ok: Color,
    pub tool_result_fail: Color,
    pub request: Color,
    pub response: Color,
    pub error: Color,
    pub thinking: Color,
    pub api_usage: Color,
    pub headers: Color,
    pub rate_limit: Color,
    pub context_compact: Color,

    // UI element colors
    pub context_bar_fill: Color,
    pub context_bar_warn: Color,
    pub context_bar_danger: Color,
    pub status_bar: Color,
    pub title: Color,
    pub border: Color,
    pub highlight: Color,

    // Panel identity colors (used when focused)
    pub panel_events: Color,
    pub panel_thinking: Color,
    pub panel_logs: Color,
    pub panel_detail: Color,
}

impl Theme {
    /// Load theme by name
    pub fn by_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "dracula" => Self::dracula(),
            "monokai" => Self::monokai(),
            "monokai-pro-gogh" => Self::monokai_pro_gogh(),
            "nord" => Self::nord(),
            "gruvbox" => Self::gruvbox(),
            _ => Self::auto(), // "auto" or unknown
        }
    }

    /// Auto theme - uses terminal's ANSI palette
    pub fn auto() -> Self {
        Self {
            name: "auto".to_string(),
            tool_call: Color::Cyan,
            tool_result_ok: Color::Green,
            tool_result_fail: Color::Red,
            request: Color::Blue,
            response: Color::Magenta,
            error: Color::Red,
            thinking: Color::Magenta,
            api_usage: Color::LightBlue,
            headers: Color::Gray,
            rate_limit: Color::Yellow,
            context_compact: Color::Yellow,
            // Muted gauge fills for white text contrast
            context_bar_fill: Color::Rgb(0x00, 0x64, 0x00), // muted green
            context_bar_warn: Color::Rgb(0x80, 0x80, 0x00), // muted yellow
            context_bar_danger: Color::Rgb(0x8b, 0x00, 0x00), // muted red
            status_bar: Color::Green,
            title: Color::Cyan,
            border: Color::White,
            highlight: Color::Yellow,
            // Panel identity colors
            panel_events: Color::Cyan,
            panel_thinking: Color::Magenta,
            panel_logs: Color::DarkGray,
            panel_detail: Color::Yellow,
        }
    }

    /// Dracula theme - https://draculatheme.com
    pub fn dracula() -> Self {
        Self {
            name: "dracula".to_string(),
            tool_call: Color::Rgb(0x8b, 0xe9, 0xfd), // cyan
            tool_result_ok: Color::Rgb(0x50, 0xfa, 0x7b), // green
            tool_result_fail: Color::Rgb(0xff, 0x55, 0x55), // red
            request: Color::Rgb(0xbd, 0x93, 0xf9),   // purple
            response: Color::Rgb(0xff, 0x79, 0xc6),  // pink
            error: Color::Rgb(0xff, 0x55, 0x55),     // red
            thinking: Color::Rgb(0xbd, 0x93, 0xf9),  // purple
            api_usage: Color::Rgb(0x8b, 0xe9, 0xfd), // cyan
            headers: Color::Rgb(0x62, 0x72, 0xa4),   // comment
            rate_limit: Color::Rgb(0xf1, 0xfa, 0x8c), // yellow
            context_compact: Color::Rgb(0xff, 0xb8, 0x6c), // orange
            // Muted gauge fills for white text contrast
            context_bar_fill: Color::Rgb(0x28, 0x7d, 0x3d), // muted green
            context_bar_warn: Color::Rgb(0x78, 0x7d, 0x46), // muted yellow
            context_bar_danger: Color::Rgb(0x80, 0x2a, 0x2a), // muted red
            status_bar: Color::Rgb(0x50, 0xfa, 0x7b),       // green
            title: Color::Rgb(0x8b, 0xe9, 0xfd),            // cyan
            border: Color::Rgb(0x62, 0x72, 0xa4),           // comment
            highlight: Color::Rgb(0xf1, 0xfa, 0x8c),        // yellow
            // Panel identity colors
            panel_events: Color::Rgb(0x8b, 0xe9, 0xfd), // cyan
            panel_thinking: Color::Rgb(0xbd, 0x93, 0xf9), // purple
            panel_logs: Color::Rgb(0x62, 0x72, 0xa4),   // comment
            panel_detail: Color::Rgb(0xf1, 0xfa, 0x8c), // yellow
        }
    }

    /// Monokai Pro theme
    pub fn monokai() -> Self {
        Self {
            name: "monokai".to_string(),
            tool_call: Color::Rgb(0x66, 0xd9, 0xef), // blue
            tool_result_ok: Color::Rgb(0xa6, 0xe2, 0x2e), // green
            tool_result_fail: Color::Rgb(0xf9, 0x26, 0x72), // pink/red
            request: Color::Rgb(0xae, 0x81, 0xff),   // purple
            response: Color::Rgb(0xf9, 0x26, 0x72),  // pink
            error: Color::Rgb(0xf9, 0x26, 0x72),     // pink/red
            thinking: Color::Rgb(0xae, 0x81, 0xff),  // purple
            api_usage: Color::Rgb(0x66, 0xd9, 0xef), // blue
            headers: Color::Rgb(0x75, 0x71, 0x5e),   // comment
            rate_limit: Color::Rgb(0xe6, 0xdb, 0x74), // yellow
            context_compact: Color::Rgb(0xfd, 0x97, 0x1f), // orange
            // Muted gauge fills for white text contrast
            context_bar_fill: Color::Rgb(0x53, 0x71, 0x17), // muted green
            context_bar_warn: Color::Rgb(0x73, 0x6d, 0x3a), // muted yellow
            context_bar_danger: Color::Rgb(0x7c, 0x13, 0x39), // muted pink
            status_bar: Color::Rgb(0xa6, 0xe2, 0x2e),       // green
            title: Color::Rgb(0x66, 0xd9, 0xef),            // blue
            border: Color::Rgb(0x75, 0x71, 0x5e),           // comment
            highlight: Color::Rgb(0xe6, 0xdb, 0x74),        // yellow
            // Panel identity colors
            panel_events: Color::Rgb(0x66, 0xd9, 0xef), // blue
            panel_thinking: Color::Rgb(0xae, 0x81, 0xff), // purple
            panel_logs: Color::Rgb(0x75, 0x71, 0x5e),   // comment
            panel_detail: Color::Rgb(0xe6, 0xdb, 0x74), // yellow
        }
    }

    /// Monokai Pro (Gogh filter) - https://monokai.pro
    pub fn monokai_pro_gogh() -> Self {
        Self {
            name: "monokai-pro-gogh".to_string(),
            tool_call: Color::Rgb(0x78, 0xdc, 0xe8), // blue
            tool_result_ok: Color::Rgb(0xa9, 0xdc, 0x76), // green
            tool_result_fail: Color::Rgb(0xff, 0x61, 0x88), // red
            request: Color::Rgb(0xab, 0x9d, 0xf2),   // purple
            response: Color::Rgb(0xff, 0x61, 0x88),  // red/pink
            error: Color::Rgb(0xff, 0x61, 0x88),     // red
            thinking: Color::Rgb(0xab, 0x9d, 0xf2),  // purple
            api_usage: Color::Rgb(0x78, 0xdc, 0xe8), // blue
            headers: Color::Rgb(0x72, 0x70, 0x72),   // comment gray
            rate_limit: Color::Rgb(0xff, 0xd8, 0x66), // yellow
            context_compact: Color::Rgb(0xfc, 0x98, 0x67), // orange
            // Muted gauge fills for white text contrast
            context_bar_fill: Color::Rgb(0x54, 0x6e, 0x3b), // muted green
            context_bar_warn: Color::Rgb(0x80, 0x6c, 0x33), // muted yellow
            context_bar_danger: Color::Rgb(0x80, 0x30, 0x44), // muted red
            status_bar: Color::Rgb(0xa9, 0xdc, 0x76),       // green
            title: Color::Rgb(0x78, 0xdc, 0xe8),            // blue
            border: Color::Rgb(0x72, 0x70, 0x72),           // comment gray
            highlight: Color::Rgb(0xff, 0xd8, 0x66),        // yellow
            // Panel identity colors
            panel_events: Color::Rgb(0x78, 0xdc, 0xe8), // blue
            panel_thinking: Color::Rgb(0xab, 0x9d, 0xf2), // purple
            panel_logs: Color::Rgb(0x72, 0x70, 0x72),   // comment gray
            panel_detail: Color::Rgb(0xff, 0xd8, 0x66), // yellow
        }
    }

    /// Nord theme - https://nordtheme.com
    pub fn nord() -> Self {
        Self {
            name: "nord".to_string(),
            tool_call: Color::Rgb(0x88, 0xc0, 0xd0), // frost cyan
            tool_result_ok: Color::Rgb(0xa3, 0xbe, 0x8c), // aurora green
            tool_result_fail: Color::Rgb(0xbf, 0x61, 0x6a), // aurora red
            request: Color::Rgb(0xb4, 0x8e, 0xad),   // aurora purple
            response: Color::Rgb(0x81, 0xa1, 0xc1),  // frost blue
            error: Color::Rgb(0xbf, 0x61, 0x6a),     // aurora red
            thinking: Color::Rgb(0xb4, 0x8e, 0xad),  // aurora purple
            api_usage: Color::Rgb(0x8f, 0xbc, 0xbb), // frost teal
            headers: Color::Rgb(0x4c, 0x56, 0x6a),   // polar night
            rate_limit: Color::Rgb(0xeb, 0xcb, 0x8b), // aurora yellow
            context_compact: Color::Rgb(0xd0, 0x87, 0x70), // aurora orange
            // Muted gauge fills for white text contrast
            context_bar_fill: Color::Rgb(0x51, 0x5f, 0x46), // muted green
            context_bar_warn: Color::Rgb(0x75, 0x65, 0x45), // muted yellow
            context_bar_danger: Color::Rgb(0x5f, 0x30, 0x35), // muted red
            status_bar: Color::Rgb(0xa3, 0xbe, 0x8c),       // green
            title: Color::Rgb(0x88, 0xc0, 0xd0),            // frost cyan
            border: Color::Rgb(0x4c, 0x56, 0x6a),           // polar night
            highlight: Color::Rgb(0xeb, 0xcb, 0x8b),        // yellow
            // Panel identity colors
            panel_events: Color::Rgb(0x88, 0xc0, 0xd0), // frost cyan
            panel_thinking: Color::Rgb(0xb4, 0x8e, 0xad), // aurora purple
            panel_logs: Color::Rgb(0x4c, 0x56, 0x6a),   // polar night
            panel_detail: Color::Rgb(0xeb, 0xcb, 0x8b), // yellow
        }
    }

    /// Gruvbox theme - https://github.com/morhetz/gruvbox
    pub fn gruvbox() -> Self {
        Self {
            name: "gruvbox".to_string(),
            tool_call: Color::Rgb(0x83, 0xa5, 0x98), // aqua
            tool_result_ok: Color::Rgb(0xb8, 0xbb, 0x26), // green
            tool_result_fail: Color::Rgb(0xfb, 0x49, 0x34), // red
            request: Color::Rgb(0xd3, 0x86, 0x9b),   // purple
            response: Color::Rgb(0xb1, 0x62, 0x86),  // magenta
            error: Color::Rgb(0xfb, 0x49, 0x34),     // red
            thinking: Color::Rgb(0xd3, 0x86, 0x9b),  // purple
            api_usage: Color::Rgb(0x83, 0xa5, 0x98), // aqua
            headers: Color::Rgb(0x92, 0x83, 0x74),   // gray
            rate_limit: Color::Rgb(0xfa, 0xbd, 0x2f), // yellow
            context_compact: Color::Rgb(0xfe, 0x80, 0x19), // orange
            // Muted gauge fills for white text contrast
            context_bar_fill: Color::Rgb(0x5c, 0x5d, 0x13), // muted green
            context_bar_warn: Color::Rgb(0x7d, 0x5e, 0x17), // muted yellow
            context_bar_danger: Color::Rgb(0x7d, 0x24, 0x1a), // muted red
            status_bar: Color::Rgb(0xb8, 0xbb, 0x26),       // green
            title: Color::Rgb(0x83, 0xa5, 0x98),            // aqua
            border: Color::Rgb(0x92, 0x83, 0x74),           // gray
            highlight: Color::Rgb(0xfa, 0xbd, 0x2f),        // yellow
            // Panel identity colors
            panel_events: Color::Rgb(0x83, 0xa5, 0x98), // aqua
            panel_thinking: Color::Rgb(0xd3, 0x86, 0x9b), // purple
            panel_logs: Color::Rgb(0x92, 0x83, 0x74),   // gray
            panel_detail: Color::Rgb(0xfa, 0xbd, 0x2f), // yellow
        }
    }
}

impl Theme {
    /// Get border color for a panel based on focus state
    ///
    /// Focused panels use their identity color, unfocused use the general border color.
    /// This creates clear visual distinction while preserving panel identity.
    pub fn panel_border(&self, panel: crate::tui::scroll::FocusablePanel, focused: bool) -> Color {
        if focused {
            match panel {
                crate::tui::scroll::FocusablePanel::Events => self.panel_events,
                crate::tui::scroll::FocusablePanel::Thinking => self.panel_thinking,
                crate::tui::scroll::FocusablePanel::Logs => self.panel_logs,
                crate::tui::scroll::FocusablePanel::Detail => self.panel_detail,
            }
        } else {
            self.border
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::auto()
    }
}
