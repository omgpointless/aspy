//! Data types for cortex query results
//!
//! Contains all DTOs (Data Transfer Objects) used by the cortex query interface:
//! - Search result types (`ThinkingMatch`, `PromptMatch`, `ResponseMatch`, etc.)
//! - Statistics types (`LifetimeStats`, `ModelStats`, `ToolStats`)
//! - Search mode configuration (`SearchMode`)

use serde::{Deserialize, Serialize};

// ============================================================================
// Search Mode
// ============================================================================

/// Search mode for FTS queries
///
/// Controls how the query string is processed before being sent to FTS5.
/// Different modes offer trade-offs between safety and power.
///
/// # Safety Spectrum
///
/// ```text
/// Phrase ────────────► Natural ────────────► Raw
/// (Safest)          (Balanced)          (Most Powerful)
/// ```
///
/// # Examples
///
/// ```rust,ignore
/// use aspy::pipeline::cortex_query::SearchMode;
///
/// // Phrase mode - escapes everything
/// let query = SearchMode::Phrase.process("user's query");
/// // Result: "\"user's query\""
///
/// // Natural mode - allows boolean operators
/// let query = SearchMode::Natural.process("solarized AND NOT vomit");
/// // Result: "solarized AND NOT vomit"
///
/// // Raw mode - full FTS5 syntax
/// let query = SearchMode::Raw.process("content:theme NEAR/5 solarized");
/// // Result: "content:theme NEAR/5 solarized" (passed through)
/// ```
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    /// Phrase search - query is wrapped in quotes
    ///
    /// Safe: Special characters are escaped, no FTS syntax allowed.
    /// Best for: Simple keyword searches, user-provided queries.
    /// Example: "solarized theme" → "\"solarized theme\""
    #[default]
    Phrase,

    /// Natural language search - basic operators allowed
    ///
    /// Allows: AND, OR, NOT (case-insensitive), word prefixes (*)
    /// Escapes: Quotes (doubled)
    /// Removes: Parentheses, column prefixes (colon syntax)
    /// Best for: Power users who understand basic boolean logic.
    /// Example: "solarized AND NOT vomit" → solarized AND NOT vomit
    Natural,

    /// Raw FTS5 query - no escaping
    ///
    /// Full FTS5 syntax: AND, OR, NOT, NEAR, *, ^, column:
    /// Warning: Can cause query errors if syntax is invalid.
    /// Best for: Expert users, programmatic queries, MCP tools.
    /// Example: "content:solarized NEAR/5 theme" → passed through as-is
    Raw,
}

impl SearchMode {
    /// Process query according to this search mode
    ///
    /// # Modes
    /// - **Phrase**: Escape everything, wrap in quotes (safest)
    /// - **Natural**: Allow AND/OR/NOT and prefix wildcards, escape rest
    /// - **Raw**: Pass through as-is (dangerous, full FTS5 syntax)
    pub fn process(self, query: &str) -> String {
        match self {
            SearchMode::Phrase => {
                // Escape internal quotes and wrap in quotes for exact phrase
                format!("\"{}\"", query.replace('"', "\"\""))
            }
            SearchMode::Natural => {
                // Preserve AND, OR, NOT operators and * wildcards
                // Escape quotes, parentheses, and column prefixes
                let mut result = String::with_capacity(query.len());
                let tokens: Vec<&str> = query.split_whitespace().collect();

                for (i, token) in tokens.iter().enumerate() {
                    if i > 0 {
                        result.push(' ');
                    }

                    // Preserve boolean operators (case-insensitive check)
                    let upper = token.to_uppercase();
                    if upper == "AND" || upper == "OR" || upper == "NOT" {
                        result.push_str(&upper);
                        continue;
                    }

                    // Escape special characters but preserve trailing *
                    // Using strip_suffix to avoid byte slicing on UTF-8 strings
                    let (base, has_wildcard) = match token.strip_suffix('*') {
                        Some(stripped) => (stripped, true),
                        None => (*token, false),
                    };

                    // Escape quotes and parentheses
                    let escaped = base
                        .replace('"', "\"\"")
                        .replace(['(', ')'], "")
                        .replace(':', " "); // Remove column prefixes

                    result.push_str(&escaped);
                    if has_wildcard {
                        result.push('*');
                    }
                }
                result
            }
            SearchMode::Raw => {
                // Pass through as-is - caller is responsible for validity
                query.to_string()
            }
        }
    }
}

// ============================================================================
// Search Result Types
// ============================================================================

/// Query result for thinking block searches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingMatch {
    pub session_id: Option<String>,
    pub timestamp: String,
    pub content: String,
    pub tokens: Option<u32>,
    pub rank: f64,
}

/// Query result for user prompt searches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMatch {
    pub session_id: Option<String>,
    pub timestamp: String,
    pub content: String,
    pub rank: f64,
}

/// Query result for assistant response searches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMatch {
    pub session_id: Option<String>,
    pub timestamp: String,
    pub content: String,
    pub rank: f64,
}

/// Query result for todo searches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoMatch {
    pub session_id: Option<String>,
    pub timestamp: String,
    /// Concatenated todo content (from FTS index)
    pub content: String,
    /// Full todo list as JSON (original format)
    pub todos_json: String,
    pub pending_count: u32,
    pub in_progress_count: u32,
    pub completed_count: u32,
    pub rank: f64,
}

/// Type of context match
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchType {
    Thinking,
    UserPrompt,
    AssistantResponse,
}

/// Combined context match result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMatch {
    pub match_type: MatchType,
    pub session_id: Option<String>,
    pub timestamp: String,
    pub content: String,
    pub rank: f64,
}

// ============================================================================
// Statistics Types
// ============================================================================

/// Lifetime statistics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifetimeStats {
    pub total_sessions: i64,
    // Token breakdown
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64, // = input + output + cache_read + cache_creation
    // Cost breakdown
    pub total_cost_usd: f64,
    pub cache_savings_usd: f64, // Estimated savings from cache reads
    // Counts
    pub total_tool_calls: i64,
    pub total_thinking_blocks: i64,
    pub total_prompts: i64,
    pub first_session: Option<String>,
    pub last_session: Option<String>,
    pub by_model: Vec<ModelStats>,
    pub by_tool: Vec<ToolStats>,
}

/// Statistics breakdown by model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStats {
    pub model: String,
    // Token breakdown per model
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub tokens: i64, // = sum of all 4 (backwards compat)
    pub cost_usd: f64,
    pub calls: i64,
}

/// Statistics breakdown by tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStats {
    pub tool: String,
    pub calls: i64,
    pub avg_duration_ms: f64,
    pub success_rate: f64,
    pub rejections: i64,
    pub errors: i64,
}

/// Embedding statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingStats {
    pub provider: String,
    pub model: String,
    pub dimensions: usize,
    pub thinking_embedded: u64,
    pub thinking_total: u64,
    pub prompts_embedded: u64,
    pub prompts_total: u64,
    pub responses_embedded: u64,
    pub responses_total: u64,
    pub total_embedded: u64,
    pub total_documents: u64,
    pub progress_pct: f64,
}
