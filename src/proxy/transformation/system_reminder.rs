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

    /// Check if any Inject rules are configured
    fn has_inject_rules(&self) -> bool {
        self.rules
            .iter()
            .any(|r| matches!(r, ReminderRule::Inject { .. }))
    }

    /// Apply only Remove and Replace rules (not Inject)
    /// Used for non-last text blocks to clean up reminders without adding new ones
    fn apply_rules_remove_replace_only(&self, content: &str) -> Option<String> {
        let mut modified = false;
        let mut result = content.to_string();
        let mut reminders = parse_system_reminders(&result);

        // Apply Remove rules
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

        if !modified {
            return None;
        }

        // Reconstruct text
        result = remove_all_system_reminders(&result);
        for reminder in &reminders {
            result.push_str("\n<system-reminder>\n");
            result.push_str(&reminder.content);
            result.push_str("\n</system-reminder>");
        }
        Some(result)
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

        // Track content structure
        enum ContentStructure {
            String,
            Array { text_block_indices: Vec<usize> },
        }

        let structure = match content {
            Some(Value::String(_)) => ContentStructure::String,
            Some(Value::Array(arr)) => {
                // Find ALL text block indices - preserve order
                let text_block_indices: Vec<usize> = arr
                    .iter()
                    .enumerate()
                    .filter(|(_, block)| block.get("type").and_then(|t| t.as_str()) == Some("text"))
                    .map(|(idx, _)| idx)
                    .collect();
                ContentStructure::Array { text_block_indices }
            }
            _ => return TransformResult::Unchanged,
        };

        // Clone body for modification
        let mut new_body = body.clone();
        let messages_mut = new_body
            .get_mut("messages")
            .and_then(|m| m.as_array_mut())
            .unwrap();

        let mut any_modified = false;

        match structure {
            ContentStructure::String => {
                let original = messages_mut[last_user_idx]["content"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                if let Some(new_text) = self.apply_rules(&original) {
                    if new_text.trim().is_empty() {
                        // Empty string content: use minimal valid content
                        // (Can't delete the message, so use single space as fallback)
                        tracing::debug!(
                            "String content became empty after transformation, using minimal content"
                        );
                        messages_mut[last_user_idx]["content"] = Value::String(" ".to_string());
                    } else {
                        messages_mut[last_user_idx]["content"] = Value::String(new_text);
                    }
                    any_modified = true;
                }
            }
            ContentStructure::Array { text_block_indices } => {
                let content_arr = messages_mut[last_user_idx]
                    .get_mut("content")
                    .and_then(|c| c.as_array_mut())
                    .unwrap();

                let last_text_idx = text_block_indices.last().copied();

                // Track indices of text blocks that become empty (to delete later)
                let mut empty_block_indices: Vec<usize> = Vec::new();

                // Apply rules to ALL text blocks (Remove/Replace scan all, Inject only last)
                for &idx in &text_block_indices {
                    if let Some(block) = content_arr.get_mut(idx) {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            let is_last = Some(idx) == last_text_idx;
                            let new_text = if is_last {
                                self.apply_rules(text)
                            } else {
                                self.apply_rules_remove_replace_only(text)
                            };
                            if let Some(new_text) = new_text {
                                // If text is empty after transformation, mark for deletion
                                if new_text.trim().is_empty() {
                                    empty_block_indices.push(idx);
                                } else {
                                    block["text"] = Value::String(new_text);
                                }
                                any_modified = true;
                            }
                        }
                    }
                }

                // Remove empty text blocks (in reverse order to preserve indices)
                for &idx in empty_block_indices.iter().rev() {
                    content_arr.remove(idx);
                    tracing::debug!(
                        removed_idx = idx,
                        "Removed empty text block after transformation"
                    );
                }

                // If no text blocks but we have Inject rules, append one
                if text_block_indices.is_empty() && self.has_inject_rules() {
                    if let Some(new_text) = self.apply_rules("") {
                        content_arr.push(serde_json::json!({
                            "type": "text",
                            "text": new_text
                        }));
                        any_modified = true;
                    }
                }
            }
        }

        if !any_modified {
            return TransformResult::Unchanged;
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

    // ============================================================================
    // Order Preservation Tests
    // ============================================================================

    #[test]
    fn test_preserves_block_order_and_attributes() {
        // This test verifies the critical order preservation fix:
        // - Block order must be preserved (tool_result, text, etc.)
        // - Attributes like cache_control must be preserved
        // - Only the last text block gets Inject rules
        let rules = vec![ReminderRule::Inject {
            content: "Injected content".to_string(),
            position: InjectPosition::End,
        }];

        let editor = SystemReminderEditor::new(rules);
        let body = serde_json::json!({
            "model": "claude-3",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "tool_result",
                            "tool_use_id": "tool-1",
                            "content": "First tool result"
                        },
                        {
                            "type": "text",
                            "text": "First text",
                            "cache_control": {"type": "ephemeral"}
                        },
                        {
                            "type": "tool_result",
                            "tool_use_id": "tool-2",
                            "content": "Second tool result"
                        },
                        {
                            "type": "text",
                            "text": "Last text"
                        }
                    ]
                }
            ]
        });

        let ctx = TransformContext::new(None, "/v1/messages", Some("claude-3"));

        match editor.transform(&body, &ctx) {
            TransformResult::Modified(new_body) => {
                let content = new_body["messages"][0]["content"].as_array().unwrap();

                // Verify block count preserved (4 blocks)
                assert_eq!(content.len(), 4, "Block count should be preserved");

                // Verify order: tool_result, text, tool_result, text
                assert_eq!(content[0]["type"], "tool_result");
                assert_eq!(content[1]["type"], "text");
                assert_eq!(content[2]["type"], "tool_result");
                assert_eq!(content[3]["type"], "text");

                // Verify tool_result content preserved
                assert_eq!(content[0]["tool_use_id"], "tool-1");
                assert_eq!(content[2]["tool_use_id"], "tool-2");

                // Verify cache_control preserved on first text block
                assert!(
                    content[1].get("cache_control").is_some(),
                    "cache_control should be preserved on first text block"
                );

                // Verify injection only on LAST text block
                let first_text = content[1]["text"].as_str().unwrap();
                let last_text = content[3]["text"].as_str().unwrap();

                assert!(
                    !first_text.contains("Injected content"),
                    "First text block should NOT have injection"
                );
                assert!(
                    last_text.contains("Injected content"),
                    "Last text block SHOULD have injection"
                );
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_remove_rule_scans_all_text_blocks() {
        // This test verifies that Remove rules scan ALL text blocks,
        // not just the last one. Critical for cleaning up reminders
        // that might appear in earlier text blocks.
        let rules = vec![ReminderRule::Remove {
            pattern: Regex::new("Learning output style").unwrap(),
        }];

        let editor = SystemReminderEditor::new(rules);
        let body = serde_json::json!({
            "model": "claude-3",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "tool_result",
                            "tool_use_id": "tool-1",
                            "content": "Tool output"
                        },
                        {
                            "type": "text",
                            "text": "Some text\n<system-reminder>\nLearning output style is active\n</system-reminder>"
                        },
                        {
                            "type": "tool_result",
                            "tool_use_id": "tool-2",
                            "content": "More output"
                        },
                        {
                            "type": "text",
                            "text": "Final text"
                        }
                    ]
                }
            ]
        });

        let ctx = TransformContext::new(None, "/v1/messages", Some("claude-3"));

        match editor.transform(&body, &ctx) {
            TransformResult::Modified(new_body) => {
                let content = new_body["messages"][0]["content"].as_array().unwrap();

                // The reminder in the first text block should be removed
                let first_text = content[1]["text"].as_str().unwrap();
                assert!(
                    !first_text.contains("Learning output style"),
                    "Remove rule should scan and remove from ALL text blocks"
                );
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_mixed_content_with_tool_results() {
        // Real-world scenario: multiple tool_results interleaved with text
        let rules = vec![ReminderRule::Inject {
            content: "Context info".to_string(),
            position: InjectPosition::End,
        }];

        let editor = SystemReminderEditor::new(rules);
        let body = serde_json::json!({
            "model": "claude-3",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "tool_result", "tool_use_id": "read-1", "content": "File contents..."},
                        {"type": "tool_result", "tool_use_id": "read-2", "content": "More files..."},
                        {"type": "text", "text": "Here's what I found"}
                    ]
                }
            ]
        });

        let ctx = TransformContext::new(None, "/v1/messages", Some("claude-3"));

        match editor.transform(&body, &ctx) {
            TransformResult::Modified(new_body) => {
                let content = new_body["messages"][0]["content"].as_array().unwrap();

                // All 3 blocks preserved
                assert_eq!(content.len(), 3);

                // Order preserved: tool_result, tool_result, text
                assert_eq!(content[0]["type"], "tool_result");
                assert_eq!(content[1]["type"], "tool_result");
                assert_eq!(content[2]["type"], "text");

                // Tool IDs preserved
                assert_eq!(content[0]["tool_use_id"], "read-1");
                assert_eq!(content[1]["tool_use_id"], "read-2");

                // Injection in text block
                let text = content[2]["text"].as_str().unwrap();
                assert!(text.contains("Context info"));
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_no_text_blocks_creates_one_for_inject() {
        // When there are only tool_results, inject should create a text block
        let rules = vec![ReminderRule::Inject {
            content: "Added context".to_string(),
            position: InjectPosition::End,
        }];

        let editor = SystemReminderEditor::new(rules);
        let body = serde_json::json!({
            "model": "claude-3",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "tool_result", "tool_use_id": "tool-1", "content": "Output"}
                    ]
                }
            ]
        });

        let ctx = TransformContext::new(None, "/v1/messages", Some("claude-3"));

        match editor.transform(&body, &ctx) {
            TransformResult::Modified(new_body) => {
                let content = new_body["messages"][0]["content"].as_array().unwrap();

                // Should now have 2 blocks: tool_result + new text
                assert_eq!(content.len(), 2);
                assert_eq!(content[0]["type"], "tool_result");
                assert_eq!(content[1]["type"], "text");

                let text = content[1]["text"].as_str().unwrap();
                assert!(text.contains("Added context"));
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_replace_rule_scans_all_text_blocks() {
        // Replace rules should also scan all text blocks
        let rules = vec![ReminderRule::Replace {
            pattern: Regex::new("old-value").unwrap(),
            replacement: "new-value".to_string(),
        }];

        let editor = SystemReminderEditor::new(rules);
        let body = serde_json::json!({
            "model": "claude-3",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": "<system-reminder>\nConfig: old-value\n</system-reminder>"
                        },
                        {"type": "tool_result", "tool_use_id": "tool-1", "content": "Output"},
                        {
                            "type": "text",
                            "text": "Final message"
                        }
                    ]
                }
            ]
        });

        let ctx = TransformContext::new(None, "/v1/messages", Some("claude-3"));

        match editor.transform(&body, &ctx) {
            TransformResult::Modified(new_body) => {
                let content = new_body["messages"][0]["content"].as_array().unwrap();

                // First text block should have replacement applied
                let first_text = content[0]["text"].as_str().unwrap();
                assert!(
                    first_text.contains("new-value"),
                    "Replace should work in non-last text blocks"
                );
                assert!(
                    !first_text.contains("old-value"),
                    "Old value should be replaced"
                );
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }
}
