//! FTS5 full-text search methods
//!
//! Contains all FTS5-based search functionality for thinking blocks,
//! prompts, responses, and todos. Both global and user-scoped variants.

use super::types::{
    ContextMatch, MatchType, PromptMatch, ResponseMatch, SearchMode, ThinkingMatch, TodoMatch,
};
use super::CortexQuery;
use rusqlite::params;

impl CortexQuery {
    // =========================================================================
    // Global FTS Search
    // =========================================================================

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

    // =========================================================================
    // User-Scoped FTS Search
    // =========================================================================

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
}
