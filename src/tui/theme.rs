// Theme system for the TUI
//
// Provides customizable color themes that can be switched at runtime.
// Each theme defines colors for all UI elements.

use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

/// Available themes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeKind {
    #[default]
    Dark,
    Light,
    Monokai,
    Dracula,
    Nord,
    Solarized,
}

impl ThemeKind {
    /// Get all available themes
    pub fn all() -> &'static [ThemeKind] {
        &[
            ThemeKind::Dark,
            ThemeKind::Light,
            ThemeKind::Monokai,
            ThemeKind::Dracula,
            ThemeKind::Nord,
            ThemeKind::Solarized,
        ]
    }

    /// Get the next theme in the cycle
    pub fn next(self) -> Self {
        let themes = Self::all();
        let current = themes.iter().position(|&t| t == self).unwrap_or(0);
        themes[(current + 1) % themes.len()]
    }

    /// Get the previous theme in the cycle
    pub fn prev(self) -> Self {
        let themes = Self::all();
        let current = themes.iter().position(|&t| t == self).unwrap_or(0);
        themes[(current + themes.len() - 1) % themes.len()]
    }

    /// Get display name
    pub fn name(&self) -> &'static str {
        match self {
            ThemeKind::Dark => "Dark",
            ThemeKind::Light => "Light",
            ThemeKind::Monokai => "Monokai",
            ThemeKind::Dracula => "Dracula",
            ThemeKind::Nord => "Nord",
            ThemeKind::Solarized => "Solarized",
        }
    }

    /// Get the theme configuration
    pub fn theme(&self) -> Theme {
        match self {
            ThemeKind::Dark => Theme::dark(),
            ThemeKind::Light => Theme::light(),
            ThemeKind::Monokai => Theme::monokai(),
            ThemeKind::Dracula => Theme::dracula(),
            ThemeKind::Nord => Theme::nord(),
            ThemeKind::Solarized => Theme::solarized(),
        }
    }
}

/// Complete theme definition with all UI colors
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are public for future extensibility
pub struct Theme {
    // Base colors
    pub bg: Color,
    pub fg: Color,
    pub border: Color,
    pub border_focused: Color,

    // Title and status
    pub title: Color,
    pub status_bar: Color,

    // Selection
    pub selected_bg: Color,
    pub selected_fg: Color,

    // Event colors
    pub tool_call: Color,
    pub tool_result_success: Color,
    pub tool_result_failure: Color,
    pub request: Color,
    pub response: Color,
    pub error: Color,
    pub headers: Color,
    pub rate_limit: Color,
    pub api_usage: Color,
    pub thinking: Color,

    // Log levels
    pub log_error: Color,
    pub log_warn: Color,
    pub log_info: Color,
    pub log_debug: Color,
    pub log_trace: Color,

    // Stats view
    pub stats_label: Color,
    pub stats_value: Color,
    pub stats_highlight: Color,

    // Context window
    pub context_low: Color,    // < 50% used
    pub context_medium: Color, // 50-80% used
    pub context_high: Color,   // > 80% used

    // Chart colors
    pub chart_primary: Color,
    pub chart_secondary: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[allow(dead_code)] // Methods are public for future extensibility
impl Theme {
    /// Dark theme (default)
    pub fn dark() -> Self {
        Self {
            bg: Color::Reset,
            fg: Color::White,
            border: Color::Gray,
            border_focused: Color::Cyan,

            title: Color::Cyan,
            status_bar: Color::Green,

            selected_bg: Color::DarkGray,
            selected_fg: Color::Yellow,

            tool_call: Color::Cyan,
            tool_result_success: Color::Green,
            tool_result_failure: Color::Red,
            request: Color::Blue,
            response: Color::Magenta,
            error: Color::Red,
            headers: Color::Gray,
            rate_limit: Color::Yellow,
            api_usage: Color::LightBlue,
            thinking: Color::Magenta,

            log_error: Color::Red,
            log_warn: Color::Yellow,
            log_info: Color::Blue,
            log_debug: Color::Gray,
            log_trace: Color::DarkGray,

            stats_label: Color::Gray,
            stats_value: Color::White,
            stats_highlight: Color::Cyan,

            context_low: Color::Green,
            context_medium: Color::Yellow,
            context_high: Color::Red,

            chart_primary: Color::Cyan,
            chart_secondary: Color::Magenta,
        }
    }

    /// Light theme
    pub fn light() -> Self {
        Self {
            bg: Color::White,
            fg: Color::Black,
            border: Color::DarkGray,
            border_focused: Color::Blue,

            title: Color::Blue,
            status_bar: Color::DarkGray,

            selected_bg: Color::LightBlue,
            selected_fg: Color::Black,

            tool_call: Color::Blue,
            tool_result_success: Color::Green,
            tool_result_failure: Color::Red,
            request: Color::Magenta,
            response: Color::Cyan,
            error: Color::Red,
            headers: Color::DarkGray,
            rate_limit: Color::Rgb(184, 134, 11), // Dark goldenrod
            api_usage: Color::Blue,
            thinking: Color::Magenta,

            log_error: Color::Red,
            log_warn: Color::Rgb(184, 134, 11),
            log_info: Color::Blue,
            log_debug: Color::DarkGray,
            log_trace: Color::Gray,

            stats_label: Color::DarkGray,
            stats_value: Color::Black,
            stats_highlight: Color::Blue,

            context_low: Color::Green,
            context_medium: Color::Rgb(184, 134, 11),
            context_high: Color::Red,

            chart_primary: Color::Blue,
            chart_secondary: Color::Magenta,
        }
    }

    /// Monokai theme
    pub fn monokai() -> Self {
        Self {
            bg: Color::Rgb(39, 40, 34),
            fg: Color::Rgb(248, 248, 242),
            border: Color::Rgb(117, 113, 94),
            border_focused: Color::Rgb(166, 226, 46),

            title: Color::Rgb(166, 226, 46),       // Green
            status_bar: Color::Rgb(102, 217, 239), // Cyan

            selected_bg: Color::Rgb(73, 72, 62),
            selected_fg: Color::Rgb(230, 219, 116), // Yellow

            tool_call: Color::Rgb(102, 217, 239), // Cyan
            tool_result_success: Color::Rgb(166, 226, 46), // Green
            tool_result_failure: Color::Rgb(249, 38, 114), // Pink/Red
            request: Color::Rgb(102, 217, 239),   // Cyan
            response: Color::Rgb(174, 129, 255),  // Purple
            error: Color::Rgb(249, 38, 114),
            headers: Color::Rgb(117, 113, 94),
            rate_limit: Color::Rgb(230, 219, 116), // Yellow
            api_usage: Color::Rgb(102, 217, 239),
            thinking: Color::Rgb(174, 129, 255),

            log_error: Color::Rgb(249, 38, 114),
            log_warn: Color::Rgb(230, 219, 116),
            log_info: Color::Rgb(102, 217, 239),
            log_debug: Color::Rgb(117, 113, 94),
            log_trace: Color::Rgb(117, 113, 94),

            stats_label: Color::Rgb(117, 113, 94),
            stats_value: Color::Rgb(248, 248, 242),
            stats_highlight: Color::Rgb(166, 226, 46),

            context_low: Color::Rgb(166, 226, 46),
            context_medium: Color::Rgb(230, 219, 116),
            context_high: Color::Rgb(249, 38, 114),

            chart_primary: Color::Rgb(102, 217, 239),
            chart_secondary: Color::Rgb(174, 129, 255),
        }
    }

    /// Dracula theme
    pub fn dracula() -> Self {
        Self {
            bg: Color::Rgb(40, 42, 54),
            fg: Color::Rgb(248, 248, 242),
            border: Color::Rgb(68, 71, 90),
            border_focused: Color::Rgb(189, 147, 249), // Purple

            title: Color::Rgb(139, 233, 253),     // Cyan
            status_bar: Color::Rgb(80, 250, 123), // Green

            selected_bg: Color::Rgb(68, 71, 90),
            selected_fg: Color::Rgb(241, 250, 140), // Yellow

            tool_call: Color::Rgb(139, 233, 253), // Cyan
            tool_result_success: Color::Rgb(80, 250, 123), // Green
            tool_result_failure: Color::Rgb(255, 85, 85), // Red
            request: Color::Rgb(139, 233, 253),
            response: Color::Rgb(189, 147, 249), // Purple
            error: Color::Rgb(255, 85, 85),
            headers: Color::Rgb(98, 114, 164),     // Comment color
            rate_limit: Color::Rgb(241, 250, 140), // Yellow
            api_usage: Color::Rgb(255, 184, 108),  // Orange
            thinking: Color::Rgb(255, 121, 198),   // Pink

            log_error: Color::Rgb(255, 85, 85),
            log_warn: Color::Rgb(241, 250, 140),
            log_info: Color::Rgb(139, 233, 253),
            log_debug: Color::Rgb(98, 114, 164),
            log_trace: Color::Rgb(68, 71, 90),

            stats_label: Color::Rgb(98, 114, 164),
            stats_value: Color::Rgb(248, 248, 242),
            stats_highlight: Color::Rgb(189, 147, 249),

            context_low: Color::Rgb(80, 250, 123),
            context_medium: Color::Rgb(241, 250, 140),
            context_high: Color::Rgb(255, 85, 85),

            chart_primary: Color::Rgb(139, 233, 253),
            chart_secondary: Color::Rgb(255, 121, 198),
        }
    }

    /// Nord theme
    pub fn nord() -> Self {
        Self {
            bg: Color::Rgb(46, 52, 64),
            fg: Color::Rgb(236, 239, 244),
            border: Color::Rgb(76, 86, 106),
            border_focused: Color::Rgb(136, 192, 208), // Frost

            title: Color::Rgb(136, 192, 208),      // Frost
            status_bar: Color::Rgb(163, 190, 140), // Green

            selected_bg: Color::Rgb(67, 76, 94),
            selected_fg: Color::Rgb(235, 203, 139), // Yellow

            tool_call: Color::Rgb(129, 161, 193), // Frost 2
            tool_result_success: Color::Rgb(163, 190, 140), // Green
            tool_result_failure: Color::Rgb(191, 97, 106), // Red
            request: Color::Rgb(129, 161, 193),
            response: Color::Rgb(180, 142, 173), // Purple
            error: Color::Rgb(191, 97, 106),
            headers: Color::Rgb(76, 86, 106),
            rate_limit: Color::Rgb(235, 203, 139), // Yellow
            api_usage: Color::Rgb(136, 192, 208),
            thinking: Color::Rgb(180, 142, 173),

            log_error: Color::Rgb(191, 97, 106),
            log_warn: Color::Rgb(235, 203, 139),
            log_info: Color::Rgb(129, 161, 193),
            log_debug: Color::Rgb(76, 86, 106),
            log_trace: Color::Rgb(59, 66, 82),

            stats_label: Color::Rgb(76, 86, 106),
            stats_value: Color::Rgb(236, 239, 244),
            stats_highlight: Color::Rgb(136, 192, 208),

            context_low: Color::Rgb(163, 190, 140),
            context_medium: Color::Rgb(235, 203, 139),
            context_high: Color::Rgb(191, 97, 106),

            chart_primary: Color::Rgb(136, 192, 208),
            chart_secondary: Color::Rgb(180, 142, 173),
        }
    }

    /// Solarized dark theme
    pub fn solarized() -> Self {
        Self {
            bg: Color::Rgb(0, 43, 54),
            fg: Color::Rgb(131, 148, 150),
            border: Color::Rgb(88, 110, 117),
            border_focused: Color::Rgb(38, 139, 210), // Blue

            title: Color::Rgb(38, 139, 210),     // Blue
            status_bar: Color::Rgb(133, 153, 0), // Green

            selected_bg: Color::Rgb(7, 54, 66),
            selected_fg: Color::Rgb(181, 137, 0), // Yellow

            tool_call: Color::Rgb(42, 161, 152),          // Cyan
            tool_result_success: Color::Rgb(133, 153, 0), // Green
            tool_result_failure: Color::Rgb(220, 50, 47), // Red
            request: Color::Rgb(38, 139, 210),            // Blue
            response: Color::Rgb(108, 113, 196),          // Violet
            error: Color::Rgb(220, 50, 47),
            headers: Color::Rgb(88, 110, 117),
            rate_limit: Color::Rgb(181, 137, 0), // Yellow
            api_usage: Color::Rgb(42, 161, 152),
            thinking: Color::Rgb(211, 54, 130), // Magenta

            log_error: Color::Rgb(220, 50, 47),
            log_warn: Color::Rgb(181, 137, 0),
            log_info: Color::Rgb(38, 139, 210),
            log_debug: Color::Rgb(88, 110, 117),
            log_trace: Color::Rgb(101, 123, 131),

            stats_label: Color::Rgb(88, 110, 117),
            stats_value: Color::Rgb(147, 161, 161),
            stats_highlight: Color::Rgb(38, 139, 210),

            context_low: Color::Rgb(133, 153, 0),
            context_medium: Color::Rgb(181, 137, 0),
            context_high: Color::Rgb(220, 50, 47),

            chart_primary: Color::Rgb(42, 161, 152),
            chart_secondary: Color::Rgb(211, 54, 130),
        }
    }

    // Helper methods for creating styles

    /// Base style with theme foreground
    pub fn base_style(&self) -> Style {
        Style::default().fg(self.fg)
    }

    /// Border style (unfocused)
    pub fn border_style(&self) -> Style {
        Style::default().fg(self.border)
    }

    /// Border style (focused)
    pub fn border_focused_style(&self) -> Style {
        Style::default().fg(self.border_focused)
    }

    /// Title style
    pub fn title_style(&self) -> Style {
        Style::default().fg(self.title).add_modifier(Modifier::BOLD)
    }

    /// Status bar style
    pub fn status_style(&self) -> Style {
        Style::default().fg(self.status_bar)
    }

    /// Selected item style
    pub fn selected_style(&self) -> Style {
        Style::default()
            .fg(self.selected_fg)
            .add_modifier(Modifier::BOLD)
    }

    /// Error style
    pub fn error_style(&self) -> Style {
        Style::default().fg(self.error).add_modifier(Modifier::BOLD)
    }

    /// Get style for context window usage percentage
    pub fn context_style(&self, usage_pct: f64) -> Style {
        let color = if usage_pct < 0.5 {
            self.context_low
        } else if usage_pct < 0.8 {
            self.context_medium
        } else {
            self.context_high
        };
        Style::default().fg(color)
    }
}
