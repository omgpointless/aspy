// Theme system for the TUI
//
// Architecture (v2 - TOML-based):
// - TomlTheme: Native format with explicit semantic color definitions
// - Theme: Final resolved theme with all colors ready for use
// - Legacy JSON support for backwards compatibility during migration
//
// Theme loading priority:
// 1. External TOML themes from ~/.config/anthropic-spy/themes/*.toml
// 2. External JSON themes (legacy, will be converted)
// 3. Bundled themes (extracted on first run)
// 4. Fallback to hardcoded default

mod bundled;
mod embedded;
mod palette;
mod semantic;
mod toml_format;

pub use toml_format::TomlTheme;

// Legacy exports (for migration period)
pub use palette::ColorPalette;
pub use semantic::SemanticTheme;

use ratatui::style::Color;
use ratatui::widgets::BorderType;
use std::path::PathBuf;

/// Theme configuration options
#[derive(Debug, Clone)]
pub struct ThemeConfig {
    /// Use theme's background color (true) or terminal's default (false)
    pub use_theme_background: bool,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            use_theme_background: true,
        }
    }
}

/// Complete resolved theme ready for use in the TUI.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,

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
    pub context_bar_fill: Color,
    pub context_bar_warn: Color,
    pub context_bar_danger: Color,
    pub status_bar: Color,
    pub title: Color,
    pub border: Color,
    pub highlight: Color, // Maps to border_focused

    // ─── Panel Identity Colors ───────────────────────────────
    pub panel_events: Color,
    pub panel_thinking: Color,
    pub panel_logs: Color,

    // ─── Terminal Colors ─────────────────────────────────────
    pub background: Color,
    pub foreground: Color,

    // ─── Selection Colors ────────────────────────────────────
    pub selection: Color,
    pub selection_fg: Color,

    // ─── Secondary/Muted Text ────────────────────────────────
    pub muted: Color,

    // ─── Border Style ────────────────────────────────────────
    pub border_type: BorderType,

    // ─── Code Highlighting Colors ────────────────────────────
    pub code_inline: Color,
    pub code_block: Color,

    // ─── Source for VHS export ───────────────────────────────
    #[allow(dead_code)] // Used by to_vhs_json() for demo recording export
    toml_source: Option<TomlTheme>,
}

impl Theme {
    /// Load theme by name with default configuration
    pub fn by_name(name: &str) -> Self {
        Self::by_name_with_config(name, &ThemeConfig::default())
    }

    /// Load theme by name with custom configuration
    pub fn by_name_with_config(name: &str, config: &ThemeConfig) -> Self {
        // Try TOML format first (native)
        if let Some(theme) = Self::load_toml(name, config) {
            return theme;
        }

        // Fall back to legacy JSON format
        if let Some(theme) = Self::load_legacy_json(name, config) {
            return theme;
        }

        // Ultimate fallback: hardcoded default
        Self::hardcoded_default(config)
    }

    /// Load from TOML theme file or bundled theme
    fn load_toml(name: &str, config: &ThemeConfig) -> Option<Self> {
        // Try external TOML file first
        if let Some(config_dir) = Self::themes_dir() {
            let toml_path = config_dir.join(format!("{}.toml", name));

            if toml_path.exists() {
                if let Ok(contents) = std::fs::read_to_string(&toml_path) {
                    if let Ok(toml_theme) = TomlTheme::from_str(&contents) {
                        return Some(Self::from_toml(toml_theme, config));
                    }
                }
            }

            // Try with spaces replaced by underscores
            let normalized = name.replace(' ', "_");
            let normalized_path = config_dir.join(format!("{}.toml", normalized));

            if normalized_path.exists() {
                if let Ok(contents) = std::fs::read_to_string(&normalized_path) {
                    if let Ok(toml_theme) = TomlTheme::from_str(&contents) {
                        return Some(Self::from_toml(toml_theme, config));
                    }
                }
            }
        }

        // Try bundled themes (compiled into binary)
        let normalized = name.replace(' ', "_");
        let filename = format!("{}.toml", normalized);
        for theme in bundled::BUNDLED_THEMES {
            if theme.filename == filename || theme.filename.eq_ignore_ascii_case(&filename) {
                if let Ok(toml_theme) = TomlTheme::from_str(theme.content) {
                    return Some(Self::from_toml(toml_theme, config));
                }
            }
        }

        None
    }

    /// Load from legacy JSON format (VHS-compatible)
    fn load_legacy_json(name: &str, config: &ThemeConfig) -> Option<Self> {
        // Try embedded themes first
        if let Some(json) = embedded::get_embedded_theme(name) {
            if let Ok(palette) = ColorPalette::from_json(json) {
                return Some(Self::from_palette_legacy(palette, config));
            }
        }

        // Try external JSON file
        let config_dir = Self::themes_dir()?;

        for extension in ["json", "json"] {
            let json_path = config_dir.join(format!("{}.{}", name, extension));
            if json_path.exists() {
                if let Ok(contents) = std::fs::read_to_string(&json_path) {
                    if let Ok(palette) = ColorPalette::from_json(&contents) {
                        return Some(Self::from_palette_legacy(palette, config));
                    }
                }
            }

            // Try normalized name
            let normalized = name.replace(' ', "_");
            let normalized_path = config_dir.join(format!("{}.{}", normalized, extension));
            if normalized_path.exists() {
                if let Ok(contents) = std::fs::read_to_string(&normalized_path) {
                    if let Ok(palette) = ColorPalette::from_json(&contents) {
                        return Some(Self::from_palette_legacy(palette, config));
                    }
                }
            }
        }

        None
    }

    /// Get themes directory path
    fn themes_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".config").join("anthropic-spy").join("themes"))
    }

    /// Parse border type string to BorderType enum
    fn parse_border_type(value: Option<&String>) -> BorderType {
        match value.map(|s| s.as_str()) {
            Some("rounded") => BorderType::Rounded,
            Some("double") => BorderType::Double,
            Some("thick") => BorderType::Thick,
            _ => BorderType::Plain,
        }
    }

    /// Create theme from native TOML format
    fn from_toml(toml: TomlTheme, config: &ThemeConfig) -> Self {
        let parse = TomlTheme::parse_color;

        let background = if config.use_theme_background {
            parse(&toml.ui.background)
        } else {
            Color::Reset
        };

        Self {
            name: toml.meta.name.clone(),

            // Events
            tool_call: parse(&toml.events.tool_call),
            tool_result_ok: parse(&toml.events.tool_result_ok),
            tool_result_fail: parse(&toml.events.tool_result_fail),
            request: parse(&toml.events.request),
            response: parse(&toml.events.response),
            error: parse(&toml.events.error),
            thinking: parse(&toml.events.thinking),
            api_usage: parse(&toml.events.api_usage),
            headers: parse(&toml.events.headers),
            rate_limit: parse(&toml.events.rate_limit),
            context_compact: parse(&toml.events.context_compact),

            // Context bar
            context_bar_fill: parse(&toml.context_bar.fill),
            context_bar_warn: parse(&toml.context_bar.warn),
            context_bar_danger: parse(&toml.context_bar.danger),

            // UI chrome
            status_bar: parse(&toml.ui.status_bar),
            title: parse(&toml.ui.title),
            border: parse(&toml.ui.border),
            highlight: parse(&toml.ui.border_focused),

            // Panels
            panel_events: parse(&toml.panels.events),
            panel_thinking: parse(&toml.panels.thinking),
            panel_logs: parse(&toml.panels.logs),

            // Terminal
            background,
            foreground: parse(&toml.ui.foreground),
            selection: parse(&toml.ui.selection_bg),
            selection_fg: parse(&toml.ui.selection_fg),

            // Muted text (explicit or fallback to api_usage)
            muted: toml
                .ui
                .muted
                .as_ref()
                .map(|m| parse(m))
                .unwrap_or_else(|| parse(&toml.events.api_usage)),

            // Border style (explicit or fallback to plain)
            border_type: Self::parse_border_type(toml.ui.border_type.as_ref()),

            // Code highlighting (explicit or fallback to events)
            code_inline: toml
                .code
                .as_ref()
                .map(|c| parse(&c.inline))
                .unwrap_or_else(|| parse(&toml.events.tool_call)),
            code_block: toml
                .code
                .as_ref()
                .map(|c| parse(&c.block))
                .unwrap_or_else(|| parse(&toml.events.api_usage)),

            toml_source: Some(toml),
        }
    }

    /// Create theme from legacy palette (with auto-mapping)
    fn from_palette_legacy(palette: ColorPalette, config: &ThemeConfig) -> Self {
        let semantic = SemanticTheme::from_palette(&palette);

        let background = if config.use_theme_background {
            semantic.background
        } else {
            Color::Reset
        };

        Self {
            name: palette.name.clone(),

            tool_call: semantic.tool_call,
            tool_result_ok: semantic.tool_result_ok,
            tool_result_fail: semantic.tool_result_fail,
            request: semantic.request,
            response: semantic.response,
            error: semantic.error,
            thinking: semantic.thinking,
            api_usage: semantic.api_usage,
            headers: semantic.headers,
            rate_limit: semantic.rate_limit,
            context_compact: semantic.context_compact,

            context_bar_fill: semantic.context_bar_fill,
            context_bar_warn: semantic.context_bar_warn,
            context_bar_danger: semantic.context_bar_danger,
            status_bar: semantic.status_bar,
            title: semantic.title,
            border: semantic.border,
            highlight: semantic.highlight,

            panel_events: semantic.panel_events,
            panel_thinking: semantic.panel_thinking,
            panel_logs: semantic.panel_logs,

            background,
            foreground: semantic.foreground,
            selection: semantic.selection,
            selection_fg: semantic.selection_fg,

            // Legacy: fall back to semantic mappings
            muted: semantic.api_usage,
            border_type: BorderType::Plain,
            code_inline: semantic.tool_call,
            code_block: semantic.api_usage,

            toml_source: None,
        }
    }

    /// Hardcoded fallback when no themes can be loaded
    fn hardcoded_default(config: &ThemeConfig) -> Self {
        // One Half Dark colors
        let background = if config.use_theme_background {
            Color::Rgb(40, 44, 52)
        } else {
            Color::Reset
        };

        Self {
            name: "One Half Dark (Fallback)".to_string(),

            tool_call: Color::Rgb(86, 182, 194),
            tool_result_ok: Color::Rgb(152, 195, 121),
            tool_result_fail: Color::Rgb(224, 108, 117),
            request: Color::Rgb(97, 175, 239),
            response: Color::Rgb(198, 120, 221),
            error: Color::Rgb(224, 108, 117),
            thinking: Color::Rgb(198, 120, 221),
            api_usage: Color::Rgb(220, 223, 228),
            headers: Color::Rgb(220, 223, 228),
            rate_limit: Color::Rgb(220, 223, 228),
            context_compact: Color::Rgb(229, 192, 123),

            context_bar_fill: Color::Rgb(152, 195, 121),
            context_bar_warn: Color::Rgb(229, 192, 123),
            context_bar_danger: Color::Rgb(224, 108, 117),
            status_bar: Color::Rgb(220, 223, 228),
            title: Color::Rgb(86, 182, 194),
            border: Color::Rgb(220, 223, 228),
            highlight: Color::Rgb(229, 192, 123),

            panel_events: Color::Rgb(86, 182, 194),
            panel_thinking: Color::Rgb(198, 120, 221),
            panel_logs: Color::Rgb(152, 195, 121),

            background,
            foreground: Color::Rgb(220, 223, 228),
            selection: Color::Rgb(71, 78, 93),
            selection_fg: Color::Rgb(220, 223, 228),

            // Fallback: muted uses api_usage tone
            muted: Color::Rgb(220, 223, 228),
            border_type: BorderType::Plain,
            code_inline: Color::Rgb(86, 182, 194),
            code_block: Color::Rgb(220, 223, 228),

            toml_source: None,
        }
    }

    /// Export theme as VHS-compatible JSON (if available)
    #[allow(dead_code)] // CLI utility for VHS demo recordings
    pub fn to_vhs_json(&self) -> Option<String> {
        self.toml_source.as_ref().and_then(|t| t.to_vhs_json())
    }

    /// Get border color for a panel based on focus state
    pub fn panel_border(&self, panel: crate::tui::scroll::FocusablePanel, focused: bool) -> Color {
        if focused {
            match panel {
                crate::tui::scroll::FocusablePanel::Events => self.panel_events,
                crate::tui::scroll::FocusablePanel::Thinking => self.panel_thinking,
                crate::tui::scroll::FocusablePanel::Logs => self.panel_logs,
            }
        } else {
            self.foreground
        }
    }

    /// List all available themes (bundled + external)
    pub fn list_available() -> Vec<String> {
        let mut themes: Vec<String> = Vec::new();

        // Add bundled themes (always available)
        for name in bundled::list_bundled_themes() {
            themes.push(name.to_string());
        }

        // Add external themes from config dir
        if let Some(themes_dir) = Self::themes_dir() {
            if let Ok(entries) = std::fs::read_dir(themes_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        if ext == "toml" || ext == "json" {
                            if let Some(stem) = path.file_stem() {
                                let name = stem.to_string_lossy().to_string();
                                // Convert filename format (underscore) to display format (space)
                                let display_name = name.replace('_', " ");
                                if !themes.iter().any(|t| t.eq_ignore_ascii_case(&display_name)) {
                                    themes.push(display_name);
                                }
                            }
                        }
                    }
                }
            }
        }

        themes
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::by_name("Spy Dark")
    }
}

/// Ensure themes directory exists and extract bundled themes on first run
pub fn ensure_themes_extracted() {
    let Some(themes_dir) = Theme::themes_dir() else {
        return;
    };

    // Create directory if needed
    if std::fs::create_dir_all(&themes_dir).is_err() {
        return;
    }

    // Check if we've already extracted (marker file)
    let marker = themes_dir.join(".extracted_v2");
    if marker.exists() {
        return;
    }

    // Extract bundled TOML themes
    for theme in bundled::BUNDLED_THEMES {
        let path = themes_dir.join(theme.filename);
        // Only write if file doesn't exist (don't overwrite user modifications)
        if !path.exists() {
            let _ = std::fs::write(&path, theme.content);
        }
    }

    // Create marker file
    let _ = std::fs::write(&marker, "2");
}

/// Write VHS theme JSON file for demo recordings
#[allow(dead_code)] // CLI utility for VHS demo recordings
pub fn export_vhs_theme(theme: &Theme, path: &std::path::Path) -> std::io::Result<()> {
    if let Some(json) = theme.to_vhs_json() {
        std::fs::write(path, json)
    } else {
        Err(std::io::Error::other(
            "Theme does not have VHS export configuration",
        ))
    }
}
