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

use super::{CompletionSignal, EventProcessor, ProcessContext, ProcessResult};
use crate::events::ProxyEvent;
use crate::util::truncate_utf8_safe;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Known Claude Code rejection message patterns.
/// These indicate the user rejected a tool call (not an actual error).
const REJECTION_PATTERNS: &[&str] = &[
    "The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file)",
    "The user doesn't want to take this action right now",
];

/// Check if a tool result output indicates a user rejection
fn is_user_rejection(output: &str) -> bool {
    REJECTION_PATTERNS
        .iter()
        .any(|pattern| output.contains(pattern))
}

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
    Store(Box<ProxyEvent>, ProcessContext),
    Shutdown,
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

    /// Extract searchable text from todos JSON for FTS indexing
    ///
    /// Concatenates all todo `content` fields into a single searchable string.
    /// Example: "Fix auth bug. Run tests. Deploy to staging."
    fn extract_todo_content_for_fts(todos_json: &str) -> String {
        // Parse JSON and extract content fields
        if let Ok(todos) = serde_json::from_str::<Vec<serde_json::Value>>(todos_json) {
            todos
                .iter()
                .filter_map(|t| t.get("content").and_then(|v| v.as_str()))
                .collect::<Vec<_>>()
                .join(". ")
        } else {
            // Fallback: use raw JSON as searchable text
            todos_json.to_string()
        }
    }

    /// Dedicated writer thread - runs SQLite operations
    fn writer_thread(
        rx: mpsc::Receiver<WriterCommand>,
        config: LifestatsConfig,
        metrics: Arc<LifestatsMetrics>,
    ) -> anyhow::Result<()> {
        // Open connection with WAL mode
        let conn = Connection::open(&config.db_path)?;

        // Disable FK constraints for this connection (per-connection setting)
        // This allows events to arrive out of order (e.g., tool_results before tool_calls)
        conn.execute("PRAGMA foreign_keys=OFF", [])?;

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
                    batch.push((*event, ctx));
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
                            tracing::debug!(
                                "Starting retention cleanup ({}d retention)",
                                config.retention_days
                            );
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
                tracing::warn!(
                    "Failed to store event (type={:?}, session_id={:?}): {}",
                    std::mem::discriminant(&event),
                    ctx.session_id.as_deref(),
                    e
                );
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
        if current_version < 3 {
            Self::migrate_v2_to_v3(conn)?;
        }
        if current_version < 4 {
            Self::migrate_v3_to_v4(conn)?;
        }
        if current_version < 5 {
            Self::migrate_v4_to_v5(conn)?;
        }
        if current_version < 6 {
            Self::migrate_v5_to_v6(conn)?;
        }

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
                is_rejection INTEGER DEFAULT 0,

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

    /// Migration from v2 to v3 (adds is_rejection column to tool_results)
    ///
    /// This distinguishes user rejections from actual errors. The column is
    /// populated at write time using `is_user_rejection()` pattern matching.
    fn migrate_v2_to_v3(conn: &Connection) -> anyhow::Result<()> {
        // Check if column already exists (idempotent)
        let has_column: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('tool_results') WHERE name='is_rejection'",
            [],
            |row| row.get(0),
        )?;

        if !has_column {
            conn.execute(
                "ALTER TABLE tool_results ADD COLUMN is_rejection INTEGER DEFAULT 0",
                [],
            )?;

            // Backfill existing rejections based on known patterns
            conn.execute(
                r#"UPDATE tool_results SET is_rejection = 1
                   WHERE success = 0 AND (
                       output_json LIKE '%The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file)%'
                       OR output_json LIKE '%The user doesn''t want to take this action right now%'
                   )"#,
                [],
            )?;
        }

        conn.execute(
            "UPDATE metadata SET value = '3' WHERE key = 'schema_version'",
            [],
        )?;

        tracing::info!("Migrated lifestats database from v2 to v3 (added is_rejection)");
        Ok(())
    }

    /// Migration from v3 to v4 (adds embedding tables for semantic search)
    ///
    /// Creates tables to store document embeddings alongside FTS indexes.
    /// Embeddings are stored as BLOBs (f32 arrays) and can be used for
    /// vector similarity search to complement FTS5 keyword search.
    ///
    /// # Schema Design
    ///
    /// Each content table (thinking_blocks, user_prompts, assistant_responses)
    /// gets a corresponding embedding table:
    /// - thinking_embeddings
    /// - prompts_embeddings
    /// - responses_embeddings
    ///
    /// Plus a metadata table for tracking embedding configuration:
    /// - embedding_config (provider, model, dimensions)
    fn migrate_v3_to_v4(conn: &Connection) -> anyhow::Result<()> {
        // Check if embedding tables already exist (idempotent)
        let has_table: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='thinking_embeddings'",
            [],
            |row| row.get(0),
        )?;

        if !has_table {
            conn.execute_batch(
                r#"
                -- Embedding configuration (tracks provider, model, dimensions)
                -- If these change, all embeddings need to be re-indexed
                CREATE TABLE IF NOT EXISTS embedding_config (
                    id INTEGER PRIMARY KEY CHECK (id = 1),  -- Singleton row
                    provider TEXT NOT NULL,                  -- 'none', 'local', 'openai', 'azure'
                    model TEXT NOT NULL,                     -- Model identifier
                    dimensions INTEGER NOT NULL,             -- Vector dimensions (e.g., 384, 1536)
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                -- Thinking block embeddings
                -- One-to-one with thinking_blocks, indexed by content_id
                CREATE TABLE IF NOT EXISTS thinking_embeddings (
                    content_id INTEGER PRIMARY KEY,
                    embedding BLOB NOT NULL,                 -- f32 array as bytes
                    embedded_at TEXT NOT NULL,
                    FOREIGN KEY (content_id) REFERENCES thinking_blocks(id) ON DELETE CASCADE
                );

                -- User prompt embeddings
                CREATE TABLE IF NOT EXISTS prompts_embeddings (
                    content_id INTEGER PRIMARY KEY,
                    embedding BLOB NOT NULL,
                    embedded_at TEXT NOT NULL,
                    FOREIGN KEY (content_id) REFERENCES user_prompts(id) ON DELETE CASCADE
                );

                -- Assistant response embeddings
                CREATE TABLE IF NOT EXISTS responses_embeddings (
                    content_id INTEGER PRIMARY KEY,
                    embedding BLOB NOT NULL,
                    embedded_at TEXT NOT NULL,
                    FOREIGN KEY (content_id) REFERENCES assistant_responses(id) ON DELETE CASCADE
                );

                -- Index for finding un-embedded content (for background indexer)
                -- Note: These are partial indexes - only useful for finding rows WITHOUT embeddings
                -- SQLite doesn't support partial indexes, so we use LEFT JOIN in queries instead
                "#,
            )?;
        }

        conn.execute(
            "UPDATE metadata SET value = '4' WHERE key = 'schema_version'",
            [],
        )?;

        tracing::info!("Migrated lifestats database from v3 to v4 (added embedding tables)");
        Ok(())
    }

    /// v4 → v5: Add transcript_path column to sessions table
    ///
    /// This enables cross-session transcript lookup by persisting the path
    /// to Claude Code's transcript file (e.g., ~/.claude/projects/.../abc123.jsonl).
    /// Maps user_id → session → transcript_path for context recovery across proxy restarts.
    fn migrate_v4_to_v5(conn: &Connection) -> anyhow::Result<()> {
        // Check if column already exists (idempotent)
        let has_column: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('sessions') WHERE name = 'transcript_path'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !has_column {
            conn.execute("ALTER TABLE sessions ADD COLUMN transcript_path TEXT", [])?;
        }

        conn.execute(
            "UPDATE metadata SET value = '5' WHERE key = 'schema_version'",
            [],
        )?;

        tracing::info!(
            "Migrated lifestats database from v4 to v5 (added transcript_path to sessions)"
        );
        Ok(())
    }

    /// v5 → v6: Add todos table for tracking Claude's task lists
    ///
    /// Stores snapshots of TodoWrite tool calls for cross-session recall.
    /// Each TodoWrite call creates a new row (append-only), enabling:
    /// - "What was I working on yesterday?"
    /// - Session discovery via todo keywords
    /// - Progress tracking across sessions
    fn migrate_v5_to_v6(conn: &Connection) -> anyhow::Result<()> {
        // Check if table already exists (idempotent)
        let has_table: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='todos'",
            [],
            |row| row.get(0),
        )?;

        if !has_table {
            conn.execute_batch(
                r#"
                -- Todo snapshots (append-only for history)
                -- Each TodoWrite call = new row, captures state at that moment
                CREATE TABLE IF NOT EXISTS todos (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_id TEXT NOT NULL,
                    timestamp TEXT NOT NULL,

                    -- The todo list as JSON array (preserves exact Claude state)
                    -- Format: [{"content": "...", "status": "...", "activeForm": "..."}]
                    todos_json TEXT NOT NULL,

                    -- Denormalized counts for efficient dashboard queries
                    pending_count INTEGER NOT NULL DEFAULT 0,
                    in_progress_count INTEGER NOT NULL DEFAULT 0,
                    completed_count INTEGER NOT NULL DEFAULT 0,

                    FOREIGN KEY (session_id) REFERENCES sessions(id)
                );

                CREATE INDEX IF NOT EXISTS idx_todos_session ON todos(session_id);
                CREATE INDEX IF NOT EXISTS idx_todos_timestamp ON todos(timestamp DESC);

                -- Full-text search on todo content (concatenated from todos_json)
                CREATE VIRTUAL TABLE IF NOT EXISTS todos_fts USING fts5(
                    content,
                    content=todos,
                    content_rowid=id,
                    tokenize='porter unicode61'
                );

                -- Embedding table for semantic search
                CREATE TABLE IF NOT EXISTS todos_embeddings (
                    content_id INTEGER PRIMARY KEY,
                    embedding BLOB NOT NULL,
                    embedded_at TEXT NOT NULL,
                    FOREIGN KEY (content_id) REFERENCES todos(id) ON DELETE CASCADE
                );
                "#,
            )?;
        }

        conn.execute(
            "UPDATE metadata SET value = '6' WHERE key = 'schema_version'",
            [],
        )?;

        tracing::info!("Migrated lifestats database from v5 to v6 (added todos table)");
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
        tracing::debug!(
            "Deleted {} entries from responses_fts",
            responses_fts_deleted
        );

        // 3b. Delete from todos_fts FIRST
        let todos_fts_deleted: i64 = conn.execute(
            r#"
            DELETE FROM todos_fts
            WHERE rowid IN (
                SELECT id FROM todos WHERE timestamp < ?1
            )
            "#,
            params![cutoff_str],
        )? as i64;
        tracing::debug!("Deleted {} entries from todos_fts", todos_fts_deleted);

        // 4. Delete from embedding tables BEFORE base tables
        //    (FK cascade is disabled, so we delete explicitly)
        conn.execute(
            r#"
            DELETE FROM thinking_embeddings
            WHERE content_id IN (
                SELECT id FROM thinking_blocks WHERE timestamp < ?1
            )
            "#,
            params![cutoff_str],
        )?;
        conn.execute(
            r#"
            DELETE FROM prompts_embeddings
            WHERE content_id IN (
                SELECT id FROM user_prompts WHERE timestamp < ?1
            )
            "#,
            params![cutoff_str],
        )?;
        conn.execute(
            r#"
            DELETE FROM responses_embeddings
            WHERE content_id IN (
                SELECT id FROM assistant_responses WHERE timestamp < ?1
            )
            "#,
            params![cutoff_str],
        )?;
        conn.execute(
            r#"
            DELETE FROM todos_embeddings
            WHERE content_id IN (
                SELECT id FROM todos WHERE timestamp < ?1
            )
            "#,
            params![cutoff_str],
        )?;

        // 5. Now delete from base tables (order matters for FK relationships)
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
            "DELETE FROM todos WHERE timestamp < ?1",
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

        // 6. Clean up orphaned sessions (no recent activity)
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

        // Ensure session exists before storing any event
        // Use UPSERT to handle transcript_path updates (may arrive after session creation)
        if let Some(sid) = session_id {
            conn.execute(
                "INSERT INTO sessions (id, user_id, started_at, source, transcript_path)
                 VALUES (?1, ?2, datetime('now'), 'first_seen', ?3)
                 ON CONFLICT(id) DO UPDATE SET
                     transcript_path = COALESCE(excluded.transcript_path, sessions.transcript_path)",
                params![sid, ctx.user_id.as_deref(), ctx.transcript_path.as_deref()],
            )?;
        }

        match event {
            ProxyEvent::Thinking {
                timestamp,
                content,
                token_estimate,
            } if config.store_thinking => {
                // Truncate if too large (safely respecting UTF-8 boundaries)
                let content = if content.len() > config.max_thinking_size {
                    format!(
                        "{}... [truncated, {} bytes total]",
                        truncate_utf8_safe(content, config.max_thinking_size),
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
                let output_str = output.to_string();
                let output_json = if config.store_tool_io {
                    Some(&output_str)
                } else {
                    None
                };

                // Detect user rejections vs actual errors
                let is_rejection = !success && is_user_rejection(&output_str);

                conn.execute(
                    "INSERT OR REPLACE INTO tool_results (call_id, timestamp, output_json, duration_ms, success, is_rejection)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        id,
                        timestamp.to_rfc3339(),
                        output_json,
                        duration.as_millis() as i64,
                        *success as i32,
                        is_rejection as i32
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

            ProxyEvent::TodoSnapshot {
                timestamp,
                todos_json,
                pending_count,
                in_progress_count,
                completed_count,
            } => {
                conn.execute(
                    "INSERT INTO todos (session_id, timestamp, todos_json, pending_count, in_progress_count, completed_count)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        session_id,
                        timestamp.to_rfc3339(),
                        todos_json,
                        pending_count,
                        in_progress_count,
                        completed_count
                    ],
                )?;

                // Update FTS index with concatenated todo content for search
                // Extract content from todos_json for searchable text
                let rowid = conn.last_insert_rowid();
                let fts_content = Self::extract_todo_content_for_fts(todos_json);
                conn.execute(
                    "INSERT INTO todos_fts(rowid, content) VALUES (?1, ?2)",
                    params![rowid, fts_content],
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
            .try_send(WriterCommand::Store(Box::new(event.clone()), ctx.clone()))
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
