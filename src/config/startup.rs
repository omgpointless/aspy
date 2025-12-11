//! Feature definitions for StartupRegistry
//!
//! This is the SINGLE SOURCE OF TRUTH for what features exist.
//! Adding a new feature? Add it here, and it shows up in startup automatically.

use super::Config;

impl Config {
    /// Get all feature definitions based on current configuration.
    ///
    /// This is the single source of truth for what features exist in Aspy.
    /// The StartupRegistry uses this to build the startup display.
    /// main.rs can update statuses based on actual initialization results.
    pub fn feature_definitions(&self) -> Vec<crate::startup::FeatureDefinition> {
        use crate::startup::{FeatureCategory, FeatureDefinition};

        let mut features = vec![
            // ─────────────────────────────────────────────────────────────────
            // Core (always enabled)
            // ─────────────────────────────────────────────────────────────────
            FeatureDefinition::core("proxy", "proxy", "HTTP interception"),
            FeatureDefinition::core("parser", "parser", "Event extraction"),
            // ─────────────────────────────────────────────────────────────────
            // Interface
            // ─────────────────────────────────────────────────────────────────
            FeatureDefinition::optional(
                "tui",
                "tui",
                FeatureCategory::Interface,
                self.enable_tui,
                "Terminal interface",
            ),
            // ─────────────────────────────────────────────────────────────────
            // Storage
            // ─────────────────────────────────────────────────────────────────
            FeatureDefinition::optional(
                "jsonl",
                "jsonl",
                FeatureCategory::Storage,
                self.features.json_logging,
                "JSONL logging",
            ),
            FeatureDefinition::optional(
                "cortex",
                "cortex",
                FeatureCategory::Storage,
                self.cortex.enabled,
                "Cortex memory (SQLite)",
            ),
            // ─────────────────────────────────────────────────────────────────
            // Pipeline
            // ─────────────────────────────────────────────────────────────────
            FeatureDefinition::optional(
                "thinking",
                "thinking",
                FeatureCategory::Pipeline,
                self.features.thinking_panel && self.enable_tui,
                "Thinking panel",
            ),
            FeatureDefinition::optional(
                "stats",
                "stats",
                FeatureCategory::Pipeline,
                self.features.stats,
                "Token tracking",
            ),
            FeatureDefinition::optional(
                "ctx-warn",
                "ctx-warn",
                FeatureCategory::Pipeline,
                self.augmentation.context_warning,
                "Context warnings",
            ),
        ];

        // Embeddings: configurable (needs setup, not just enable/disable)
        let embeddings_def = if self.embeddings.is_enabled() {
            FeatureDefinition::configurable(
                "embeddings",
                "embeddings",
                FeatureCategory::Pipeline,
                true,
                "Semantic search",
            )
            .with_detail(format!(
                "{}: {}",
                self.embeddings.provider, self.embeddings.model
            ))
        } else {
            FeatureDefinition::configurable(
                "embeddings",
                "embeddings",
                FeatureCategory::Pipeline,
                false,
                "Semantic search",
            )
        };
        features.push(embeddings_def);

        // Translation: optional (API format conversion)
        features.push(FeatureDefinition::optional(
            "translation",
            "translation",
            FeatureCategory::Pipeline,
            self.translation.enabled,
            "API translation (experimental)",
        ));

        // Transformation: optional (request modification before forwarding)
        // Shows as active when enabled=true AND has configured rules
        let tag_editor_active = self.transformers.enabled
            && self
                .transformers
                .tag_editor
                .as_ref()
                .map(|c| c.enabled)
                .unwrap_or(false);
        features.push(FeatureDefinition::optional(
            "tag-editor",
            "tag-editor",
            FeatureCategory::Pipeline,
            tag_editor_active,
            "Request tag editing",
        ));

        // System editor: optional (modifies system prompts)
        let system_editor_active = self.transformers.enabled
            && self
                .transformers
                .system_editor
                .as_ref()
                .map(|c| c.enabled && !c.rules.is_empty())
                .unwrap_or(false);
        features.push(
            FeatureDefinition::optional(
                "system-editor",
                "system",
                FeatureCategory::Pipeline,
                system_editor_active,
                "System prompt editing",
            )
            .highlight_when_missing("[transformers.system-editor]\nenabled = true"),
        );

        // Compact enhancer: optional (enhances compaction prompts)
        let compact_enhancer_active = self.transformers.enabled
            && self
                .transformers
                .compact_enhancer
                .as_ref()
                .map(|c| c.enabled)
                .unwrap_or(false);
        features.push(
            FeatureDefinition::optional(
                "compact-enhancer",
                "compact",
                FeatureCategory::Pipeline,
                compact_enhancer_active,
                "Continuity on /compact",
            )
            .highlight_when_missing("[transformers.compact-enhancer]\nenabled = true"),
        );

        // OpenTelemetry: configurable (requires connection string)
        let otel_def = if self.otel.is_configured() {
            FeatureDefinition::configurable(
                "otel",
                "otel",
                FeatureCategory::Pipeline,
                true,
                "Open telemetry",
            )
            .with_detail(format!("service: {}", self.otel.service_name))
        } else {
            FeatureDefinition::configurable(
                "otel",
                "otel",
                FeatureCategory::Pipeline,
                false,
                "Open telemetry",
            )
        };
        features.push(otel_def);

        // Routing: configurable (needs client definitions)
        features.push(FeatureDefinition::configurable(
            "routing",
            "routing",
            FeatureCategory::Routing,
            self.clients.is_configured(),
            "Multi-client",
        ));

        features
    }
}
