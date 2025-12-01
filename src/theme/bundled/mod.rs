//! Bundled TOML themes (compiled into binary, extracted on first run)
//!
//! These themes are written to ~/.config/aspy/themes/ on first run.
//! Users can then modify them freely.
//!
//! Each theme lives in its own module file for easy editing without loading
//! all 32 themes into context. See spy_dark.rs for the flagship theme.

// ─────────────────────────────────────────────────────────────────────────────
// Theme modules (one per theme for easy editing)
// ─────────────────────────────────────────────────────────────────────────────

mod ayu_mirage;
mod catppuccin_latte;
mod catppuccin_mocha;
mod dracula;
mod everforest_dark;
mod github_dark;
mod gruvbox_dark;
mod jetbrains_darcula;
mod kanagawa_wave;
mod material_darker;
mod material_deep_ocean;
mod material_forest;
mod material_lighter;
mod material_oceanic;
mod material_palenight;
mod material_sandy_beach;
mod material_sky_blue;
mod material_space;
mod material_volcano;
mod monokai_pro;
mod monokai_pro_machine;
mod monokai_pro_ristretto;
mod monokai_soda;
mod nord;
mod one_half_dark;
mod rose_pine;
mod solarized_light;
mod spy_dark;
mod spy_light;
mod synthwave_84;
mod terminal_ansi;
mod tokyo_night;

// ─────────────────────────────────────────────────────────────────────────────
// Re-export theme constants (for backwards compatibility if needed)
// ─────────────────────────────────────────────────────────────────────────────

pub use ayu_mirage::THEME as AYU_MIRAGE;
pub use catppuccin_latte::THEME as CATPPUCCIN_LATTE;
pub use catppuccin_mocha::THEME as CATPPUCCIN_MOCHA;
pub use dracula::THEME as DRACULA;
pub use everforest_dark::THEME as EVERFOREST_DARK;
pub use github_dark::THEME as GITHUB_DARK;
pub use gruvbox_dark::THEME as GRUVBOX_DARK;
pub use jetbrains_darcula::THEME as JETBRAINS_DARCULA;
pub use kanagawa_wave::THEME as KANAGAWA_WAVE;
pub use material_darker::THEME as MATERIAL_DARKER;
pub use material_deep_ocean::THEME as MATERIAL_DEEP_OCEAN;
pub use material_forest::THEME as MATERIAL_FOREST;
pub use material_lighter::THEME as MATERIAL_LIGHTER;
pub use material_oceanic::THEME as MATERIAL_OCEANIC;
pub use material_palenight::THEME as MATERIAL_PALENIGHT;
pub use material_sandy_beach::THEME as MATERIAL_SANDY_BEACH;
pub use material_sky_blue::THEME as MATERIAL_SKY_BLUE;
pub use material_space::THEME as MATERIAL_SPACE;
pub use material_volcano::THEME as MATERIAL_VOLCANO;
pub use monokai_pro::THEME as MONOKAI_PRO;
pub use monokai_pro_machine::THEME as MONOKAI_PRO_MACHINE;
pub use monokai_pro_ristretto::THEME as MONOKAI_PRO_RISTRETTO;
pub use monokai_soda::THEME as MONOKAI_SODA;
pub use nord::THEME as NORD;
pub use one_half_dark::THEME as ONE_HALF_DARK;
pub use rose_pine::THEME as ROSE_PINE;
pub use solarized_light::THEME as SOLARIZED_LIGHT;
pub use spy_dark::THEME as SPY_DARK;
pub use spy_light::THEME as SPY_LIGHT;
pub use synthwave_84::THEME as SYNTHWAVE_84;
pub use terminal_ansi::THEME as TERMINAL_ANSI;
pub use tokyo_night::THEME as TOKYO_NIGHT;

// ─────────────────────────────────────────────────────────────────────────────
// Bundled theme collection
// ─────────────────────────────────────────────────────────────────────────────

/// Bundled theme: name and TOML content
pub struct BundledTheme {
    pub filename: &'static str,
    pub content: &'static str,
}

/// All bundled themes
pub const BUNDLED_THEMES: &[BundledTheme] = &[
    BundledTheme {
        filename: "Spy_Dark.toml",
        content: SPY_DARK,
    },
    BundledTheme {
        filename: "Spy_Light.toml",
        content: SPY_LIGHT,
    },
    BundledTheme {
        filename: "One_Half_Dark.toml",
        content: ONE_HALF_DARK,
    },
    BundledTheme {
        filename: "Dracula.toml",
        content: DRACULA,
    },
    BundledTheme {
        filename: "Catppuccin_Mocha.toml",
        content: CATPPUCCIN_MOCHA,
    },
    BundledTheme {
        filename: "Monokai_Pro.toml",
        content: MONOKAI_PRO,
    },
    BundledTheme {
        filename: "Solarized_Light.toml",
        content: SOLARIZED_LIGHT,
    },
    BundledTheme {
        filename: "Nord.toml",
        content: NORD,
    },
    BundledTheme {
        filename: "Gruvbox_Dark.toml",
        content: GRUVBOX_DARK,
    },
    BundledTheme {
        filename: "Monokai_Pro_Ristretto.toml",
        content: MONOKAI_PRO_RISTRETTO,
    },
    BundledTheme {
        filename: "Monokai_Pro_Machine.toml",
        content: MONOKAI_PRO_MACHINE,
    },
    BundledTheme {
        filename: "Monokai_Soda.toml",
        content: MONOKAI_SODA,
    },
    BundledTheme {
        filename: "Tokyo_Night.toml",
        content: TOKYO_NIGHT,
    },
    BundledTheme {
        filename: "GitHub_Dark.toml",
        content: GITHUB_DARK,
    },
    BundledTheme {
        filename: "JetBrains_Darcula.toml",
        content: JETBRAINS_DARCULA,
    },
    BundledTheme {
        filename: "Material_Oceanic.toml",
        content: MATERIAL_OCEANIC,
    },
    BundledTheme {
        filename: "Material_Darker.toml",
        content: MATERIAL_DARKER,
    },
    BundledTheme {
        filename: "Material_Lighter.toml",
        content: MATERIAL_LIGHTER,
    },
    BundledTheme {
        filename: "Material_Palenight.toml",
        content: MATERIAL_PALENIGHT,
    },
    BundledTheme {
        filename: "Material_Deep_Ocean.toml",
        content: MATERIAL_DEEP_OCEAN,
    },
    BundledTheme {
        filename: "Material_Forest.toml",
        content: MATERIAL_FOREST,
    },
    BundledTheme {
        filename: "Material_Sky_Blue.toml",
        content: MATERIAL_SKY_BLUE,
    },
    BundledTheme {
        filename: "Material_Sandy_Beach.toml",
        content: MATERIAL_SANDY_BEACH,
    },
    BundledTheme {
        filename: "Material_Volcano.toml",
        content: MATERIAL_VOLCANO,
    },
    BundledTheme {
        filename: "Material_Space.toml",
        content: MATERIAL_SPACE,
    },
    BundledTheme {
        filename: "Synthwave_84.toml",
        content: SYNTHWAVE_84,
    },
    BundledTheme {
        filename: "Terminal_ANSI.toml",
        content: TERMINAL_ANSI,
    },
    BundledTheme {
        filename: "Rose_Pine.toml",
        content: ROSE_PINE,
    },
    BundledTheme {
        filename: "Everforest_Dark.toml",
        content: EVERFOREST_DARK,
    },
    BundledTheme {
        filename: "Ayu_Mirage.toml",
        content: AYU_MIRAGE,
    },
    BundledTheme {
        filename: "Catppuccin_Latte.toml",
        content: CATPPUCCIN_LATTE,
    },
    BundledTheme {
        filename: "Kanagawa_Wave.toml",
        content: KANAGAWA_WAVE,
    },
];

/// List bundled theme names (for display)
pub fn list_bundled_themes() -> Vec<&'static str> {
    vec![
        "Spy Dark",
        "Spy Light",
        "One Half Dark",
        "Dracula",
        "Catppuccin Mocha",
        "Monokai Pro",
        "Solarized Light",
        "Nord",
        "Gruvbox Dark",
        "Monokai Pro Ristretto",
        "Monokai Pro Machine",
        "Monokai Soda",
        "Tokyo Night",
        "GitHub Dark",
        "JetBrains Darcula",
        "Material Oceanic",
        "Material Darker",
        "Material Lighter",
        "Material Palenight",
        "Material Deep Ocean",
        "Material Forest",
        "Material Sky Blue",
        "Material Sandy Beach",
        "Material Volcano",
        "Material Space",
        "Synthwave 84",
        "Terminal ANSI",
        "Rosé Pine",
        "Everforest Dark",
        "Ayu Mirage",
        "Catppuccin Latte",
        "Kanagawa Wave",
    ]
}
