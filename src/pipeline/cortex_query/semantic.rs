//! Semantic (vector) search methods
//!
//! Contains methods for searching using embedding vectors and cosine similarity.
//! Requires embeddings to be enabled and indexed.

use super::types::{PromptMatch, ResponseMatch, ThinkingMatch};
use super::CortexQuery;

impl CortexQuery {
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
        use crate::pipeline::embedding_indexer::{blob_to_embedding, cosine_similarity};

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
        use crate::pipeline::embedding_indexer::{blob_to_embedding, cosine_similarity};

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
        use crate::pipeline::embedding_indexer::{blob_to_embedding, cosine_similarity};

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
}
