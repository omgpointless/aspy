//! Query interface for lifestats database
//!
//! Provides structured queries for context recovery, used by MCP tools.
//! Uses connection pooling for efficient concurrent access.
//!
//! # Architecture
//!
//! ```text
//! MCP Tools / HTTP API
//!         │
//!         └──→ LifestatsQuery (r2d2 pool)
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
//! The lifestats database uses WAL (Write-Ahead Logging) mode, which allows
//! multiple concurrent readers while the writer thread is active. The connection
//! pool manages up to 4 read-only connections for query parallelism.

use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
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
/// use aspy::pipeline::lifestats_query::SearchMode;
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
                    let has_wildcard = token.ends_with('*');
                    let base = if has_wildcard {
                        &token[..token.len() - 1]
                    } else {
                        token
                    };

                    // Escape quotes and parentheses
                    let escaped = base
                        .replace('"', "\"\"")
                        .replace('(', "")
                        .replace(')', "")
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
    pub total_tokens: i64,
    pub total_cost_usd: f64,
    pub total_tool_calls: i64,
    pub total_thinking_blocks: i64,
    pub first_session: Option<String>,
    pub last_session: Option<String>,
    pub by_model: Vec<ModelStats>,
    pub by_tool: Vec<ToolStats>,
}

/// Statistics breakdown by model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStats {
    pub model: String,
    pub tokens: i64,
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
}

/// Query interface for lifestats database
///
/// Uses connection pooling for efficient concurrent access.
///
/// # Example
///
/// ```rust,no_run
/// use aspy::pipeline::lifestats_query::{LifestatsQuery, SearchMode};
///
/// # fn main() -> anyhow::Result<()> {
/// let query = LifestatsQuery::new("./data/lifestats.db")?;
///
/// // Search thinking blocks
/// let results = query.search_thinking("solarized theme", 10, SearchMode::Phrase)?;
/// for m in results {
///     println!("[{}] {}", m.timestamp, m.content);
/// }
/// # Ok(())
/// # }
/// ```
pub struct LifestatsQuery {
    pool: Pool<SqliteConnectionManager>,
}

impl LifestatsQuery {
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
        conn.execute("SELECT 1", [])?;

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
        results.sort_by(|a, b| a.rank.partial_cmp(&b.rank).unwrap_or(std::cmp::Ordering::Equal));

        // Limit total results
        results.truncate(limit);

        Ok(results)
    }

    /// Get lifetime statistics
    ///
    /// Aggregates data across all sessions to provide summary statistics.
    ///
    /// # Returns
    /// Statistics including total tokens, cost, tool calls, and breakdowns by model and tool.
    pub fn get_lifetime_stats(&self) -> anyhow::Result<LifetimeStats> {
        let conn = self.conn()?;

        // Aggregate stats from api_usage (more accurate than sessions table)
        let (total_tokens, total_cost): (i64, f64) = conn.query_row(
            r#"
            SELECT
                COALESCE(SUM(input_tokens + output_tokens + cache_read_tokens), 0),
                COALESCE(SUM(cost_usd), 0)
            FROM api_usage
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

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

        let total_tool_calls: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tool_calls",
            [],
            |row| row.get(0),
        )?;

        let total_thinking: i64 = conn.query_row(
            "SELECT COUNT(*) FROM thinking_blocks",
            [],
            |row| row.get(0),
        )?;

        // By model
        let mut by_model = Vec::new();
        {
            let mut stmt = conn.prepare(
                r#"
                SELECT
                    model,
                    SUM(input_tokens + output_tokens + cache_read_tokens) as tokens,
                    SUM(cost_usd) as cost,
                    COUNT(*) as calls
                FROM api_usage
                GROUP BY model
                ORDER BY tokens DESC
                "#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(ModelStats {
                    model: row.get(0)?,
                    tokens: row.get(1)?,
                    cost_usd: row.get(2)?,
                    calls: row.get(3)?,
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
                    COALESCE(AVG(CAST(tr.success AS FLOAT)), 1.0) as success_rate
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
                })
            })?;
            for row in rows {
                by_tool.push(row?);
            }
        }

        Ok(LifetimeStats {
            total_sessions,
            total_tokens,
            total_cost_usd: total_cost,
            total_tool_calls,
            total_thinking_blocks: total_thinking,
            first_session,
            last_session,
            by_model,
            by_tool,
        })
    }
}
