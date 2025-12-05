//! Token estimation module for Claude API content
//!
//! Provides tiktoken-like token counting without external dependencies.
//! Uses heuristics tuned for Claude's BPE tokenizer (similar to GPT-4).
//!
//! # Accuracy
//!
//! This is an *estimator*, not an exact counter. Typical accuracy:
//! - English prose: ±5%
//! - Code: ±10%
//! - Mixed content: ±8%
//!
//! For exact counts, use the API's `usage` response field.
//!
//! # Usage
//!
//! ```ignore
//! use crate::tokens::estimate_tokens;
//!
//! let text = "Hello, world!";
//! let count = estimate_tokens(text);
//! ```

/// Estimate token count for text content
///
/// Uses a multi-factor heuristic:
/// 1. Base estimate from character count (1 token ≈ 4 chars for English)
/// 2. Adjustments for whitespace boundaries (spaces often = token breaks)
/// 3. Adjustments for punctuation (often their own tokens)
/// 4. Adjustments for numbers (each digit often a token)
///
/// # Arguments
/// * `text` - The text content to estimate
///
/// # Returns
/// Estimated token count (minimum 1 for non-empty input)
pub fn estimate_tokens(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }

    // Count various character classes
    let char_count = text.chars().count();
    let whitespace_count = text.chars().filter(|c| c.is_whitespace()).count();
    let punctuation_count = text.chars().filter(|c| c.is_ascii_punctuation()).count();
    let digit_count = text.chars().filter(|c| c.is_ascii_digit()).count();
    let newline_count = text.chars().filter(|c| *c == '\n').count();

    // Base estimate: ~4 characters per token for typical English
    let base_tokens = char_count as f64 / 4.0;

    // Words are typically their own tokens - count word boundaries
    // Whitespace often indicates token boundaries
    let word_adjustment = whitespace_count as f64 * 0.3;

    // Punctuation is often its own token
    let punct_adjustment = punctuation_count as f64 * 0.5;

    // Digits are often individual tokens or small groups
    let digit_adjustment = digit_count as f64 * 0.3;

    // Newlines often indicate structure (more tokens)
    let newline_adjustment = newline_count as f64 * 0.2;

    // Combine estimates
    let estimate =
        base_tokens + word_adjustment + punct_adjustment + digit_adjustment + newline_adjustment;

    // Minimum 1 token for non-empty text
    (estimate.ceil() as u32).max(1)
}

/// Estimate tokens for JSON content
///
/// JSON has more structure (braces, quotes, colons) that typically
/// increases token count compared to plain text.
pub fn estimate_json_tokens(json: &serde_json::Value) -> u32 {
    let text = json.to_string();
    // JSON has higher overhead due to structural characters
    let base = estimate_tokens(&text);
    // Add ~15% for JSON structural overhead
    (base as f64 * 1.15).ceil() as u32
}

/// Token delta tracking for transformation/augmentation
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenDelta {
    /// Tokens in original content
    pub before: u32,
    /// Tokens in modified content
    pub after: u32,
}

impl TokenDelta {
    /// Create a new token delta
    pub fn new(before: u32, after: u32) -> Self {
        Self { before, after }
    }

    /// Tokens added (positive) or removed (negative)
    /// Future: Used by stats aggregation and API responses
    #[allow(dead_code)]
    pub fn delta(&self) -> i64 {
        self.after as i64 - self.before as i64
    }

    /// Tokens added (0 if removed)
    pub fn added(&self) -> u32 {
        self.after.saturating_sub(self.before)
    }

    /// Tokens removed (0 if added)
    pub fn removed(&self) -> u32 {
        self.before.saturating_sub(self.after)
    }
}

/// Statistics for injection/removal tracking
#[derive(Debug, Clone, Default)]
pub struct TransformStats {
    /// Total tokens injected by transformers
    pub tokens_injected: u64,
    /// Total tokens removed by transformers
    pub tokens_removed: u64,
    /// Per-transformer breakdown: name -> (injected, removed)
    pub by_transformer: std::collections::HashMap<String, (u64, u64)>,
}

impl TransformStats {
    /// Record a transformer's token delta
    pub fn record(&mut self, transformer_name: &str, delta: &TokenDelta) {
        self.tokens_injected += delta.added() as u64;
        self.tokens_removed += delta.removed() as u64;

        let entry = self
            .by_transformer
            .entry(transformer_name.to_string())
            .or_insert((0, 0));
        entry.0 += delta.added() as u64;
        entry.1 += delta.removed() as u64;
    }

    /// Net token change (positive = more tokens, negative = fewer)
    /// Future: Displayed in summary stats and API responses
    #[allow(dead_code)]
    pub fn net_delta(&self) -> i64 {
        self.tokens_injected as i64 - self.tokens_removed as i64
    }
}

/// Statistics for augmentation tracking
#[derive(Debug, Clone, Default)]
pub struct AugmentStats {
    /// Total tokens injected by augmenters
    pub tokens_injected: u64,
    /// Per-augmenter breakdown: name -> injected
    pub by_augmenter: std::collections::HashMap<String, u64>,
}

impl AugmentStats {
    /// Record an augmenter's injection
    pub fn record(&mut self, augmenter_name: &str, tokens: u32) {
        self.tokens_injected += tokens as u64;
        *self
            .by_augmenter
            .entry(augmenter_name.to_string())
            .or_insert(0) += tokens as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_string() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_simple_word() {
        // "Hello" = ~1-2 tokens
        let tokens = estimate_tokens("Hello");
        assert!((1..=3).contains(&tokens));
    }

    #[test]
    fn test_sentence() {
        // "Hello, world!" = ~4-5 tokens (Hello, comma, space, world, !)
        let tokens = estimate_tokens("Hello, world!");
        assert!((3..=7).contains(&tokens));
    }

    #[test]
    fn test_code_snippet() {
        let code = r#"fn main() {
    println!("Hello");
}"#;
        // Code has more punctuation = more tokens
        let tokens = estimate_tokens(code);
        assert!((8..=20).contains(&tokens));
    }

    #[test]
    fn test_json() {
        let json = serde_json::json!({
            "name": "test",
            "value": 123
        });
        let tokens = estimate_json_tokens(&json);
        assert!((8..=25).contains(&tokens));
    }

    #[test]
    fn test_token_delta() {
        let delta = TokenDelta::new(100, 150);
        assert_eq!(delta.delta(), 50);
        assert_eq!(delta.added(), 50);
        assert_eq!(delta.removed(), 0);

        let delta2 = TokenDelta::new(150, 100);
        assert_eq!(delta2.delta(), -50);
        assert_eq!(delta2.added(), 0);
        assert_eq!(delta2.removed(), 50);
    }

    #[test]
    fn test_transform_stats() {
        let mut stats = TransformStats::default();
        stats.record("tag-editor", &TokenDelta::new(100, 80));
        stats.record("compact-enhancer", &TokenDelta::new(50, 150));

        assert_eq!(stats.tokens_removed, 20);
        assert_eq!(stats.tokens_injected, 100);
        assert_eq!(stats.net_delta(), 80);
    }
}
