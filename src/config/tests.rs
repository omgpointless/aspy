//! Configuration tests
//!
//! These tests serve as compile-time guards to ensure all config fields are
//! properly serialized and have feature definitions. When you add a new field,
//! these tests will fail until you update all the necessary places.

use super::*;
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// Round-trip tests
// ─────────────────────────────────────────────────────────────────────────────

/// Verify that serialized config can be parsed back.
/// This catches TOML syntax errors like using `[array.property]`
/// instead of dotted keys for array-of-tables elements.
#[test]
fn test_config_roundtrip_default() {
    let config = Config::default();
    let toml_str = config.to_toml();

    // Should parse without error
    let parsed: Result<FileConfig, _> = toml::from_str(&toml_str);
    assert!(
        parsed.is_ok(),
        "Default config should round-trip.\nTOML:\n{}\nError: {:?}",
        toml_str,
        parsed.err()
    );
}

/// Test round-trip with transformers containing rules and conditions.
/// This specifically tests the nested struct serialization that was buggy.
#[test]
fn test_config_roundtrip_with_transformers() {
    use crate::proxy::transformation::{RuleConfig, TagEditorConfig, WhenCondition};

    let mut config = Config::default();
    config.transformers.enabled = true;
    config.transformers.tag_editor = Some(TagEditorConfig {
        enabled: true,
        rules: vec![
            RuleConfig::Remove {
                tag: "system-reminder".to_string(),
                pattern: "test-pattern".to_string(),
                when: Some(WhenCondition {
                    turn_number: Some(">2".to_string()),
                    has_tool_results: None,
                    client_id: None,
                }),
            },
            RuleConfig::Inject {
                tag: "aspy-context".to_string(),
                content: "Injected content".to_string(),
                position: crate::proxy::transformation::PositionConfig::End,
                when: Some(WhenCondition {
                    turn_number: Some("every:3".to_string()),
                    has_tool_results: Some("=0".to_string()),
                    client_id: Some("dev-1|foundry".to_string()),
                }),
            },
        ],
    });

    let toml_str = config.to_toml();

    // Should parse without error
    let parsed: Result<FileConfig, _> = toml::from_str(&toml_str);
    assert!(
        parsed.is_ok(),
        "Config with transformers should round-trip.\nTOML:\n{}\nError: {:?}",
        toml_str,
        parsed.err()
    );

    // Verify the parsed config has the rules
    let file_config = parsed.unwrap();
    let tag_editor = file_config
        .transformers
        .and_then(|t| t.tag_editor)
        .expect("tag_editor should be present");
    assert_eq!(tag_editor.rules.len(), 2, "Should have 2 rules");
}

// ─────────────────────────────────────────────────────────────────────────────
// EXHAUSTIVE TESTS: Compile-time guards for config completeness
// ─────────────────────────────────────────────────────────────────────────────

/// EXHAUSTIVE TEST: Ensures every transformer field is serialized to TOML.
///
/// When you add a new transformer:
/// 1. Add the field to `Transformers` struct
/// 2. Add the field to `FileTransformers` struct
/// 3. THIS TEST WILL FAIL until you:
///    a. Initialize it below with a minimal config
///    b. Add serialization in `transformers_to_toml()`
///    c. Add the assertion for the TOML key
///
/// This prevents the "forgot to serialize" bug that caused compact-enhancer
/// to be missing from user configs.
#[test]
fn test_all_transformers_have_toml_serialization() {
    use crate::proxy::transformation::system_editor::RuleConfig as SystemRuleConfig;
    use crate::proxy::transformation::{
        CompactEnhancerConfig, PositionConfig, RuleConfig, SystemEditorConfig, TagEditorConfig,
    };

    // ─────────────────────────────────────────────────────────────────────
    // STEP 1: Create config with ALL transformer fields populated.
    // When you add a new transformer, ADD IT HERE or the test won't compile.
    // ─────────────────────────────────────────────────────────────────────
    let mut config = Config::default();
    config.transformers.enabled = true;

    // Tag editor with minimal valid config
    config.transformers.tag_editor = Some(TagEditorConfig {
        enabled: true,
        rules: vec![RuleConfig::Inject {
            tag: "test".to_string(),
            content: "test".to_string(),
            position: PositionConfig::End,
            when: None,
        }],
    });

    // System editor with minimal valid config
    config.transformers.system_editor = Some(SystemEditorConfig {
        enabled: true,
        rules: vec![SystemRuleConfig::Append {
            content: "test".to_string(),
        }],
    });

    // Compact enhancer with minimal valid config
    config.transformers.compact_enhancer = Some(CompactEnhancerConfig { enabled: true });

    // ─────────────────────────────────────────────────────────────────────
    // STEP 2: Generate TOML output
    // ─────────────────────────────────────────────────────────────────────
    let toml_str = config.to_toml();

    // ─────────────────────────────────────────────────────────────────────
    // STEP 3: Assert EVERY transformer appears in output.
    // When you add a new transformer, ADD AN ASSERTION HERE.
    // ─────────────────────────────────────────────────────────────────────

    assert!(
        toml_str.contains("[transformers.tag-editor]"),
        "tag-editor missing from TOML output!\n\
         Did you forget to serialize it in transformers_to_toml()?\n\
         TOML output:\n{}",
        toml_str
    );

    assert!(
        toml_str.contains("[transformers.system-editor]"),
        "system-editor missing from TOML output!\n\
         Did you forget to serialize it in transformers_to_toml()?\n\
         TOML output:\n{}",
        toml_str
    );

    assert!(
        toml_str.contains("[transformers.compact-enhancer]"),
        "compact-enhancer missing from TOML output!\n\
         Did you forget to serialize it in transformers_to_toml()?\n\
         TOML output:\n{}",
        toml_str
    );

    // ─────────────────────────────────────────────────────────────────────
    // STEP 4: Verify round-trip works (catches TOML syntax errors)
    // ─────────────────────────────────────────────────────────────────────
    let parsed: Result<FileConfig, _> = toml::from_str(&toml_str);
    assert!(
        parsed.is_ok(),
        "Config with all transformers should round-trip.\nError: {:?}",
        parsed.err()
    );

    // ─────────────────────────────────────────────────────────────────────
    // STEP 5: Verify VALUES survived round-trip (catches mangled serialization)
    // ─────────────────────────────────────────────────────────────────────
    let file_config = parsed.unwrap();
    let transformers = file_config
        .transformers
        .expect("transformers section should be present");

    // Verify tag-editor
    let tag_editor = transformers
        .tag_editor
        .expect("tag_editor should be present");
    assert!(tag_editor.enabled, "tag_editor.enabled should be true");
    assert_eq!(tag_editor.rules.len(), 1, "tag_editor should have 1 rule");

    // Verify system-editor
    let system_editor = transformers
        .system_editor
        .expect("system_editor should be present");
    assert!(
        system_editor.enabled,
        "system_editor.enabled should be true"
    );
    assert_eq!(
        system_editor.rules.len(),
        1,
        "system_editor should have 1 rule"
    );

    // Verify compact-enhancer
    let compact = transformers
        .compact_enhancer
        .expect("compact_enhancer should be present");
    assert!(compact.enabled, "compact_enhancer.enabled should be true");
}

/// Ensures the DEFAULT template includes commented examples for all transformers.
/// This catches the discoverability problem: feature works but users don't know it exists.
#[test]
fn test_default_template_documents_all_transformers() {
    let config = Config::default();
    let toml_str = config.to_toml();

    // Default config has no transformers enabled, but should have COMMENTED examples
    // for users to discover them.

    assert!(
        toml_str.contains("transformers.tag-editor")
            || toml_str.contains("# [transformers.tag-editor]"),
        "tag-editor not documented in default template!\n\
         Add a commented example so users can discover this feature."
    );

    assert!(
        toml_str.contains("transformers.system-editor")
            || toml_str.contains("# [transformers.system-editor]"),
        "system-editor not documented in default template!\n\
         Add a commented example so users can discover this feature."
    );

    assert!(
        toml_str.contains("transformers.compact-enhancer")
            || toml_str.contains("# [transformers.compact-enhancer]"),
        "compact-enhancer not documented in default template!\n\
         Add a commented example so users can discover this feature."
    );
}

/// EXHAUSTIVE TEST: Ensures every transformer has a feature_definitions entry.
///
/// When you add a new transformer:
/// 1. Add the field to `Transformers` struct
/// 2. THIS TEST WILL FAIL until you add it to `feature_definitions()`
///
/// This prevents the "forgot to add to startup display" bug.
#[test]
fn test_all_transformers_have_feature_definitions() {
    use crate::proxy::transformation::system_editor::RuleConfig as SystemRuleConfig;
    use crate::proxy::transformation::{
        CompactEnhancerConfig, PositionConfig, RuleConfig, SystemEditorConfig, TagEditorConfig,
    };

    // ─────────────────────────────────────────────────────────────────────
    // STEP 1: Create config with ALL transformer fields populated and enabled.
    // When you add a new transformer, ADD IT HERE or the test won't compile.
    // ─────────────────────────────────────────────────────────────────────
    let mut config = Config::default();
    config.transformers.enabled = true;

    config.transformers.tag_editor = Some(TagEditorConfig {
        enabled: true,
        rules: vec![RuleConfig::Inject {
            tag: "test".to_string(),
            content: "test".to_string(),
            position: PositionConfig::End,
            when: None,
        }],
    });

    config.transformers.system_editor = Some(SystemEditorConfig {
        enabled: true,
        rules: vec![SystemRuleConfig::Append {
            content: "test".to_string(),
        }],
    });

    config.transformers.compact_enhancer = Some(CompactEnhancerConfig { enabled: true });

    // ─────────────────────────────────────────────────────────────────────
    // STEP 2: Get feature definitions
    // ─────────────────────────────────────────────────────────────────────
    let features = config.feature_definitions();
    let feature_ids: Vec<&str> = features.iter().map(|f| f.id).collect();

    // ─────────────────────────────────────────────────────────────────────
    // STEP 3: Assert EVERY transformer appears in feature_definitions.
    // When you add a new transformer, ADD AN ASSERTION HERE.
    // ─────────────────────────────────────────────────────────────────────

    assert!(
        feature_ids.contains(&"tag-editor"),
        "tag-editor missing from feature_definitions()!\n\
         Add it to Config::feature_definitions() so it shows in startup logs.\n\
         Features found: {:?}",
        feature_ids
    );

    assert!(
        feature_ids.contains(&"system-editor"),
        "system-editor missing from feature_definitions()!\n\
         Add it to Config::feature_definitions() so it shows in startup logs.\n\
         Features found: {:?}",
        feature_ids
    );

    assert!(
        feature_ids.contains(&"compact-enhancer"),
        "compact-enhancer missing from feature_definitions()!\n\
         Add it to Config::feature_definitions() so it shows in startup logs.\n\
         Features found: {:?}",
        feature_ids
    );

    // ─────────────────────────────────────────────────────────────────────
    // STEP 4: Verify they show as ACTIVE when enabled
    // ─────────────────────────────────────────────────────────────────────
    use crate::startup::FeatureStatus;
    for id in ["tag-editor", "system-editor", "compact-enhancer"] {
        let feature = features.iter().find(|f| f.id == id).unwrap();
        assert!(
            matches!(feature.status, FeatureStatus::Active),
            "{} should be active when enabled in config, but was {:?}",
            id,
            feature.status
        );
    }
}

/// EXHAUSTIVE TEST: Ensures every augmentation field is serialized to TOML.
///
/// When you add a new augmenter:
/// 1. Add the field to `Augmentation` struct
/// 2. Add the field to `FileAugmentation` struct
/// 3. Add merge logic in `Config::from_env()`
/// 4. THIS TEST WILL FAIL until you:
///    a. Set the field below
///    b. Add serialization in `to_toml()`
///    c. Add the assertion for the TOML key
///
/// Currently flat bools, but designed to catch growth to Option<SubConfig> pattern.
#[test]
fn test_all_augmenters_have_toml_serialization() {
    // ─────────────────────────────────────────────────────────────────────
    // STEP 1: Create config with ALL augmentation fields set to non-default.
    // When you add a new augmenter, ADD IT HERE.
    // ─────────────────────────────────────────────────────────────────────
    let mut config = Config::default();

    // Context warning (currently the only augmenter)
    config.augmentation.context_warning = true;
    config.augmentation.context_warning_thresholds = vec![50, 75, 90];

    // ─────────────────────────────────────────────────────────────────────
    // STEP 2: Generate TOML output
    // ─────────────────────────────────────────────────────────────────────
    let toml_str = config.to_toml();

    // ─────────────────────────────────────────────────────────────────────
    // STEP 3: Assert EVERY augmenter field appears in output.
    // When you add a new augmenter, ADD AN ASSERTION HERE.
    // ─────────────────────────────────────────────────────────────────────

    assert!(
        toml_str.contains("[augmentation]"),
        "augmentation section missing from TOML output!"
    );

    assert!(
        toml_str.contains("context_warning = true"),
        "context_warning missing from TOML output!\n\
         Did you forget to serialize it in to_toml()?"
    );

    assert!(
        toml_str.contains("context_warning_thresholds"),
        "context_warning_thresholds missing from TOML output!\n\
         Did you forget to serialize it in to_toml()?"
    );

    // ─────────────────────────────────────────────────────────────────────
    // STEP 4: Verify round-trip works
    // ─────────────────────────────────────────────────────────────────────
    let parsed: Result<FileConfig, _> = toml::from_str(&toml_str);
    assert!(
        parsed.is_ok(),
        "Config with all augmenters should round-trip.\nError: {:?}",
        parsed.err()
    );

    // Verify values survived round-trip
    let file_config = parsed.unwrap();
    let aug = file_config
        .augmentation
        .expect("augmentation should be present");
    assert_eq!(aug.context_warning, Some(true));
    assert_eq!(aug.context_warning_thresholds, Some(vec![50, 75, 90]));
}

/// EXHAUSTIVE TEST: Ensures every feature flag is serialized to TOML.
///
/// When you add a new feature flag:
/// 1. Add the field to `Features` struct
/// 2. Add the field to `FileFeatures` struct
/// 3. Add merge logic in `Config::from_env()`
/// 4. THIS TEST WILL FAIL until you:
///    a. Set the field below
///    b. Add serialization in `to_toml()`
///    c. Add the assertion for the TOML key
#[test]
fn test_all_features_have_toml_serialization() {
    // ─────────────────────────────────────────────────────────────────────
    // STEP 1: Create config with ALL feature fields set to non-default.
    // When you add a new feature, ADD IT HERE.
    // ─────────────────────────────────────────────────────────────────────
    let mut config = Config::default();

    // All current feature flags (defaults are all true, so flip to false)
    config.features.json_logging = false;
    config.features.thinking_panel = false;
    config.features.stats = false;

    // ─────────────────────────────────────────────────────────────────────
    // STEP 2: Generate TOML output
    // ─────────────────────────────────────────────────────────────────────
    let toml_str = config.to_toml();

    // ─────────────────────────────────────────────────────────────────────
    // STEP 3: Assert EVERY feature field appears in output.
    // When you add a new feature, ADD AN ASSERTION HERE.
    // ─────────────────────────────────────────────────────────────────────

    assert!(
        toml_str.contains("[features]"),
        "features section missing from TOML output!"
    );

    assert!(
        toml_str.contains("storage = false"),
        "storage missing from TOML output!\n\
         Did you forget to serialize it in to_toml()?"
    );

    assert!(
        toml_str.contains("thinking_panel = false"),
        "thinking_panel missing from TOML output!\n\
         Did you forget to serialize it in to_toml()?"
    );

    assert!(
        toml_str.contains("stats = false"),
        "stats missing from TOML output!\n\
         Did you forget to serialize it in to_toml()?"
    );

    // ─────────────────────────────────────────────────────────────────────
    // STEP 4: Verify round-trip works
    // ─────────────────────────────────────────────────────────────────────
    let parsed: Result<FileConfig, _> = toml::from_str(&toml_str);
    assert!(
        parsed.is_ok(),
        "Config with all features should round-trip.\nError: {:?}",
        parsed.err()
    );

    // Verify values survived round-trip
    let file_config = parsed.unwrap();
    let features = file_config.features.expect("features should be present");
    assert_eq!(features.storage, Some(false));
    assert_eq!(features.thinking_panel, Some(false));
    assert_eq!(features.stats, Some(false));
}

// ─────────────────────────────────────────────────────────────────────────────
// Provider api_path tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_provider_effective_api_path_default_anthropic() {
    let provider = ProviderConfig {
        base_url: "https://api.anthropic.com".to_string(),
        name: None,
        api_format: ApiFormat::Anthropic,
        api_path: None,
        auth: None,
        count_tokens: None,
        model_mapping: HashMap::new(),
    };
    assert_eq!(provider.effective_api_path(), "/v1/messages");
}

#[test]
fn test_provider_effective_api_path_default_openai() {
    let provider = ProviderConfig {
        base_url: "https://openrouter.ai/api/v1".to_string(),
        name: None,
        api_format: ApiFormat::Openai,
        api_path: None,
        auth: None,
        count_tokens: None,
        model_mapping: HashMap::new(),
    };
    assert_eq!(provider.effective_api_path(), "/v1/chat/completions");
}

#[test]
fn test_provider_effective_api_path_custom_overrides_default() {
    // z.ai example: custom path without /v1 prefix
    let provider = ProviderConfig {
        base_url: "https://api.z.ai/api/coding/paas/v4".to_string(),
        name: None,
        api_format: ApiFormat::Openai,
        api_path: Some("/chat/completions".to_string()),
        auth: None,
        model_mapping: HashMap::new(),
        count_tokens: None,
    };
    assert_eq!(provider.effective_api_path(), "/chat/completions");
}

#[test]
fn test_provider_effective_api_path_custom_with_anthropic_format() {
    // Custom Anthropic-compatible endpoint
    let provider = ProviderConfig {
        base_url: "https://custom.example.com/api/v2".to_string(),
        name: None,
        api_format: ApiFormat::Anthropic,
        api_path: Some("/messages".to_string()),
        auth: None,
        count_tokens: None,
        model_mapping: HashMap::new(),
    };
    assert_eq!(provider.effective_api_path(), "/messages");
}

#[test]
fn test_clients_config_get_client_api_path() {
    let mut clients = HashMap::new();
    clients.insert(
        "zai".to_string(),
        ClientConfig {
            name: "Z.AI".to_string(),
            provider: "zai".to_string(),
            tags: vec![],
            auth: None,
        },
    );

    let mut providers = HashMap::new();
    providers.insert(
        "zai".to_string(),
        ProviderConfig {
            base_url: "https://api.z.ai/api/coding/paas/v4".to_string(),
            name: None,
            api_format: ApiFormat::Openai,
            api_path: Some("/chat/completions".to_string()),
            auth: None,
            model_mapping: HashMap::new(),
            count_tokens: None,
        },
    );

    let config = ClientsConfig { clients, providers };

    // Client with custom api_path
    assert_eq!(config.get_client_api_path("zai"), Some("/chat/completions"));

    // Unknown client returns None
    assert_eq!(config.get_client_api_path("unknown"), None);
}

#[test]
fn test_provider_api_path_serialization() {
    let mut config = Config::default();

    // Add a provider with custom api_path
    config.clients.providers.insert(
        "zai".to_string(),
        ProviderConfig {
            base_url: "https://api.z.ai/api/coding/paas/v4".to_string(),
            name: None,
            api_format: ApiFormat::Openai,
            api_path: Some("/chat/completions".to_string()),
            auth: None,
            model_mapping: HashMap::new(),
            count_tokens: None,
        },
    );

    let toml_str = config.to_toml();

    // Verify api_path is serialized
    assert!(
        toml_str.contains("api_path = \"/chat/completions\""),
        "api_path should be serialized.\nTOML:\n{}",
        toml_str
    );

    // Verify round-trip
    let parsed: Result<FileConfig, _> = toml::from_str(&toml_str);
    assert!(parsed.is_ok(), "Config should round-trip");
    let file_config = parsed.unwrap();

    let provider = file_config
        .providers
        .get("zai")
        .expect("zai provider should exist");
    assert_eq!(provider.api_path, Some("/chat/completions".to_string()));
}

// ─────────────────────────────────────────────────────────────────────────────
// Provider count_tokens tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_provider_effective_count_tokens_default_anthropic() {
    let provider = ProviderConfig {
        base_url: "https://api.anthropic.com".to_string(),
        name: None,
        api_format: ApiFormat::Anthropic,
        api_path: None,
        auth: None,
        count_tokens: None,
        model_mapping: HashMap::new(),
    };
    // Anthropic defaults to Passthrough
    assert_eq!(
        provider.effective_count_tokens(),
        CountTokensHandling::Passthrough
    );
}

#[test]
fn test_provider_effective_count_tokens_default_openai() {
    let provider = ProviderConfig {
        base_url: "https://openrouter.ai/api/v1".to_string(),
        name: None,
        api_format: ApiFormat::Openai,
        api_path: None,
        auth: None,
        count_tokens: None,
        model_mapping: HashMap::new(),
    };
    // OpenAI defaults to Synthetic
    assert_eq!(
        provider.effective_count_tokens(),
        CountTokensHandling::Synthetic
    );
}

#[test]
fn test_provider_effective_count_tokens_explicit_override() {
    // OpenAI provider can override to Dedupe
    let provider = ProviderConfig {
        base_url: "https://openrouter.ai/api/v1".to_string(),
        name: None,
        api_format: ApiFormat::Openai,
        api_path: None,
        auth: None,
        count_tokens: Some(CountTokensHandling::Dedupe),
        model_mapping: HashMap::new(),
    };
    assert_eq!(
        provider.effective_count_tokens(),
        CountTokensHandling::Dedupe
    );
}

#[test]
fn test_clients_config_get_client_count_tokens() {
    let mut clients = HashMap::new();
    clients.insert(
        "anthropic-client".to_string(),
        ClientConfig {
            name: "Anthropic".to_string(),
            provider: "anthropic".to_string(),
            tags: vec![],
            auth: None,
        },
    );
    clients.insert(
        "openai-client".to_string(),
        ClientConfig {
            name: "OpenAI".to_string(),
            provider: "openai".to_string(),
            tags: vec![],
            auth: None,
        },
    );

    let mut providers = HashMap::new();
    providers.insert(
        "anthropic".to_string(),
        ProviderConfig {
            base_url: "https://api.anthropic.com".to_string(),
            name: None,
            api_format: ApiFormat::Anthropic,
            api_path: None,
            auth: None,
            count_tokens: None,
            model_mapping: HashMap::new(),
        },
    );
    providers.insert(
        "openai".to_string(),
        ProviderConfig {
            base_url: "https://openrouter.ai/api/v1".to_string(),
            name: None,
            api_format: ApiFormat::Openai,
            api_path: None,
            auth: None,
            count_tokens: None,
            model_mapping: HashMap::new(),
        },
    );

    let config = ClientsConfig { clients, providers };

    // Anthropic client gets Passthrough
    assert_eq!(
        config.get_client_count_tokens("anthropic-client"),
        Some(CountTokensHandling::Passthrough)
    );

    // OpenAI client gets Synthetic
    assert_eq!(
        config.get_client_count_tokens("openai-client"),
        Some(CountTokensHandling::Synthetic)
    );

    // Unknown client returns None
    assert_eq!(config.get_client_count_tokens("unknown"), None);
}

#[test]
fn test_count_tokens_handling_serialization() {
    assert_eq!(CountTokensHandling::Passthrough.as_str(), "passthrough");
    assert_eq!(CountTokensHandling::Synthetic.as_str(), "synthetic");
    assert_eq!(CountTokensHandling::Dedupe.as_str(), "dedupe");
}

#[test]
fn test_provider_count_tokens_toml_only_serializes_non_default() {
    let mut config = Config::default();

    // Add an Anthropic provider with default count_tokens (should NOT serialize)
    config.clients.providers.insert(
        "anthropic".to_string(),
        ProviderConfig {
            base_url: "https://api.anthropic.com".to_string(),
            name: None,
            api_format: ApiFormat::Anthropic,
            api_path: None,
            auth: None,
            count_tokens: None, // Default for Anthropic is Passthrough
            model_mapping: HashMap::new(),
        },
    );

    // Add an OpenAI provider with explicit Dedupe (SHOULD serialize)
    config.clients.providers.insert(
        "openrouter".to_string(),
        ProviderConfig {
            base_url: "https://openrouter.ai/api/v1".to_string(),
            name: None,
            api_format: ApiFormat::Openai,
            api_path: None,
            auth: None,
            count_tokens: Some(CountTokensHandling::Dedupe), // Non-default
            model_mapping: HashMap::new(),
        },
    );

    let toml_str = config.to_toml();

    // Anthropic provider should NOT have count_tokens (it's the default)
    // OpenRouter should have count_tokens = "dedupe" (non-default)
    assert!(
        toml_str.contains(r#"count_tokens = "dedupe""#),
        "Non-default count_tokens should be serialized.\nTOML:\n{}",
        toml_str
    );

    // Verify the anthropic section doesn't have count_tokens
    let anthropic_section = toml_str
        .split("[providers.anthropic]")
        .nth(1)
        .and_then(|s| s.split("[providers.").next());
    if let Some(section) = anthropic_section {
        assert!(
            !section.contains("count_tokens"),
            "Anthropic section should not have count_tokens (it's default).\nSection:\n{}",
            section
        );
    }
}
