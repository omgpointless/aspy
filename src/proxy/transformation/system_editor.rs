//! System Editor - Transform system prompts in API requests
//!
//! This transformer modifies the `system` field in Claude API requests,
//! allowing injection, replacement, or augmentation of system prompts.
//!
//! # Rules
//!
//! Rules are applied in order:
//! - **Append**: Add text to the end of system prompt
//! - **Prepend**: Add text to the beginning of system prompt
//! - **Replace**: Find and replace text within system blocks
//!
//! # Example Config
//!
//! ```toml
//! [transformers.system-editor]
//! enabled = true
//!
//! [[transformers.system-editor.rules]]
//! type = "append"
//! content = "\n\nYou are augmented by Aspy observability."
//!
//! [[transformers.system-editor.rules]]
//! type = "replace"
//! pattern = "Claude Code"
//! replacement = "Claude Code (Aspy-enhanced)"
//! ```

use super::{RequestTransformer, TransformContext, TransformResult};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

// ============================================================================
// Rule Types
// ============================================================================

/// Rules for modifying system prompts
#[derive(Debug, Clone)]
pub enum SystemRule {
    /// Append text to the end of the last system block
    Append { content: String },
    /// Prepend text to the beginning of the first system block
    Prepend { content: String },
    /// Replace matching text in all system blocks
    Replace { pattern: Regex, replacement: String },
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for a single rule (from TOML)
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RuleConfig {
    Append {
        content: String,
    },
    Prepend {
        content: String,
    },
    Replace {
        pattern: String,
        replacement: String,
    },
}

/// Configuration for the SystemEditor transformer
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SystemEditorConfig {
    /// Whether the transformer is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Rules to apply (in order)
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
}

// ============================================================================
// SystemEditor
// ============================================================================

/// Transformer that edits system prompts in API requests
pub struct SystemEditor {
    rules: Vec<SystemRule>,
}

impl SystemEditor {
    /// Create a new editor with the given rules
    #[allow(dead_code)]
    pub fn new(rules: Vec<SystemRule>) -> Self {
        Self { rules }
    }

    /// Create from configuration
    pub fn from_config(config: &SystemEditorConfig) -> anyhow::Result<Self> {
        let mut rules = Vec::with_capacity(config.rules.len());

        for rule_config in &config.rules {
            let rule = match rule_config {
                RuleConfig::Append { content } => {
                    tracing::debug!(content_len = content.len(), "Loaded Append rule");
                    SystemRule::Append {
                        content: content.clone(),
                    }
                }
                RuleConfig::Prepend { content } => {
                    tracing::debug!(content_len = content.len(), "Loaded Prepend rule");
                    SystemRule::Prepend {
                        content: content.clone(),
                    }
                }
                RuleConfig::Replace {
                    pattern,
                    replacement,
                } => {
                    tracing::debug!(
                        pattern = %pattern,
                        replacement = %replacement,
                        "Loaded Replace rule"
                    );
                    SystemRule::Replace {
                        pattern: Regex::new(pattern)?,
                        replacement: replacement.clone(),
                    }
                }
            };
            rules.push(rule);
        }

        Ok(Self { rules })
    }

    /// Get the number of rules
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Apply all rules to the system array
    fn apply_rules(&self, system: &mut [Value]) -> (bool, Vec<String>) {
        let mut modified = false;
        let mut modifications = Vec::new();

        for rule in &self.rules {
            match rule {
                SystemRule::Append { content } => {
                    // Find last text block and append
                    if let Some(block) = system
                        .iter_mut()
                        .rev()
                        .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                    {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            block["text"] = Value::String(format!("{}{}", text, content));
                            modified = true;
                            modifications.push("Appended to system prompt".to_string());
                        }
                    }
                }
                SystemRule::Prepend { content } => {
                    // Find first text block and prepend
                    if let Some(block) = system
                        .iter_mut()
                        .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                    {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            block["text"] = Value::String(format!("{}{}", content, text));
                            modified = true;
                            modifications.push("Prepended to system prompt".to_string());
                        }
                    }
                }
                SystemRule::Replace {
                    pattern,
                    replacement,
                } => {
                    let mut replace_count = 0;
                    for block in system.iter_mut() {
                        if block.get("type").and_then(|t| t.as_str()) != Some("text") {
                            continue;
                        }
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            if pattern.is_match(text) {
                                let new_text = pattern.replace_all(text, replacement.as_str());
                                block["text"] = Value::String(new_text.into_owned());
                                replace_count += 1;
                                modified = true;
                            }
                        }
                    }
                    if replace_count > 0 {
                        modifications.push(format!(
                            "Replaced '{}' in {} system block(s)",
                            pattern, replace_count
                        ));
                    }
                }
            }
        }

        (modified, modifications)
    }
}

impl RequestTransformer for SystemEditor {
    fn name(&self) -> &'static str {
        "system-editor"
    }

    fn should_apply(&self, ctx: &TransformContext) -> bool {
        ctx.path.ends_with("/messages") || ctx.path.ends_with("/v1/messages")
    }

    fn transform(&self, body: &Value, _ctx: &TransformContext) -> TransformResult {
        if self.rules.is_empty() {
            return TransformResult::Unchanged;
        }

        // Get system array
        let system = match body.get("system") {
            Some(Value::Array(arr)) => arr.clone(),
            Some(Value::String(s)) => {
                // Convert string format to array format
                vec![serde_json::json!({"type": "text", "text": s})]
            }
            _ => return TransformResult::Unchanged,
        };

        if system.is_empty() {
            return TransformResult::Unchanged;
        }

        // Clone and apply rules
        let mut new_system = system;
        let (modified, modifications) = self.apply_rules(&mut new_system);

        if !modified {
            return TransformResult::Unchanged;
        }

        // Build new body with modified system
        let mut new_body = body.clone();
        new_body["system"] = Value::Array(new_system);

        // Estimate token changes
        let tokens_before = crate::tokens::estimate_json_tokens(body);
        let tokens_after = crate::tokens::estimate_json_tokens(&new_body);

        tracing::info!(
            rules = self.rules.len(),
            modifications = ?modifications,
            "SystemEditor: applied {} rules",
            self.rules.len()
        );

        TransformResult::modified_with_info(new_body, tokens_before, tokens_after, modifications)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> TransformContext<'static> {
        TransformContext::new(None, "/v1/messages", Some("claude-3"))
    }

    #[test]
    fn test_config_parsing() {
        let config = SystemEditorConfig {
            enabled: true,
            rules: vec![
                RuleConfig::Append {
                    content: "Appended text".to_string(),
                },
                RuleConfig::Prepend {
                    content: "Prepended text".to_string(),
                },
                RuleConfig::Replace {
                    pattern: "old".to_string(),
                    replacement: "new".to_string(),
                },
            ],
        };

        let editor = SystemEditor::from_config(&config).unwrap();
        assert_eq!(editor.rule_count(), 3);
    }

    #[test]
    fn test_should_apply_to_messages_endpoint() {
        let editor = SystemEditor::new(vec![]);

        assert!(editor.should_apply(&TransformContext::new(None, "/v1/messages", None)));
        assert!(editor.should_apply(&TransformContext::new(None, "/dev-1/v1/messages", None)));
        assert!(!editor.should_apply(&TransformContext::new(None, "/v1/embeddings", None)));
    }

    #[test]
    fn test_empty_rules_returns_unchanged() {
        let editor = SystemEditor::new(vec![]);
        let body = serde_json::json!({
            "model": "claude-3",
            "system": [{"type": "text", "text": "You are Claude."}],
            "messages": []
        });

        match editor.transform(&body, &test_ctx()) {
            TransformResult::Unchanged => {}
            other => panic!("Expected Unchanged, got {:?}", other),
        }
    }

    #[test]
    fn test_no_system_returns_unchanged() {
        let editor = SystemEditor::new(vec![SystemRule::Append {
            content: "test".to_string(),
        }]);
        let body = serde_json::json!({
            "model": "claude-3",
            "messages": []
        });

        match editor.transform(&body, &test_ctx()) {
            TransformResult::Unchanged => {}
            other => panic!("Expected Unchanged for no system field, got {:?}", other),
        }
    }

    #[test]
    fn test_append_rule() {
        let editor = SystemEditor::new(vec![SystemRule::Append {
            content: " Augmented by Aspy.".to_string(),
        }]);
        let body = serde_json::json!({
            "model": "claude-3",
            "system": [{"type": "text", "text": "You are Claude."}],
            "messages": []
        });
        match editor.transform(&body, &test_ctx()) {
            TransformResult::Modified { body: new_body, .. } => {
                let text = new_body["system"][0]["text"].as_str().unwrap();
                assert_eq!(text, "You are Claude. Augmented by Aspy.");
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_prepend_rule() {
        let editor = SystemEditor::new(vec![SystemRule::Prepend {
            content: "[ENHANCED] ".to_string(),
        }]);
        let body = serde_json::json!({
            "model": "claude-3",
            "system": [{"type": "text", "text": "You are Claude."}],
            "messages": []
        });
        match editor.transform(&body, &test_ctx()) {
            TransformResult::Modified { body: new_body, .. } => {
                let text = new_body["system"][0]["text"].as_str().unwrap();
                assert_eq!(text, "[ENHANCED] You are Claude.");
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_replace_rule() {
        let editor = SystemEditor::new(vec![SystemRule::Replace {
            pattern: Regex::new("Claude Code").unwrap(),
            replacement: "Claude Code (Aspy)".to_string(),
        }]);
        let body = serde_json::json!({
            "model": "claude-3",
            "system": [{"type": "text", "text": "You are Claude Code, the CLI."}],
            "messages": []
        });
        match editor.transform(&body, &test_ctx()) {
            TransformResult::Modified { body: new_body, .. } => {
                let text = new_body["system"][0]["text"].as_str().unwrap();
                assert_eq!(text, "You are Claude Code (Aspy), the CLI.");
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_multiple_system_blocks() {
        let editor = SystemEditor::new(vec![
            SystemRule::Prepend {
                content: "START: ".to_string(),
            },
            SystemRule::Append {
                content: " :END".to_string(),
            },
        ]);
        let body = serde_json::json!({
            "model": "claude-3",
            "system": [
                {"type": "text", "text": "First block."},
                {"type": "text", "text": "Second block."}
            ],
            "messages": []
        });
        match editor.transform(&body, &test_ctx()) {
            TransformResult::Modified { body: new_body, .. } => {
                // Prepend affects first block
                let first = new_body["system"][0]["text"].as_str().unwrap();
                assert_eq!(first, "START: First block.");
                // Append affects last block
                let second = new_body["system"][1]["text"].as_str().unwrap();
                assert_eq!(second, "Second block. :END");
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_replace_affects_all_blocks() {
        let editor = SystemEditor::new(vec![SystemRule::Replace {
            pattern: Regex::new("Claude").unwrap(),
            replacement: "Assistant".to_string(),
        }]);
        let body = serde_json::json!({
            "model": "claude-3",
            "system": [
                {"type": "text", "text": "You are Claude."},
                {"type": "text", "text": "Claude helps users."}
            ],
            "messages": []
        });
        match editor.transform(&body, &test_ctx()) {
            TransformResult::Modified {
                body: new_body,
                modifications,
                ..
            } => {
                let first = new_body["system"][0]["text"].as_str().unwrap();
                let second = new_body["system"][1]["text"].as_str().unwrap();
                assert_eq!(first, "You are Assistant.");
                assert_eq!(second, "Assistant helps users.");
                assert!(modifications[0].contains("2 system block"));
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_string_system_format() {
        // System can be a string instead of array
        let editor = SystemEditor::new(vec![SystemRule::Append {
            content: " (enhanced)".to_string(),
        }]);
        let body = serde_json::json!({
            "model": "claude-3",
            "system": "You are Claude.",
            "messages": []
        });
        match editor.transform(&body, &test_ctx()) {
            TransformResult::Modified { body: new_body, .. } => {
                // Should convert to array format
                let text = new_body["system"][0]["text"].as_str().unwrap();
                assert_eq!(text, "You are Claude. (enhanced)");
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_no_match_returns_unchanged() {
        let editor = SystemEditor::new(vec![SystemRule::Replace {
            pattern: Regex::new("nonexistent").unwrap(),
            replacement: "replacement".to_string(),
        }]);
        let body = serde_json::json!({
            "model": "claude-3",
            "system": [{"type": "text", "text": "You are Claude."}],
            "messages": []
        });
        match editor.transform(&body, &test_ctx()) {
            TransformResult::Unchanged => {}
            other => panic!("Expected Unchanged when no match, got {:?}", other),
        }
    }
}
