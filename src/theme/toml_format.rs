// TOML theme format parser
//
// This is the native theme format for anthropic-spy. Each theme explicitly
// defines all semantic colors - no automatic mapping from palette slots.
//
// Format version: 1

use ratatui::style::Color;
use serde::Deserialize;

/// Root structure for TOML theme files
#[derive(Debug, Clone, Deserialize)]
pub struct TomlTheme {
    pub meta: ThemeMeta,
    pub ui: UiColors,
    pub events: EventColors,
    pub context_bar: ContextBarColors,
    pub panels: PanelColors,
    /// Optional code/syntax highlighting colors
    pub code: Option<CodeColors>,
    /// Optional VHS export configuration
    #[allow(dead_code)] // Used by to_vhs_json() for demo recordings
    pub vhs: Option<VhsColors>,
}

/// Theme metadata
#[derive(Debug, Clone, Deserialize)]
pub struct ThemeMeta {
    pub name: String,
    #[allow(dead_code)] // For future schema evolution
    pub version: u32,
    #[serde(default)]
    #[allow(dead_code)] // Metadata for theme attribution
    pub author: Option<String>,
}

/// Base UI chrome colors
#[derive(Debug, Clone, Deserialize)]
pub struct UiColors {
    pub background: String,
    pub foreground: String,
    pub border: String,
    pub border_focused: String,
    pub title: String,
    pub status_bar: String,
    pub selection_bg: String,
    pub selection_fg: String,
    /// Optional muted/secondary text color (falls back to api_usage)
    pub muted: Option<String>,
    /// Optional border style: "plain", "rounded", "double", "thick" (default: plain)
    pub border_type: Option<String>,
}

/// Event type colors for the events panel
#[derive(Debug, Clone, Deserialize)]
pub struct EventColors {
    pub tool_call: String,
    pub tool_result_ok: String,
    pub tool_result_fail: String,
    pub request: String,
    pub response: String,
    pub error: String,
    pub thinking: String,
    pub api_usage: String,
    pub headers: String,
    pub rate_limit: String,
    pub context_compact: String,
}

/// Context bar (gauge) colors
#[derive(Debug, Clone, Deserialize)]
pub struct ContextBarColors {
    pub fill: String,
    pub warn: String,
    pub danger: String,
}

/// Panel identity colors (focused border)
#[derive(Debug, Clone, Deserialize)]
pub struct PanelColors {
    pub events: String,
    pub thinking: String,
    pub logs: String,
}

/// Code/syntax highlighting colors (optional)
#[derive(Debug, Clone, Deserialize)]
pub struct CodeColors {
    /// Color for `inline code` spans
    pub inline: String,
    /// Color for fenced code blocks
    pub block: String,
}

/// VHS export colors (optional, for demo recordings)
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // All fields used by to_vhs_json()
pub struct VhsColors {
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub purple: String,
    pub cyan: String,
    pub white: String,
    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_purple: String,
    pub bright_cyan: String,
    pub bright_white: String,
    pub cursor: String,
}

impl TomlTheme {
    /// Parse a TOML theme from string
    pub fn from_str(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Parse a color string to ratatui Color
    /// Supports:
    /// - Hex format: #RRGGBB
    /// - ANSI format: ansi:0-15, ansi:fg, ansi:bg (for terminal-native colors)
    pub fn parse_color(value: &str) -> Color {
        // Handle ANSI color codes (for Terminal theme - inherits terminal colors)
        if let Some(ansi) = value.strip_prefix("ansi:") {
            return match ansi {
                "0" => Color::Black,
                "1" => Color::Red,
                "2" => Color::Green,
                "3" => Color::Yellow,
                "4" => Color::Blue,
                "5" => Color::Magenta,
                "6" => Color::Cyan,
                "7" => Color::White,
                "8" => Color::DarkGray,
                "9" => Color::LightRed,
                "10" => Color::LightGreen,
                "11" => Color::LightYellow,
                "12" => Color::LightBlue,
                "13" => Color::LightMagenta,
                "14" => Color::LightCyan,
                "15" => Color::Gray,
                "fg" => Color::Reset, // Use terminal default foreground
                "bg" => Color::Reset, // Use terminal default background
                _ => Color::White,
            };
        }

        // Handle hex format
        let hex = value.trim_start_matches('#');
        if hex.len() != 6 {
            return Color::White; // fallback
        }
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
        Color::Rgb(r, g, b)
    }

    /// Export VHS-compatible JSON (if vhs section is defined)
    #[allow(dead_code)] // CLI utility for VHS demo recordings
    pub fn to_vhs_json(&self) -> Option<String> {
        let vhs = self.vhs.as_ref()?;
        Some(
            serde_json::json!({
                "name": self.meta.name,
                "black": &vhs.black,
                "red": &vhs.red,
                "green": &vhs.green,
                "yellow": &vhs.yellow,
                "blue": &vhs.blue,
                "purple": &vhs.purple,
                "cyan": &vhs.cyan,
                "white": &vhs.white,
                "brightBlack": &vhs.bright_black,
                "brightRed": &vhs.bright_red,
                "brightGreen": &vhs.bright_green,
                "brightYellow": &vhs.bright_yellow,
                "brightBlue": &vhs.bright_blue,
                "brightPurple": &vhs.bright_purple,
                "brightCyan": &vhs.bright_cyan,
                "brightWhite": &vhs.bright_white,
                "background": &self.ui.background,
                "foreground": &self.ui.foreground,
                "cursor": &vhs.cursor,
                "selection": &self.ui.selection_bg
            })
            .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_color() {
        assert_eq!(TomlTheme::parse_color("#ff0000"), Color::Rgb(255, 0, 0));
        assert_eq!(TomlTheme::parse_color("#00ff00"), Color::Rgb(0, 255, 0));
        assert_eq!(TomlTheme::parse_color("0000ff"), Color::Rgb(0, 0, 255));
    }

    #[test]
    fn test_parse_theme() {
        let toml = r##"
[meta]
name = "Test Theme"
version = 1

[ui]
background = "#1e1e2e"
foreground = "#cdd6f4"
border = "#45475a"
border_focused = "#f5c2e7"
title = "#cdd6f4"
status_bar = "#cdd6f4"
selection_bg = "#45475a"
selection_fg = "#cdd6f4"

[events]
tool_call = "#89b4fa"
tool_result_ok = "#a6e3a1"
tool_result_fail = "#f38ba8"
request = "#9399b2"
response = "#f5c2e7"
error = "#f38ba8"
thinking = "#f5c2e7"
api_usage = "#9399b2"
headers = "#9399b2"
rate_limit = "#9399b2"
context_compact = "#f9e2af"

[context_bar]
fill = "#89b4fa"
warn = "#f9e2af"
danger = "#f38ba8"

[panels]
events = "#89b4fa"
thinking = "#f5c2e7"
logs = "#a6e3a1"
"##;

        let theme = TomlTheme::from_str(toml).unwrap();
        assert_eq!(theme.meta.name, "Test Theme");
        assert_eq!(theme.meta.version, 1);
        assert_eq!(theme.ui.background, "#1e1e2e");
        assert!(theme.vhs.is_none());
    }
}
