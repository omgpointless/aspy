// Startup module - displays banner and module loading status
//
// This module provides a professional startup experience showing:
// - Version info and branding
// - Configuration loaded from file
// - Module loading status with checkmarks
//
// The StartupRegistry system provides a single source of truth for features:
// - Config::feature_definitions() defines WHAT features exist
// - main.rs updates registry with actual init results
// - This module renders the final status

use crate::config::{Config, VERSION};
use crate::util::truncate_utf8_safe;
use std::collections::HashMap;

/// ANSI color codes for terminal output
mod colors {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";
    pub const CYAN: &str = "\x1b[36m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const RED: &str = "\x1b[31m";
    pub const MAGENTA: &str = "\x1b[35m";
}

// ═══════════════════════════════════════════════════════════════════════════
// Feature Registry System
// ═══════════════════════════════════════════════════════════════════════════

/// Category for grouping features in startup display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FeatureCategory {
    /// Core features - always on (proxy, parser)
    Core,
    /// Interface features (tui)
    Interface,
    /// Storage features (jsonl, cortex)
    Storage,
    /// Pipeline features (embeddings, augmentations)
    Pipeline,
    /// Routing features (multi-client)
    Routing,
}

impl FeatureCategory {
    /// Display order for categories
    fn order(&self) -> u8 {
        match self {
            Self::Core => 0,
            Self::Interface => 1,
            Self::Storage => 2,
            Self::Pipeline => 3,
            Self::Routing => 4,
        }
    }

    /// Human-readable name
    fn name(&self) -> &'static str {
        match self {
            Self::Core => "Core",
            Self::Interface => "Interface",
            Self::Storage => "Storage",
            Self::Pipeline => "Pipeline",
            Self::Routing => "Routing",
        }
    }
}

/// Status of a feature after initialization
#[derive(Debug, Clone)]
pub enum FeatureStatus {
    /// Feature is running successfully
    Active,
    /// Feature is disabled by user in config
    Disabled,
    /// Optional feature not configured (no config section)
    NotConfigured,
    /// Feature attempted to init but failed
    Failed(String),
}

/// Definition of a feature for the registry
#[derive(Debug, Clone)]
pub struct FeatureDefinition {
    /// Unique identifier (used for updates)
    pub id: &'static str,
    /// Display name
    pub name: &'static str,
    /// Category for grouping
    pub category: FeatureCategory,
    /// Current status
    pub status: FeatureStatus,
    /// Brief description
    pub description: &'static str,
    /// Optional detail (e.g., "remote: text-embedding-3-small")
    pub detail: Option<String>,
    /// If true, warn user when this feature is not configured (for new/recommended features)
    pub highlight_if_missing: bool,
    /// Config snippet to enable this feature (shown in warnings)
    pub config_hint: Option<&'static str>,
}

impl FeatureDefinition {
    /// Create a core feature (always active)
    pub fn core(id: &'static str, name: &'static str, description: &'static str) -> Self {
        Self {
            id,
            name,
            category: FeatureCategory::Core,
            status: FeatureStatus::Active,
            description,
            detail: None,
            highlight_if_missing: false,
            config_hint: None,
        }
    }

    /// Create an optional feature based on enabled flag
    pub fn optional(
        id: &'static str,
        name: &'static str,
        category: FeatureCategory,
        enabled: bool,
        description: &'static str,
    ) -> Self {
        Self {
            id,
            name,
            category,
            status: if enabled {
                FeatureStatus::Active
            } else {
                FeatureStatus::Disabled
            },
            description,
            detail: None,
            highlight_if_missing: false,
            config_hint: None,
        }
    }

    /// Create a feature that requires configuration
    pub fn configurable(
        id: &'static str,
        name: &'static str,
        category: FeatureCategory,
        configured: bool,
        description: &'static str,
    ) -> Self {
        Self {
            id,
            name,
            category,
            status: if configured {
                FeatureStatus::Active
            } else {
                FeatureStatus::NotConfigured
            },
            description,
            detail: None,
            highlight_if_missing: false,
            config_hint: None,
        }
    }

    /// Add detail string (builder pattern)
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Mark this feature to highlight in warnings if not configured (builder pattern)
    pub fn highlight_when_missing(mut self, hint: &'static str) -> Self {
        self.highlight_if_missing = true;
        self.config_hint = Some(hint);
        self
    }
}

/// Registry tracking all features and their status
#[derive(Debug)]
pub struct StartupRegistry {
    features: HashMap<&'static str, FeatureDefinition>,
}

impl StartupRegistry {
    /// Create registry from config's feature definitions
    pub fn from_config(config: &Config) -> Self {
        let definitions = config.feature_definitions();
        let mut features = HashMap::new();
        for def in definitions {
            features.insert(def.id, def);
        }
        Self { features }
    }

    /// Mark a feature as successfully activated
    pub fn activate(&mut self, id: &'static str) {
        if let Some(feature) = self.features.get_mut(id) {
            feature.status = FeatureStatus::Active;
        }
    }

    /// Mark a feature as failed with error message
    pub fn fail(&mut self, id: &'static str, error: impl Into<String>) {
        if let Some(feature) = self.features.get_mut(id) {
            feature.status = FeatureStatus::Failed(error.into());
        }
    }

    /// Get features grouped by category, sorted
    pub fn by_category(&self) -> Vec<(FeatureCategory, Vec<&FeatureDefinition>)> {
        let mut by_cat: HashMap<FeatureCategory, Vec<&FeatureDefinition>> = HashMap::new();

        for feature in self.features.values() {
            by_cat.entry(feature.category).or_default().push(feature);
        }

        // Sort by category order
        let mut result: Vec<_> = by_cat.into_iter().collect();
        result.sort_by_key(|(cat, _)| cat.order());

        // Sort features within each category by name
        for (_, features) in &mut result {
            features.sort_by_key(|f| f.name);
        }

        result
    }

    /// Get features that are not configured but marked as highlight_if_missing
    pub fn missing_recommended(&self) -> Vec<&FeatureDefinition> {
        self.features
            .values()
            .filter(|f| {
                f.highlight_if_missing
                    && matches!(
                        f.status,
                        FeatureStatus::Disabled | FeatureStatus::NotConfigured
                    )
            })
            .collect()
    }
}

/// Print the startup banner and module loading status using the registry
/// This runs before the TUI takes over the screen (or in headless mode)
pub fn print_startup_with_registry(config: &Config, registry: &StartupRegistry) {
    use colors::*;

    // Banner
    println!();
    println!("  {BOLD}{CYAN}Aspy{RESET} {DIM}v{VERSION}{RESET}");
    println!("  {DIM}Observability proxy for Claude Code{RESET}");
    println!();

    // Config file status
    if let Some(path) = Config::config_path() {
        if path.exists() {
            println!("  {DIM}Config:{RESET} {GREEN}✓{RESET} {}", path.display());
        } else {
            println!("  {DIM}Config:{RESET} {DIM}(using defaults){RESET}");
        }
    }
    println!();

    // Module loading by category
    println!("  {DIM}Loading modules...{RESET}");

    for (category, features) in registry.by_category() {
        // Skip empty categories
        if features.is_empty() {
            continue;
        }

        // Category header (subtle)
        println!("  {DIM}─ {} ─{RESET}", category.name());

        for feature in features {
            print_feature_status(feature);
        }
    }

    // Check for recommended features that aren't configured
    let missing = registry.missing_recommended();
    if !missing.is_empty() {
        println!();
        println!(
            "  {YELLOW}⚠{RESET} {BOLD}New features available{RESET} {DIM}(not in your config){RESET}"
        );
        for feature in &missing {
            println!(
                "    {YELLOW}•{RESET} {BOLD}{}{RESET} - {}",
                feature.name, feature.description
            );
            if let Some(hint) = feature.config_hint {
                println!("      {DIM}Add: {hint}{RESET}");
            }
        }
    }

    println!();

    // Proxy info
    println!(
        "  {MAGENTA}▸{RESET} Proxy listening on {BOLD}{}{RESET}",
        config.bind_addr
    );
    if config.demo_mode {
        println!("  {YELLOW}▸{RESET} {YELLOW}Demo mode active{RESET} {DIM}(mock events){RESET}");
    }
    println!();
}

/// Print a single feature's status with visual distinction
fn print_feature_status(feature: &FeatureDefinition) {
    use colors::*;

    let (icon, name_style, desc) = match &feature.status {
        FeatureStatus::Active => {
            let detail = feature
                .detail
                .as_ref()
                .map(|d| format!(" ({d})"))
                .unwrap_or_default();
            (
                format!("{GREEN}✓{RESET}"),
                "",
                format!("{}{}", feature.description, detail),
            )
        }
        FeatureStatus::Disabled => (
            format!("{DIM}○{RESET}"),
            DIM,
            format!("{} {DIM}(disabled){RESET}", feature.description),
        ),
        FeatureStatus::NotConfigured => (
            format!("{DIM}⊘{RESET}"),
            DIM,
            format!("{} {DIM}(not configured){RESET}", feature.description),
        ),
        FeatureStatus::Failed(err) => {
            // Truncate long errors (safely respecting UTF-8 boundaries)
            let short_err = if err.len() > 30 {
                format!("{}...", truncate_utf8_safe(err, 30))
            } else {
                err.clone()
            };
            (
                format!("{RED}✗{RESET}"),
                RED,
                format!("{} {DIM}({}){RESET}", feature.description, short_err),
            )
        }
    };

    println!(
        "    {icon} {name_style}{:<12}{RESET} {DIM}{desc}{RESET}",
        feature.name
    );
}

/// Print startup messages to TUI log panel using registry
pub fn log_startup_with_registry(config: &Config, registry: &StartupRegistry) {
    // ASCII art header
    tracing::info!("═══════════════════════════════════");
    tracing::info!("  🔍 ASPY v{}", VERSION);
    tracing::info!("═══════════════════════════════════");

    // Module loading by category
    for (category, features) in registry.by_category() {
        if features.is_empty() {
            continue;
        }

        tracing::info!("─ {} ─", category.name());

        for feature in features {
            let icon = match &feature.status {
                FeatureStatus::Active => "✓",
                FeatureStatus::Disabled => "○",
                FeatureStatus::NotConfigured => "⊘",
                FeatureStatus::Failed(_) => "✗",
            };

            let detail = feature
                .detail
                .as_ref()
                .map(|d| format!(" ({d})"))
                .unwrap_or_default();

            let status_note = match &feature.status {
                FeatureStatus::Active => String::new(),
                FeatureStatus::Disabled => " [disabled]".to_string(),
                FeatureStatus::NotConfigured => " [not configured]".to_string(),
                FeatureStatus::Failed(e) => format!(" [FAILED: {}]", e),
            };

            tracing::info!(
                "  {} {} - {}{}{}",
                icon,
                feature.name,
                feature.description,
                detail,
                status_note
            );
        }
    }

    // Warn about recommended features that aren't configured
    let missing = registry.missing_recommended();
    if !missing.is_empty() {
        tracing::warn!("⚠ New features available (not in config):");
        for feature in &missing {
            tracing::warn!("  • {} - {}", feature.name, feature.description);
            if let Some(hint) = feature.config_hint {
                // Show hint on single line for log readability
                let hint_oneline = hint.replace('\n', " ");
                tracing::warn!("    Add: {}", hint_oneline);
            }
        }
    }

    // Proxy ready message
    tracing::info!("▸ Listening on {}", config.bind_addr);

    if config.demo_mode {
        tracing::info!("▸ Demo mode active (mock events)");
    }

    tracing::info!("Ready. Waiting for Claude Code...");
}

// Old log_startup removed - use log_startup_with_registry instead
