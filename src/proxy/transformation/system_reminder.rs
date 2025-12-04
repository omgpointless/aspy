//! System Reminder Editor - Transform `<system-reminder>` tags in requests
//!
//! This transformer allows editing, injecting, or removing `<system-reminder>` tags
//! that appear in user messages. These tags are commonly used by Claude Code to
//! inject contextual information.
//!
//! # Rules
//!
//! Rules are applied in order: Remove → Replace → Inject
//!
//! - **Remove**: Filter out reminders matching a regex pattern
//! - **Replace**: Modify content within matching reminders
//! - **Inject**: Add new `<system-reminder>` blocks at specified positions

use super::{RequestTransformer, TransformContext, TransformResult};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

// ============================================================================
// Rule Types
// ============================================================================

/// Where to inject new system-reminder content
#[derive(Debug, Clone)]
pub enum InjectPosition {
    /// At the start of the last user message (before any existing content)
    Start,
    /// At the end of the last user message (after all existing content)
    End,
    /// Before the first system-reminder matching pattern (or End if none match)
    Before(Regex),
    /// After the last system-reminder matching pattern (or End if none match)
    After(Regex),
}

/// Rules for modifying system-reminder tags
#[derive(Debug, Clone)]
pub enum ReminderRule {
    /// Inject a new `<system-reminder>` block
    Inject {
        /// Content to inject (will be wrapped in `<system-reminder>` tags)
        content: String,
        /// Where to inject
        position: InjectPosition,
    },
    /// Remove all `<system-reminder>` blocks matching pattern
    Remove {
        /// Regex to match against reminder content
        pattern: Regex,
    },
    /// Replace content within matching `<system-reminder>` blocks
    Replace {
        /// Regex to match against reminder content
        pattern: Regex,
        /// Replacement string (supports $1, $2 capture groups)
        replacement: String,
    },
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for a single rule (from TOML)
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RuleConfig {
    Inject {
        content: String,
        #[serde(default = "default_position")]
        position: PositionConfig,
    },
    Remove {
        pattern: String,
    },
    Replace {
        pattern: String,
        replacement: String,
    },
}

fn default_position() -> PositionConfig {
    PositionConfig::End
}

/// Position configuration (from TOML)
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PositionConfig {
    Start,
    #[default]
    End,
    Before {
        pattern: String,
    },
    After {
        pattern: String,
    },
}

/// Configuration for the SystemReminderEditor transformer
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SystemReminderEditorConfig {
    /// Whether the transformer is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Rules to apply (in order)
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
}

// ============================================================================
// SystemReminderEditor
// ============================================================================

/// Transformer that edits `<system-reminder>` tags in user messages
pub struct SystemReminderEditor {
    rules: Vec<ReminderRule>,
}

impl SystemReminderEditor {
    /// Create a new editor with the given rules
    /// Used by: Tests and programmatic rule construction (config uses from_config)
    #[allow(dead_code)]
    pub fn new(rules: Vec<ReminderRule>) -> Self {
        Self { rules }
    }

    /// Create from configuration
    pub fn from_config(config: &SystemReminderEditorConfig) -> anyhow::Result<Self> {
        let mut rules = Vec::with_capacity(config.rules.len());

        for rule_config in &config.rules {
            let rule = match rule_config {
                RuleConfig::Inject { content, position } => {
                    let pos = match position {
                        PositionConfig::Start => InjectPosition::Start,
                        PositionConfig::End => InjectPosition::End,
                        PositionConfig::Before { pattern } => {
                            InjectPosition::Before(Regex::new(pattern)?)
                        }
                        PositionConfig::After { pattern } => {
                            InjectPosition::After(Regex::new(pattern)?)
                        }
                    };
                    ReminderRule::Inject {
                        content: content.clone(),
                        position: pos,
                    }
                }
                RuleConfig::Remove { pattern } => ReminderRule::Remove {
                    pattern: Regex::new(pattern)?,
                },
                RuleConfig::Replace {
                    pattern,
                    replacement,
                } => ReminderRule::Replace {
                    pattern: Regex::new(pattern)?,
                    replacement: replacement.clone(),
                },
            };
            rules.push(rule);
        }

        Ok(Self { rules })
    }

    /// Get the number of rules
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Apply all rules to the given text content
    fn apply_rules(&self, content: &str) -> Option<String> {
        let mut modified = false;
        let mut result = content.to_string();

        // Parse existing system-reminder blocks
        let mut reminders = parse_system_reminders(&result);

        // Apply Remove rules first
        for rule in &self.rules {
            if let ReminderRule::Remove { pattern } = rule {
                let before_len = reminders.len();
                reminders.retain(|r| !pattern.is_match(&r.content));
                if reminders.len() != before_len {
                    modified = true;
                }
            }
        }

        // Apply Replace rules
        for rule in &self.rules {
            if let ReminderRule::Replace {
                pattern,
                replacement,
            } = rule
            {
                for reminder in &mut reminders {
                    if pattern.is_match(&reminder.content) {
                        let new_content = pattern.replace_all(&reminder.content, replacement);
                        if new_content != reminder.content {
                            reminder.content = new_content.into_owned();
                            modified = true;
                        }
                    }
                }
            }
        }

        // Apply Inject rules
        for rule in &self.rules {
            if let ReminderRule::Inject { content, position } = rule {
                let inject_content = if content.contains("<system-reminder>") {
                    content.clone()
                } else {
                    format!("<system-reminder>\n{}\n</system-reminder>", content)
                };

                let new_reminder = SystemReminder {
                    content: inject_content,
                    original_start: 0,
                    original_end: 0,
                };

                let insert_idx = match position {
                    InjectPosition::Start => 0,
                    InjectPosition::End => reminders.len(),
                    InjectPosition::Before(pattern) => reminders
                        .iter()
                        .position(|r| pattern.is_match(&r.content))
                        .unwrap_or(reminders.len()),
                    InjectPosition::After(pattern) => reminders
                        .iter()
                        .rposition(|r| pattern.is_match(&r.content))
                        .map(|i| i + 1)
                        .unwrap_or(reminders.len()),
                };

                reminders.insert(insert_idx, new_reminder);
                modified = true;
            }
        }

        if modified {
            // Rebuild content: remove all original reminders and insert new ones
            result = remove_all_system_reminders(&result);

            // Insert all reminders at the end (they're in order)
            let reminder_block: String = reminders
                .iter()
                .map(|r| {
                    if r.content.contains("<system-reminder>") {
                        r.content.clone()
                    } else {
                        format!("<system-reminder>\n{}\n</system-reminder>", r.content)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            if !reminder_block.is_empty() {
                result = format!("{}\n{}", result.trim_end(), reminder_block);
            }

            Some(result)
        } else {
            None
        }
    }
}

impl RequestTransformer for SystemReminderEditor {
    fn name(&self) -> &'static str {
        "system-reminder-editor"
    }

    fn should_apply(&self, ctx: &TransformContext) -> bool {
        // Only apply to messages endpoint
        ctx.path.ends_with("/messages") || ctx.path.ends_with("/v1/messages")
    }

    fn transform(&self, body: &Value, _ctx: &TransformContext) -> TransformResult {
        if self.rules.is_empty() {
            return TransformResult::Unchanged;
        }

        // Get messages array
        let messages = match body.get("messages").and_then(|m| m.as_array()) {
            Some(m) => m,
            None => return TransformResult::Unchanged,
        };

        // Find the last user message
        let last_user_idx = messages
            .iter()
            .enumerate()
            .rev()
            .find(|(_, msg)| msg.get("role").and_then(|r| r.as_str()) == Some("user"))
            .map(|(i, _)| i);

        let last_user_idx = match last_user_idx {
            Some(i) => i,
            None => return TransformResult::Unchanged,
        };

        // Extract content from last user message
        let user_msg = &messages[last_user_idx];
        let content = user_msg.get("content");

        let (original_text, is_array_content) = match content {
            Some(Value::String(s)) => (s.clone(), false),
            Some(Value::Array(arr)) => {
                // Concatenate all text blocks
                let text: String = arr
                    .iter()
                    .filter_map(|block| {
                        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                            block.get("text").and_then(|t| t.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                (text, true)
            }
            _ => return TransformResult::Unchanged,
        };

        // Apply rules
        let new_text = match self.apply_rules(&original_text) {
            Some(t) => t,
            None => return TransformResult::Unchanged,
        };

        // Rebuild the body with modified content
        let mut new_body = body.clone();
        let messages_mut = new_body
            .get_mut("messages")
            .and_then(|m| m.as_array_mut())
            .unwrap();

        if is_array_content {
            // For array content, find the last text block and update it
            // (simplified: replace with single text block)
            messages_mut[last_user_idx]["content"] = serde_json::json!([
                {
                    "type": "text",
                    "text": new_text
                }
            ]);
        } else {
            messages_mut[last_user_idx]["content"] = Value::String(new_text);
        }

        tracing::debug!(
            rules = self.rules.len(),
            "Applied system-reminder transformations"
        );

        TransformResult::Modified(new_body)
    }
}

// ============================================================================
// Parsing Helpers
// ============================================================================

/// A parsed system-reminder block
#[derive(Debug, Clone)]
struct SystemReminder {
    /// The content (including tags for reconstructed, excluding for parsed)
    content: String,
    /// Original start position in source text
    original_start: usize,
    /// Original end position in source text
    original_end: usize,
}

/// Parse all `<system-reminder>` blocks from text
fn parse_system_reminders(text: &str) -> Vec<SystemReminder> {
    let mut reminders = Vec::new();
    let mut search_start = 0;

    while let Some(start) = text[search_start..].find("<system-reminder>") {
        let abs_start = search_start + start;
        let after_tag = abs_start + "<system-reminder>".len();

        if let Some(end_offset) = text[after_tag..].find("</system-reminder>") {
            let abs_end = after_tag + end_offset + "</system-reminder>".len();
            let inner_content = &text[after_tag..after_tag + end_offset];

            reminders.push(SystemReminder {
                content: inner_content.trim().to_string(),
                original_start: abs_start,
                original_end: abs_end,
            });

            search_start = abs_end;
        } else {
            // Unclosed tag - skip past this start tag
            search_start = after_tag;
        }
    }

    reminders
}

/// Remove all `<system-reminder>` blocks from text
fn remove_all_system_reminders(text: &str) -> String {
    let reminders = parse_system_reminders(text);
    if reminders.is_empty() {
        return text.to_string();
    }

    let mut result = String::with_capacity(text.len());
    let mut last_end = 0;

    for reminder in &reminders {
        // Add text before this reminder
        result.push_str(&text[last_end..reminder.original_start]);
        last_end = reminder.original_end;
    }

    // Add remaining text after last reminder
    result.push_str(&text[last_end..]);

    // Clean up extra newlines
    let result = result
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    result
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_system_reminders() {
        let text = r#"Hello
<system-reminder>
First reminder
</system-reminder>
Middle text
<system-reminder>
Second reminder
</system-reminder>
End"#;

        let reminders = parse_system_reminders(text);
        assert_eq!(reminders.len(), 2);
        assert_eq!(reminders[0].content, "First reminder");
        assert_eq!(reminders[1].content, "Second reminder");
    }

    #[test]
    fn test_remove_all_system_reminders() {
        let text = r#"Hello
<system-reminder>
Remove me
</system-reminder>
World"#;

        let result = remove_all_system_reminders(text);
        assert!(!result.contains("<system-reminder>"));
        assert!(result.contains("Hello"));
        assert!(result.contains("World"));
    }

    #[test]
    fn test_inject_rule() {
        let rules = vec![ReminderRule::Inject {
            content: "Injected content".to_string(),
            position: InjectPosition::End,
        }];

        let editor = SystemReminderEditor::new(rules);
        let text = "Hello world";
        let result = editor.apply_rules(text).unwrap();

        assert!(result.contains("Injected content"));
        assert!(result.contains("<system-reminder>"));
    }

    #[test]
    fn test_remove_rule() {
        let rules = vec![ReminderRule::Remove {
            pattern: Regex::new("noisy").unwrap(),
        }];

        let editor = SystemReminderEditor::new(rules);
        let text = r#"Keep this
<system-reminder>
This is noisy and should be removed
</system-reminder>
<system-reminder>
This should stay
</system-reminder>"#;

        let result = editor.apply_rules(text).unwrap();
        assert!(!result.contains("noisy"));
        assert!(result.contains("This should stay"));
    }

    #[test]
    fn test_replace_rule() {
        let rules = vec![ReminderRule::Replace {
            pattern: Regex::new("old-url\\.com").unwrap(),
            replacement: "new-url.com".to_string(),
        }];

        let editor = SystemReminderEditor::new(rules);
        let text = r#"<system-reminder>
Visit old-url.com for docs
</system-reminder>"#;

        let result = editor.apply_rules(text).unwrap();
        assert!(result.contains("new-url.com"));
        assert!(!result.contains("old-url.com"));
    }

    #[test]
    fn test_transform_request_body() {
        let rules = vec![ReminderRule::Inject {
            content: "Custom context".to_string(),
            position: InjectPosition::End,
        }];

        let editor = SystemReminderEditor::new(rules);
        let body = serde_json::json!({
            "model": "claude-3",
            "messages": [
                {"role": "user", "content": "Hello"}
            ]
        });

        let ctx = TransformContext::new(None, "/v1/messages", Some("claude-3"));

        match editor.transform(&body, &ctx) {
            TransformResult::Modified(new_body) => {
                let content = new_body["messages"][0]["content"].as_str().unwrap();
                assert!(content.contains("Custom context"));
                assert!(content.contains("<system-reminder>"));
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_no_changes_returns_unchanged() {
        let rules = vec![ReminderRule::Remove {
            pattern: Regex::new("nonexistent").unwrap(),
        }];

        let editor = SystemReminderEditor::new(rules);
        let text = "Just plain text without any matching content";

        assert!(editor.apply_rules(text).is_none());
    }
}
