# RFC: Event Pipeline & Lifestats Storage

**Status:** Draft (v3 - Final review pass)
**Author:** Claude (with human direction)
**Created:** 2025-12-01
**Revised:** 2025-12-01
**Target Version:** v0.2.0

## Summary

This RFC proposes an extensible event processing pipeline that enables:
1. **Event transformation** (redaction, enrichment)
2. **Event filtering** (drop by type/content)
3. **Side-effect processing** (metrics export, persistent storage, webhooks)
4. **Cross-session context recovery** via queryable lifetime statistics

The primary use case is enabling Claude to recover lost context from past sessions—validated by manual jq workflows that this system will replace.

---

## Motivation

### The Problem

Context compaction is inevitable in long Claude Code sessions. When context is compacted:
- Claude loses awareness of earlier conversation
- User preferences, decisions, and "vibe" are forgotten
- Manual recovery requires jq archaeology on JSONL logs

### Validated Solution

The maintainer has already validated this workflow manually:
1. Query session logs with jq for specific topics
2. Extract user prompts and Claude's thinking
3. Feed recovered context to a fresh Claude session
4. Claude continues with full awareness of prior decisions

**Example:** Recovering theme preferences after compaction:
```bash
jq 'select(.type == "Thinking") | select(.content | contains("solarized"))' \
  logs/session-xyz.jsonl
```

This RFC productizes that workflow into first-class MCP tools.

### Design Goals

1. **Minimal changes to kernel** - Event pipeline is opt-in, existing flow unchanged when disabled
2. **Composable processors** - Each processor is independent, can be enabled/disabled
3. **Query-first storage** - SQLite schema optimized for context recovery queries
4. **MCP-native access** - Claude can query past context without user intervention
5. **No event loss** - Observability tool must not silently drop events
6. **Non-blocking writes** - Storage must not impact proxy latency

---

## Architecture

### Current Event Flow

```
ProxyEvent
    │
    └──→ send_event()
            │
            ├──→ event_tx_tui.send()      → TUI
            ├──→ event_tx_storage.send()  → JSONL files
            └──→ sessions.record_event()  → In-memory session
```

### Proposed Event Flow

```
ProxyEvent
    │
    └──→ EventPipeline.process()
            │
            ├──→ [Processor 1] transform/filter/side-effect
            ├──→ [Processor 2] transform/filter/side-effect
            └──→ [Processor N] transform/filter/side-effect
            │
            └──→ send_event() (if not filtered)
                    │
                    ├──→ event_tx_tui.send()
                    ├──→ event_tx_storage.send()
                    └──→ sessions.record_event()
```

### Layer Classification

| Component | Layer | Rationale |
|-----------|-------|-----------|
| `EventPipeline` trait | Kernel | Core infrastructure, always available |
| `LifestatsProcessor` | Userland | Optional, config-toggleable |
| `MetricsProcessor` | Userland | Optional, config-toggleable |
| `RedactionProcessor` | Userland | Optional, security feature |
| Custom processors | User Space | User-provided via plugins |

---

## Detailed Design

### 1. EventPipeline Trait

**Location:** `src/pipeline/mod.rs`

```rust
//! Event processing pipeline for extensible event handling
//!
//! This module provides a trait-based system for processing events before
//! they are dispatched to consumers (TUI, storage, sessions). Processors
//! can transform, filter, or react to events without modifying core logic.
//!
//! # Architecture
//!
//! ```text
//! ProxyEvent → EventPipeline → [Processor₁, Processor₂, ...] → Processed Event
//! ```
//!
//! # Processor Types
//!
//! Processors can perform three operations:
//! - **Filter**: Drop events (return `ProcessResult::Drop`)
//! - **Transform**: Modify events (return `ProcessResult::Transform(modified)`)
//! - **Side-effect**: React to events without modification (return `ProcessResult::Continue`)

use crate::events::ProxyEvent;
use std::borrow::Cow;
use std::sync::Arc;

/// Result of processing an event
#[derive(Debug)]
pub enum ProcessResult {
    /// Event continues unchanged (side-effect only processor)
    Continue,
    /// Event was transformed - use this new version
    Transform(ProxyEvent),
    /// Event should be dropped (filtered out)
    Drop,
    /// Processor encountered an error (event continues, error logged)
    Error(anyhow::Error),
}

/// Context provided to processors for decision-making
///
/// Uses `Arc<str>` for cheap cloning - processor side-effects often need
/// to clone context for async operations, and Arc clone is just a refcount bump.
#[derive(Debug, Clone)]
pub struct ProcessContext {
    /// Current session ID (if known)
    pub session_id: Option<Arc<str>>,
    /// User ID (API key hash, if known)
    pub user_id: Option<Arc<str>>,
    /// Whether this is a demo/test event
    pub is_demo: bool,
}

impl ProcessContext {
    pub fn new(session_id: Option<&str>, user_id: Option<&str>, is_demo: bool) -> Self {
        Self {
            session_id: session_id.map(Arc::from),
            user_id: user_id.map(Arc::from),
            is_demo,
        }
    }
}

impl Default for ProcessContext {
    fn default() -> Self {
        Self {
            session_id: None,
            user_id: None,
            is_demo: false,
        }
    }
}

/// Trait for event processors
///
/// Processors are called in registration order. Each processor can:
/// - Transform the event (return `ProcessResult::Transform(new_event)`)
/// - Filter the event (return `ProcessResult::Drop`)
/// - Perform side effects and pass through (return `ProcessResult::Continue`)
///
/// # Sync Design
///
/// `process` is intentionally synchronous. For I/O-bound operations
/// (database writes, HTTP calls), processors should use internal
/// channels to offload work to dedicated threads. This ensures the
/// pipeline never blocks the async runtime.
///
/// # Reference Semantics
///
/// Processors receive a reference to the event. Only processors that
/// need to transform the event should clone it. Side-effect processors
/// return `Continue` without any allocation.
pub trait EventProcessor: Send + Sync {
    /// Human-readable name for logging and debugging
    fn name(&self) -> &'static str;

    /// Process an event, returning the result
    ///
    /// # Arguments
    /// * `event` - Reference to the event (clone only if transforming)
    /// * `ctx` - Context about the current session/user
    ///
    /// # Returns
    /// * `ProcessResult::Continue` - Pass event unchanged to next processor
    /// * `ProcessResult::Transform(event)` - Pass modified event to next processor
    /// * `ProcessResult::Drop` - Remove event from pipeline
    /// * `ProcessResult::Error(e)` - Log error, continue with original event
    fn process(&self, event: &ProxyEvent, ctx: &ProcessContext) -> ProcessResult;

    /// Called when the pipeline is shutting down
    ///
    /// Use this for cleanup: flush buffers, signal threads to stop, etc.
    /// Implementations MUST block until cleanup is complete (e.g., background
    /// threads have finished flushing).
    fn shutdown(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Pipeline that runs events through registered processors
pub struct EventPipeline {
    processors: Vec<Arc<dyn EventProcessor>>,
}

impl EventPipeline {
    /// Create an empty pipeline (passthrough)
    pub fn new() -> Self {
        Self {
            processors: Vec::new(),
        }
    }

    /// Register a processor
    ///
    /// Processors are called in registration order.
    pub fn register(&mut self, processor: impl EventProcessor + 'static) {
        self.processors.push(Arc::new(processor));
    }

    /// Process an event through all registered processors
    ///
    /// Returns `Some(Cow::Borrowed(event))` if no transformation occurred,
    /// `Some(Cow::Owned(event))` if any processor transformed the event,
    /// `None` if any processor filtered it out.
    ///
    /// Using `Cow` avoids cloning when all processors are side-effect-only.
    pub fn process<'a>(
        &self,
        event: &'a ProxyEvent,
        ctx: &ProcessContext,
    ) -> Option<Cow<'a, ProxyEvent>> {
        if self.processors.is_empty() {
            return Some(Cow::Borrowed(event));
        }

        // Track whether we've had to clone yet
        let mut current: Cow<'a, ProxyEvent> = Cow::Borrowed(event);

        for processor in &self.processors {
            match processor.process(current.as_ref(), ctx) {
                ProcessResult::Continue => {
                    // No change, keep current (borrowed or owned)
                }
                ProcessResult::Transform(new_event) => {
                    // Processor transformed the event
                    current = Cow::Owned(new_event);
                }
                ProcessResult::Drop => {
                    tracing::trace!(
                        "Event dropped by processor '{}'",
                        processor.name()
                    );
                    return None;
                }
                ProcessResult::Error(error) => {
                    tracing::error!(
                        "Processor '{}' error: {}",
                        processor.name(),
                        error
                    );
                    // Continue with current event despite error
                }
            }
        }
        Some(current)
    }

    /// Shutdown all processors gracefully
    ///
    /// Calls shutdown() on each processor in reverse registration order.
    /// Blocks until all processors have completed cleanup.
    pub fn shutdown(&self) -> anyhow::Result<()> {
        // Shutdown in reverse order (LIFO) - processors registered last
        // may depend on those registered first
        for processor in self.processors.iter().rev() {
            if let Err(e) = processor.shutdown() {
                tracing::warn!(
                    "Processor '{}' shutdown error: {}",
                    processor.name(),
                    e
                );
            }
        }
        Ok(())
    }

    /// Check if pipeline has any processors
    pub fn is_empty(&self) -> bool {
        self.processors.is_empty()
    }

    /// Get names of registered processors (for logging/debug)
    pub fn processor_names(&self) -> Vec<&'static str> {
        self.processors.iter().map(|p| p.name()).collect()
    }
}

impl Default for EventPipeline {
    fn default() -> Self {
        Self::new()
    }
}
```

### 2. LifestatsProcessor (Revised)

**Key changes from v1:**
- Dedicated OS thread for SQLite (not tokio task)
- Write batching with flush timer
- Explicit backpressure handling with metrics
- WAL mode for concurrent reads

**Key changes from v2 (this revision):**
- Completion signal for graceful shutdown (fixes race condition)
- Schema migration system for safe upgrades
- FTS delete sync contract documented
- `events_store_failed` metric for partial batch failures

**Location:** `src/pipeline/lifestats.rs`

```rust
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
            channel_buffer: 10_000,     // Buffer before backpressure
            batch_size: 100,            // Flush every 100 events
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
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            events_stored: self.events_stored.load(Ordering::Relaxed),
            events_dropped: self.events_dropped.load(Ordering::Relaxed),
            events_store_failed: self.events_store_failed.load(Ordering::Relaxed),
            batch_pending: self.batch_pending.load(Ordering::Relaxed),
            avg_write_latency_us: {
                let total = self.write_latency_us.load(Ordering::Relaxed);
                let count = self.flush_count.load(Ordering::Relaxed);
                if count > 0 { total / count } else { 0 }
            },
        }
    }
}

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
    /// Config for reference
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

        loop {
            // Wait for event with timeout (for periodic flush)
            match rx.recv_timeout(config.flush_interval) {
                Ok(WriterCommand::Store(event, ctx)) => {
                    batch.push((event, ctx));
                    metrics.batch_pending.store(batch.len() as u64, Ordering::Relaxed);

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
        metrics.events_stored.fetch_add(stored_count, Ordering::Relaxed);
        if failed_count > 0 {
            metrics.events_store_failed.fetch_add(failed_count, Ordering::Relaxed);
        }
        metrics.write_latency_us.fetch_add(latency, Ordering::Relaxed);
        metrics.flush_count.fetch_add(1, Ordering::Relaxed);
        metrics.batch_pending.store(0, Ordering::Relaxed);

        tracing::trace!(
            "Flushed {} events ({} failed) in {}µs",
            count, failed_count, latency
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

        // 3. Now delete from base tables (order matters for FK relationships)
        deleted += conn.execute(
            "DELETE FROM thinking_blocks WHERE timestamp < ?1",
            params![cutoff_str],
        )? as u64;

        deleted += conn.execute(
            "DELETE FROM user_prompts WHERE timestamp < ?1",
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

            // Note: User prompts are extracted in parser from request body
            // See "User Prompt Extraction" section below

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
        match self.tx.try_send(WriterCommand::Store(event.clone(), ctx.clone())) {
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
```

### 3. Query Module with Connection Pooling

**Location:** `src/pipeline/lifestats_query.rs`

```rust
//! Query interface for lifestats database
//!
//! Provides structured queries for context recovery, used by MCP tools.
//! Uses connection pooling for efficient concurrent access.

use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::Path;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStats {
    pub model: String,
    pub tokens: i64,
    pub cost_usd: f64,
    pub calls: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStats {
    pub tool: String,
    pub calls: i64,
    pub avg_duration_ms: f64,
    pub success_rate: f64,
}

/// Search mode for FTS queries
///
/// Controls how the query string is processed before being sent to FTS5.
/// Different modes offer trade-offs between safety and power.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
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

/// Type of context match
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MatchType {
    Thinking,
    UserPrompt,
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

/// Query interface for lifestats database
///
/// Uses connection pooling for efficient concurrent access.
pub struct LifestatsQuery {
    pool: Pool<SqliteConnectionManager>,
}

impl LifestatsQuery {
    /// Create a new query interface with connection pool
    pub fn new(db_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let manager = SqliteConnectionManager::file(db_path);
        let pool = Pool::builder()
            .max_size(4)  // Read-only pool
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
    /// # Arguments
    /// * `query` - The search query
    /// * `limit` - Maximum number of results
    /// * `mode` - How to interpret the query (default: Phrase)
    pub fn search_thinking(
        &self,
        query: &str,
        limit: usize,
        mode: SearchMode,
    ) -> anyhow::Result<Vec<ThinkingMatch>> {
        let conn = self.conn()?;

        // Process query according to mode
        let safe_query = Self::process_query(query, mode);

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
    /// # Arguments
    /// * `query` - The search query
    /// * `limit` - Maximum number of results
    /// * `mode` - How to interpret the query (default: Phrase)
    pub fn search_prompts(
        &self,
        query: &str,
        limit: usize,
        mode: SearchMode,
    ) -> anyhow::Result<Vec<PromptMatch>> {
        let conn = self.conn()?;
        let safe_query = Self::process_query(query, mode);

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

    /// Get lifetime statistics
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

    /// Combined context recovery query
    ///
    /// # Arguments
    /// * `topic` - The topic to search for
    /// * `limit` - Maximum results per source (thinking + prompts)
    /// * `mode` - How to interpret the query (default: Phrase)
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

        // Sort by rank (lower = more relevant)
        results.sort_by(|a, b| a.rank.partial_cmp(&b.rank).unwrap_or(std::cmp::Ordering::Equal));

        // Limit total results
        results.truncate(limit);

        Ok(results)
    }

    /// Process query according to SearchMode
    ///
    /// # Modes
    /// - **Phrase**: Escape everything, wrap in quotes (safest)
    /// - **Natural**: Allow AND/OR/NOT and prefix wildcards, escape rest
    /// - **Raw**: Pass through as-is (dangerous, full FTS5 syntax)
    fn process_query(query: &str, mode: SearchMode) -> String {
        match mode {
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
```

---

## User Prompt Extraction

**Location:** Proxy module (`src/proxy/mod.rs`) during request interception

### Extraction Flow

User prompts are extracted from the **request body** (not the response) during the proxy's request interception. This happens in the proxy module when a POST `/messages` request arrives.

```text
┌─────────────────────────────────────────────────────────────────────┐
│                     User Prompt Extraction Flow                      │
└─────────────────────────────────────────────────────────────────────┘

 Claude Code                Proxy                     Parser
     │                        │                          │
     │  POST /v1/messages     │                          │
     │  ─────────────────────>│                          │
     │                        │                          │
     │                        │   intercept_request()    │
     │                        │  ──────────────────────> │
     │                        │                          │
     │                        │                          ├─── parse body JSON
     │                        │                          │
     │                        │                          ├─── extract messages[]
     │                        │                          │
     │                        │                          ├─── find last user message
     │                        │                          │     (iterate reverse)
     │                        │                          │
     │                        │                          ├─── extract text content
     │                        │                          │     (string or array)
     │                        │                          │
     │                        │   ProxyEvent::UserPrompt │
     │                        │  <────────────────────── │
     │                        │                          │
     │                        │                          │
     │                        │   send_event()           │
     │                        │  ───────────────────────>│ EventPipeline
     │                        │                          │     │
     │                        │                          │     ├─→ LifestatsProcessor
     │                        │                          │     │       (stores to SQLite)
     │                        │                          │     │
     │                        │                          │     └─→ TUI, Storage, etc.
     │                        │                          │
     │                        │ forward to Anthropic     │
     │                        │  ─────────────────────────────────────────>
```

### Key Points

1. **Timing**: Extraction happens BEFORE the request is forwarded to Anthropic
2. **Source**: The `messages` array in the request body
3. **Selection**: We take the LAST user message (most recent in conversation)
4. **Content formats**: Handle both string and array (multipart) content

### Implementation

```rust
// In proxy/mod.rs, when intercepting POST /messages requests

fn extract_user_prompt(body: &serde_json::Value) -> Option<String> {
    // Get the messages array
    let messages = body.get("messages")?.as_array()?;

    // Find the last user message
    for message in messages.iter().rev() {
        if message.get("role")?.as_str()? == "user" {
            // Handle both string and array content formats
            match message.get("content")? {
                serde_json::Value::String(s) => return Some(s.clone()),
                serde_json::Value::Array(parts) => {
                    // Concatenate text parts
                    let text: Vec<&str> = parts
                        .iter()
                        .filter_map(|p| {
                            if p.get("type")?.as_str()? == "text" {
                                p.get("text")?.as_str()
                            } else {
                                None
                            }
                        })
                        .collect();
                    if !text.is_empty() {
                        return Some(text.join("\n"));
                    }
                }
                _ => {}
            }
        }
    }
    None
}

// Emit as a new event type or store directly
pub enum ProxyEvent {
    // ... existing variants ...

    /// User's prompt extracted from request
    UserPrompt {
        timestamp: DateTime<Utc>,
        content: String,
    },
}
```

---

## Session Boundary Strategy

Hybrid approach as recommended:

```rust
pub enum SessionSource {
    /// Explicit from SessionStart hook (best source of truth)
    Hook,
    /// Implicit from first event with new user_id
    FirstSeen,
    /// Detected from warmup/ping request pattern
    Warmup,
}

impl SessionManager {
    /// Get or create session for a user
    pub fn ensure_session(&mut self, user_id: &UserId, source: SessionSource) -> &mut Session {
        if !self.has_active_session(user_id) {
            self.create_session(user_id, source);
        }
        self.get_session_mut(user_id).unwrap()
    }

    /// Close session after inactivity
    pub fn close_idle_sessions(&mut self, idle_threshold: Duration) {
        let now = Instant::now();
        for session in self.sessions.values_mut() {
            if session.is_active() && session.last_event_at.elapsed() > idle_threshold {
                session.close(EndReason::Timeout);
            }
        }
    }
}
```

---

## Dependencies (Revised)

```toml
[dependencies]
rusqlite = { version = "0.31", features = ["bundled", "fts5"] }
r2d2 = "0.8"
r2d2_sqlite = "0.24"
# Note: async-trait NOT needed - processor trait is sync
```

---

## Implementation Phases (Revised)

### Phase 1a: Core Pipeline (Minimal)
1. `EventProcessor` trait + `EventPipeline` struct
2. Wire into `send_event()` with empty pipeline (no-op)
3. Add `LoggingProcessor` to validate flow
4. Tests for pipeline mechanics

### Phase 1b: Storage Foundation
1. SQLite schema with WAL mode
2. Dedicated writer thread with batch buffer
3. Backpressure handling with metrics
4. `/api/lifestats/health` endpoint
5. **TODO**: Wire up `run_retention_cleanup()` - options:
   - Periodic check in writer thread (e.g., every 24 hours)
   - CLI command: `anthropic-spy --cleanup`
   - HTTP endpoint: `POST /api/lifestats/cleanup`

### Phase 1c: LifestatsProcessor
1. Connect to writer thread
2. Event routing logic
3. User prompt extraction in proxy (request interception)
4. FTS index updates

### Phase 2: Query Interface
1. `LifestatsQuery` with connection pool
2. FTS5 search methods
3. Lifetime stats aggregation
4. HTTP API endpoints

### Phase 3: MCP Tools
1. `aspy_recall_context` tool
2. `aspy_lifetime_stats` tool
3. `aspy_search_thinking` tool
4. `aspy_session_history` tool

---

## Testing Strategy (Expanded)

### Unit Tests
- Pipeline: processor ordering, filtering, error handling
- SQLite: schema initialization, WAL mode verification
- FTS: special characters, empty queries, relevance ranking

### Integration Tests
- Event flow through complete pipeline
- Concurrent read/write access
- Backpressure behavior under load

### Load Tests
- 100 events/second sustained for 10 minutes
- Verify no event loss (unless explicit backpressure)
- Measure write latency distribution

### Recovery Tests
- Kill process during batch write
- Verify database integrity on restart
- WAL checkpoint recovery

---

## Metrics Exposure

```rust
// Add to /api/lifestats/health
#[derive(Serialize)]
pub struct LifestatsHealth {
    pub status: &'static str,  // "healthy", "degraded", "unhealthy"
    pub events_stored: u64,
    pub events_dropped: u64,
    pub batch_pending: u64,
    pub avg_write_latency_us: u64,
    pub db_size_bytes: u64,
}
```

---

## Open Questions (Resolved)

| Question | Resolution |
|----------|------------|
| User prompt extraction | Extract from `messages` array in request body |
| Session boundaries | Hybrid: Hook (preferred) → FirstSeen (fallback) → Timeout (close) |
| Cost calculation | Calculate at write time using pricing module |
| Privacy/redaction | Defer to separate `RedactionProcessor` in Phase 2 |

---

## Summary of Peer Review Changes

### v1 → v2 (First Peer Review)

| Issue | Fix |
|-------|-----|
| 🔴 SQLite blocking async | Dedicated OS thread via `std::thread::spawn` |
| 🔴 Silent event loss | Metrics counter, warn log on drop |
| 🔴 No write batching | Batch buffer with size/time flush triggers |
| 🟠 Event cloning | Reference semantics in trait, clone only when needed |
| 🟠 FTS trigger overhead | Manual FTS insert in batch, not per-row trigger |
| 🟡 Missing WAL mode | Added in schema init with proper PRAGMAs |
| 🟡 No connection pooling | r2d2-sqlite for query interface |
| 🟡 No error handling | `ProcessResult::Error` variant added |

### v2 → v3 (Second Peer Review)

| Issue | Fix |
|-------|-----|
| 🔴 Shutdown race condition | `CompletionSignal` with Condvar, `shutdown()` waits for completion |
| 🔴 FTS delete sync not documented | `run_retention_cleanup()` with explicit FTS delete contract |
| 🔴 No schema migration system | Version-based migrations (`init_schema`, `apply_schema_v1`, `migrate_v1_to_v2`) |
| 🔴 User prompt extraction underspecified | Sequence diagram and implementation guidance added |
| 🟠 FTS query escaping disables advanced search | `SearchMode` enum (Phrase/Natural/Raw) |
| 🟠 Partial batch commits no metric | `events_store_failed` counter in `LifestatsMetrics` |
| 🟠 ProcessContext cloning overhead | Changed to `Arc<str>` for cheap cloning |
| 🟡 Pipeline always clones event | Changed to `Cow<ProxyEvent>` for zero-copy passthrough |
| 🟡 ProcessResult::Continue still clones | Removed event parameter, now returns unit `Continue` |
