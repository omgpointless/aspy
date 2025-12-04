//! Tag Editor - Transform XML-style tags in requests
//!
//! This transformer allows editing, injecting, or removing configurable XML tags
//! that appear in user messages. Supports any tag name (e.g., `<system-reminder>`,
//! `<aspy-context>`, `<policy_spec>`).
//!
//! # Rules
//!
//! Rules are applied in order: Remove → Replace → Inject
//!
//! - **Remove**: Filter out blocks matching a regex pattern
//! - **Replace**: Modify content within matching blocks
//! - **Inject**: Add new blocks at specified positions

use super::{RequestTransformer, TransformContext, TransformResult};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

// ============================================================================
// Rule Types
// ============================================================================

/// Where to inject new tag content
#[derive(Debug, Clone)]
pub enum InjectPosition {
    /// At the start of the last user message (before any existing content)
    Start,
    /// At the end of the last user message (after all existing content)
    End,
    /// Before the first block matching pattern (or End if none match)
    Before(Regex),
    /// After the last block matching pattern (or End if none match)
    After(Regex),
}

/// Rules for modifying XML-style tags
#[derive(Debug, Clone)]
pub enum TagRule {
    /// Inject a new block
    Inject {
        /// Which XML tag this rule targets (e.g., "system-reminder", "aspy-context")
        tag: String,
        /// Content to inject (will be wrapped in the specified tag)
        content: String,
        /// Where to inject
        position: InjectPosition,
    },
    /// Remove all blocks matching pattern
    Remove {
        /// Which XML tag this rule targets
        tag: String,
        /// Regex to match against block content
        pattern: Regex,
    },
    /// Replace content within matching blocks
    Replace {
        /// Which XML tag this rule targets
        tag: String,
        /// Regex to match against block content
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
        /// Which XML tag this rule targets (e.g., "system-reminder", "aspy-context")
        tag: String,
        content: String,
        #[serde(default = "default_position")]
        position: PositionConfig,
    },
    Remove {
        /// Which XML tag this rule targets
        tag: String,
        pattern: String,
    },
    Replace {
        /// Which XML tag this rule targets
        tag: String,
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

/// Configuration for the TagEditor transformer
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TagEditorConfig {
    /// Whether the transformer is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Rules to apply (in order). Each rule specifies its target tag explicitly.
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
}

// ============================================================================
// TagEditor
// ============================================================================

/// Transformer that edits XML-style tags in user messages
///
/// Each rule explicitly specifies which tag it targets (e.g., `<system-reminder>`,
/// `<aspy-context>`). This allows fine-grained control over different tag types.
pub struct TagEditor {
    rules: Vec<TagRule>,
}

impl TagEditor {
    /// Create a new editor with the given rules
    /// Used by: Tests and programmatic rule construction (config uses from_config)
    #[allow(dead_code)]
    pub fn new(rules: Vec<TagRule>) -> Self {
        Self { rules }
    }

    /// Create from configuration
    pub fn from_config(config: &TagEditorConfig) -> anyhow::Result<Self> {
        let mut rules = Vec::with_capacity(config.rules.len());

        for rule_config in &config.rules {
            let rule = match rule_config {
                RuleConfig::Inject {
                    tag,
                    content,
                    position,
                } => {
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
                    TagRule::Inject {
                        tag: tag.clone(),
                        content: content.clone(),
                        position: pos,
                    }
                }
                RuleConfig::Remove { tag, pattern } => TagRule::Remove {
                    tag: tag.clone(),
                    pattern: Regex::new(pattern)?,
                },
                RuleConfig::Replace {
                    tag,
                    pattern,
                    replacement,
                } => TagRule::Replace {
                    tag: tag.clone(),
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

    /// Collect all unique tags referenced by rules
    fn collect_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self
            .rules
            .iter()
            .map(|r| match r {
                TagRule::Inject { tag, .. } => tag.clone(),
                TagRule::Remove { tag, .. } => tag.clone(),
                TagRule::Replace { tag, .. } => tag.clone(),
            })
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }

    /// Check if any Inject rules are configured
    fn has_inject_rules(&self) -> bool {
        self.rules
            .iter()
            .any(|r| matches!(r, TagRule::Inject { .. }))
    }

    /// Apply only Remove and Replace rules (not Inject)
    /// Used for non-last text blocks to clean up reminders without adding new ones
    fn apply_rules_remove_replace_only(&self, content: &str) -> Option<String> {
        let mut modified = false;
        let mut result = content.to_string();
        let tags = self.collect_tags();
        let mut blocks = parse_tagged_blocks(&result, &tags);

        // Apply Remove rules (only to blocks matching the rule's tag)
        for rule in &self.rules {
            if let TagRule::Remove { tag, pattern } = rule {
                let before_len = blocks.len();
                blocks.retain(|b| !(b.tag == *tag && pattern.is_match(&b.content)));
                if blocks.len() != before_len {
                    modified = true;
                }
            }
        }

        // Apply Replace rules (only to blocks matching the rule's tag)
        for rule in &self.rules {
            if let TagRule::Replace {
                tag,
                pattern,
                replacement,
            } = rule
            {
                for block in &mut blocks {
                    if block.tag == *tag && pattern.is_match(&block.content) {
                        let new_content = pattern.replace_all(&block.content, replacement);
                        if new_content != block.content {
                            block.content = new_content.into_owned();
                            modified = true;
                        }
                    }
                }
            }
        }

        if !modified {
            return None;
        }

        // Reconstruct text, preserving original tag for each block
        result = remove_all_tagged_blocks(&result, &tags);
        for block in &blocks {
            result.push_str(&format!("\n<{}>\n", block.tag));
            result.push_str(&block.content);
            result.push_str(&format!("\n</{}>", block.tag));
        }
        Some(result)
    }

    /// Apply all rules to the given text content
    fn apply_rules(&self, content: &str) -> Option<String> {
        let mut modified = false;
        let mut result = content.to_string();
        let tags = self.collect_tags();

        // Parse existing blocks for all tags referenced by rules
        let mut blocks = parse_tagged_blocks(&result, &tags);

        // Apply Remove rules first (only to blocks matching the rule's tag)
        for rule in &self.rules {
            if let TagRule::Remove { tag, pattern } = rule {
                let before_len = blocks.len();
                blocks.retain(|b| !(b.tag == *tag && pattern.is_match(&b.content)));
                if blocks.len() != before_len {
                    modified = true;
                }
            }
        }

        // Apply Replace rules (only to blocks matching the rule's tag)
        for rule in &self.rules {
            if let TagRule::Replace {
                tag,
                pattern,
                replacement,
            } = rule
            {
                for block in &mut blocks {
                    if block.tag == *tag && pattern.is_match(&block.content) {
                        let new_content = pattern.replace_all(&block.content, replacement);
                        if new_content != block.content {
                            block.content = new_content.into_owned();
                            modified = true;
                        }
                    }
                }
            }
        }

        // Apply Inject rules (each rule specifies its own tag)
        for rule in &self.rules {
            if let TagRule::Inject {
                tag,
                content,
                position,
            } = rule
            {
                let new_block = TaggedBlock {
                    content: content.clone(),
                    tag: tag.clone(),
                    original_start: 0,
                    original_end: 0,
                };

                let insert_idx = match position {
                    InjectPosition::Start => 0,
                    InjectPosition::End => blocks.len(),
                    InjectPosition::Before(pattern) => blocks
                        .iter()
                        .position(|b| pattern.is_match(&b.content))
                        .unwrap_or(blocks.len()),
                    InjectPosition::After(pattern) => blocks
                        .iter()
                        .rposition(|b| pattern.is_match(&b.content))
                        .map(|i| i + 1)
                        .unwrap_or(blocks.len()),
                };

                blocks.insert(insert_idx, new_block);
                modified = true;
            }
        }

        if modified {
            // Rebuild content: remove all original blocks and insert new ones
            result = remove_all_tagged_blocks(&result, &tags);

            // Insert all blocks at the end (they're in order), preserving original tags
            let block_text: String = blocks
                .iter()
                .map(|b| format!("<{}>\n{}\n</{}>", b.tag, b.content, b.tag))
                .collect::<Vec<_>>()
                .join("\n");

            if !block_text.is_empty() {
                result = format!("{}\n{}", result.trim_end(), block_text);
            }

            Some(result)
        } else {
            None
        }
    }
}

impl RequestTransformer for TagEditor {
    fn name(&self) -> &'static str {
        "tag-editor"
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

/// A parsed tagged block (e.g., system-reminder or aspy-context)
#[derive(Debug, Clone)]
struct TaggedBlock {
    /// The inner content (excluding tags)
    content: String,
    /// The tag name this block uses (e.g., "system-reminder")
    tag: String,
    /// Original start position in source text
    original_start: usize,
    /// Original end position in source text
    original_end: usize,
}

/// Parse all blocks matching any of the given tags from text
fn parse_tagged_blocks(text: &str, tags: &[String]) -> Vec<TaggedBlock> {
    let mut blocks = Vec::new();

    for tag in tags {
        let open_tag = format!("<{}>", tag);
        let close_tag = format!("</{}>", tag);
        let mut search_start = 0;

        while let Some(start) = text[search_start..].find(&open_tag) {
            let abs_start = search_start + start;
            let after_tag = abs_start + open_tag.len();

            if let Some(end_offset) = text[after_tag..].find(&close_tag) {
                let abs_end = after_tag + end_offset + close_tag.len();
                let inner_content = &text[after_tag..after_tag + end_offset];

                blocks.push(TaggedBlock {
                    content: inner_content.trim().to_string(),
                    tag: tag.clone(),
                    original_start: abs_start,
                    original_end: abs_end,
                });

                search_start = abs_end;
            } else {
                // Unclosed tag - skip past this start tag
                search_start = after_tag;
            }
        }
    }

    // Sort by position to maintain document order
    blocks.sort_by_key(|b| b.original_start);
    blocks
}

/// Remove all blocks matching any of the given tags from text
fn remove_all_tagged_blocks(text: &str, tags: &[String]) -> String {
    let blocks = parse_tagged_blocks(text, tags);
    if blocks.is_empty() {
        return text.to_string();
    }

    let mut result = String::with_capacity(text.len());
    let mut last_end = 0;

    for block in &blocks {
        // Add text before this block
        result.push_str(&text[last_end..block.original_start]);
        last_end = block.original_end;
    }

    // Add remaining text after last block
    result.push_str(&text[last_end..]);

    // Clean up extra newlines
    let result = result
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    result
}

// Legacy wrappers for tests (use system-reminder tag only)
#[cfg(test)]
fn parse_system_reminders(text: &str) -> Vec<TaggedBlock> {
    parse_tagged_blocks(text, &["system-reminder".to_string()])
}

#[cfg(test)]
fn remove_all_system_reminders(text: &str) -> String {
    remove_all_tagged_blocks(text, &["system-reminder".to_string()])
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
        let rules = vec![TagRule::Inject {
            tag: "system-reminder".to_string(),
            content: "Injected content".to_string(),
            position: InjectPosition::End,
        }];

        let editor = TagEditor::new(rules);
        let text = "Hello world";
        let result = editor.apply_rules(text).unwrap();

        assert!(result.contains("Injected content"));
        assert!(result.contains("<system-reminder>"));
    }

    #[test]
    fn test_remove_rule() {
        let rules = vec![TagRule::Remove {
            tag: "system-reminder".to_string(),
            pattern: Regex::new("noisy").unwrap(),
        }];

        let editor = TagEditor::new(rules);
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
        let rules = vec![TagRule::Replace {
            tag: "system-reminder".to_string(),
            pattern: Regex::new("old-url\\.com").unwrap(),
            replacement: "new-url.com".to_string(),
        }];

        let editor = TagEditor::new(rules);
        let text = r#"<system-reminder>
Visit old-url.com for docs
</system-reminder>"#;

        let result = editor.apply_rules(text).unwrap();
        assert!(result.contains("new-url.com"));
        assert!(!result.contains("old-url.com"));
    }

    #[test]
    fn test_transform_request_body() {
        let rules = vec![TagRule::Inject {
            tag: "system-reminder".to_string(),
            content: "Custom context".to_string(),
            position: InjectPosition::End,
        }];

        let editor = TagEditor::new(rules);
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
        let rules = vec![TagRule::Remove {
            tag: "system-reminder".to_string(),
            pattern: Regex::new("nonexistent").unwrap(),
        }];

        let editor = TagEditor::new(rules);
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
        let rules = vec![TagRule::Inject {
            tag: "system-reminder".to_string(),
            content: "Injected content".to_string(),
            position: InjectPosition::End,
        }];

        let editor = TagEditor::new(rules);
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
        let rules = vec![TagRule::Remove {
            tag: "system-reminder".to_string(),
            pattern: Regex::new("Learning output style").unwrap(),
        }];

        let editor = TagEditor::new(rules);
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
        let rules = vec![TagRule::Inject {
            tag: "system-reminder".to_string(),
            content: "Context info".to_string(),
            position: InjectPosition::End,
        }];

        let editor = TagEditor::new(rules);
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
        let rules = vec![TagRule::Inject {
            tag: "system-reminder".to_string(),
            content: "Added context".to_string(),
            position: InjectPosition::End,
        }];

        let editor = TagEditor::new(rules);
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
        let rules = vec![TagRule::Replace {
            tag: "system-reminder".to_string(),
            pattern: Regex::new("old-value").unwrap(),
            replacement: "new-value".to_string(),
        }];

        let editor = TagEditor::new(rules);
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

    #[test]
    fn test_inject_start_position() {
        // Test InjectPosition::Start - inject at the very beginning
        let rules = vec![TagRule::Inject {
            tag: "system-reminder".to_string(),
            content: "Injected at start".to_string(),
            position: InjectPosition::Start,
        }];

        let editor = TagEditor::new(rules);
        let text = r#"Some text
<system-reminder>
Existing reminder
</system-reminder>"#;

        let result = editor.apply_rules(text).unwrap();

        // Should have both reminders
        assert!(result.contains("Injected at start"));
        assert!(result.contains("Existing reminder"));

        // Injected should come before existing
        let injected_pos = result.find("Injected at start").unwrap();
        let existing_pos = result.find("Existing reminder").unwrap();
        assert!(
            injected_pos < existing_pos,
            "Start injection should appear before existing reminders"
        );
    }

    #[test]
    fn test_inject_before_position() {
        // Test InjectPosition::Before - inject before matching reminder
        let rules = vec![TagRule::Inject {
            tag: "system-reminder".to_string(),
            content: "Injected before".to_string(),
            position: InjectPosition::Before(Regex::new("existing").unwrap()),
        }];

        let editor = TagEditor::new(rules);
        let text = r#"Some text
<system-reminder>
This is existing content
</system-reminder>"#;

        let result = editor.apply_rules(text).unwrap();

        // Should have both reminders
        assert!(result.contains("Injected before"));
        assert!(result.contains("existing content"));

        // Injected should come before existing
        let injected_pos = result.find("Injected before").unwrap();
        let existing_pos = result.find("existing content").unwrap();
        assert!(
            injected_pos < existing_pos,
            "Injected content should appear before existing"
        );
    }

    #[test]
    fn test_inject_after_position() {
        // Test InjectPosition::After - inject after matching reminder
        let rules = vec![TagRule::Inject {
            tag: "system-reminder".to_string(),
            content: "Injected after".to_string(),
            position: InjectPosition::After(Regex::new("existing").unwrap()),
        }];

        let editor = TagEditor::new(rules);
        let text = r#"Some text
<system-reminder>
This is existing content
</system-reminder>"#;

        let result = editor.apply_rules(text).unwrap();

        // Should have both reminders
        assert!(result.contains("Injected after"));
        assert!(result.contains("existing content"));

        // Injected should come after existing
        let injected_pos = result.find("Injected after").unwrap();
        let existing_pos = result.find("existing content").unwrap();
        assert!(
            injected_pos > existing_pos,
            "Injected content should appear after existing"
        );
    }

    // ============================================================================
    // Per-Rule Tag Tests
    // ============================================================================

    #[test]
    fn test_remove_only_affects_matching_tag() {
        // A Remove rule for "system-reminder" should NOT affect "aspy-context" blocks
        let rules = vec![TagRule::Remove {
            tag: "system-reminder".to_string(),
            pattern: Regex::new(".*").unwrap(), // Match everything
        }];

        let editor = TagEditor::new(rules);
        let text = r#"Some text
<system-reminder>
Should be removed
</system-reminder>
<aspy-context>
Should stay
</aspy-context>"#;

        let result = editor.apply_rules(text).unwrap();

        // system-reminder content should be gone
        assert!(
            !result.contains("Should be removed"),
            "system-reminder content should be removed"
        );
        // aspy-context should remain untouched (note: it won't appear in result
        // because only tags referenced by rules are tracked)
        // The text without any referenced tags should still be there
        assert!(result.contains("Some text"));
    }

    #[test]
    fn test_replace_only_affects_matching_tag() {
        // A Replace rule for "aspy-context" should NOT modify "system-reminder" blocks
        let rules = vec![TagRule::Replace {
            tag: "aspy-context".to_string(),
            pattern: Regex::new("old").unwrap(),
            replacement: "new".to_string(),
        }];

        let editor = TagEditor::new(rules);
        let text = r#"<system-reminder>
Contains old value - should stay unchanged
</system-reminder>
<aspy-context>
Contains old value - should become new
</aspy-context>"#;

        let result = editor.apply_rules(text).unwrap();

        // system-reminder should still have "old"
        assert!(
            result.contains("Contains old value - should stay unchanged"),
            "system-reminder should not be modified by aspy-context rule"
        );
        // aspy-context should have "new"
        assert!(
            result.contains("Contains new value - should become new"),
            "aspy-context should be modified"
        );
    }

    #[test]
    fn test_inject_creates_block_with_specified_tag() {
        // Inject rule should create block with its specified tag
        let rules = vec![TagRule::Inject {
            tag: "custom-tag".to_string(),
            content: "Custom content".to_string(),
            position: InjectPosition::End,
        }];

        let editor = TagEditor::new(rules);
        let text = "Hello world";
        let result = editor.apply_rules(text).unwrap();

        assert!(result.contains("<custom-tag>"), "Should create opening tag");
        assert!(
            result.contains("</custom-tag>"),
            "Should create closing tag"
        );
        assert!(
            result.contains("Custom content"),
            "Should contain injected content"
        );
        assert!(
            !result.contains("<system-reminder>"),
            "Should NOT create system-reminder tag"
        );
    }

    #[test]
    fn test_multiple_rules_different_tags() {
        // Multiple rules targeting different tags should all work independently
        let rules = vec![
            TagRule::Remove {
                tag: "noisy-tag".to_string(),
                pattern: Regex::new(".*").unwrap(),
            },
            TagRule::Replace {
                tag: "config-tag".to_string(),
                pattern: Regex::new("v1").unwrap(),
                replacement: "v2".to_string(),
            },
            TagRule::Inject {
                tag: "aspy-context".to_string(),
                content: "Injected context".to_string(),
                position: InjectPosition::End,
            },
        ];

        let editor = TagEditor::new(rules);
        let text = r#"User message
<noisy-tag>
Remove this noise
</noisy-tag>
<config-tag>
Version: v1
</config-tag>"#;

        let result = editor.apply_rules(text).unwrap();

        // noisy-tag content removed
        assert!(
            !result.contains("Remove this noise"),
            "noisy-tag content should be removed"
        );
        // config-tag updated
        assert!(
            result.contains("v2"),
            "config-tag should have v1 replaced with v2"
        );
        assert!(!result.contains("v1"), "v1 should be replaced");
        // aspy-context injected
        assert!(
            result.contains("<aspy-context>"),
            "Should have aspy-context tag"
        );
        assert!(
            result.contains("Injected context"),
            "Should have injected content"
        );
    }

    #[test]
    fn test_same_pattern_different_tags() {
        // Same pattern in rules targeting different tags should work independently
        let rules = vec![
            TagRule::Remove {
                tag: "tag-a".to_string(),
                pattern: Regex::new("remove-me").unwrap(),
            },
            TagRule::Remove {
                tag: "tag-b".to_string(),
                pattern: Regex::new("keep-me").unwrap(), // Different pattern for tag-b
            },
        ];

        let editor = TagEditor::new(rules);
        let text = r#"<tag-a>
Content with remove-me marker
</tag-a>
<tag-a>
Safe content without marker
</tag-a>
<tag-b>
Content with remove-me marker stays
</tag-b>
<tag-b>
Content with keep-me gets removed
</tag-b>"#;

        let result = editor.apply_rules(text).unwrap();

        // tag-a with "remove-me" should be gone
        assert!(
            !result.contains("Content with remove-me marker\n</tag-a>"),
            "tag-a with remove-me should be removed"
        );
        // tag-a without marker should stay
        assert!(
            result.contains("Safe content without marker"),
            "tag-a without marker should stay"
        );
        // tag-b with "remove-me" should stay (different rule)
        assert!(
            result.contains("Content with remove-me marker stays"),
            "tag-b with remove-me should stay (not matching tag-b rule)"
        );
        // tag-b with "keep-me" should be removed
        assert!(
            !result.contains("Content with keep-me gets removed"),
            "tag-b with keep-me should be removed"
        );
    }

    #[test]
    fn test_config_parsing_per_rule_tag() {
        // Verify config parsing correctly extracts per-rule tags
        let config = TagEditorConfig {
            enabled: true,
            rules: vec![
                RuleConfig::Inject {
                    tag: "custom-inject".to_string(),
                    content: "Injected".to_string(),
                    position: PositionConfig::End,
                },
                RuleConfig::Remove {
                    tag: "custom-remove".to_string(),
                    pattern: ".*".to_string(),
                },
                RuleConfig::Replace {
                    tag: "custom-replace".to_string(),
                    pattern: "old".to_string(),
                    replacement: "new".to_string(),
                },
            ],
        };

        let editor = TagEditor::from_config(&config).unwrap();
        assert_eq!(editor.rule_count(), 3);

        // Verify collect_tags returns all unique tags
        let tags = editor.collect_tags();
        assert!(tags.contains(&"custom-inject".to_string()));
        assert!(tags.contains(&"custom-remove".to_string()));
        assert!(tags.contains(&"custom-replace".to_string()));
    }
}
