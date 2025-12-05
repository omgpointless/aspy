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
//!
//! # Conditions
//!
//! Rules can have optional `when` conditions that must be met for the rule to apply:
//! - `turn_number`: Match on conversation turn (e.g., "=1", ">5", "every:3")
//! - `has_tool_results`: Match on tool result count (e.g., ">0", "=0")
//! - `client_id`: Match on client ID (e.g., "dev-1", "foundry|local")

use super::{RequestTransformer, TransformContext, TransformResult};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

// ============================================================================
// Condition Types
// ============================================================================

/// Conditions that must be met for a rule to apply
///
/// Multiple conditions in the same WhenCondition are AND'd together.
/// Pipe-separated values within a condition are OR'd (e.g., "dev-1|foundry").
#[derive(Debug, Clone, Deserialize, Default)]
pub struct WhenCondition {
    /// Turn number condition: "=1", ">5", "<10", "every:3"
    #[serde(default)]
    pub turn_number: Option<String>,

    /// Tool results condition: ">0", "=0", ">3"
    #[serde(default)]
    pub has_tool_results: Option<String>,

    /// Client ID condition: "dev-1", "foundry|local" (pipe = OR)
    #[serde(default)]
    pub client_id: Option<String>,
}

impl WhenCondition {
    /// Check if all conditions are met
    pub fn evaluate(&self, ctx: &TransformContext) -> bool {
        let turn_ok = self.check_turn_number(ctx);
        let tools_ok = self.check_tool_results(ctx);
        let client_ok = self.check_client_id(ctx);
        let result = turn_ok && tools_ok && client_ok;

        tracing::debug!(
            turn_cond = ?self.turn_number,
            actual_turn = ctx.turn_number,
            turn_ok,
            tools_cond = ?self.has_tool_results,
            actual_tools = ctx.tool_result_count,
            tools_ok,
            result,
            "Condition evaluation"
        );

        result
    }

    fn check_turn_number(&self, ctx: &TransformContext) -> bool {
        let Some(ref condition) = self.turn_number else {
            return true;
        };
        let Some(turn) = ctx.turn_number else {
            return true; // No turn info = pass (permissive)
        };
        parse_numeric_condition(condition, turn)
    }

    fn check_tool_results(&self, ctx: &TransformContext) -> bool {
        let Some(ref condition) = self.has_tool_results else {
            return true;
        };
        let count = ctx.tool_result_count.unwrap_or(0) as u64;
        parse_numeric_condition(condition, count)
    }

    fn check_client_id(&self, ctx: &TransformContext) -> bool {
        let Some(ref condition) = self.client_id else {
            return true;
        };
        let Some(client) = ctx.client_id else {
            return true;
        };
        // Pipe-separated = OR
        condition.split('|').any(|c| c.trim() == client)
    }
}

/// Parse numeric conditions like "=1", ">5", "<10", "every:3"
fn parse_numeric_condition(condition: &str, value: u64) -> bool {
    let condition = condition.trim();

    if let Some(n) = condition.strip_prefix("every:") {
        if let Ok(interval) = n.parse::<u64>() {
            return interval > 0 && value.is_multiple_of(interval);
        }
        return true; // Invalid = pass
    }

    if let Some(n) = condition.strip_prefix(">=") {
        return n.parse::<u64>().map(|n| value >= n).unwrap_or(true);
    }
    if let Some(n) = condition.strip_prefix("<=") {
        return n.parse::<u64>().map(|n| value <= n).unwrap_or(true);
    }
    if let Some(n) = condition.strip_prefix('>') {
        return n.parse::<u64>().map(|n| value > n).unwrap_or(true);
    }
    if let Some(n) = condition.strip_prefix('<') {
        return n.parse::<u64>().map(|n| value < n).unwrap_or(true);
    }
    if let Some(n) = condition.strip_prefix('=') {
        return n.parse::<u64>().map(|n| value == n).unwrap_or(true);
    }

    // Plain number = equals
    condition.parse::<u64>().map(|n| value == n).unwrap_or(true)
}

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
        /// Optional conditions for when this rule applies
        when: Option<WhenCondition>,
    },
    /// Remove all blocks matching pattern
    Remove {
        /// Which XML tag this rule targets
        tag: String,
        /// Regex to match against block content
        pattern: Regex,
        /// Optional conditions for when this rule applies
        when: Option<WhenCondition>,
    },
    /// Replace content within matching blocks
    Replace {
        /// Which XML tag this rule targets
        tag: String,
        /// Regex to match against block content
        pattern: Regex,
        /// Replacement string (supports $1, $2 capture groups)
        replacement: String,
        /// Optional conditions for when this rule applies
        when: Option<WhenCondition>,
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
        /// Optional conditions for when this rule applies
        #[serde(default)]
        when: Option<WhenCondition>,
    },
    Remove {
        /// Which XML tag this rule targets
        tag: String,
        pattern: String,
        /// Optional conditions for when this rule applies
        #[serde(default)]
        when: Option<WhenCondition>,
    },
    Replace {
        /// Which XML tag this rule targets
        tag: String,
        pattern: String,
        replacement: String,
        /// Optional conditions for when this rule applies
        #[serde(default)]
        when: Option<WhenCondition>,
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
                    when,
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
                    let content_preview: String = content.chars().take(50).collect();
                    tracing::debug!(
                        tag = %tag,
                        content_preview = %content_preview,
                        position = ?pos,
                        has_when = when.is_some(),
                        "Loaded Inject rule: tag={} position={:?} content_preview={} has_when={}",
                        tag,
                        pos,
                        content_preview,
                        when.is_some()
                    );
                    TagRule::Inject {
                        tag: tag.clone(),
                        content: content.clone(),
                        position: pos,
                        when: when.clone(),
                    }
                }
                RuleConfig::Remove { tag, pattern, when } => {
                    tracing::debug!(
                        tag = %tag,
                        pattern = %pattern,
                        has_when = when.is_some(),
                        "Loaded Remove rule: tag={} pattern={} has_when={}",
                        tag,
                        pattern,
                        when.is_some()
                    );
                    TagRule::Remove {
                        tag: tag.clone(),
                        pattern: Regex::new(pattern)?,
                        when: when.clone(),
                    }
                }
                RuleConfig::Replace {
                    tag,
                    pattern,
                    replacement,
                    when,
                } => TagRule::Replace {
                    tag: tag.clone(),
                    pattern: Regex::new(pattern)?,
                    replacement: replacement.clone(),
                    when: when.clone(),
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

    /// Count rule types for status reporting
    fn rule_type_counts(&self) -> (usize, usize, usize) {
        let mut remove = 0;
        let mut replace = 0;
        let mut inject = 0;
        for rule in &self.rules {
            match rule {
                TagRule::Remove { .. } => remove += 1,
                TagRule::Replace { .. } => replace += 1,
                TagRule::Inject { .. } => inject += 1,
            }
        }
        (remove, replace, inject)
    }

    /// Apply only Remove and Replace rules (not Inject)
    /// Used for non-last text blocks to clean up reminders without adding new ones
    fn apply_rules_remove_replace_only(
        &self,
        content: &str,
        ctx: &TransformContext,
    ) -> Option<String> {
        let mut modified = false;
        let mut result = content.to_string();
        let tags = self.collect_tags();
        let mut blocks = parse_tagged_blocks(&result, &tags);

        tracing::debug!(
            tags = ?tags,
            block_count = blocks.len(),
            "apply_rules_remove_replace_only: parsed blocks {:?}",
            blocks.iter().map(|b| (&b.tag, b.content.chars().take(50).collect::<String>())).collect::<Vec<_>>()
        );

        // Apply Remove rules (only to blocks matching the rule's tag)
        for rule in &self.rules {
            if let TagRule::Remove { tag, pattern, when } = rule {
                tracing::debug!(
                    rule_tag = %tag,
                    rule_pattern = %pattern,
                    has_when = when.is_some(),
                    "Processing Remove rule"
                );
                // Check conditions
                if let Some(ref cond) = when {
                    if !cond.evaluate(ctx) {
                        tracing::debug!("Remove rule skipped: condition not met");
                        continue;
                    }
                }
                let before_len = blocks.len();
                blocks.retain(|b| !(b.tag == *tag && pattern.is_match(&b.content)));
                if blocks.len() != before_len {
                    tracing::debug!(removed = before_len - blocks.len(), "Blocks removed");
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
                when,
            } = rule
            {
                // Check conditions
                if let Some(ref cond) = when {
                    if !cond.evaluate(ctx) {
                        continue;
                    }
                }
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
    fn apply_rules(&self, content: &str, ctx: &TransformContext) -> Option<String> {
        let mut modified = false;
        let mut result = content.to_string();
        let tags = self.collect_tags();

        // Parse existing blocks for all tags referenced by rules
        let mut blocks = parse_tagged_blocks(&result, &tags);

        tracing::debug!(
            tags = ?tags,
            block_count = blocks.len(),
            blocks_summary = ?blocks.iter().map(|b| (&b.tag, b.content.chars().take(50).collect::<String>())).collect::<Vec<_>>(),
            "apply_rules: parsed blocks {:?}",
            blocks.iter().map(|b| (&b.tag, b.content.chars().take(50).collect::<String>())).collect::<Vec<_>>()
        );

        // Apply Remove rules first (only to blocks matching the rule's tag)
        for rule in &self.rules {
            if let TagRule::Remove { tag, pattern, when } = rule {
                tracing::debug!(
                    rule_tag = %tag,
                    rule_pattern = %pattern,
                    has_when = when.is_some(),
                    "Processing Remove rule (apply_rules) tag={:?} pattern={:?} when={:?}",
                    tag,
                    pattern,
                    when
                );
                // Check conditions
                if let Some(ref cond) = when {
                    if !cond.evaluate(ctx) {
                        tracing::debug!("Remove rule skipped: condition not met for tag={:?} pattern={:?} when={:?}",
                            tag,
                            pattern,
                            when
                        );
                        continue;
                    }
                }
                let before_len = blocks.len();
                blocks.retain(|b| {
                    let tag_matches = b.tag == *tag;
                    let pattern_matches = pattern.is_match(&b.content);
                    let should_remove = tag_matches && pattern_matches;
                    if tag_matches {
                        tracing::debug!(
                            block_tag = %b.tag,
                            rule_tag = %tag,
                            content_preview = %b.content.chars().take(80).collect::<String>(),
                            pattern = %pattern,
                            pattern_matches = pattern_matches,
                            should_remove = should_remove,
                            "Remove rule: checking block tag={:?} pattern={:?} when={:?}",
                            b.tag,
                            pattern,
                            when
                        );
                    }
                    !should_remove
                });
                if blocks.len() != before_len {
                    tracing::debug!(
                        removed = before_len - blocks.len(),
                        "Blocks removed by Remove rule: {} blocks removed for tag={:?} pattern={:?} when={:?}",
                        before_len - blocks.len(),
                        tag,
                        pattern,
                        when
                    );
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
                when,
            } = rule
            {
                // Check conditions
                if let Some(ref cond) = when {
                    if !cond.evaluate(ctx) {
                        tracing::debug!("Replace rule skipped: condition not met for tag={:?} pattern={:?} when={:?}",
                            tag,
                            pattern,
                            when
                        );
                        continue;
                    }
                }
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
                when,
            } = rule
            {
                // Check conditions
                if let Some(ref cond) = when {
                    if !cond.evaluate(ctx) {
                        tracing::debug!("Inject rule skipped: condition not met for tag={:?} content={:?} position={:?} when={:?}",
                            tag,
                            content,
                            position,
                            when
                        );
                        continue;
                    }
                }
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

    fn transform(&self, body: &Value, ctx: &TransformContext) -> TransformResult {
        if self.rules.is_empty() {
            return TransformResult::Unchanged;
        }

        tracing::debug!(
            turn = ctx.turn_number,
            tool_results = ctx.tool_result_count,
            client = ctx.client_id,
            rules = self.rules.len(),
            "TagEditor::transform called: turn={:?} tool_results={:?} client={:?} rules={}",
            ctx.turn_number,
            ctx.tool_result_count,
            ctx.client_id,
            self.rules.len()
        );

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
                if let Some(new_text) = self.apply_rules(&original, ctx) {
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

                tracing::trace!(
                    text_block_count = text_block_indices.len(),
                    text_block_indices = ?text_block_indices,
                    last_text_idx = ?last_text_idx,
                    total_content_blocks = content_arr.len(),
                    "Processing Array content structure: {} text blocks, {} total blocks, last_text_idx={:?}",
                    text_block_indices.len(),
                    content_arr.len(),
                    last_text_idx
                );

                // Track indices of text blocks that become empty (to delete later)
                let mut empty_block_indices: Vec<usize> = Vec::new();

                // Apply rules to ALL text blocks (Remove/Replace scan all, Inject only last)
                for &idx in &text_block_indices {
                    if let Some(block) = content_arr.get_mut(idx) {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            let is_last = Some(idx) == last_text_idx;
                            let new_text = if is_last {
                                self.apply_rules(text, ctx)
                            } else {
                                self.apply_rules_remove_replace_only(text, ctx)
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
                        "Removed empty text block after transformation at index {}",
                        idx
                    );
                }

                // ─────────────────────────────────────────────────────────────
                // Process tool_result content fields (Remove/Replace only)
                // ─────────────────────────────────────────────────────────────
                // Claude Code injects <system-reminder> tags into tool_result
                // content fields, not just text blocks. We need to scan those too.
                for (idx, block) in content_arr.iter_mut().enumerate() {
                    if block.get("type").and_then(|t| t.as_str()) != Some("tool_result") {
                        continue;
                    }

                    // Handle string content
                    if let Some(content_str) = block
                        .get("content")
                        .and_then(|c| c.as_str())
                        .map(String::from)
                    {
                        if let Some(new_content) =
                            self.apply_rules_remove_replace_only(&content_str, ctx)
                        {
                            // Don't remove tool_result even if content becomes empty
                            let final_content = if new_content.trim().is_empty() {
                                " ".to_string() // Minimal valid content
                            } else {
                                new_content
                            };
                            block["content"] = Value::String(final_content);
                            any_modified = true;
                            tracing::debug!(
                                tool_result_idx = idx,
                                "Applied Remove/Replace rules to tool_result content at index {}",
                                idx
                            );
                        }
                    }
                    // Handle array content (nested text blocks within tool_result)
                    else if let Some(nested_arr) =
                        block.get("content").and_then(|c| c.as_array()).cloned()
                    {
                        let mut nested_modified = false;
                        let mut new_nested: Vec<Value> = Vec::with_capacity(nested_arr.len());

                        for nested_block in nested_arr {
                            if nested_block.get("type").and_then(|t| t.as_str()) == Some("text") {
                                if let Some(text) =
                                    nested_block.get("text").and_then(|t| t.as_str())
                                {
                                    if let Some(new_text) =
                                        self.apply_rules_remove_replace_only(text, ctx)
                                    {
                                        nested_modified = true;
                                        if !new_text.trim().is_empty() {
                                            let mut updated = nested_block.clone();
                                            updated["text"] = Value::String(new_text);
                                            new_nested.push(updated);
                                        }
                                        // Skip empty text blocks (don't add to new_nested)
                                        continue;
                                    }
                                }
                            }
                            new_nested.push(nested_block);
                        }

                        if nested_modified {
                            block["content"] = Value::Array(new_nested);
                            any_modified = true;
                            tracing::debug!(
                                tool_result_idx = idx,
                                "Applied Remove/Replace rules to nested tool_result content at index {}",
                                idx
                            );
                        }
                    }
                }

                // If no text blocks but we have Inject rules, append one
                if text_block_indices.is_empty() && self.has_inject_rules() {
                    if let Some(new_text) = self.apply_rules("", ctx) {
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
            tracing::debug!("TagEditor: no modifications made, returning Unchanged");
            return TransformResult::Unchanged;
        }

        let (remove_count, replace_count, inject_count) = self.rule_type_counts();
        tracing::info!(
            rules = self.rules.len(),
            remove_rules = remove_count,
            replace_rules = replace_count,
            inject_rules = inject_count,
            "TagEditor: applied transformations successfully with {} rules ({} Remove, {} Replace, {} Inject)",
            self.rules.len(),
            remove_count,
            replace_count,
            inject_count
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

        while let Some(slice) = text.get(search_start..) {
            let Some(start) = slice.find(&open_tag) else {
                break;
            };
            let abs_start = search_start + start;
            let after_tag = abs_start + open_tag.len();

            let Some(search_slice) = text.get(after_tag..) else {
                break;
            };

            if let Some(end_offset) = search_slice.find(&close_tag) {
                let abs_end = after_tag + end_offset + close_tag.len();
                let inner_content = text
                    .get(after_tag..after_tag + end_offset)
                    .unwrap_or_default();

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
        // Add text before this block (use get() for UTF-8 safety)
        if let Some(before) = text.get(last_end..block.original_start) {
            result.push_str(before);
        }
        last_end = block.original_end;
    }

    // Add remaining text after last block
    if let Some(after) = text.get(last_end..) {
        result.push_str(after);
    }

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

    // Helper to create a default test context
    fn test_ctx() -> TransformContext<'static> {
        TransformContext::new(None, "/v1/messages", Some("claude-3"))
    }

    #[test]
    fn test_inject_rule() {
        let rules = vec![TagRule::Inject {
            tag: "system-reminder".to_string(),
            content: "Injected content".to_string(),
            position: InjectPosition::End,
            when: None,
        }];

        let editor = TagEditor::new(rules);
        let text = "Hello world";
        let ctx = test_ctx();
        let result = editor.apply_rules(text, &ctx).unwrap();

        assert!(result.contains("Injected content"));
        assert!(result.contains("<system-reminder>"));
    }

    #[test]
    fn test_remove_rule() {
        let rules = vec![TagRule::Remove {
            tag: "system-reminder".to_string(),
            pattern: Regex::new("noisy").unwrap(),
            when: None,
        }];

        let editor = TagEditor::new(rules);
        let text = r#"Keep this
<system-reminder>
This is noisy and should be removed
</system-reminder>
<system-reminder>
This should stay
</system-reminder>"#;

        let ctx = test_ctx();
        let result = editor.apply_rules(text, &ctx).unwrap();
        assert!(!result.contains("noisy"));
        assert!(result.contains("This should stay"));
    }

    #[test]
    fn test_replace_rule() {
        let rules = vec![TagRule::Replace {
            tag: "system-reminder".to_string(),
            pattern: Regex::new("old-url\\.com").unwrap(),
            replacement: "new-url.com".to_string(),
            when: None,
        }];

        let editor = TagEditor::new(rules);
        let text = r#"<system-reminder>
Visit old-url.com for docs
</system-reminder>"#;

        let ctx = test_ctx();
        let result = editor.apply_rules(text, &ctx).unwrap();
        assert!(result.contains("new-url.com"));
        assert!(!result.contains("old-url.com"));
    }

    #[test]
    fn test_transform_request_body() {
        let rules = vec![TagRule::Inject {
            tag: "system-reminder".to_string(),
            content: "Custom context".to_string(),
            position: InjectPosition::End,
            when: None,
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
            when: None,
        }];

        let editor = TagEditor::new(rules);
        let text = "Just plain text without any matching content";
        let ctx = test_ctx();

        assert!(editor.apply_rules(text, &ctx).is_none());
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
            when: None,
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
            when: None,
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
            when: None,
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
            when: None,
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
            when: None,
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
            when: None,
        }];

        let editor = TagEditor::new(rules);
        let text = r#"Some text
<system-reminder>
Existing reminder
</system-reminder>"#;

        let ctx = test_ctx();
        let result = editor.apply_rules(text, &ctx).unwrap();

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
            when: None,
        }];

        let editor = TagEditor::new(rules);
        let text = r#"Some text
<system-reminder>
This is existing content
</system-reminder>"#;

        let ctx = test_ctx();
        let result = editor.apply_rules(text, &ctx).unwrap();

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
            when: None,
        }];

        let editor = TagEditor::new(rules);
        let text = r#"Some text
<system-reminder>
This is existing content
</system-reminder>"#;

        let ctx = test_ctx();
        let result = editor.apply_rules(text, &ctx).unwrap();

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
            when: None,
        }];

        let editor = TagEditor::new(rules);
        let text = r#"Some text
<system-reminder>
Should be removed
</system-reminder>
<aspy-context>
Should stay
</aspy-context>"#;

        let ctx = test_ctx();
        let result = editor.apply_rules(text, &ctx).unwrap();

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
            when: None,
        }];

        let editor = TagEditor::new(rules);
        let text = r#"<system-reminder>
Contains old value - should stay unchanged
</system-reminder>
<aspy-context>
Contains old value - should become new
</aspy-context>"#;

        let ctx = test_ctx();
        let result = editor.apply_rules(text, &ctx).unwrap();

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
            when: None,
        }];

        let editor = TagEditor::new(rules);
        let text = "Hello world";
        let ctx = test_ctx();
        let result = editor.apply_rules(text, &ctx).unwrap();

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
                when: None,
            },
            TagRule::Replace {
                tag: "config-tag".to_string(),
                pattern: Regex::new("v1").unwrap(),
                replacement: "v2".to_string(),
                when: None,
            },
            TagRule::Inject {
                tag: "aspy-context".to_string(),
                content: "Injected context".to_string(),
                position: InjectPosition::End,
                when: None,
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

        let ctx = test_ctx();
        let result = editor.apply_rules(text, &ctx).unwrap();

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
                when: None,
            },
            TagRule::Remove {
                tag: "tag-b".to_string(),
                pattern: Regex::new("keep-me").unwrap(), // Different pattern for tag-b
                when: None,
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

        let ctx = test_ctx();
        let result = editor.apply_rules(text, &ctx).unwrap();

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
                    when: None,
                },
                RuleConfig::Remove {
                    tag: "custom-remove".to_string(),
                    pattern: ".*".to_string(),
                    when: None,
                },
                RuleConfig::Replace {
                    tag: "custom-replace".to_string(),
                    pattern: "old".to_string(),
                    replacement: "new".to_string(),
                    when: None,
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

    // ============================================================================
    // Tool Result Content Scanning Tests
    // ============================================================================

    #[test]
    fn test_remove_rule_scans_tool_result_string_content() {
        // Critical test: Remove rules should scan tool_result content fields
        // This fixes the bug where <system-reminder> in tool_result was ignored
        let rules = vec![TagRule::Remove {
            tag: "system-reminder".to_string(),
            pattern: Regex::new("Learning output style").unwrap(),
            when: None,
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
                            "tool_use_id": "toolu_123",
                            "content": "<system-reminder>\nLearning output style is active\n</system-reminder>\nFile contents here: fn main() {}"
                        }
                    ]
                }
            ]
        });

        let ctx = TransformContext::new(None, "/v1/messages", Some("claude-3"));

        match editor.transform(&body, &ctx) {
            TransformResult::Modified(new_body) => {
                let content = new_body["messages"][0]["content"][0]["content"]
                    .as_str()
                    .unwrap();
                assert!(
                    !content.contains("Learning output style"),
                    "Remove rule should scan tool_result string content"
                );
                assert!(
                    content.contains("fn main()"),
                    "Non-matching content should be preserved"
                );
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_remove_rule_scans_tool_result_array_content() {
        // Test nested array content within tool_result
        let rules = vec![TagRule::Remove {
            tag: "system-reminder".to_string(),
            pattern: Regex::new("Learning output style").unwrap(),
            when: None,
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
                            "tool_use_id": "toolu_123",
                            "content": [
                                {
                                    "type": "text",
                                    "text": "<system-reminder>\nLearning output style is active\n</system-reminder>"
                                },
                                {
                                    "type": "text",
                                    "text": "Actual tool output here"
                                }
                            ]
                        }
                    ]
                }
            ]
        });

        let ctx = TransformContext::new(None, "/v1/messages", Some("claude-3"));

        match editor.transform(&body, &ctx) {
            TransformResult::Modified(new_body) => {
                let nested_content = new_body["messages"][0]["content"][0]["content"]
                    .as_array()
                    .unwrap();
                // The system-reminder text block should be removed (was only reminder)
                // The "Actual tool output" text block should remain
                let all_text: String = nested_content
                    .iter()
                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ");
                assert!(
                    !all_text.contains("Learning output style"),
                    "Remove rule should scan nested tool_result array content"
                );
                assert!(
                    all_text.contains("Actual tool output"),
                    "Non-matching nested content should be preserved"
                );
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_multiple_tool_results_all_scanned() {
        // All tool_result blocks should be scanned, not just the first/last
        let rules = vec![TagRule::Remove {
            tag: "system-reminder".to_string(),
            pattern: Regex::new("noisy").unwrap(),
            when: None,
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
                            "tool_use_id": "toolu_1",
                            "content": "<system-reminder>noisy reminder 1</system-reminder>\nFirst result"
                        },
                        {
                            "type": "tool_result",
                            "tool_use_id": "toolu_2",
                            "content": "<system-reminder>noisy reminder 2</system-reminder>\nSecond result"
                        },
                        {
                            "type": "tool_result",
                            "tool_use_id": "toolu_3",
                            "content": "<system-reminder>noisy reminder 3</system-reminder>\nThird result"
                        }
                    ]
                }
            ]
        });

        let ctx = TransformContext::new(None, "/v1/messages", Some("claude-3"));

        match editor.transform(&body, &ctx) {
            TransformResult::Modified(new_body) => {
                let content_arr = new_body["messages"][0]["content"].as_array().unwrap();
                for (i, block) in content_arr.iter().enumerate() {
                    let text = block["content"].as_str().unwrap();
                    assert!(
                        !text.contains("noisy"),
                        "tool_result {} should have reminder removed",
                        i
                    );
                    assert!(
                        text.contains("result"),
                        "tool_result {} should preserve other content",
                        i
                    );
                }
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_tool_result_with_condition() {
        // Test that conditional rules work with tool_result content
        let rules = vec![TagRule::Remove {
            tag: "system-reminder".to_string(),
            pattern: Regex::new("Learning output style").unwrap(),
            when: Some(WhenCondition {
                turn_number: Some(">2".to_string()),
                has_tool_results: None,
                client_id: None,
            }),
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
                            "tool_use_id": "toolu_123",
                            "content": "<system-reminder>\nLearning output style is active\n</system-reminder>\nOutput"
                        }
                    ]
                }
            ]
        });

        // Test with turn_number = 2 (condition not met, should NOT remove)
        let mut ctx = TransformContext::new(None, "/v1/messages", Some("claude-3"));
        ctx.turn_number = Some(2);

        match editor.transform(&body, &ctx) {
            TransformResult::Unchanged => {} // Expected - condition not met
            other => panic!("Expected Unchanged when turn=2, got {:?}", other),
        }

        // Test with turn_number = 3 (condition met, SHOULD remove)
        ctx.turn_number = Some(3);

        match editor.transform(&body, &ctx) {
            TransformResult::Modified(new_body) => {
                let content = new_body["messages"][0]["content"][0]["content"]
                    .as_str()
                    .unwrap();
                assert!(
                    !content.contains("Learning output style"),
                    "Should remove when turn > 2"
                );
            }
            other => panic!("Expected Modified when turn=3, got {:?}", other),
        }
    }

    #[test]
    fn test_mixed_text_and_tool_result_blocks() {
        // Real-world scenario: text blocks and tool_results interleaved
        let rules = vec![TagRule::Remove {
            tag: "system-reminder".to_string(),
            pattern: Regex::new("Learning output style").unwrap(),
            when: None,
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
                            "text": "Here's what I found\n<system-reminder>\nLearning output style is active\n</system-reminder>"
                        },
                        {
                            "type": "tool_result",
                            "tool_use_id": "toolu_1",
                            "content": "<system-reminder>\nLearning output style is active\n</system-reminder>\nFile A contents"
                        },
                        {
                            "type": "tool_result",
                            "tool_use_id": "toolu_2",
                            "content": "<system-reminder>\nLearning output style is active\n</system-reminder>\nFile B contents"
                        }
                    ]
                }
            ]
        });

        let ctx = TransformContext::new(None, "/v1/messages", Some("claude-3"));

        match editor.transform(&body, &ctx) {
            TransformResult::Modified(new_body) => {
                let content_arr = new_body["messages"][0]["content"].as_array().unwrap();

                // Check text block
                let text_content = content_arr[0]["text"].as_str().unwrap();
                assert!(
                    !text_content.contains("Learning output style"),
                    "Text block should have reminder removed"
                );

                // Check both tool_result blocks
                for i in 1..=2 {
                    let tool_content = content_arr[i]["content"].as_str().unwrap();
                    assert!(
                        !tool_content.contains("Learning output style"),
                        "tool_result {} should have reminder removed",
                        i
                    );
                    assert!(
                        tool_content.contains("File"),
                        "tool_result {} should preserve actual content",
                        i
                    );
                }
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }
}
