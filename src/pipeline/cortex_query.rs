//! Query interface for Cortex database
//!
//! Provides structured queries for context recovery, used by MCP tools.
//! Uses connection pooling for efficient concurrent access.
//!
//! # Architecture
//!
//! ```text
//! MCP Tools / HTTP API
//!         │
//!         └──→ CortexQuery (r2d2 pool)
//!                 │
//!                 ├──→ SQLite Reader Connection 1
//!                 ├──→ SQLite Reader Connection 2
//!                 └──→ SQLite Reader Connection N (max 4)
//!                         │
//!                         └──→ FTS5 Queries (BM25 ranking)
//! ```
//!
//! # WAL Mode Concurrency
//!
//! The cortex database uses WAL (Write-Ahead Logging) mode, which allows
//! multiple concurrent readers while the writer thread is active. The connection
//! pool manages up to 4 read-only connections for query parallelism.

use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

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
/// ```rust
/// use aspy::pipeline::cortext_query::SearchMode;
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

/// Query interface for cortex database
///
/// Uses connection pooling for efficient concurrent access.
///
/// # Example
///
/// ```rust,no_run
/// use aspy::pipeline::cortex_query::{CortexQuery, SearchMode};
///
/// # fn main() -> anyhow::Result<()> {
/// let query = CortexQuery::new("./data/lifestats.db")?;
///
/// // Search thinking blocks
/// let results = query.search_thinking("solarized theme", 10, SearchMode::Phrase)?;
/// for m in results {
///     println!("[{}] {}", m.timestamp, m.content);
/// }
/// # Ok(())
/// # }
/// ```
pub struct CortexQuery {
    pool: Pool<SqliteConnectionManager>,
}

impl CortexQuery {
    /// Create a new query interface with connection pool
    ///
    /// # Arguments
    /// * `db_path` - Path to the SQLite database file
    ///
    /// # Errors
    /// Returns an error if the database cannot be opened or if a test
    /// connection cannot be established.
    pub fn new(db_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let manager = SqliteConnectionManager::file(db_path);
        let pool = Pool::builder()
            .max_size(4) // Read-only pool for concurrent queries
            .build(manager)?;

        // Verify connection works
        let conn = pool.get()?;
        conn.query_row("SELECT 1", [], |row| row.get::<_, i32>(0))?;

        Ok(Self { pool })
    }

    /// Get a connection from the pool
    fn conn(&self) -> anyhow::Result<PooledConnection<SqliteConnectionManager>> {
        Ok(self.pool.get()?)
    }

    /// Search thinking blocks by keyword (FTS5)
    ///
    /// Uses FTS5 full-text search with BM25 ranking algorithm.
    ///
    /// # Arguments
    /// * `query` - The search query
    /// * `limit` - Maximum number of results
    /// * `mode` - How to interpret the query (default: Phrase)
    ///
    /// # Returns
    /// Results sorted by relevance (lower rank = more relevant)
    pub fn search_thinking(
        &self,
        query: &str,
        limit: usize,
        mode: SearchMode,
    ) -> anyhow::Result<Vec<ThinkingMatch>> {
        let conn = self.conn()?;

        // Process query according to mode
        let safe_query = mode.process(query);

        let sql = r#"
            SELECT
                t.session_id,
                t.timestamp,
                t.content,
                t.tokens,
                bm25(thinking_fts) as rank
            FROM thinking_fts f
            JOIN thinking_blocks t ON f.rowid = t.id
            WHERE thinking_fts MATCH ?1
            ORDER BY rank
            LIMIT ?2
        "#;

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![safe_query, limit as i64], |row| {
            Ok(ThinkingMatch {
                session_id: row.get(0)?,
                timestamp: row.get(1)?,
                content: row.get(2)?,
                tokens: row.get(3)?,
                rank: row.get(4)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Search user prompts by keyword (FTS5)
    ///
    /// Uses FTS5 full-text search with BM25 ranking algorithm.
    ///
    /// # Arguments
    /// * `query` - The search query
    /// * `limit` - Maximum number of results
    /// * `mode` - How to interpret the query (default: Phrase)
    ///
    /// # Returns
    /// Results sorted by relevance (lower rank = more relevant)
    pub fn search_prompts(
        &self,
        query: &str,
        limit: usize,
        mode: SearchMode,
    ) -> anyhow::Result<Vec<PromptMatch>> {
        let conn = self.conn()?;
        let safe_query = mode.process(query);

        let sql = r#"
            SELECT
                p.session_id,
                p.timestamp,
                p.content,
                bm25(prompts_fts) as rank
            FROM prompts_fts f
            JOIN user_prompts p ON f.rowid = p.id
            WHERE prompts_fts MATCH ?1
            ORDER BY rank
            LIMIT ?2
        "#;

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![safe_query, limit as i64], |row| {
            Ok(PromptMatch {
                session_id: row.get(0)?,
                timestamp: row.get(1)?,
                content: row.get(2)?,
                rank: row.get(3)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Search assistant responses by keyword (FTS5)
    ///
    /// Uses FTS5 full-text search with BM25 ranking algorithm.
    ///
    /// # Arguments
    /// * `query` - The search query
    /// * `limit` - Maximum number of results
    /// * `mode` - How to interpret the query (default: Phrase)
    ///
    /// # Returns
    /// Results sorted by relevance (lower rank = more relevant)
    pub fn search_responses(
        &self,
        query: &str,
        limit: usize,
        mode: SearchMode,
    ) -> anyhow::Result<Vec<ResponseMatch>> {
        let conn = self.conn()?;
        let safe_query = mode.process(query);

        let sql = r#"
            SELECT
                r.session_id,
                r.timestamp,
                r.content,
                bm25(responses_fts) as rank
            FROM responses_fts f
            JOIN assistant_responses r ON f.rowid = r.id
            WHERE responses_fts MATCH ?1
            ORDER BY rank
            LIMIT ?2
        "#;

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![safe_query, limit as i64], |row| {
            Ok(ResponseMatch {
                session_id: row.get(0)?,
                timestamp: row.get(1)?,
                content: row.get(2)?,
                rank: row.get(3)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Search todos by keyword (FTS5)
    ///
    /// Uses FTS5 full-text search with BM25 ranking on todo content.
    ///
    /// # Arguments
    /// * `query` - The search query
    /// * `limit` - Maximum number of results
    /// * `mode` - How to interpret the query (default: Phrase)
    ///
    /// # Returns
    /// Results sorted by relevance (lower rank = more relevant)
    pub fn search_todos(
        &self,
        query: &str,
        limit: usize,
        mode: SearchMode,
    ) -> anyhow::Result<Vec<TodoMatch>> {
        let conn = self.conn()?;
        let safe_query = mode.process(query);

        let sql = r#"
            SELECT
                t.session_id,
                t.timestamp,
                f.content,
                t.todos_json,
                t.pending_count,
                t.in_progress_count,
                t.completed_count,
                bm25(todos_fts) as rank
            FROM todos_fts f
            JOIN todos t ON f.rowid = t.id
            WHERE todos_fts MATCH ?1
            ORDER BY rank
            LIMIT ?2
        "#;

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![safe_query, limit as i64], |row| {
            Ok(TodoMatch {
                session_id: row.get(0)?,
                timestamp: row.get(1)?,
                content: row.get(2)?,
                todos_json: row.get(3)?,
                pending_count: row.get(4)?,
                in_progress_count: row.get(5)?,
                completed_count: row.get(6)?,
                rank: row.get(7)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Get recent todos (no search, just latest snapshots)
    ///
    /// Returns the most recent todo snapshots, optionally filtered by timeframe.
    ///
    /// # Arguments
    /// * `limit` - Maximum number of results
    /// * `days` - Optional number of days to look back
    ///
    /// # Returns
    /// Results sorted by timestamp (most recent first)
    pub fn get_recent_todos(
        &self,
        limit: usize,
        days: Option<u32>,
    ) -> anyhow::Result<Vec<TodoMatch>> {
        let conn = self.conn()?;

        let sql = if let Some(d) = days {
            format!(
                r#"
                SELECT
                    t.session_id,
                    t.timestamp,
                    COALESCE(f.content, '') as content,
                    t.todos_json,
                    t.pending_count,
                    t.in_progress_count,
                    t.completed_count,
                    0.0 as rank
                FROM todos t
                LEFT JOIN todos_fts f ON f.rowid = t.id
                WHERE t.timestamp > datetime('now', '-{} days')
                ORDER BY t.timestamp DESC
                LIMIT ?1
                "#,
                d
            )
        } else {
            r#"
            SELECT
                t.session_id,
                t.timestamp,
                COALESCE(f.content, '') as content,
                t.todos_json,
                t.pending_count,
                t.in_progress_count,
                t.completed_count,
                0.0 as rank
            FROM todos t
            LEFT JOIN todos_fts f ON f.rowid = t.id
            ORDER BY t.timestamp DESC
            LIMIT ?1
            "#
            .to_string()
        };

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(TodoMatch {
                session_id: row.get(0)?,
                timestamp: row.get(1)?,
                content: row.get(2)?,
                todos_json: row.get(3)?,
                pending_count: row.get(4)?,
                in_progress_count: row.get(5)?,
                completed_count: row.get(6)?,
                rank: row.get(7)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Combined context recovery query
    ///
    /// Searches across thinking blocks, user prompts, and assistant responses,
    /// then combines and sorts by relevance.
    ///
    /// # Arguments
    /// * `topic` - The topic to search for
    /// * `limit` - Maximum results per source (thinking + prompts + responses)
    /// * `mode` - How to interpret the query (default: Phrase)
    ///
    /// # Returns
    /// Combined results sorted by BM25 rank (lower = more relevant), limited to `limit` total results.
    pub fn recover_context(
        &self,
        topic: &str,
        limit: usize,
        mode: SearchMode,
    ) -> anyhow::Result<Vec<ContextMatch>> {
        let mut results = Vec::new();

        // Search thinking blocks
        for m in self.search_thinking(topic, limit, mode)? {
            results.push(ContextMatch {
                match_type: MatchType::Thinking,
                session_id: m.session_id,
                timestamp: m.timestamp,
                content: m.content,
                rank: m.rank,
            });
        }

        // Search user prompts
        for m in self.search_prompts(topic, limit, mode)? {
            results.push(ContextMatch {
                match_type: MatchType::UserPrompt,
                session_id: m.session_id,
                timestamp: m.timestamp,
                content: m.content,
                rank: m.rank,
            });
        }

        // Search assistant responses
        for m in self.search_responses(topic, limit, mode)? {
            results.push(ContextMatch {
                match_type: MatchType::AssistantResponse,
                session_id: m.session_id,
                timestamp: m.timestamp,
                content: m.content,
                rank: m.rank,
            });
        }

        // Sort by rank (lower = more relevant)
        results.sort_by(|a, b| {
            a.rank
                .partial_cmp(&b.rank)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit total results
        results.truncate(limit);

        Ok(results)
    }

    // ═════════════════════════════════════════════════════════════════════════
    // User-Scoped Queries (Cross-Session Context Recovery)
    // ═════════════════════════════════════════════════════════════════════════

    /// Search thinking blocks for a specific user across all their sessions (FTS5)
    ///
    /// Uses FTS5 full-text search with BM25 ranking, filtered by user_id.
    /// Enables queries like "show me all of foundry's past thinking about themes".
    ///
    /// # Arguments
    /// * `user_id` - The user identifier (e.g., "foundry")
    /// * `query` - The search query
    /// * `limit` - Maximum number of results
    /// * `mode` - How to interpret the query (default: Phrase)
    ///
    /// # Returns
    /// Results sorted by relevance (lower rank = more relevant)
    pub fn search_user_thinking(
        &self,
        user_id: &str,
        query: &str,
        limit: usize,
        mode: SearchMode,
    ) -> anyhow::Result<Vec<ThinkingMatch>> {
        let conn = self.conn()?;
        let safe_query = mode.process(query);

        let sql = r#"
            SELECT
                t.session_id,
                t.timestamp,
                t.content,
                t.tokens,
                bm25(thinking_fts) as rank
            FROM thinking_fts f
            JOIN thinking_blocks t ON f.rowid = t.id
            JOIN sessions s ON t.session_id = s.id
            WHERE thinking_fts MATCH ?1 AND s.user_id = ?2
            ORDER BY rank
            LIMIT ?3
        "#;

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![safe_query, user_id, limit as i64], |row| {
            Ok(ThinkingMatch {
                session_id: row.get(0)?,
                timestamp: row.get(1)?,
                content: row.get(2)?,
                tokens: row.get(3)?,
                rank: row.get(4)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Search user prompts for a specific user across all their sessions (FTS5)
    ///
    /// Uses FTS5 full-text search with BM25 ranking, filtered by user_id.
    ///
    /// # Arguments
    /// * `user_id` - The user identifier (e.g., "foundry")
    /// * `query` - The search query
    /// * `limit` - Maximum number of results
    /// * `mode` - How to interpret the query (default: Phrase)
    ///
    /// # Returns
    /// Results sorted by relevance (lower rank = more relevant)
    pub fn search_user_prompts(
        &self,
        user_id: &str,
        query: &str,
        limit: usize,
        mode: SearchMode,
    ) -> anyhow::Result<Vec<PromptMatch>> {
        let conn = self.conn()?;
        let safe_query = mode.process(query);

        let sql = r#"
            SELECT
                p.session_id,
                p.timestamp,
                p.content,
                bm25(prompts_fts) as rank
            FROM prompts_fts f
            JOIN user_prompts p ON f.rowid = p.id
            JOIN sessions s ON p.session_id = s.id
            WHERE prompts_fts MATCH ?1 AND s.user_id = ?2
            ORDER BY rank
            LIMIT ?3
        "#;

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![safe_query, user_id, limit as i64], |row| {
            Ok(PromptMatch {
                session_id: row.get(0)?,
                timestamp: row.get(1)?,
                content: row.get(2)?,
                rank: row.get(3)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Search assistant responses for a specific user across all their sessions (FTS5)
    ///
    /// Uses FTS5 full-text search with BM25 ranking, filtered by user_id.
    ///
    /// # Arguments
    /// * `user_id` - The user identifier (e.g., "foundry")
    /// * `query` - The search query
    /// * `limit` - Maximum number of results
    /// * `mode` - How to interpret the query (default: Phrase)
    ///
    /// # Returns
    /// Results sorted by relevance (lower rank = more relevant)
    pub fn search_user_responses(
        &self,
        user_id: &str,
        query: &str,
        limit: usize,
        mode: SearchMode,
    ) -> anyhow::Result<Vec<ResponseMatch>> {
        let conn = self.conn()?;
        let safe_query = mode.process(query);

        let sql = r#"
            SELECT
                r.session_id,
                r.timestamp,
                r.content,
                bm25(responses_fts) as rank
            FROM responses_fts f
            JOIN assistant_responses r ON f.rowid = r.id
            JOIN sessions s ON r.session_id = s.id
            WHERE responses_fts MATCH ?1 AND s.user_id = ?2
            ORDER BY rank
            LIMIT ?3
        "#;

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![safe_query, user_id, limit as i64], |row| {
            Ok(ResponseMatch {
                session_id: row.get(0)?,
                timestamp: row.get(1)?,
                content: row.get(2)?,
                rank: row.get(3)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Combined context recovery for a specific user across all their sessions
    ///
    /// Searches across thinking blocks, user prompts, and assistant responses,
    /// then combines and sorts by relevance. Only includes data from the specified user's sessions.
    ///
    /// # Arguments
    /// * `user_id` - The user identifier (e.g., "foundry")
    /// * `topic` - The topic to search for
    /// * `limit` - Maximum results per source (thinking + prompts + responses)
    /// * `mode` - How to interpret the query (default: Phrase)
    ///
    /// # Returns
    /// Combined results sorted by BM25 rank (lower = more relevant), limited to `limit` total results.
    pub fn recover_user_context(
        &self,
        user_id: &str,
        topic: &str,
        limit: usize,
        mode: SearchMode,
    ) -> anyhow::Result<Vec<ContextMatch>> {
        let mut results = Vec::new();

        // Search thinking blocks
        for m in self.search_user_thinking(user_id, topic, limit, mode)? {
            results.push(ContextMatch {
                match_type: MatchType::Thinking,
                session_id: m.session_id,
                timestamp: m.timestamp,
                content: m.content,
                rank: m.rank,
            });
        }

        // Search user prompts
        for m in self.search_user_prompts(user_id, topic, limit, mode)? {
            results.push(ContextMatch {
                match_type: MatchType::UserPrompt,
                session_id: m.session_id,
                timestamp: m.timestamp,
                content: m.content,
                rank: m.rank,
            });
        }

        // Search assistant responses
        for m in self.search_user_responses(user_id, topic, limit, mode)? {
            results.push(ContextMatch {
                match_type: MatchType::AssistantResponse,
                session_id: m.session_id,
                timestamp: m.timestamp,
                content: m.content,
                rank: m.rank,
            });
        }

        // Sort by rank (lower = more relevant)
        results.sort_by(|a, b| {
            a.rank
                .partial_cmp(&b.rank)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit total results
        results.truncate(limit);

        Ok(results)
    }

    /// Get lifetime statistics for a specific user across all their sessions
    ///
    /// Aggregates data across all sessions belonging to the specified user.
    ///
    /// # Arguments
    /// * `user_id` - The user identifier (e.g., "foundry")
    ///
    /// # Returns
    /// Statistics including total tokens, cost, tool calls, and breakdowns by model and tool.
    pub fn get_user_lifetime_stats(&self, user_id: &str) -> anyhow::Result<LifetimeStats> {
        let conn = self.conn()?;

        // Aggregate stats from api_usage for this user's sessions
        let (input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens, total_cost): (
            i64,
            i64,
            i64,
            i64,
            f64,
        ) = conn.query_row(
            r#"
            SELECT
                COALESCE(SUM(a.input_tokens), 0),
                COALESCE(SUM(a.output_tokens), 0),
                COALESCE(SUM(a.cache_read_tokens), 0),
                COALESCE(SUM(a.cache_creation_tokens), 0),
                COALESCE(SUM(a.cost_usd), 0)
            FROM api_usage a
            JOIN sessions s ON a.session_id = s.id
            WHERE s.user_id = ?1
            "#,
            params![user_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;

        // Calculate derived values
        let total_tokens = input_tokens + output_tokens + cache_read_tokens + cache_creation_tokens;
        let cache_savings_usd = (cache_read_tokens as f64 / 1_000_000.0) * 2.70;

        let total_sessions: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT id) FROM sessions WHERE user_id = ?1",
            params![user_id],
            |row| row.get(0),
        )?;

        let (first_session, last_session): (Option<String>, Option<String>) = conn.query_row(
            r#"
            SELECT MIN(a.timestamp), MAX(a.timestamp)
            FROM api_usage a
            JOIN sessions s ON a.session_id = s.id
            WHERE s.user_id = ?1
            "#,
            params![user_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let total_tool_calls: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM tool_calls t
            JOIN sessions s ON t.session_id = s.id
            WHERE s.user_id = ?1
            "#,
            params![user_id],
            |row| row.get(0),
        )?;

        let total_thinking: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM thinking_blocks t
            JOIN sessions s ON t.session_id = s.id
            WHERE s.user_id = ?1
            "#,
            params![user_id],
            |row| row.get(0),
        )?;

        let total_prompts: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM user_prompts p
            JOIN sessions s ON p.session_id = s.id
            WHERE s.user_id = ?1
            "#,
            params![user_id],
            |row| row.get(0),
        )?;

        // By model
        let mut by_model = Vec::new();
        {
            let mut stmt = conn.prepare(
                r#"
                SELECT
                    a.model,
                    COALESCE(SUM(a.input_tokens), 0) as input_tokens,
                    COALESCE(SUM(a.output_tokens), 0) as output_tokens,
                    COALESCE(SUM(a.cache_read_tokens), 0) as cache_read_tokens,
                    COALESCE(SUM(a.cache_creation_tokens), 0) as cache_creation_tokens,
                    SUM(a.cost_usd) as cost,
                    COUNT(*) as calls
                FROM api_usage a
                JOIN sessions s ON a.session_id = s.id
                WHERE s.user_id = ?1
                GROUP BY a.model
                ORDER BY (a.input_tokens + a.output_tokens + a.cache_read_tokens + a.cache_creation_tokens) DESC
                "#,
            )?;
            let rows = stmt.query_map(params![user_id], |row| {
                let input: i64 = row.get(1)?;
                let output: i64 = row.get(2)?;
                let cache_read: i64 = row.get(3)?;
                let cache_creation: i64 = row.get(4)?;
                Ok(ModelStats {
                    model: row.get(0)?,
                    input_tokens: input,
                    output_tokens: output,
                    cache_read_tokens: cache_read,
                    cache_creation_tokens: cache_creation,
                    tokens: input + output + cache_read + cache_creation,
                    cost_usd: row.get(5)?,
                    calls: row.get(6)?,
                })
            })?;
            for row in rows {
                by_model.push(row?);
            }
        }

        // By tool
        let mut by_tool = Vec::new();
        {
            let mut stmt = conn.prepare(
                r#"
                SELECT
                    tc.tool_name,
                    COUNT(*) as calls,
                    COALESCE(AVG(tr.duration_ms), 0) as avg_duration,
                    COALESCE(AVG(CAST(tr.success AS FLOAT)), 1.0) as success_rate,
                    SUM(CASE WHEN tr.is_rejection = 1 THEN 1 ELSE 0 END) as rejections,
                    SUM(CASE WHEN tr.success = 0 AND tr.is_rejection = 0 THEN 1 ELSE 0 END) as errors
                FROM tool_calls tc
                JOIN sessions s ON tc.session_id = s.id
                LEFT JOIN tool_results tr ON tc.id = tr.call_id
                WHERE s.user_id = ?1
                GROUP BY tc.tool_name
                ORDER BY calls DESC
                "#,
            )?;
            let rows = stmt.query_map(params![user_id], |row| {
                Ok(ToolStats {
                    tool: row.get(0)?,
                    calls: row.get(1)?,
                    avg_duration_ms: row.get(2)?,
                    success_rate: row.get(3)?,
                    rejections: row.get(4)?,
                    errors: row.get(5)?,
                })
            })?;
            for row in rows {
                by_tool.push(row?);
            }
        }

        Ok(LifetimeStats {
            total_sessions,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
            total_tokens,
            total_cost_usd: total_cost,
            cache_savings_usd,
            total_tool_calls,
            total_thinking_blocks: total_thinking,
            total_prompts,
            first_session,
            last_session,
            by_model,
            by_tool,
        })
    }

    /// Get session history for a specific user
    ///
    /// Returns a list of sessions from the database for the specified user,
    /// ordered by start time (most recent first).
    ///
    /// # Arguments
    /// * `user_id` - The user identifier (api_key_hash)
    /// * `limit` - Maximum number of sessions to return
    /// * `offset` - Number of sessions to skip (for pagination)
    ///
    /// # Returns
    /// Vector of session history items matching the criteria.
    pub fn get_user_sessions(
        &self,
        user_id: &str,
        limit: usize,
        offset: usize,
    ) -> anyhow::Result<Vec<crate::proxy::api::SessionHistoryItem>> {
        let conn = self.conn()?;

        // Query sessions for this user
        let mut stmt = conn.prepare(
            r#"
            SELECT
                s.id,
                s.user_id,
                s.started_at,
                s.ended_at,
                s.source,
                COALESCE(s.total_tokens, 0),
                COALESCE(s.total_cost_usd, 0),
                COALESCE(s.tool_calls, 0),
                COALESCE(s.thinking_blocks, 0),
                (SELECT COUNT(*) FROM api_usage a WHERE a.session_id = s.id) as request_count
            FROM sessions s
            WHERE s.user_id = ?1
            ORDER BY s.started_at DESC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;

        let rows = stmt.query_map(params![user_id, limit as i64, offset as i64], |row| {
            let session_id: String = row.get(0)?;
            let user_id: String = row.get(1)?;
            let started_at: String = row.get(2)?;
            let ended_at: Option<String> = row.get(3)?;
            let source: Option<String> = row.get(4)?;
            let total_tokens: i64 = row.get(5)?;
            let total_cost_usd: f64 = row.get(6)?;
            let tool_calls: i64 = row.get(7)?;
            let request_count: i64 = row.get(9)?;

            Ok(crate::proxy::api::SessionHistoryItem {
                session_id,
                user_id,
                claude_session_id: None, // Not stored in DB
                started: started_at,
                ended: ended_at,
                source: source.unwrap_or_else(|| "unknown".to_string()),
                end_reason: None,      // Not stored in DB currently
                transcript_path: None, // Not stored in DB currently
                stats: crate::proxy::api::SessionStatsSummary {
                    requests: request_count as usize,
                    tool_calls: tool_calls as usize,
                    input_tokens: (total_tokens / 2) as u64, // Approximate split
                    output_tokens: (total_tokens / 2) as u64,
                    cost_usd: total_cost_usd,
                },
            })
        })?;

        let sessions: Vec<_> = rows.filter_map(|r| r.ok()).collect();
        Ok(sessions)
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Global Queries (All Sessions)
    // ═════════════════════════════════════════════════════════════════════════

    /// Get lifetime statistics
    ///
    /// Aggregates data across all sessions to provide summary statistics.
    ///
    /// # Returns
    /// Statistics including total tokens, cost, tool calls, and breakdowns by model and tool.
    pub fn get_lifetime_stats(&self) -> anyhow::Result<LifetimeStats> {
        let conn = self.conn()?;

        // Aggregate stats from api_usage (more accurate than sessions table)
        let (input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens, total_cost): (
            i64,
            i64,
            i64,
            i64,
            f64,
        ) = conn.query_row(
            r#"
            SELECT
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cache_read_tokens), 0),
                COALESCE(SUM(cache_creation_tokens), 0),
                COALESCE(SUM(cost_usd), 0)
            FROM api_usage
            "#,
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;

        // Calculate derived values
        let total_tokens = input_tokens + output_tokens + cache_read_tokens + cache_creation_tokens;

        // Cache savings estimate: cached reads cost ~10% of uncached input
        // For Sonnet: ~$3/1M input, ~$0.30/1M cached = $2.70/1M savings
        let cache_savings_usd = (cache_read_tokens as f64 / 1_000_000.0) * 2.70;

        let total_sessions: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT session_id) FROM api_usage WHERE session_id IS NOT NULL",
            [],
            |row| row.get(0),
        )?;

        let (first_session, last_session): (Option<String>, Option<String>) = conn.query_row(
            "SELECT MIN(timestamp), MAX(timestamp) FROM api_usage",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let total_tool_calls: i64 =
            conn.query_row("SELECT COUNT(*) FROM tool_calls", [], |row| row.get(0))?;

        let total_thinking: i64 =
            conn.query_row("SELECT COUNT(*) FROM thinking_blocks", [], |row| row.get(0))?;

        let total_prompts: i64 =
            conn.query_row("SELECT COUNT(*) FROM user_prompts", [], |row| row.get(0))?;

        // By model
        let mut by_model = Vec::new();
        {
            let mut stmt = conn.prepare(
                r#"
                SELECT
                    model,
                    COALESCE(SUM(input_tokens), 0) as input_tokens,
                    COALESCE(SUM(output_tokens), 0) as output_tokens,
                    COALESCE(SUM(cache_read_tokens), 0) as cache_read_tokens,
                    COALESCE(SUM(cache_creation_tokens), 0) as cache_creation_tokens,
                    SUM(cost_usd) as cost,
                    COUNT(*) as calls
                FROM api_usage
                GROUP BY model
                ORDER BY (input_tokens + output_tokens + cache_read_tokens + cache_creation_tokens) DESC
                "#,
            )?;
            let rows = stmt.query_map([], |row| {
                let input: i64 = row.get(1)?;
                let output: i64 = row.get(2)?;
                let cache_read: i64 = row.get(3)?;
                let cache_creation: i64 = row.get(4)?;
                Ok(ModelStats {
                    model: row.get(0)?,
                    input_tokens: input,
                    output_tokens: output,
                    cache_read_tokens: cache_read,
                    cache_creation_tokens: cache_creation,
                    tokens: input + output + cache_read + cache_creation,
                    cost_usd: row.get(5)?,
                    calls: row.get(6)?,
                })
            })?;
            for row in rows {
                by_model.push(row?);
            }
        }

        // By tool
        let mut by_tool = Vec::new();
        {
            let mut stmt = conn.prepare(
                r#"
                SELECT
                    tc.tool_name,
                    COUNT(*) as calls,
                    COALESCE(AVG(tr.duration_ms), 0) as avg_duration,
                    COALESCE(AVG(CAST(tr.success AS FLOAT)), 1.0) as success_rate,
                    SUM(CASE WHEN tr.is_rejection = 1 THEN 1 ELSE 0 END) as rejections,
                    SUM(CASE WHEN tr.success = 0 AND tr.is_rejection = 0 THEN 1 ELSE 0 END) as errors
                FROM tool_calls tc
                LEFT JOIN tool_results tr ON tc.id = tr.call_id
                GROUP BY tc.tool_name
                ORDER BY calls DESC
                "#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(ToolStats {
                    tool: row.get(0)?,
                    calls: row.get(1)?,
                    avg_duration_ms: row.get(2)?,
                    success_rate: row.get(3)?,
                    rejections: row.get(4)?,
                    errors: row.get(5)?,
                })
            })?;
            for row in rows {
                by_tool.push(row?);
            }
        }

        Ok(LifetimeStats {
            total_sessions,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
            total_tokens,
            total_cost_usd: total_cost,
            cache_savings_usd,
            total_tool_calls,
            total_thinking_blocks: total_thinking,
            total_prompts,
            first_session,
            last_session,
            by_model,
            by_tool,
        })
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Semantic Search (Vector Similarity)
    // ═════════════════════════════════════════════════════════════════════════

    /// Search thinking blocks using semantic similarity
    ///
    /// Requires embeddings to be enabled and indexed. Falls back to FTS if
    /// embeddings are not available.
    ///
    /// # Arguments
    /// * `query_embedding` - Pre-computed embedding for the search query
    /// * `limit` - Maximum number of results
    ///
    /// # Returns
    /// Results sorted by cosine similarity (higher = more relevant)
    pub fn search_thinking_semantic(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> anyhow::Result<Vec<ThinkingMatch>> {
        use super::embedding_indexer::{blob_to_embedding, cosine_similarity};

        let conn = self.conn()?;

        // Fetch all embedded thinking blocks
        let sql = r#"
            SELECT
                t.id,
                t.session_id,
                t.timestamp,
                t.content,
                t.tokens,
                e.embedding
            FROM thinking_blocks t
            JOIN thinking_embeddings e ON t.id = e.content_id
        "#;

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let session_id: Option<String> = row.get(1)?;
            let timestamp: String = row.get(2)?;
            let content: String = row.get(3)?;
            let tokens: Option<u32> = row.get(4)?;
            let embedding_blob: Vec<u8> = row.get(5)?;
            Ok((id, session_id, timestamp, content, tokens, embedding_blob))
        })?;

        // Compute similarities and sort
        let mut results: Vec<(f32, ThinkingMatch)> = Vec::new();
        for row in rows {
            let (_, session_id, timestamp, content, tokens, embedding_blob) = row?;
            let doc_embedding = blob_to_embedding(&embedding_blob);
            let similarity = cosine_similarity(query_embedding, &doc_embedding);

            results.push((
                similarity,
                ThinkingMatch {
                    session_id,
                    timestamp,
                    content,
                    tokens,
                    rank: -similarity as f64, // Convert to rank (lower = better for consistency)
                },
            ));
        }

        // Sort by similarity descending
        results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Take top results
        let matches: Vec<ThinkingMatch> = results.into_iter().take(limit).map(|(_, m)| m).collect();

        Ok(matches)
    }

    /// Search user prompts using semantic similarity
    pub fn search_prompts_semantic(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> anyhow::Result<Vec<PromptMatch>> {
        use super::embedding_indexer::{blob_to_embedding, cosine_similarity};

        let conn = self.conn()?;

        let sql = r#"
            SELECT
                p.id,
                p.session_id,
                p.timestamp,
                p.content,
                e.embedding
            FROM user_prompts p
            JOIN prompts_embeddings e ON p.id = e.content_id
        "#;

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let session_id: Option<String> = row.get(1)?;
            let timestamp: String = row.get(2)?;
            let content: String = row.get(3)?;
            let embedding_blob: Vec<u8> = row.get(4)?;
            Ok((id, session_id, timestamp, content, embedding_blob))
        })?;

        let mut results: Vec<(f32, PromptMatch)> = Vec::new();
        for row in rows {
            let (_, session_id, timestamp, content, embedding_blob) = row?;
            let doc_embedding = blob_to_embedding(&embedding_blob);
            let similarity = cosine_similarity(query_embedding, &doc_embedding);

            results.push((
                similarity,
                PromptMatch {
                    session_id,
                    timestamp,
                    content,
                    rank: -similarity as f64,
                },
            ));
        }

        results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        Ok(results.into_iter().take(limit).map(|(_, m)| m).collect())
    }

    /// Search assistant responses using semantic similarity
    pub fn search_responses_semantic(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> anyhow::Result<Vec<ResponseMatch>> {
        use super::embedding_indexer::{blob_to_embedding, cosine_similarity};

        let conn = self.conn()?;

        let sql = r#"
            SELECT
                r.id,
                r.session_id,
                r.timestamp,
                r.content,
                e.embedding
            FROM assistant_responses r
            JOIN responses_embeddings e ON r.id = e.content_id
        "#;

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let session_id: Option<String> = row.get(1)?;
            let timestamp: String = row.get(2)?;
            let content: String = row.get(3)?;
            let embedding_blob: Vec<u8> = row.get(4)?;
            Ok((id, session_id, timestamp, content, embedding_blob))
        })?;

        let mut results: Vec<(f32, ResponseMatch)> = Vec::new();
        for row in rows {
            let (_, session_id, timestamp, content, embedding_blob) = row?;
            let doc_embedding = blob_to_embedding(&embedding_blob);
            let similarity = cosine_similarity(query_embedding, &doc_embedding);

            results.push((
                similarity,
                ResponseMatch {
                    session_id,
                    timestamp,
                    content,
                    rank: -similarity as f64,
                },
            ));
        }

        results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        Ok(results.into_iter().take(limit).map(|(_, m)| m).collect())
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Hybrid Search (FTS + Vector via Reciprocal Rank Fusion)
    // ═════════════════════════════════════════════════════════════════════════

    /// Combined context recovery using hybrid search
    ///
    /// Combines FTS5 keyword search with semantic vector search using
    /// Reciprocal Rank Fusion (RRF). This typically provides better results
    /// than either method alone.
    ///
    /// # Algorithm
    ///
    /// For each document, compute:
    /// `RRF_score = 1/(k + fts_rank) + 1/(k + vec_rank)`
    ///
    /// Where k=60 (standard RRF constant). Higher scores indicate better matches.
    ///
    /// # Arguments
    /// * `query` - The text search query
    /// * `query_embedding` - Optional pre-computed embedding for semantic search
    /// * `limit` - Maximum total results
    /// * `mode` - How to interpret the FTS query
    ///
    /// # Returns
    /// Combined results sorted by RRF score (higher = more relevant)
    ///
    /// # Note
    /// Global (non-user-scoped) version. API uses recover_context_hybrid_user instead.
    /// Kept for MCP tool that searches across all users.
    #[allow(dead_code)]
    pub fn recover_context_hybrid(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        limit: usize,
        mode: SearchMode,
    ) -> anyhow::Result<Vec<ContextMatch>> {
        const RRF_K: f64 = 60.0;

        // Get FTS results
        let fts_results = self.recover_context(query, limit * 2, mode)?;

        // If no embedding provided, return FTS results only
        let query_embedding = match query_embedding {
            Some(e) => e,
            None => return Ok(fts_results.into_iter().take(limit).collect()),
        };

        // Get semantic results
        let mut semantic_results = Vec::new();

        for m in self.search_thinking_semantic(query_embedding, limit * 2)? {
            semantic_results.push(ContextMatch {
                match_type: MatchType::Thinking,
                session_id: m.session_id,
                timestamp: m.timestamp,
                content: m.content,
                rank: m.rank,
            });
        }

        for m in self.search_prompts_semantic(query_embedding, limit * 2)? {
            semantic_results.push(ContextMatch {
                match_type: MatchType::UserPrompt,
                session_id: m.session_id,
                timestamp: m.timestamp,
                content: m.content,
                rank: m.rank,
            });
        }

        for m in self.search_responses_semantic(query_embedding, limit * 2)? {
            semantic_results.push(ContextMatch {
                match_type: MatchType::AssistantResponse,
                session_id: m.session_id,
                timestamp: m.timestamp,
                content: m.content,
                rank: m.rank,
            });
        }

        // Build document map: (session_id, timestamp, content_hash) -> scores
        use std::collections::HashMap;

        #[derive(Hash, Eq, PartialEq, Clone)]
        struct DocKey {
            session_id: Option<String>,
            timestamp: String,
        }

        struct DocScores {
            fts_rank: Option<usize>,
            vec_rank: Option<usize>,
            match_info: ContextMatch,
        }

        let mut doc_map: HashMap<DocKey, DocScores> = HashMap::new();

        // Add FTS results with rank
        for (rank, m) in fts_results.iter().enumerate() {
            let key = DocKey {
                session_id: m.session_id.clone(),
                timestamp: m.timestamp.clone(),
            };

            doc_map.insert(
                key,
                DocScores {
                    fts_rank: Some(rank),
                    vec_rank: None,
                    match_info: m.clone(),
                },
            );
        }

        // Add/update semantic results with rank
        for (rank, m) in semantic_results.iter().enumerate() {
            let key = DocKey {
                session_id: m.session_id.clone(),
                timestamp: m.timestamp.clone(),
            };

            if let Some(scores) = doc_map.get_mut(&key) {
                scores.vec_rank = Some(rank);
            } else {
                doc_map.insert(
                    key,
                    DocScores {
                        fts_rank: None,
                        vec_rank: Some(rank),
                        match_info: m.clone(),
                    },
                );
            }
        }

        // Compute RRF scores
        let mut scored_results: Vec<(f64, ContextMatch)> = doc_map
            .into_values()
            .map(|scores| {
                let fts_score = scores
                    .fts_rank
                    .map(|r| 1.0 / (RRF_K + r as f64))
                    .unwrap_or(0.0);
                let vec_score = scores
                    .vec_rank
                    .map(|r| 1.0 / (RRF_K + r as f64))
                    .unwrap_or(0.0);
                let rrf_score = fts_score + vec_score;

                // Store negative RRF as rank (lower = better, for API consistency)
                let mut match_info = scores.match_info;
                match_info.rank = -rrf_score;

                (rrf_score, match_info)
            })
            .collect();

        // Sort by RRF score descending (higher = better)
        scored_results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Return top results
        Ok(scored_results
            .into_iter()
            .take(limit)
            .map(|(_, m)| m)
            .collect())
    }

    /// Hybrid context recovery for a specific user
    ///
    /// Combines FTS5 keyword search with semantic vector search using
    /// Reciprocal Rank Fusion (RRF), filtered to a specific user.
    /// Falls back to FTS-only if no embedding is provided.
    pub fn recover_context_hybrid_user(
        &self,
        user_id: &str,
        query: &str,
        query_embedding: Option<&[f32]>,
        limit: usize,
        mode: SearchMode,
    ) -> anyhow::Result<Vec<ContextMatch>> {
        const RRF_K: f64 = 60.0;

        // Get FTS results (user-scoped)
        let fts_results = self.recover_user_context(user_id, query, limit * 2, mode)?;

        // If no embedding provided, return FTS results only
        let query_embedding = match query_embedding {
            Some(e) => e,
            None => return Ok(fts_results.into_iter().take(limit).collect()),
        };

        // Get semantic results and filter by user
        // Note: semantic search returns all matches, we filter by user here
        let mut semantic_results = Vec::new();

        for m in self.search_thinking_semantic(query_embedding, limit * 4)? {
            // Filter by user - session_id format is "user_id/session_uuid"
            if let Some(ref session_id) = m.session_id {
                if session_id.starts_with(user_id) || session_id.contains(&format!("/{}", user_id))
                {
                    semantic_results.push(ContextMatch {
                        match_type: MatchType::Thinking,
                        session_id: m.session_id,
                        timestamp: m.timestamp,
                        content: m.content,
                        rank: m.rank,
                    });
                }
            }
        }

        for m in self.search_prompts_semantic(query_embedding, limit * 4)? {
            if let Some(ref session_id) = m.session_id {
                if session_id.starts_with(user_id) || session_id.contains(&format!("/{}", user_id))
                {
                    semantic_results.push(ContextMatch {
                        match_type: MatchType::UserPrompt,
                        session_id: m.session_id,
                        timestamp: m.timestamp,
                        content: m.content,
                        rank: m.rank,
                    });
                }
            }
        }

        for m in self.search_responses_semantic(query_embedding, limit * 4)? {
            if let Some(ref session_id) = m.session_id {
                if session_id.starts_with(user_id) || session_id.contains(&format!("/{}", user_id))
                {
                    semantic_results.push(ContextMatch {
                        match_type: MatchType::AssistantResponse,
                        session_id: m.session_id,
                        timestamp: m.timestamp,
                        content: m.content,
                        rank: m.rank,
                    });
                }
            }
        }

        // Truncate semantic results to reasonable limit after filtering
        semantic_results.truncate(limit * 2);

        // Build document map for RRF fusion
        use std::collections::HashMap;

        #[derive(Hash, Eq, PartialEq, Clone)]
        struct DocKey {
            session_id: Option<String>,
            timestamp: String,
        }

        struct DocScores {
            fts_rank: Option<usize>,
            vec_rank: Option<usize>,
            match_info: ContextMatch,
        }

        let mut doc_map: HashMap<DocKey, DocScores> = HashMap::new();

        // Add FTS results with rank
        for (rank, m) in fts_results.iter().enumerate() {
            let key = DocKey {
                session_id: m.session_id.clone(),
                timestamp: m.timestamp.clone(),
            };

            doc_map.insert(
                key,
                DocScores {
                    fts_rank: Some(rank),
                    vec_rank: None,
                    match_info: m.clone(),
                },
            );
        }

        // Add/update semantic results with rank
        for (rank, m) in semantic_results.iter().enumerate() {
            let key = DocKey {
                session_id: m.session_id.clone(),
                timestamp: m.timestamp.clone(),
            };

            if let Some(scores) = doc_map.get_mut(&key) {
                scores.vec_rank = Some(rank);
            } else {
                doc_map.insert(
                    key,
                    DocScores {
                        fts_rank: None,
                        vec_rank: Some(rank),
                        match_info: m.clone(),
                    },
                );
            }
        }

        // Compute RRF scores
        let mut scored_results: Vec<(f64, ContextMatch)> = doc_map
            .into_values()
            .map(|scores| {
                let fts_score = scores
                    .fts_rank
                    .map(|r| 1.0 / (RRF_K + r as f64))
                    .unwrap_or(0.0);
                let vec_score = scores
                    .vec_rank
                    .map(|r| 1.0 / (RRF_K + r as f64))
                    .unwrap_or(0.0);
                let rrf_score = fts_score + vec_score;

                // Store negative RRF as rank (lower = better, for API consistency)
                let mut match_info = scores.match_info;
                match_info.rank = -rrf_score;

                (rrf_score, match_info)
            })
            .collect();

        // Sort by RRF score descending (higher = better)
        scored_results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Return top results
        Ok(scored_results
            .into_iter()
            .take(limit)
            .map(|(_, m)| m)
            .collect())
    }

    /// Check if embeddings are available for hybrid search
    pub fn has_embeddings(&self) -> anyhow::Result<bool> {
        let conn = self.conn()?;

        // Check if embedding_config table exists and has a config
        let has_config: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM embedding_config WHERE provider != 'none')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !has_config {
            return Ok(false);
        }

        // Check if any embeddings exist
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM thinking_embeddings", [], |row| {
            row.get(0)
        })?;

        Ok(count > 0)
    }

    /// Get embedding statistics
    pub fn embedding_stats(&self) -> anyhow::Result<EmbeddingStats> {
        let conn = self.conn()?;

        // Get config
        let config: Option<(String, String, i64)> = conn
            .query_row(
                "SELECT provider, model, dimensions FROM embedding_config WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();

        let (provider, model, dimensions) =
            config.unwrap_or_else(|| ("none".to_string(), "".to_string(), 0));

        // Count embeddings
        let thinking_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM thinking_embeddings", [], |row| {
                row.get(0)
            })?;
        let prompts_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM prompts_embeddings", [], |row| {
                row.get(0)
            })?;
        let responses_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM responses_embeddings", [], |row| {
                row.get(0)
            })?;

        // Count total content
        let thinking_total: i64 =
            conn.query_row("SELECT COUNT(*) FROM thinking_blocks", [], |row| row.get(0))?;
        let prompts_total: i64 =
            conn.query_row("SELECT COUNT(*) FROM user_prompts", [], |row| row.get(0))?;
        let responses_total: i64 =
            conn.query_row("SELECT COUNT(*) FROM assistant_responses", [], |row| {
                row.get(0)
            })?;

        let embedded = thinking_count + prompts_count + responses_count;
        let total = thinking_total + prompts_total + responses_total;

        Ok(EmbeddingStats {
            provider,
            model,
            dimensions: dimensions as usize,
            thinking_embedded: thinking_count as u64,
            thinking_total: thinking_total as u64,
            prompts_embedded: prompts_count as u64,
            prompts_total: prompts_total as u64,
            responses_embedded: responses_count as u64,
            responses_total: responses_total as u64,
            total_embedded: embedded as u64,
            total_documents: total as u64,
            progress_pct: if total > 0 {
                (embedded as f64 / total as f64) * 100.0
            } else {
                100.0
            },
        })
    }

    /// Find a session by transcript_path
    ///
    /// Used for session reconnection after proxy restart. Returns the most recent
    /// session that was using this transcript file.
    ///
    /// # Arguments
    /// * `transcript_path` - Path to Claude Code's transcript file
    ///
    /// # Returns
    /// `Some((session_id, user_id))` if found, `None` otherwise
    pub fn find_session_by_transcript(
        &self,
        transcript_path: &str,
    ) -> anyhow::Result<Option<(String, String)>> {
        let conn = self.pool.get()?;

        let result: Option<(String, String)> = conn
            .query_row(
                r#"
                SELECT id, user_id FROM sessions
                WHERE transcript_path = ?1
                ORDER BY started_at DESC
                LIMIT 1
                "#,
                params![transcript_path],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        Ok(result)
    }
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
