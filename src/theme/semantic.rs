// Semantic theme layer
//
// Maps a ColorPalette's 16 ANSI colors to meaningful UI concepts.
// This separation allows any VHS-compatible palette to work with our UI
// without needing to define 25+ color values manually.
//
// Light theme support: Detects background luminance and adjusts mappings
// to maintain contrast (e.g., uses dark colors for highlights on light bg).

use super::palette::ColorPalette;
use ratatui::style::Color;

/// Semantic color assignments for the TUI.
/// Generated automatically from a ColorPalette using consistent mapping rules.
#[derive(Debug, Clone)]
pub struct SemanticTheme {
    // ─── Event Type Colors ───────────────────────────────────
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

    // ─── UI Element Colors ───────────────────────────────────
    pub context_bar_fill: Color,   // Progress bar: normal
    pub context_bar_warn: Color,   // Progress bar: warning
    pub context_bar_danger: Color, // Progress bar: danger
    pub status_bar: Color,
    pub title: Color,
    pub border: Color,
    pub highlight: Color,

    // ─── Panel Identity Colors (focused border) ──────────────
    pub panel_events: Color,
    pub panel_thinking: Color,
    pub panel_logs: Color,

    // ─── Terminal Colors ─────────────────────────────────────
    pub background: Color,
    pub foreground: Color,
    /// Selection background (from palette)
    pub selection: Color,
    /// Selection foreground (calculated for contrast against selection bg)
    pub selection_fg: Color,
}

impl SemanticTheme {
    /// Calculate relative luminance of a color (0.0 = black, 1.0 = white)
    /// Uses the standard formula: 0.2126*R + 0.7152*G + 0.0722*B
    fn luminance(color: Color) -> f32 {
        match color {
            Color::Rgb(r, g, b) => {
                let r = r as f32 / 255.0;
                let g = g as f32 / 255.0;
                let b = b as f32 / 255.0;
                0.2126 * r + 0.7152 * g + 0.0722 * b
            }
            // Approximate luminance for ANSI colors
            Color::Black => 0.0,
            Color::White | Color::Gray => 0.75,
            Color::DarkGray => 0.25,
            _ => 0.5, // Mid-range for other colors
        }
    }

    /// Create semantic theme from a color palette using standard mapping.
    ///
    /// Mapping philosophy:
    /// - Cyan family → navigation, interactive elements (tool calls, titles)
    /// - Green family → success, completion (results OK, status)
    /// - Red family → errors, failures, danger states
    /// - Yellow family → warnings, highlights, attention
    /// - Blue family → informational (requests, API usage)
    /// - Purple/Magenta family → special states (thinking, responses)
    ///
    /// Light theme adjustments:
    /// - Uses darker variants for highlights and titles
    /// - Swaps to high-contrast alternatives where needed
    pub fn from_palette(p: &ColorPalette) -> Self {
        let is_light = Self::luminance(p.background) > 0.5;

        if is_light {
            Self::from_palette_light(p)
        } else {
            Self::from_palette_dark(p)
        }
    }

    /// Dark theme mappings (original behavior)
    fn from_palette_dark(p: &ColorPalette) -> Self {
        Self {
            // Event types - use bright variants for visibility
            tool_call: p.cyan,
            tool_result_ok: p.green,
            tool_result_fail: p.red,
            request: p.blue,
            response: p.purple,
            error: p.red,
            thinking: p.purple,
            // Metadata/noise tier - blend into background
            api_usage: p.foreground,
            headers: p.foreground,
            rate_limit: p.foreground,
            context_compact: p.yellow,

            // Progress bar fills - ratatui handles text contrast via color inversion
            context_bar_fill: p.green,
            context_bar_warn: p.yellow,
            context_bar_danger: p.red,

            // UI elements
            status_bar: p.foreground,
            title: p.cyan,
            border: p.white,
            highlight: p.yellow,

            // Panel identity (focused state)
            panel_events: p.cyan,
            panel_thinking: p.purple,
            panel_logs: p.green,

            // Terminal integration
            background: p.background,
            foreground: p.foreground,
            selection: p.selection,
            // Calculate selection foreground for contrast
            selection_fg: if Self::luminance(p.selection) > 0.5 {
                Color::Black // Dark text on light selection
            } else {
                Color::White // Light text on dark selection
            },
        }
    }

    /// Light theme mappings - adjusted for contrast on bright backgrounds
    fn from_palette_light(p: &ColorPalette) -> Self {
        Self {
            // Event types - keep saturated colors, they work on light
            tool_call: p.cyan,
            tool_result_ok: p.green,
            tool_result_fail: p.red,
            request: p.blue,
            response: p.purple,
            error: p.red,
            thinking: p.purple,
            // Metadata/noise tier - blend into background
            api_usage: p.foreground,
            headers: p.foreground,
            rate_limit: p.foreground,
            context_compact: p.red,

            // Progress bar fills - ratatui handles text contrast via color inversion
            context_bar_fill: p.green,
            context_bar_warn: p.yellow,
            context_bar_danger: p.red,

            // UI elements - use dark colors for contrast
            status_bar: p.foreground,
            title: p.foreground,     // Dark text, not pastel cyan
            border: p.bright_black,  // Dark grey borders
            highlight: p.foreground, // Dark highlight, not yellow

            // Panel identity - keep colors but they need to be readable
            panel_events: p.blue,
            panel_thinking: p.purple,
            panel_logs: p.green,

            // Terminal integration
            background: p.background,
            foreground: p.foreground,
            selection: p.selection,
            // Calculate selection foreground for contrast
            selection_fg: if Self::luminance(p.selection) > 0.5 {
                Color::Black // Dark text on light selection
            } else {
                Color::White // Light text on dark selection
            },
        }
    }
}
