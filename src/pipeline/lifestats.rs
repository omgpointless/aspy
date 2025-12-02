//! Lifetime statistics storage processor
//!
//! Stores events in SQLite for cross-session querying. Uses a dedicated
//! writer thread to avoid blocking the async runtime.
//!
//! # Architecture
//!
//! ```text
//! EventPipeline (sync)
//!     │
//!     └──→ LifestatsProcessor.process()
//!             │
//!             └──→ std::sync::mpsc::Sender (bounded)
//!                     │
//!                     └──→ Dedicated Writer Thread
//!                             │
//!                             ├──→ Batch buffer (100 events or 1s)
//!                             └──→ SQLite (WAL mode)
//! ```

use super::{EventProcessor, ProcessContext, ProcessResult};
use crate::events::ProxyEvent;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Configuration for lifestats storage
#[derive(Debug, Clone)]
pub struct LifestatsConfig {
    /// Path to SQLite database file
    pub db_path: PathBuf,
    /// Whether to store thinking blocks (can be large)
    pub store_thinking: bool,
    /// Whether to store full tool inputs/outputs
    pub store_tool_io: bool,
    /// Maximum thinking block size to store (bytes)
    pub max_thinking_size: usize,
    /// Retention period in days (0 = forever)
    pub retention_days: u32,
    /// Channel buffer size (backpressure threshold)
    pub channel_buffer: usize,
    /// Batch size before flush
    pub batch_size: usize,
    /// Maximum time before flush (even if batch not full)
    pub flush_interval: Duration,
}

impl Default for LifestatsConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("./data/lifestats.db"),
            store_thinking: true,
            store_tool_io: true,
            max_thinking_size: 100_000, // ~100KB per thinking block
            retention_days: 90,
            channel_buffer: 10_000, // Buffer before backpressure
            batch_size: 100,        // Flush every 100 events
            flush_interval: Duration::from_secs(1), // Or every 1 second
        }
    }
}

/// Metrics for observability of the lifestats system itself
#[derive(Debug, Default)]
pub struct LifestatsMetrics {
    /// Events successfully stored
    pub events_stored: AtomicU64,
    /// Events dropped due to backpressure (channel full)
    pub events_dropped: AtomicU64,
    /// Events that failed to store (DB error during batch)
    pub events_store_failed: AtomicU64,
    /// Current batch buffer size
    pub batch_pending: AtomicU64,
    /// Total write latency (for averaging)
    pub write_latency_us: AtomicU64,
    /// Number of batch flushes
    pub flush_count: AtomicU64,
}

impl LifestatsMetrics {
    #[allow(dead_code)] // Phase 2: Used by /api/lifestats/health endpoint
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            events_stored: self.events_stored.load(Ordering::Relaxed),
            events_dropped: self.events_dropped.load(Ordering::Relaxed),
            events_store_failed: self.events_store_failed.load(Ordering::Relaxed),
            batch_pending: self.batch_pending.load(Ordering::Relaxed),
            avg_write_latency_us: {
                let total = self.write_latency_us.load(Ordering::Relaxed);
                let count = self.flush_count.load(Ordering::Relaxed);
                if count > 0 {
                    total / count
                } else {
                    0
                }
            },
        }
    }
}

#[allow(dead_code)] // Phase 2: Return type for health endpoint
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub events_stored: u64,
    pub events_dropped: u64,
    pub events_store_failed: u64,
    pub batch_pending: u64,
    pub avg_write_latency_us: u64,
}

/// Commands sent to the writer thread
enum WriterCommand {
    Store(ProxyEvent, ProcessContext),
    Shutdown,
}

/// Completion signal for graceful shutdown
///
/// Uses a Condvar to block shutdown() until the writer thread has finished
/// flushing its batch and exited cleanly.
struct CompletionSignal {
    mutex: Mutex<bool>,
    condvar: Condvar,
}

impl CompletionSignal {
    fn new() -> Self {
        Self {
            mutex: Mutex::new(false),
            condvar: Condvar::new(),
        }
    }

    /// Signal that the writer thread has completed
    fn complete(&self) {
        let mut done = self.mutex.lock().unwrap();
        *done = true;
        self.condvar.notify_all();
    }

    /// Wait for the writer thread to complete (with timeout)
    fn wait(&self, timeout: Duration) -> bool {
        let mut done = self.mutex.lock().unwrap();
        while !*done {
            let result = self.condvar.wait_timeout(done, timeout).unwrap();
            done = result.0;
            if result.1.timed_out() {
                return false; // Timeout
            }
        }
        true // Completed
    }
}

/// Lifetime statistics processor
///
/// Writes events to SQLite using a dedicated thread.
pub struct LifestatsProcessor {
    /// Channel to send events to writer thread
    tx: SyncSender<WriterCommand>,
    /// Handle to writer thread (for join on shutdown)
    writer_handle: Option<JoinHandle<()>>,
    /// Completion signal for graceful shutdown
    completion: Arc<CompletionSignal>,
    /// Shared metrics
    metrics: Arc<LifestatsMetrics>,
    /// Config for reference (reserved for future introspection API)
    #[allow(dead_code)] // Phase 2: Config introspection
    config: LifestatsConfig,
}

impl LifestatsProcessor {
    /// Create a new lifestats processor
    ///
    /// Spawns a dedicated OS thread for database writes.
    pub fn new(config: LifestatsConfig) -> anyhow::Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = config.db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Create bounded sync channel for backpressure
        let (tx, rx) = mpsc::sync_channel::<WriterCommand>(config.channel_buffer);

        // Shared metrics
        let metrics = Arc::new(LifestatsMetrics::default());
        let writer_metrics = metrics.clone();

        // Completion signal for graceful shutdown
        let completion = Arc::new(CompletionSignal::new());
        let writer_completion = completion.clone();

        // Clone config for writer thread
        let writer_config = config.clone();

        // Spawn dedicated writer thread (NOT tokio task)
        let writer_handle = thread::Builder::new()
            .name("lifestats-writer".into())
            .spawn(move || {
                if let Err(e) = Self::writer_thread(rx, writer_config, writer_metrics) {
                    tracing::error!("Lifestats writer thread error: {}", e);
                }
                // Signal completion regardless of success/failure
                writer_completion.complete();
            })?;

        Ok(Self {
            tx,
            writer_handle: Some(writer_handle),
            completion,
            metrics,
            config,
        })
    }

    /// Get current metrics snapshot
    #[allow(dead_code)] // Phase 2: Used by /api/lifestats/health endpoint
    pub fn metrics(&self) -> MetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Dedicated writer thread - runs SQLite operations
    fn writer_thread(
        rx: mpsc::Receiver<WriterCommand>,
        config: LifestatsConfig,
        metrics: Arc<LifestatsMetrics>,
    ) -> anyhow::Result<()> {
        // Open connection with WAL mode
        let conn = Connection::open(&config.db_path)?;
        Self::init_schema(&conn)?;

        // Batch buffer
        let mut batch: Vec<(ProxyEvent, ProcessContext)> = Vec::with_capacity(config.batch_size);
        let mut last_flush = Instant::now();

        // Retention cleanup tracking (runs every 24 hours)
        let mut last_cleanup = Instant::now();
        const CLEANUP_INTERVAL: Duration = Duration::from_secs(24 * 3600); // 24 hours

        loop {
            // Wait for event with timeout (for periodic flush)
            match rx.recv_timeout(config.flush_interval) {
                Ok(WriterCommand::Store(event, ctx)) => {
                    batch.push((event, ctx));
                    metrics
                        .batch_pending
                        .store(batch.len() as u64, Ordering::Relaxed);

                    // Flush if batch full
                    if batch.len() >= config.batch_size {
                        Self::flush_batch(&conn, &mut batch, &config, &metrics)?;
                        last_flush = Instant::now();
                    }
                }
                Ok(WriterCommand::Shutdown) => {
                    // Final flush before exit
                    if !batch.is_empty() {
                        Self::flush_batch(&conn, &mut batch, &config, &metrics)?;
                    }
                    tracing::debug!("Lifestats writer thread shutting down");
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {
                    // Periodic flush even if batch not full
                    if !batch.is_empty() && last_flush.elapsed() >= config.flush_interval {
                        Self::flush_batch(&conn, &mut batch, &config, &metrics)?;
                        last_flush = Instant::now();
                    }

                    // Periodic retention cleanup (every 24 hours)
                    if last_cleanup.elapsed() >= CLEANUP_INTERVAL {
                        if config.retention_days > 0 {
                            tracing::debug!("Starting retention cleanup ({}d retention)", config.retention_days);
                            match Self::run_retention_cleanup(&conn, config.retention_days) {
                                Ok(deleted) => {
                                    tracing::info!(
                                        "Retention cleanup complete: {} records deleted",
                                        deleted
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!("Retention cleanup failed: {}", e);
                                    // Non-fatal: continue operation despite cleanup failure
                                }
                            }
                        } else {
                            tracing::trace!("Retention cleanup skipped (retention disabled)");
                        }
                        last_cleanup = Instant::now();
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    // Channel closed, flush and exit
                    if !batch.is_empty() {
                        Self::flush_batch(&conn, &mut batch, &config, &metrics)?;
                    }
                    break;
                }
            }
        }

        Ok(())
    }

    /// Flush batch to database in a transaction
    fn flush_batch(
        conn: &Connection,
        batch: &mut Vec<(ProxyEvent, ProcessContext)>,
        config: &LifestatsConfig,
        metrics: &LifestatsMetrics,
    ) -> anyhow::Result<()> {
        if batch.is_empty() {
            return Ok(());
        }

        let start = Instant::now();
        let count = batch.len();
        let mut failed_count = 0u64;

        conn.execute("BEGIN TRANSACTION", [])?;

        for (event, ctx) in batch.drain(..) {
            if let Err(e) = Self::store_event(conn, &event, &ctx, config) {
                // Log but don't fail the batch (best-effort storage)
                failed_count += 1;
                tracing::warn!("Failed to store event: {}", e);
            }
        }

        conn.execute("COMMIT", [])?;

        // Update metrics
        let latency = start.elapsed().as_micros() as u64;
        let stored_count = count as u64 - failed_count;
        metrics
            .events_stored
            .fetch_add(stored_count, Ordering::Relaxed);
        if failed_count > 0 {
            metrics
                .events_store_failed
                .fetch_add(failed_count, Ordering::Relaxed);
        }
        metrics
            .write_latency_us
            .fetch_add(latency, Ordering::Relaxed);
        metrics.flush_count.fetch_add(1, Ordering::Relaxed);
        metrics.batch_pending.store(0, Ordering::Relaxed);

        tracing::trace!(
            "Flushed {} events ({} failed) in {}µs",
            count,
            failed_count,
            latency
        );

        Ok(())
    }

    /// Initialize database schema with WAL mode and run migrations
    fn init_schema(conn: &Connection) -> anyhow::Result<()> {
        // Performance settings (always applied)
        conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            PRAGMA busy_timeout=5000;
            PRAGMA cache_size=-64000;  -- 64MB cache
            -- Note: FK constraints are declarative only (PRAGMA foreign_keys=OFF by default)
            -- This allows tool_results to arrive before/without their tool_calls
            "#,
        )?;

        // Check current schema version
        let current_version: i32 = conn
            .query_row(
                "SELECT COALESCE(
                    (SELECT CAST(value AS INTEGER) FROM metadata WHERE key = 'schema_version'),
                    0
                )",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Apply migrations
        if current_version < 1 {
            Self::apply_schema_v1(conn)?;
        }
        if current_version < 2 {
            Self::migrate_v1_to_v2(conn)?;
        }
        // Future: if current_version < 3 { Self::migrate_v2_to_v3(conn)?; }

        Ok(())
    }

    /// Initial schema (v1)
    fn apply_schema_v1(conn: &Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            r#"
            -- Metadata table (created first for version tracking)
            CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY,
                value TEXT
            );

            -- Sessions table
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                user_id TEXT,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                source TEXT,  -- 'hook', 'warmup', 'first_seen'

                -- Aggregated stats (updated on session end)
                total_tokens INTEGER DEFAULT 0,
                total_cost_usd REAL DEFAULT 0,
                tool_calls INTEGER DEFAULT 0,
                thinking_blocks INTEGER DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
            CREATE INDEX IF NOT EXISTS idx_sessions_started ON sessions(started_at);

            -- Thinking blocks (Claude's reasoning)
            CREATE TABLE IF NOT EXISTS thinking_blocks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT,
                timestamp TEXT NOT NULL,
                content TEXT NOT NULL,
                tokens INTEGER,

                FOREIGN KEY (session_id) REFERENCES sessions(id)
            );
            CREATE INDEX IF NOT EXISTS idx_thinking_session ON thinking_blocks(session_id);
            CREATE INDEX IF NOT EXISTS idx_thinking_timestamp ON thinking_blocks(timestamp);

            -- Full-text search on thinking blocks (external content mode)
            CREATE VIRTUAL TABLE IF NOT EXISTS thinking_fts USING fts5(
                content,
                content=thinking_blocks,
                content_rowid=id,
                tokenize='porter unicode61'
            );

            -- Tool calls
            CREATE TABLE IF NOT EXISTS tool_calls (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                timestamp TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                input_json TEXT,

                FOREIGN KEY (session_id) REFERENCES sessions(id)
            );
            CREATE INDEX IF NOT EXISTS idx_tools_session ON tool_calls(session_id);
            CREATE INDEX IF NOT EXISTS idx_tools_name ON tool_calls(tool_name);
            CREATE INDEX IF NOT EXISTS idx_tools_timestamp ON tool_calls(timestamp);

            -- Tool results (linked to calls)
            CREATE TABLE IF NOT EXISTS tool_results (
                call_id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                output_json TEXT,
                duration_ms INTEGER,
                success INTEGER,

                FOREIGN KEY (call_id) REFERENCES tool_calls(id)
            );

            -- API usage records (for cost tracking)
            CREATE TABLE IF NOT EXISTS api_usage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT,
                timestamp TEXT NOT NULL,
                model TEXT NOT NULL,
                input_tokens INTEGER,
                output_tokens INTEGER,
                cache_read_tokens INTEGER,
                cache_creation_tokens INTEGER,
                cost_usd REAL,

                FOREIGN KEY (session_id) REFERENCES sessions(id)
            );
            CREATE INDEX IF NOT EXISTS idx_usage_session ON api_usage(session_id);
            CREATE INDEX IF NOT EXISTS idx_usage_model ON api_usage(model);
            CREATE INDEX IF NOT EXISTS idx_usage_timestamp ON api_usage(timestamp);

            -- User prompts (extracted from requests)
            CREATE TABLE IF NOT EXISTS user_prompts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT,
                timestamp TEXT NOT NULL,
                content TEXT NOT NULL,

                FOREIGN KEY (session_id) REFERENCES sessions(id)
            );
            CREATE INDEX IF NOT EXISTS idx_prompts_session ON user_prompts(session_id);
            CREATE INDEX IF NOT EXISTS idx_prompts_timestamp ON user_prompts(timestamp);

            -- Full-text search on user prompts (external content mode)
            CREATE VIRTUAL TABLE IF NOT EXISTS prompts_fts USING fts5(
                content,
                content=user_prompts,
                content_rowid=id,
                tokenize='porter unicode61'
            );

            -- Assistant responses (Claude's text output)
            CREATE TABLE IF NOT EXISTS assistant_responses (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT,
                timestamp TEXT NOT NULL,
                content TEXT NOT NULL,

                FOREIGN KEY (session_id) REFERENCES sessions(id)
            );
            CREATE INDEX IF NOT EXISTS idx_responses_session ON assistant_responses(session_id);
            CREATE INDEX IF NOT EXISTS idx_responses_timestamp ON assistant_responses(timestamp);

            -- Full-text search on assistant responses (external content mode)
            CREATE VIRTUAL TABLE IF NOT EXISTS responses_fts USING fts5(
                content,
                content=assistant_responses,
                content_rowid=id,
                tokenize='porter unicode61'
            );

            -- Set initial version
            INSERT INTO metadata (key, value) VALUES ('schema_version', '1');
            "#,
        )?;

        Ok(())
    }

    /// Migration from v1 to v2 (adds source column to sessions)
    ///
    /// # Idempotency
    ///
    /// This migration is idempotent - safe to run multiple times. This is critical
    /// because if the process crashes between ALTER TABLE and UPDATE metadata,
    /// the next startup would retry the migration. Without idempotency, SQLite
    /// would error with "duplicate column name: source".
    fn migrate_v1_to_v2(conn: &Connection) -> anyhow::Result<()> {
        // Check if column already exists (idempotent)
        let has_source: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('sessions') WHERE name='source'",
            [],
            |row| row.get(0),
        )?;

        if !has_source {
            conn.execute("ALTER TABLE sessions ADD COLUMN source TEXT", [])?;
        }

        conn.execute(
            "UPDATE metadata SET value = '2' WHERE key = 'schema_version'",
            [],
        )?;

        tracing::info!("Migrated lifestats database from v1 to v2");
        Ok(())
    }

    /// Retention cleanup - deletes old data and syncs FTS indexes
    ///
    /// # FTS External Content Sync Contract
    ///
    /// We use FTS5 external content tables (`content=thinking_blocks`) for
    /// space efficiency - the actual text is stored once in the base table,
    /// and FTS just indexes it. However, this means:
    ///
    /// - INSERTs must update both base table AND FTS index (we do this in store_event)
    /// - DELETEs must update both base table AND FTS index (this function)
    /// - UPDATEs are not supported (we don't update stored events)
    ///
    /// **CRITICAL**: If you delete from the base table without deleting from
    /// the FTS index, searches will return "ghost" rowids that point to
    /// deleted rows, causing query errors.
    ///
    /// # Implementation
    ///
    /// We delete FTS entries FIRST, then base table entries. This order
    /// ensures we never have dangling FTS entries even if the process
    /// crashes mid-cleanup.
    pub fn run_retention_cleanup(conn: &Connection, retention_days: u32) -> anyhow::Result<u64> {
        if retention_days == 0 {
            return Ok(0); // Retention disabled
        }

        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
        let cutoff_str = cutoff.to_rfc3339();
        let mut deleted = 0u64;

        conn.execute("BEGIN TRANSACTION", [])?;

        // 1. Delete from thinking_fts FIRST (must happen before base table delete)
        //    We use the special FTS delete syntax with rowid from base table
        let fts_deleted: i64 = conn.execute(
            r#"
            DELETE FROM thinking_fts
            WHERE rowid IN (
                SELECT id FROM thinking_blocks WHERE timestamp < ?1
            )
            "#,
            params![cutoff_str],
        )? as i64;
        tracing::debug!("Deleted {} entries from thinking_fts", fts_deleted);

        // 2. Delete from prompts_fts FIRST
        let prompts_fts_deleted: i64 = conn.execute(
            r#"
            DELETE FROM prompts_fts
            WHERE rowid IN (
                SELECT id FROM user_prompts WHERE timestamp < ?1
            )
            "#,
            params![cutoff_str],
        )? as i64;
        tracing::debug!("Deleted {} entries from prompts_fts", prompts_fts_deleted);

        // 3. Delete from responses_fts FIRST
        let responses_fts_deleted: i64 = conn.execute(
            r#"
            DELETE FROM responses_fts
            WHERE rowid IN (
                SELECT id FROM assistant_responses WHERE timestamp < ?1
            )
            "#,
            params![cutoff_str],
        )? as i64;
        tracing::debug!("Deleted {} entries from responses_fts", responses_fts_deleted);

        // 4. Now delete from base tables (order matters for FK relationships)
        deleted += conn.execute(
            "DELETE FROM thinking_blocks WHERE timestamp < ?1",
            params![cutoff_str],
        )? as u64;

        deleted += conn.execute(
            "DELETE FROM user_prompts WHERE timestamp < ?1",
            params![cutoff_str],
        )? as u64;

        deleted += conn.execute(
            "DELETE FROM assistant_responses WHERE timestamp < ?1",
            params![cutoff_str],
        )? as u64;

        deleted += conn.execute(
            "DELETE FROM tool_results WHERE timestamp < ?1",
            params![cutoff_str],
        )? as u64;

        deleted += conn.execute(
            "DELETE FROM tool_calls WHERE timestamp < ?1",
            params![cutoff_str],
        )? as u64;

        deleted += conn.execute(
            "DELETE FROM api_usage WHERE timestamp < ?1",
            params![cutoff_str],
        )? as u64;

        // 4. Clean up orphaned sessions (no recent activity)
        deleted += conn.execute(
            "DELETE FROM sessions WHERE started_at < ?1 AND ended_at IS NOT NULL",
            params![cutoff_str],
        )? as u64;

        conn.execute("COMMIT", [])?;

        tracing::info!(
            "Retention cleanup: deleted {} records older than {} days",
            deleted,
            retention_days
        );

        Ok(deleted)
    }

    /// Store an event in the database
    fn store_event(
        conn: &Connection,
        event: &ProxyEvent,
        ctx: &ProcessContext,
        config: &LifestatsConfig,
    ) -> anyhow::Result<()> {
        let session_id = ctx.session_id.as_deref();

        match event {
            ProxyEvent::Thinking {
                timestamp,
                content,
                token_estimate,
            } if config.store_thinking => {
                // Truncate if too large
                let content = if content.len() > config.max_thinking_size {
                    format!(
                        "{}... [truncated, {} bytes total]",
                        &content[..config.max_thinking_size],
                        content.len()
                    )
                } else {
                    content.clone()
                };

                conn.execute(
                    "INSERT INTO thinking_blocks (session_id, timestamp, content, tokens)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![session_id, timestamp.to_rfc3339(), content, token_estimate],
                )?;

                // Update FTS index
                let rowid = conn.last_insert_rowid();
                conn.execute(
                    "INSERT INTO thinking_fts(rowid, content) VALUES (?1, ?2)",
                    params![rowid, content],
                )?;
            }

            ProxyEvent::ToolCall {
                id,
                timestamp,
                tool_name,
                input,
            } => {
                let input_json = if config.store_tool_io {
                    Some(input.to_string())
                } else {
                    None
                };

                conn.execute(
                    "INSERT OR REPLACE INTO tool_calls (id, session_id, timestamp, tool_name, input_json)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, session_id, timestamp.to_rfc3339(), tool_name, input_json],
                )?;
            }

            ProxyEvent::ToolResult {
                id,
                timestamp,
                output,
                duration,
                success,
                ..
            } => {
                let output_json = if config.store_tool_io {
                    Some(output.to_string())
                } else {
                    None
                };

                conn.execute(
                    "INSERT OR REPLACE INTO tool_results (call_id, timestamp, output_json, duration_ms, success)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        id,
                        timestamp.to_rfc3339(),
                        output_json,
                        duration.as_millis() as i64,
                        *success as i32
                    ],
                )?;
            }

            ProxyEvent::ApiUsage {
                timestamp,
                model,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_creation_tokens,
            } => {
                // Calculate cost using pricing module
                let cost_usd = crate::pricing::calculate_cost(
                    model,
                    *input_tokens,
                    *output_tokens,
                    *cache_creation_tokens,
                    *cache_read_tokens,
                );

                conn.execute(
                    "INSERT INTO api_usage (session_id, timestamp, model, input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens, cost_usd)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        session_id,
                        timestamp.to_rfc3339(),
                        model,
                        input_tokens,
                        output_tokens,
                        cache_read_tokens,
                        cache_creation_tokens,
                        cost_usd
                    ],
                )?;
            }

            ProxyEvent::UserPrompt { timestamp, content } => {
                conn.execute(
                    "INSERT INTO user_prompts (session_id, timestamp, content)
                     VALUES (?1, ?2, ?3)",
                    params![session_id, timestamp.to_rfc3339(), content],
                )?;

                // Update FTS index
                let rowid = conn.last_insert_rowid();
                conn.execute(
                    "INSERT INTO prompts_fts(rowid, content) VALUES (?1, ?2)",
                    params![rowid, content],
                )?;
            }

            ProxyEvent::AssistantResponse { timestamp, content } => {
                conn.execute(
                    "INSERT INTO assistant_responses (session_id, timestamp, content)
                     VALUES (?1, ?2, ?3)",
                    params![session_id, timestamp.to_rfc3339(), content],
                )?;

                // Update FTS index
                let rowid = conn.last_insert_rowid();
                conn.execute(
                    "INSERT INTO responses_fts(rowid, content) VALUES (?1, ?2)",
                    params![rowid, content],
                )?;
            }

            _ => {
                // Other events not stored in lifestats
            }
        }

        Ok(())
    }
}

impl EventProcessor for LifestatsProcessor {
    fn name(&self) -> &'static str {
        "lifestats"
    }

    fn process(&self, event: &ProxyEvent, ctx: &ProcessContext) -> ProcessResult {
        // Try to send to writer thread
        match self
            .tx
            .try_send(WriterCommand::Store(event.clone(), ctx.clone()))
        {
            Ok(()) => {
                // Successfully queued
            }
            Err(mpsc::TrySendError::Full(_)) => {
                // Backpressure: channel full
                self.metrics.events_dropped.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    "Lifestats backpressure: dropped event (total dropped: {})",
                    self.metrics.events_dropped.load(Ordering::Relaxed)
                );
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                // Writer thread died
                tracing::error!("Lifestats writer thread disconnected");
            }
        }

        // Always pass through (side-effect only processor)
        ProcessResult::Continue
    }

    fn shutdown(&self) -> anyhow::Result<()> {
        // Signal writer thread to stop
        let _ = self.tx.send(WriterCommand::Shutdown);

        // Wait for completion signal (with timeout)
        // This ensures all buffered events are flushed before we return
        const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
        if !self.completion.wait(SHUTDOWN_TIMEOUT) {
            tracing::warn!(
                "Lifestats writer thread did not complete within {:?}",
                SHUTDOWN_TIMEOUT
            );
            return Err(anyhow::anyhow!("Shutdown timeout"));
        }

        tracing::debug!("Lifestats processor shutdown complete");
        Ok(())
    }
}

impl Drop for LifestatsProcessor {
    fn drop(&mut self) {
        // Ensure writer thread is signaled
        let _ = self.tx.send(WriterCommand::Shutdown);

        // Join the thread
        if let Some(handle) = self.writer_handle.take() {
            let _ = handle.join();
        }
    }
}
