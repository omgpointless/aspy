//! CompactEnhancer - Detects and enhances Anthropic's compaction prompts
//!
//! When Claude Code's context window fills up, Anthropic sends a special prompt
//! asking Claude to summarize the conversation. This transformer detects that
//! prompt and appends session context to improve continuity after compression.
//!
//! # Detection Strategy
//!
//! Uses multi-signal detection to identify compaction requests:
//! 1. Primary signal: "summary of the conversation" phrase (required)
//! 2. Structural markers: Section headers like "Primary Request", "Pending Tasks" (2+ required)
//!
//! This avoids false positives from users asking about compaction or requesting
//! generic summaries.
//!
//! # Injection
//!
//! Appends `## Aspy Session Context` section to the end of the compaction prompt,
//! providing:
//! - Compact count (previous compressions this session)
//! - Context token usage
//! - Turn number
//! - Top tool usage (future)

use super::{RequestTransformer, TransformContext, TransformResult};
use crate::proxy::sessions::TodoStatus;
use serde::Deserialize;
use serde_json::Value;

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the CompactEnhancer transformer
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CompactEnhancerConfig {
    /// Whether the compact enhancer is enabled
    pub enabled: bool,
    // Future: custom context template, additional context sources
}

// ============================================================================
// Compact Detector
// ============================================================================

/// Detects Anthropic's compaction prompt within user messages
///
/// Uses a multi-signal approach:
/// - Primary signal (required): "summary of the conversation"
/// - Structural markers (need 2+): Section headers specific to compaction
pub struct CompactDetector {
    /// Primary signal - must be present (case-insensitive)
    summary_phrase: &'static str,
    /// Structural markers - need min_markers matches (case-sensitive)
    structural_markers: &'static [&'static str],
    /// Minimum structural markers required
    min_markers: usize,
}

impl Default for CompactDetector {
    fn default() -> Self {
        Self {
            summary_phrase: "summary of the conversation",
            structural_markers: &[
                "Primary Request",
                "Pending Tasks",
                "Current Work",
                "<analysis>",
                "<summary>",
                "Key Technical Concepts",
            ],
            min_markers: 2,
        }
    }
}

impl CompactDetector {
    /// Check if the content appears to be Anthropic's compaction prompt
    ///
    /// Detection requires:
    /// 1. Primary signal (case-insensitive): "summary of the conversation"
    /// 2. At least `min_markers` structural markers (case-sensitive)
    pub fn is_compaction_request(&self, content: &str) -> bool {
        // Check primary signal (case-insensitive)
        let lower = content.to_lowercase();
        if !lower.contains(self.summary_phrase) {
            return false;
        }

        // Count structural markers (case-sensitive for precision)
        let marker_count = self
            .structural_markers
            .iter()
            .filter(|marker| content.contains(*marker))
            .count();

        marker_count >= self.min_markers
    }
}

// ============================================================================
// Compact Enhancer Transformer
// ============================================================================

/// Request transformer that enhances compaction prompts with session context
pub struct CompactEnhancer {
    detector: CompactDetector,
}

impl CompactEnhancer {
    /// Create a new CompactEnhancer with default detection settings
    pub fn new() -> Self {
        Self {
            detector: CompactDetector::default(),
        }
    }

    /// Build the context injection string
    ///
    /// Generates instructions for the compacting LLM to preserve continuity.
    /// Includes tracked todos if available - these are the most concrete anchors.
    fn build_injection(&self, ctx: &TransformContext) -> String {
        let mut injection = String::from(
            r#"

## Aspy Continuity Enhancement

**For the summary:** To help the continuing Claude maintain flow, please include:
- **Active Work Tracks:** What features/bugs/tasks are in progress (with file paths if relevant)
- **Key Decisions Made:** Important choices that shouldn't be revisited
- **Current Mental Model:** The user's goals and approach being taken

**Post-compaction recovery:** The continuing Claude has `aspy_recall` to search the full pre-compaction conversation. Include 3-5 searchable keywords (feature names, concepts, file paths) that would help locate detailed context."#,
        );

        // Add todos section if we have tracked todos
        if let Some(ref todos) = ctx.todos {
            if !todos.is_empty() {
                injection.push_str("\n\n**Tracked Task State (from TodoWrite):**\n");

                // Show in-progress first (most important)
                let in_progress: Vec<_> = todos
                    .iter()
                    .filter(|t| t.status == TodoStatus::InProgress)
                    .collect();
                if !in_progress.is_empty() {
                    injection.push_str("Currently working on:\n");
                    for todo in in_progress {
                        injection.push_str(&format!("- 🔄 {}\n", todo.content));
                    }
                }

                // Then pending
                let pending: Vec<_> = todos
                    .iter()
                    .filter(|t| t.status == TodoStatus::Pending)
                    .collect();
                if !pending.is_empty() {
                    injection.push_str("Pending:\n");
                    for todo in pending {
                        injection.push_str(&format!("- ⏳ {}\n", todo.content));
                    }
                }

                // Completed last (just recent context)
                let completed: Vec<_> = todos
                    .iter()
                    .filter(|t| t.status == TodoStatus::Completed)
                    .collect();
                if !completed.is_empty() {
                    injection.push_str("Recently completed:\n");
                    for todo in completed {
                        injection.push_str(&format!("- ✅ {}\n", todo.content));
                    }
                }

                injection.push_str(
                    "\n_These task names are excellent searchable keywords for aspy_recall._",
                );
            }
        }

        injection
    }

    /// Extract text content from a user message (handles both string and array formats)
    fn extract_user_content(message: &Value) -> Option<String> {
        match &message["content"] {
            Value::String(s) => Some(s.clone()),
            Value::Array(arr) => {
                // Concatenate all text blocks
                let texts: Vec<&str> = arr
                    .iter()
                    .filter_map(|block| {
                        if block["type"].as_str() == Some("text") {
                            block["text"].as_str()
                        } else {
                            None
                        }
                    })
                    .collect();
                if texts.is_empty() {
                    None
                } else {
                    Some(texts.join("\n"))
                }
            }
            _ => None,
        }
    }

    /// Append injection to the last text block of a message
    fn append_to_message(message: &mut Value, injection: &str) -> bool {
        match &mut message["content"] {
            Value::String(s) => {
                s.push_str(injection);
                true
            }
            Value::Array(arr) => {
                // Find last text block and append
                for block in arr.iter_mut().rev() {
                    if block["type"].as_str() == Some("text") {
                        if let Some(text) = block["text"].as_str() {
                            block["text"] = Value::String(format!("{}{}", text, injection));
                            return true;
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }
}

impl Default for CompactEnhancer {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestTransformer for CompactEnhancer {
    fn name(&self) -> &'static str {
        "compact-enhancer"
    }

    fn should_apply(&self, ctx: &TransformContext) -> bool {
        ctx.path.ends_with("/messages") || ctx.path.ends_with("/v1/messages")
    }

    fn transform(&self, body: &Value, ctx: &TransformContext) -> TransformResult {
        // Get messages array
        let messages = match body["messages"].as_array() {
            Some(m) => m,
            None => return TransformResult::Unchanged,
        };

        // Find last user message
        let last_user_idx = messages
            .iter()
            .enumerate()
            .rev()
            .find(|(_, m)| m["role"].as_str() == Some("user"))
            .map(|(i, _)| i);

        let last_user_idx = match last_user_idx {
            Some(i) => i,
            None => return TransformResult::Unchanged,
        };

        // Extract content and check if it's a compaction request
        let content = match Self::extract_user_content(&messages[last_user_idx]) {
            Some(c) => c,
            None => return TransformResult::Unchanged,
        };

        if !self.detector.is_compaction_request(&content) {
            return TransformResult::Unchanged;
        }

        // It's a compaction request - inject context
        tracing::debug!("Detected compaction request, injecting session context");

        let injection = self.build_injection(ctx);

        // Clone body and modify
        let mut new_body = body.clone();
        if let Some(messages) = new_body["messages"].as_array_mut() {
            if let Some(user_msg) = messages.get_mut(last_user_idx) {
                if Self::append_to_message(user_msg, &injection) {
                    // Estimate tokens for the injection
                    let tokens_added = crate::tokens::estimate_tokens(&injection);

                    tracing::info!(
                        tokens_injected = tokens_added,
                        "Compaction detected - injected Aspy continuity context"
                    );

                    return TransformResult::modified_with_info(
                        new_body,
                        0, // We don't track what was there before (injection only)
                        tokens_added,
                        vec!["Compaction detected, injected <aspy-continuity/>".to_string()],
                    );
                }
            }
        }

        TransformResult::Unchanged
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Golden reference: Actual Anthropic compaction prompt
    const COMPACTION_PROMPT: &str = include_str!("test_data/compaction_prompt.txt");

    // ========================================================================
    // Detection Tests
    // ========================================================================

    #[test]
    fn test_detects_real_compaction_prompt() {
        let detector = CompactDetector::default();
        assert!(
            detector.is_compaction_request(COMPACTION_PROMPT),
            "Should detect the real Anthropic compaction prompt"
        );
    }

    #[test]
    fn test_rejects_user_asking_about_compaction() {
        let detector = CompactDetector::default();
        assert!(
            !detector.is_compaction_request("How does compaction work?"),
            "Should reject simple questions about compaction"
        );
    }

    #[test]
    fn test_rejects_generic_summary_request() {
        let detector = CompactDetector::default();
        let msg = "Create a summary of our conversation focusing on action items";
        assert!(
            !detector.is_compaction_request(msg),
            "Should reject generic summary requests without structural markers"
        );
    }

    #[test]
    fn test_requires_both_summary_phrase_and_markers() {
        let detector = CompactDetector::default();

        // Has summary phrase but no structural markers
        assert!(
            !detector.is_compaction_request("Please create a summary of the conversation"),
            "Should reject: has summary phrase but no markers"
        );

        // Has structural markers but no summary phrase
        assert!(
            !detector
                .is_compaction_request("Primary Request and Intent\nPending Tasks\nCurrent Work"),
            "Should reject: has markers but no summary phrase"
        );
    }

    #[test]
    fn test_summary_phrase_case_insensitive() {
        let detector = CompactDetector::default();

        // The real prompt uses "summary of the conversation" - test case variations
        let content_with_markers =
            "SUMMARY OF THE CONVERSATION\nPrimary Request\nPending Tasks\nCurrent Work";
        assert!(
            detector.is_compaction_request(content_with_markers),
            "Summary phrase detection should be case-insensitive"
        );
    }

    // ========================================================================
    // Injection Tests
    // ========================================================================

    #[test]
    fn test_injection_contains_continuity_header() {
        let enhancer = CompactEnhancer::new();
        let ctx = TransformContext::default();

        let injected = enhancer.build_injection(&ctx);

        assert!(
            injected.contains("## Aspy Continuity Enhancement"),
            "Injection should contain continuity header"
        );
    }

    #[test]
    fn test_injection_prompts_for_work_tracks() {
        let enhancer = CompactEnhancer::new();
        let ctx = TransformContext::default();

        let injected = enhancer.build_injection(&ctx);

        assert!(
            injected.contains("Active Work Tracks"),
            "Injection should prompt for active work tracks"
        );
        assert!(
            injected.contains("Key Decisions Made"),
            "Injection should prompt for key decisions"
        );
        assert!(
            injected.contains("Current Mental Model"),
            "Injection should prompt for mental model"
        );
    }

    #[test]
    fn test_injection_mentions_search_recovery() {
        let enhancer = CompactEnhancer::new();
        let ctx = TransformContext::default();

        let injected = enhancer.build_injection(&ctx);

        assert!(
            injected.contains("aspy_recall"),
            "Injection should mention the recall tool"
        );
        assert!(
            injected.contains("searchable keywords"),
            "Injection should request searchable keywords"
        );
    }

    // ========================================================================
    // Transform Tests
    // ========================================================================

    #[test]
    fn test_transform_modifies_compaction_request() {
        let enhancer = CompactEnhancer::new();
        let body = build_test_body_with_content(COMPACTION_PROMPT);
        let ctx = TransformContext::new(None, "/v1/messages", None);

        match enhancer.transform(&body, &ctx) {
            TransformResult::Modified {
                body: new_body,
                tokens,
                ..
            } => {
                let content = extract_last_user_content(&new_body);
                assert!(
                    content.contains("## Aspy Continuity Enhancement"),
                    "Modified body should contain injected continuity enhancement"
                );
                // Verify token tracking
                assert!(tokens.is_some(), "Should track token changes");
                let t = tokens.unwrap();
                assert!(t.after > 0, "Should report tokens added");
            }
            other => panic!("Expected Modified, got {:?}", other),
        }
    }

    #[test]
    fn test_transform_ignores_normal_request() {
        let enhancer = CompactEnhancer::new();
        let body = build_test_body_with_content("Hello, can you help me with my code?");
        let ctx = TransformContext::new(None, "/v1/messages", None);

        match enhancer.transform(&body, &ctx) {
            TransformResult::Unchanged => {}
            other => panic!("Expected Unchanged for normal message, got {:?}", other),
        }
    }

    #[test]
    fn test_transform_handles_array_content() {
        let enhancer = CompactEnhancer::new();
        // Build body with array-style content (multiple blocks)
        let body = serde_json::json!({
            "model": "claude-3",
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "Some context"},
                    {"type": "text", "text": COMPACTION_PROMPT}
                ]
            }]
        });
        let ctx = TransformContext::new(None, "/v1/messages", None);

        match enhancer.transform(&body, &ctx) {
            TransformResult::Modified { body: new_body, .. } => {
                // Should modify the last text block
                let messages = new_body["messages"].as_array().unwrap();
                let last_msg = messages.last().unwrap();
                let content = last_msg["content"].as_array().unwrap();
                let last_block = content.last().unwrap();
                let text = last_block["text"].as_str().unwrap();
                assert!(
                    text.contains("## Aspy Continuity Enhancement"),
                    "Should inject into last text block of array content"
                );
            }
            other => panic!("Expected Modified for array content, got {:?}", other),
        }
    }

    #[test]
    fn test_should_apply_only_to_messages_endpoint() {
        let enhancer = CompactEnhancer::new();

        assert!(enhancer.should_apply(&TransformContext::new(None, "/v1/messages", None)));
        assert!(enhancer.should_apply(&TransformContext::new(None, "/dev-1/v1/messages", None)));
        assert!(!enhancer.should_apply(&TransformContext::new(None, "/v1/embeddings", None)));
        assert!(!enhancer.should_apply(&TransformContext::new(None, "/health", None)));
    }

    // ========================================================================
    // Test Helpers
    // ========================================================================

    fn build_test_body_with_content(content: &str) -> Value {
        serde_json::json!({
            "model": "claude-3",
            "messages": [{
                "role": "user",
                "content": content
            }]
        })
    }

    fn extract_last_user_content(body: &Value) -> String {
        let messages = body["messages"].as_array().unwrap();
        let last_msg = messages.last().unwrap();
        match &last_msg["content"] {
            Value::String(s) => s.clone(),
            Value::Array(arr) => {
                // Find last text block
                arr.iter()
                    .filter_map(|b| b["text"].as_str())
                    .next_back()
                    .unwrap_or("")
                    .to_string()
            }
            _ => String::new(),
        }
    }
}
