// Color palette matching VHS/iTerm2-Color-Schemes format
//
// This is the base layer of our theme system - a 16-color ANSI palette
// plus background/foreground/cursor/selection colors. This format is
// compatible with VHS themes, enabling seamless demo recording.

use ratatui::style::Color;
use serde::Deserialize;

/// Standard 16-color ANSI palette plus terminal special colors.
/// This mirrors the VHS JSON theme format exactly.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Full palette kept for VHS export compatibility
pub struct ColorPalette {
    pub name: String,

    // Standard ANSI colors (0-7)
    pub black: Color,
    pub red: Color,
    pub green: Color,
    pub yellow: Color,
    pub blue: Color,
    pub purple: Color, // Called "magenta" in ANSI, "purple" in VHS
    pub cyan: Color,
    pub white: Color,

    // Bright variants (8-15)
    pub bright_black: Color,
    pub bright_red: Color,
    pub bright_green: Color,
    pub bright_yellow: Color,
    pub bright_blue: Color,
    pub bright_purple: Color,
    pub bright_cyan: Color,
    pub bright_white: Color,

    // Terminal special colors
    pub background: Color,
    pub foreground: Color,
    pub cursor: Color,
    pub selection: Color,
}

/// VHS JSON format (for deserialization)
#[derive(Debug, Deserialize)]
pub struct VhsTheme {
    pub name: String,
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub purple: String,
    pub cyan: String,
    pub white: String,
    #[serde(rename = "brightBlack")]
    pub bright_black: String,
    #[serde(rename = "brightRed")]
    pub bright_red: String,
    #[serde(rename = "brightGreen")]
    pub bright_green: String,
    #[serde(rename = "brightYellow")]
    pub bright_yellow: String,
    #[serde(rename = "brightBlue")]
    pub bright_blue: String,
    #[serde(rename = "brightPurple")]
    pub bright_purple: String,
    #[serde(rename = "brightCyan")]
    pub bright_cyan: String,
    #[serde(rename = "brightWhite")]
    pub bright_white: String,
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub selection: String,
}

impl VhsTheme {
    /// Parse a color string into a ratatui Color
    /// Supports:
    /// - Hex format: #RRGGBB
    /// - ANSI format: ansi:0-15, ansi:fg, ansi:bg
    fn parse_color(value: &str) -> Color {
        // Handle ANSI color codes (for Terminal theme)
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
                "fg" => Color::Reset, // Use terminal default
                "bg" => Color::Reset, // Use terminal default
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
}

impl From<VhsTheme> for ColorPalette {
    fn from(vhs: VhsTheme) -> Self {
        Self {
            name: vhs.name,
            black: VhsTheme::parse_color(&vhs.black),
            red: VhsTheme::parse_color(&vhs.red),
            green: VhsTheme::parse_color(&vhs.green),
            yellow: VhsTheme::parse_color(&vhs.yellow),
            blue: VhsTheme::parse_color(&vhs.blue),
            purple: VhsTheme::parse_color(&vhs.purple),
            cyan: VhsTheme::parse_color(&vhs.cyan),
            white: VhsTheme::parse_color(&vhs.white),
            bright_black: VhsTheme::parse_color(&vhs.bright_black),
            bright_red: VhsTheme::parse_color(&vhs.bright_red),
            bright_green: VhsTheme::parse_color(&vhs.bright_green),
            bright_yellow: VhsTheme::parse_color(&vhs.bright_yellow),
            bright_blue: VhsTheme::parse_color(&vhs.bright_blue),
            bright_purple: VhsTheme::parse_color(&vhs.bright_purple),
            bright_cyan: VhsTheme::parse_color(&vhs.bright_cyan),
            bright_white: VhsTheme::parse_color(&vhs.bright_white),
            background: VhsTheme::parse_color(&vhs.background),
            foreground: VhsTheme::parse_color(&vhs.foreground),
            cursor: VhsTheme::parse_color(&vhs.cursor),
            selection: VhsTheme::parse_color(&vhs.selection),
        }
    }
}

impl ColorPalette {
    /// Load palette from JSON string (VHS format)
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let vhs: VhsTheme = serde_json::from_str(json)?;
        Ok(vhs.into())
    }

    /// Export palette back to VHS JSON format
    #[allow(dead_code)] // Used for VHS demo recordings
    pub fn to_vhs_json(&self) -> String {
        fn color_to_hex(c: Color) -> String {
            match c {
                Color::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
                _ => "#ffffff".to_string(),
            }
        }

        serde_json::json!({
            "name": self.name,
            "black": color_to_hex(self.black),
            "red": color_to_hex(self.red),
            "green": color_to_hex(self.green),
            "yellow": color_to_hex(self.yellow),
            "blue": color_to_hex(self.blue),
            "purple": color_to_hex(self.purple),
            "cyan": color_to_hex(self.cyan),
            "white": color_to_hex(self.white),
            "brightBlack": color_to_hex(self.bright_black),
            "brightRed": color_to_hex(self.bright_red),
            "brightGreen": color_to_hex(self.bright_green),
            "brightYellow": color_to_hex(self.bright_yellow),
            "brightBlue": color_to_hex(self.bright_blue),
            "brightPurple": color_to_hex(self.bright_purple),
            "brightCyan": color_to_hex(self.bright_cyan),
            "brightWhite": color_to_hex(self.bright_white),
            "background": color_to_hex(self.background),
            "foreground": color_to_hex(self.foreground),
            "cursor": color_to_hex(self.cursor),
            "selection": color_to_hex(self.selection)
        })
        .to_string()
    }
}
