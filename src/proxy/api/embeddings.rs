// Embeddings endpoints - Semantic search and hybrid context recovery

use super::ApiError;
use crate::pipeline::cortex_query::{ContextMatch, SearchMode};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

fn default_limit() -> usize {
    10
}

// ============================================================================
// Embedding Indexer Status
// ============================================================================

/// Live indexer status response (from running indexer)
#[derive(Debug, Serialize)]
pub struct LiveIndexerStatusResponse {
    pub enabled: bool,
    pub running: bool,
    pub provider: String,
    pub model: String,
    pub dimensions: usize,
    pub documents_indexed: u64,
    pub documents_pending: u64,
    pub index_progress_pct: f64,
}

/// GET /api/cortex/embeddings/status - Get embedding indexer status
///
/// Returns status of the embedding indexer: provider, model, progress.
/// Uses live indexer handle if available, falls back to database.
pub async fn cortex_embedding_status(
    State(state): State<crate::proxy::ProxyState>,
) -> Result<Json<LiveIndexerStatusResponse>, ApiError> {
    // Try to get live status from running indexer
    if let Some(ref handle) = state.embedding_indexer {
        let status = handle.status();
        return Ok(Json(LiveIndexerStatusResponse {
            enabled: status.is_ready,
            running: true,
            provider: format!("{:?}", status.provider).to_lowercase(),
            model: status.model,
            dimensions: status.dimensions,
            documents_indexed: status.documents_indexed,
            documents_pending: status.documents_pending,
            index_progress_pct: status.index_progress_pct,
        }));
    }

    // Fall back to database stats
    let query_interface = state
        .cortex_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Cortex query interface not available".to_string()))?;

    let stats = query_interface
        .embedding_stats()
        .map_err(|e| ApiError::Internal(format!("Failed to get embedding stats: {}", e)))?;

    Ok(Json(LiveIndexerStatusResponse {
        enabled: stats.provider != "none",
        running: false, // Indexer not running, using DB fallback
        provider: stats.provider,
        model: stats.model,
        dimensions: stats.dimensions,
        documents_indexed: stats.total_embedded,
        documents_pending: stats.total_documents - stats.total_embedded,
        index_progress_pct: stats.progress_pct,
    }))
}

// ============================================================================
// Reindex and Poll Endpoints
// ============================================================================

/// Response for reindex trigger
#[derive(Debug, Serialize)]
pub struct ReindexResponse {
    pub triggered: bool,
    pub message: String,
}

/// POST /api/cortex/embeddings/reindex - Trigger a full re-index
///
/// Signals the running embedding indexer to clear and re-process all content.
/// Requires the indexer to be running.
pub async fn cortex_embedding_reindex(
    State(state): State<crate::proxy::ProxyState>,
) -> Result<Json<ReindexResponse>, ApiError> {
    if let Some(ref handle) = state.embedding_indexer {
        handle.trigger_reindex();
        Ok(Json(ReindexResponse {
            triggered: true,
            message: "Reindex triggered. The indexer will clear existing embeddings and re-process all content.".to_string(),
        }))
    } else {
        Err(ApiError::NotFound(
            "Embedding indexer not running. Start aspy with embeddings configured.".to_string(),
        ))
    }
}

/// POST /api/cortex/embeddings/poll - Trigger a poll for new content
///
/// Signals the running embedding indexer to check for un-embedded content.
pub async fn cortex_embedding_poll(
    State(state): State<crate::proxy::ProxyState>,
) -> Result<Json<ReindexResponse>, ApiError> {
    if let Some(ref handle) = state.embedding_indexer {
        handle.trigger_poll();
        Ok(Json(ReindexResponse {
            triggered: true,
            message: "Poll triggered. The indexer will check for un-embedded content.".to_string(),
        }))
    } else {
        Err(ApiError::NotFound(
            "Embedding indexer not running.".to_string(),
        ))
    }
}

// ============================================================================
// Hybrid Context Search
// ============================================================================

/// Response for hybrid context search
#[derive(Debug, Serialize)]
pub struct HybridContextResponse {
    pub topic: String,
    pub mode: String,
    pub search_type: String, // "fts_only" or "hybrid"
    pub results: Vec<ContextMatch>,
}

/// Query params for hybrid context search
#[derive(Debug, Deserialize)]
pub struct HybridContextQuery {
    /// Topic to search for
    #[serde(rename = "topic")]
    pub topic: String,
    /// Maximum results to return
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Search mode for FTS component
    #[serde(default)]
    pub mode: SearchMode,
}

/// GET /api/cortex/context/hybrid/user/:user_id - Hybrid context recovery
///
/// Combines FTS5 keyword search with semantic vector search using
/// Reciprocal Rank Fusion (RRF). Falls back to FTS-only if embeddings
/// are not available.
///
/// Query params:
///   - topic: Topic to search for (required)
///   - limit: Max results (default: 10, max: 50)
///   - mode: phrase|natural|raw (default: phrase)
pub async fn cortex_context_hybrid_user(
    State(state): State<crate::proxy::ProxyState>,
    Path(user_id): Path<String>,
    Query(params): Query<HybridContextQuery>,
) -> Result<Json<HybridContextResponse>, ApiError> {
    use crate::config::Config;
    use crate::pipeline::embeddings::{create_provider, AuthMethod, EmbeddingConfig, ProviderType};

    let query_interface = state
        .cortex_query
        .as_ref()
        .ok_or_else(|| ApiError::NotFound("Cortex query interface not available".to_string()))?;

    let limit = params.limit.min(50);

    // Check if embeddings are available
    let has_embeddings = query_interface.has_embeddings().unwrap_or(false);

    // Try to create query embedding if embeddings are enabled
    let query_embedding = if has_embeddings {
        // Load config to get embedding settings
        let config = Config::from_env();

        if config.embeddings.is_enabled() {
            // Create embedding provider for query
            let provider_type = match config.embeddings.provider.as_str() {
                "local" => ProviderType::Local,
                "remote" => ProviderType::Remote,
                _ => ProviderType::None,
            };

            let auth_method = match config.embeddings.auth_method.as_str() {
                "api-key" => AuthMethod::ApiKey,
                _ => AuthMethod::Bearer,
            };

            // Use the resolved API key from config (supports ASPY_EMBEDDINGS_API_KEY and others)
            let api_key = config.embeddings.api_key.clone();

            let embed_config = EmbeddingConfig {
                provider: provider_type,
                model: config.embeddings.model.clone(),
                api_key,
                api_base: config.embeddings.api_base.clone(),
                api_version: config.embeddings.api_version.clone(),
                auth_method,
                dimensions: None,
                batch_size: 1,    // Only need one embedding
                timeout_secs: 10, // Short timeout for query
            };

            let provider = create_provider(&embed_config);

            if provider.is_ready() {
                match provider.embed(&params.topic) {
                    Ok(result) => Some(result.embedding),
                    Err(e) => {
                        tracing::warn!("Failed to embed query: {}", e);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Perform hybrid or FTS-only search
    let (search_type, results) = if let Some(ref embedding) = query_embedding {
        let results = query_interface
            .recover_context_hybrid_user(
                &user_id,
                &params.topic,
                Some(embedding),
                limit,
                params.mode,
            )
            .map_err(|e| ApiError::Internal(format!("Hybrid search failed: {}", e)))?;
        ("hybrid".to_string(), results)
    } else {
        let results = query_interface
            .recover_user_context(&user_id, &params.topic, limit, params.mode)
            .map_err(|e| ApiError::Internal(format!("Search failed: {}", e)))?;
        ("fts_only".to_string(), results)
    };

    Ok(Json(HybridContextResponse {
        topic: params.topic,
        mode: format!("{:?}", params.mode),
        search_type,
        results,
    }))
}
