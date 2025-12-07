//! Hybrid search (FTS + Vector via Reciprocal Rank Fusion)
//!
//! Combines FTS5 keyword search with semantic vector search using RRF.
//! Also contains embedding status/statistics utilities.

use super::types::{ContextMatch, EmbeddingStats, MatchType, SearchMode};
use super::CortexQuery;
use std::collections::HashMap;

impl CortexQuery {
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

        // Perform RRF fusion
        rrf_fusion(fts_results, semantic_results, limit, RRF_K)
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

        // Perform RRF fusion
        rrf_fusion(fts_results, semantic_results, limit, RRF_K)
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
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Performs Reciprocal Rank Fusion on FTS and semantic results
fn rrf_fusion(
    fts_results: Vec<ContextMatch>,
    semantic_results: Vec<ContextMatch>,
    limit: usize,
    rrf_k: f64,
) -> anyhow::Result<Vec<ContextMatch>> {
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
                .map(|r| 1.0 / (rrf_k + r as f64))
                .unwrap_or(0.0);
            let vec_score = scores
                .vec_rank
                .map(|r| 1.0 / (rrf_k + r as f64))
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
