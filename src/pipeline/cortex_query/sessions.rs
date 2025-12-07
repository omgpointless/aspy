//! Session-related queries
//!
//! Contains methods for querying session history and finding sessions
//! by various criteria.

use super::CortexQuery;
use crate::proxy::api::{SessionHistoryItem, SessionStatsSummary};
use rusqlite::{params, OptionalExtension};

impl CortexQuery {
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
    ) -> anyhow::Result<Vec<SessionHistoryItem>> {
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

            Ok(SessionHistoryItem {
                session_id,
                user_id,
                claude_session_id: None, // Not stored in DB
                started: started_at,
                ended: ended_at,
                source: source.unwrap_or_else(|| "unknown".to_string()),
                end_reason: None,      // Not stored in DB currently
                transcript_path: None, // Not stored in DB currently
                stats: SessionStatsSummary {
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

    /// Get the last context tokens for a session
    ///
    /// Used when reconnecting to a session to estimate the context window state.
    /// Returns the sum of input + cache_creation + cache_read tokens from the
    /// most recent non-haiku API call for this session.
    ///
    /// # Arguments
    /// * `session_id` - The session identifier
    ///
    /// # Returns
    /// `Some(tokens)` if found, `None` if no API usage data exists
    pub fn get_session_last_context(&self, session_id: &str) -> anyhow::Result<Option<u64>> {
        let conn = self.pool.get()?;

        // Get the most recent non-haiku API usage for context estimation
        // Context = input_tokens + cache_creation_tokens + cache_read_tokens
        let result: Option<u64> = conn
            .query_row(
                r#"
                SELECT (input_tokens + cache_creation_tokens + cache_read_tokens) as context_tokens
                FROM api_usage
                WHERE session_id = ?1 AND model NOT LIKE '%haiku%'
                ORDER BY timestamp DESC
                LIMIT 1
                "#,
                params![session_id],
                |row| row.get(0),
            )
            .optional()?;

        Ok(result)
    }
}
