//! Background embedding indexer for semantic search
//!
//! Polls for un-embedded content and generates embeddings asynchronously.
//! Uses a dedicated OS thread to avoid blocking the async runtime.
//!
//! # Architecture
//!
//! ```text
//! LifestatsProcessor (stores content)
//!         │
//!         └──→ SQLite (thinking_blocks, user_prompts, responses)
//!                 │
//!                 └──→ EmbeddingIndexer (polls for un-embedded rows)
//!                         │
//!                         ├──→ EmbeddingProvider.embed_batch()
//!                         │
//!                         └──→ SQLite (thinking_embeddings, prompts_embeddings, responses_embeddings)
//! ```
//!
//! # Design Principles
//!
//! 1. **Non-blocking**: Runs on dedicated OS thread
//! 2. **Catch-up**: Processes backlog of un-embedded content
//! 3. **Rate-aware**: Respects provider rate limits
//! 4. **Config-aware**: Re-indexes if provider/model changes

use super::embeddings::{
    BatchEmbeddingResult, Embedding, EmbeddingConfig, EmbeddingError, EmbeddingProvider,
    EmbeddingStatus, ProviderType,
};
use super::CompletionSignal;
use crate::util::truncate_utf8_safe;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Configuration for the embedding indexer
#[derive(Debug, Clone)]
pub struct IndexerConfig {
    /// Path to SQLite database
    pub db_path: PathBuf,
    /// Embedding provider configuration
    pub embedding_config: EmbeddingConfig,
    /// How often to poll for new content (seconds)
    pub poll_interval: Duration,
    /// Batch size for embedding requests
    pub batch_size: usize,
    /// Delay between batches (for rate limiting)
    pub batch_delay: Duration,
    /// Maximum content length to embed (truncate longer)
    pub max_content_length: usize,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("./data/lifestats.db"),
            embedding_config: EmbeddingConfig::default(),
            poll_interval: Duration::from_secs(30),
            batch_size: 32,
            batch_delay: Duration::from_millis(100),
            max_content_length: 8000, // ~2k tokens for most models
        }
    }
}

/// Metrics for the embedding indexer
#[derive(Debug, Default)]
pub struct IndexerMetrics {
    /// Documents successfully embedded
    pub documents_embedded: AtomicU64,
    /// Documents pending embedding
    pub documents_pending: AtomicU64,
    /// Embedding errors encountered
    pub embedding_errors: AtomicU64,
    /// Total batches processed
    pub batches_processed: AtomicU64,
    /// Whether indexer is currently processing
    pub is_processing: AtomicBool,
}

impl IndexerMetrics {
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            documents_embedded: self.documents_embedded.load(Ordering::Relaxed),
            documents_pending: self.documents_pending.load(Ordering::Relaxed),
            embedding_errors: self.embedding_errors.load(Ordering::Relaxed),
            batches_processed: self.batches_processed.load(Ordering::Relaxed),
            is_processing: self.is_processing.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of indexer metrics for monitoring
///
/// Note: Some fields reserved for future metrics API endpoint that exposes
/// detailed indexer health (error rates, batch throughput). Currently only
/// documents_embedded/pending are used by status() endpoint.
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub documents_embedded: u64,
    pub documents_pending: u64,
    #[allow(dead_code)] // Reserved for /api/lifestats/embeddings/metrics endpoint
    pub embedding_errors: u64,
    #[allow(dead_code)] // Reserved for /api/lifestats/embeddings/metrics endpoint
    pub batches_processed: u64,
    #[allow(dead_code)] // Reserved for /api/lifestats/embeddings/metrics endpoint
    pub is_processing: bool,
}

/// Content types that can be embedded
#[derive(Debug, Clone, Copy)]
pub enum ContentType {
    Thinking,
    Prompt,
    Response,
}

impl ContentType {
    fn content_table(&self) -> &'static str {
        match self {
            Self::Thinking => "thinking_blocks",
            Self::Prompt => "user_prompts",
            Self::Response => "assistant_responses",
        }
    }

    fn embedding_table(&self) -> &'static str {
        match self {
            Self::Thinking => "thinking_embeddings",
            Self::Prompt => "prompts_embeddings",
            Self::Response => "responses_embeddings",
        }
    }
}

/// Document to be embedded
#[derive(Debug, Clone)]
struct Document {
    id: i64,
    content: String,
    content_type: ContentType,
}

/// Commands sent to the indexer thread
enum IndexerCommand {
    /// Check for new content and embed
    Poll,
    /// Re-index all content (config changed)
    Reindex,
    /// Shutdown the indexer
    Shutdown,
}

/// Clonable handle to the embedding indexer
///
/// Used to interact with the running indexer from multiple contexts
/// (API handlers, CLI, etc.) without owning the thread handle.
#[derive(Clone)]
pub struct IndexerHandle {
    /// Channel to send commands to indexer thread
    tx: SyncSender<IndexerCommand>,
    /// Shared metrics
    metrics: Arc<IndexerMetrics>,
    /// Provider type for status
    provider_type: ProviderType,
    /// Model name for status
    model: String,
    /// Dimensions for status
    dimensions: usize,
}

impl IndexerHandle {
    /// Get current metrics snapshot
    ///
    /// Reserved for detailed metrics API. Currently status() is used instead
    /// which derives key metrics. This exposes raw counters for debugging.
    #[allow(dead_code)]
    pub fn metrics(&self) -> MetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Get current status
    pub fn status(&self) -> EmbeddingStatus {
        let metrics = self.metrics.snapshot();
        let total = metrics.documents_embedded + metrics.documents_pending;
        let progress = if total > 0 {
            (metrics.documents_embedded as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        EmbeddingStatus {
            provider: self.provider_type,
            model: self.model.clone(),
            dimensions: self.dimensions,
            is_ready: self.provider_type != ProviderType::None,
            documents_indexed: metrics.documents_embedded,
            documents_pending: metrics.documents_pending,
            index_progress_pct: progress,
        }
    }

    /// Trigger a poll for new content
    pub fn trigger_poll(&self) {
        let _ = self.tx.try_send(IndexerCommand::Poll);
    }

    /// Trigger a full re-index (clears existing embeddings and re-processes all content)
    pub fn trigger_reindex(&self) {
        let _ = self.tx.send(IndexerCommand::Reindex);
    }
}

/// Background embedding indexer
///
/// Polls for un-embedded content and generates embeddings using the
/// configured provider. Runs on a dedicated OS thread.
pub struct EmbeddingIndexer {
    /// Channel to send commands to indexer thread
    tx: SyncSender<IndexerCommand>,
    /// Handle to indexer thread
    indexer_handle: Option<JoinHandle<()>>,
    /// Completion signal for graceful shutdown
    completion: Arc<CompletionSignal>,
    /// Shared metrics
    metrics: Arc<IndexerMetrics>,
    /// Current configuration
    config: IndexerConfig,
}

impl EmbeddingIndexer {
    /// Create a new embedding indexer
    ///
    /// # Arguments
    /// * `config` - Indexer configuration
    /// * `provider` - The embedding provider to use
    ///
    /// If the provider is not ready (e.g., NoOpProvider), the indexer
    /// will still run but skip embedding operations.
    pub fn new(
        config: IndexerConfig,
        provider: Box<dyn EmbeddingProvider>,
    ) -> anyhow::Result<Self> {
        // Create command channel (small buffer - commands are infrequent)
        let (tx, rx) = mpsc::sync_channel::<IndexerCommand>(10);

        // Shared metrics
        let metrics = Arc::new(IndexerMetrics::default());
        let indexer_metrics = metrics.clone();

        // Completion signal
        let completion = Arc::new(CompletionSignal::new());
        let indexer_completion = completion.clone();

        // Clone config for thread
        let indexer_config = config.clone();

        // Spawn dedicated indexer thread
        let indexer_handle = thread::Builder::new()
            .name("embedding-indexer".into())
            .spawn(move || {
                if let Err(e) = Self::indexer_thread(rx, indexer_config, provider, indexer_metrics)
                {
                    tracing::error!("Embedding indexer thread error: {}", e);
                }
                indexer_completion.complete();
            })?;

        Ok(Self {
            tx,
            indexer_handle: Some(indexer_handle),
            completion,
            metrics,
            config,
        })
    }

    /// Get a clonable handle to this indexer
    ///
    /// The handle can be shared across threads and used to:
    /// - Check indexer status
    /// - Trigger polling/reindexing
    /// - Read metrics
    pub fn handle(&self) -> IndexerHandle {
        IndexerHandle {
            tx: self.tx.clone(),
            metrics: self.metrics.clone(),
            provider_type: self.config.embedding_config.provider,
            model: self.config.embedding_config.model.clone(),
            dimensions: self.config.embedding_config.get_dimensions(),
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Direct methods (convenience wrappers around shared state)
    // These duplicate IndexerHandle methods. Callers use handle() instead.
    // Kept for potential direct access during testing/debugging.
    // ─────────────────────────────────────────────────────────────────────────

    /// Get current metrics snapshot
    #[allow(dead_code)] // Use handle().metrics() - this is direct access for testing
    pub fn metrics(&self) -> MetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Get current status
    #[allow(dead_code)] // Use handle().status() - this is direct access for testing
    pub fn status(&self) -> EmbeddingStatus {
        let metrics = self.metrics.snapshot();
        let total = metrics.documents_embedded + metrics.documents_pending;
        let progress = if total > 0 {
            (metrics.documents_embedded as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        EmbeddingStatus {
            provider: self.config.embedding_config.provider,
            model: self.config.embedding_config.model.clone(),
            dimensions: self.config.embedding_config.get_dimensions(),
            is_ready: self.config.embedding_config.is_enabled(),
            documents_indexed: metrics.documents_embedded,
            documents_pending: metrics.documents_pending,
            index_progress_pct: progress,
        }
    }

    /// Trigger a poll for new content
    #[allow(dead_code)] // Use handle().trigger_poll() - this is direct access for testing
    pub fn trigger_poll(&self) {
        let _ = self.tx.try_send(IndexerCommand::Poll);
    }

    /// Trigger a full re-index (e.g., after config change)
    #[allow(dead_code)] // Use handle().trigger_reindex() - this is direct access for testing
    pub fn trigger_reindex(&self) {
        let _ = self.tx.send(IndexerCommand::Reindex);
    }

    /// Shutdown the indexer
    pub fn shutdown(&self) -> anyhow::Result<()> {
        let _ = self.tx.send(IndexerCommand::Shutdown);

        const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);
        if !self.completion.wait(SHUTDOWN_TIMEOUT) {
            tracing::warn!(
                "Embedding indexer did not complete within {:?}",
                SHUTDOWN_TIMEOUT
            );
            return Err(anyhow::anyhow!("Shutdown timeout"));
        }

        tracing::debug!("Embedding indexer shutdown complete");
        Ok(())
    }

    /// Main indexer thread loop
    fn indexer_thread(
        rx: mpsc::Receiver<IndexerCommand>,
        config: IndexerConfig,
        provider: Box<dyn EmbeddingProvider>,
        metrics: Arc<IndexerMetrics>,
    ) -> anyhow::Result<()> {
        // Open database connection
        let conn = Connection::open(&config.db_path)?;
        conn.execute("PRAGMA foreign_keys=OFF", [])?;

        // Check/update embedding config in database
        Self::sync_embedding_config(&conn, &config.embedding_config)?;

        // Initial count of pending documents
        let pending = Self::count_pending(&conn)?;
        metrics.documents_pending.store(pending, Ordering::Relaxed);
        tracing::info!("Embedding indexer started: {} documents pending", pending);

        // Track last poll time for periodic polling
        let mut last_poll = Instant::now();

        loop {
            // Wait for command or poll interval
            match rx.recv_timeout(config.poll_interval) {
                Ok(IndexerCommand::Poll) | Err(RecvTimeoutError::Timeout) => {
                    // Only process if provider is ready and enough time has passed
                    if provider.is_ready() && last_poll.elapsed() >= config.poll_interval {
                        metrics.is_processing.store(true, Ordering::Relaxed);
                        // Handle errors gracefully - log and continue, don't crash the indexer
                        if let Err(e) =
                            Self::process_batch(&conn, &config, provider.as_ref(), &metrics)
                        {
                            tracing::error!("Embedding batch failed: {}. Will retry next poll.", e);
                            metrics.embedding_errors.fetch_add(1, Ordering::Relaxed);
                        }
                        metrics.is_processing.store(false, Ordering::Relaxed);
                        last_poll = Instant::now();
                    }
                }
                Ok(IndexerCommand::Reindex) => {
                    if provider.is_ready() {
                        tracing::info!("Starting full re-index");
                        metrics.is_processing.store(true, Ordering::Relaxed);
                        // Handle reindex errors gracefully
                        if let Err(e) = Self::clear_embeddings(&conn) {
                            tracing::error!("Failed to clear embeddings for re-index: {}", e);
                        } else {
                            metrics.documents_embedded.store(0, Ordering::Relaxed);
                            match Self::count_pending(&conn) {
                                Ok(pending) => {
                                    metrics.documents_pending.store(pending, Ordering::Relaxed)
                                }
                                Err(e) => tracing::error!("Failed to count pending docs: {}", e),
                            }
                        }
                        metrics.is_processing.store(false, Ordering::Relaxed);
                    }
                }
                Ok(IndexerCommand::Shutdown) => {
                    tracing::debug!("Embedding indexer received shutdown");
                    break;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }

        Ok(())
    }

    /// Sync embedding config with database
    ///
    /// If config has changed, clear all embeddings for re-indexing.
    fn sync_embedding_config(conn: &Connection, config: &EmbeddingConfig) -> anyhow::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        // Check if config exists
        let existing: Option<(String, String, i64)> = conn
            .query_row(
                "SELECT provider, model, dimensions FROM embedding_config WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();

        match existing {
            Some((provider, model, dimensions)) => {
                let config_changed = provider != config.provider.to_string()
                    || model != config.model
                    || dimensions != config.get_dimensions() as i64;

                if config_changed {
                    tracing::info!(
                        "Embedding config changed: {} {} {} -> {} {} {}",
                        provider,
                        model,
                        dimensions,
                        config.provider,
                        config.model,
                        config.get_dimensions()
                    );

                    // Clear all embeddings
                    Self::clear_embeddings(conn)?;

                    // Update config
                    conn.execute(
                        "UPDATE embedding_config SET provider = ?1, model = ?2, dimensions = ?3, updated_at = ?4 WHERE id = 1",
                        params![config.provider.to_string(), config.model, config.get_dimensions() as i64, now],
                    )?;
                }
            }
            None => {
                // Insert initial config
                conn.execute(
                    "INSERT INTO embedding_config (id, provider, model, dimensions, created_at, updated_at) VALUES (1, ?1, ?2, ?3, ?4, ?4)",
                    params![config.provider.to_string(), config.model, config.get_dimensions() as i64, now],
                )?;
            }
        }

        Ok(())
    }

    /// Clear all embeddings (for re-indexing)
    fn clear_embeddings(conn: &Connection) -> anyhow::Result<()> {
        conn.execute("DELETE FROM thinking_embeddings", [])?;
        conn.execute("DELETE FROM prompts_embeddings", [])?;
        conn.execute("DELETE FROM responses_embeddings", [])?;
        tracing::info!("Cleared all embeddings for re-indexing");
        Ok(())
    }

    /// Count pending (un-embedded) documents
    fn count_pending(conn: &Connection) -> anyhow::Result<u64> {
        let mut total = 0u64;

        for content_type in [
            ContentType::Thinking,
            ContentType::Prompt,
            ContentType::Response,
        ] {
            let count: i64 = conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM {} c WHERE NOT EXISTS (SELECT 1 FROM {} e WHERE e.content_id = c.id)",
                    content_type.content_table(),
                    content_type.embedding_table()
                ),
                [],
                |row| row.get(0),
            )?;
            total += count as u64;
        }

        Ok(total)
    }

    /// Process a batch of documents
    fn process_batch(
        conn: &Connection,
        config: &IndexerConfig,
        provider: &dyn EmbeddingProvider,
        metrics: &IndexerMetrics,
    ) -> anyhow::Result<()> {
        // Fetch un-embedded documents
        let documents = Self::fetch_pending_documents(conn, config.batch_size)?;

        if documents.is_empty() {
            // Update pending count (might have changed externally)
            let pending = Self::count_pending(conn)?;
            metrics.documents_pending.store(pending, Ordering::Relaxed);
            return Ok(());
        }

        tracing::debug!("Processing {} documents for embedding", documents.len());

        // Prepare texts for batch embedding (safely truncated to avoid UTF-8 boundary issues)
        let texts: Vec<&str> = documents
            .iter()
            .map(|d| truncate_utf8_safe(&d.content, config.max_content_length))
            .collect();

        // Generate embeddings
        match provider.embed_batch(&texts) {
            Ok(result) => {
                // Store embeddings
                Self::store_embeddings(conn, &documents, &result)?;

                // Update metrics
                metrics
                    .documents_embedded
                    .fetch_add(documents.len() as u64, Ordering::Relaxed);
                metrics.batches_processed.fetch_add(1, Ordering::Relaxed);

                let pending = Self::count_pending(conn)?;
                metrics.documents_pending.store(pending, Ordering::Relaxed);

                tracing::info!(
                    "Embedded {} documents, {} pending",
                    documents.len(),
                    pending
                );
            }
            Err(EmbeddingError::RateLimited { retry_after_secs }) => {
                let delay = retry_after_secs.unwrap_or(60);
                tracing::warn!("Rate limited, waiting {} seconds", delay);
                std::thread::sleep(Duration::from_secs(delay));
            }
            Err(e) => {
                metrics.embedding_errors.fetch_add(1, Ordering::Relaxed);
                tracing::warn!("Embedding error: {}", e);
            }
        }

        // Delay between batches
        if !config.batch_delay.is_zero() {
            std::thread::sleep(config.batch_delay);
        }

        Ok(())
    }

    /// Fetch documents pending embedding
    fn fetch_pending_documents(conn: &Connection, limit: usize) -> anyhow::Result<Vec<Document>> {
        let mut documents = Vec::new();

        for content_type in [
            ContentType::Thinking,
            ContentType::Prompt,
            ContentType::Response,
        ] {
            if documents.len() >= limit {
                break;
            }

            let remaining = limit - documents.len();
            let sql = format!(
                "SELECT c.id, c.content FROM {} c WHERE NOT EXISTS (SELECT 1 FROM {} e WHERE e.content_id = c.id) ORDER BY c.id LIMIT ?1",
                content_type.content_table(),
                content_type.embedding_table()
            );

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![remaining as i64], |row| {
                Ok(Document {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    content_type,
                })
            })?;

            for row in rows {
                documents.push(row?);
            }
        }

        Ok(documents)
    }

    /// Store embeddings in database
    fn store_embeddings(
        conn: &Connection,
        documents: &[Document],
        result: &BatchEmbeddingResult,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute("BEGIN TRANSACTION", [])?;

        for (doc, embedding) in documents.iter().zip(result.embeddings.iter()) {
            let embedding_blob = embedding_to_blob(embedding);

            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {} (content_id, embedding, embedded_at) VALUES (?1, ?2, ?3)",
                    doc.content_type.embedding_table()
                ),
                params![doc.id, embedding_blob, now],
            )?;
        }

        conn.execute("COMMIT", [])?;
        Ok(())
    }
}

impl Drop for EmbeddingIndexer {
    fn drop(&mut self) {
        let _ = self.tx.send(IndexerCommand::Shutdown);
        if let Some(handle) = self.indexer_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Convert embedding to BLOB for SQLite storage
///
/// Stores f32 values as little-endian bytes.
pub fn embedding_to_blob(embedding: &Embedding) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for value in embedding {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// Convert BLOB back to embedding
pub fn blob_to_embedding(blob: &[u8]) -> Embedding {
    let mut embedding = Vec::with_capacity(blob.len() / 4);
    for chunk in blob.chunks_exact(4) {
        let value = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        embedding.push(value);
    }
    embedding
}

/// Compute cosine similarity between two embeddings
///
/// Returns a value between -1 and 1, where 1 is identical.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot_product = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (x, y) in a.iter().zip(b.iter()) {
        dot_product += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let norm = norm_a.sqrt() * norm_b.sqrt();
    if norm == 0.0 {
        0.0
    } else {
        dot_product / norm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_blob_roundtrip() {
        let embedding = vec![0.1, 0.2, 0.3, -0.4, 0.5];
        let blob = embedding_to_blob(&embedding);
        let restored = blob_to_embedding(&blob);

        assert_eq!(embedding.len(), restored.len());
        for (a, b) in embedding.iter().zip(restored.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_content_type_tables() {
        assert_eq!(ContentType::Thinking.content_table(), "thinking_blocks");
        assert_eq!(
            ContentType::Thinking.embedding_table(),
            "thinking_embeddings"
        );
        assert_eq!(ContentType::Prompt.content_table(), "user_prompts");
        assert_eq!(
            ContentType::Response.embedding_table(),
            "responses_embeddings"
        );
    }
}
