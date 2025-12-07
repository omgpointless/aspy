//! Lifetime statistics queries
//!
//! Contains methods for aggregating statistics across sessions,
//! including token counts, costs, and breakdowns by model/tool.

use super::types::{LifetimeStats, ModelStats, ToolStats};
use super::CortexQuery;
use rusqlite::params;

impl CortexQuery {
    // =========================================================================
    // User-Scoped Statistics
    // =========================================================================

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

    // =========================================================================
    // Global Statistics
    // =========================================================================

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
}
