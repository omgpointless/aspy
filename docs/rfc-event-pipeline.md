# RFC: Event Pipeline & Lifestats Storage

**Status:** Draft
**Author:** Claude (with human direction)
**Created:** 2025-12-01
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
//! - **Filter**: Drop events (return `None` from `process`)
//! - **Transform**: Modify events (return `Some(modified_event)`)
//! - **Side-effect**: React to events without modification (metrics, storage)

use crate::events::ProxyEvent;
use async_trait::async_trait;
use std::sync::Arc;

/// Result of processing an event
#[derive(Debug)]
pub enum ProcessResult {
    /// Event should continue through pipeline (possibly modified)
    Continue(ProxyEvent),
    /// Event should be dropped (filtered out)
    Drop,
}

/// Context provided to processors for decision-making
#[derive(Debug, Clone)]
pub struct ProcessContext {
    /// Current session ID (if known)
    pub session_id: Option<String>,
    /// User ID (API key hash, if known)
    pub user_id: Option<String>,
    /// Whether this is a demo/test event
    pub is_demo: bool,
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
/// - Transform the event (modify and pass through)
/// - Filter the event (drop it from the pipeline)
/// - Perform side effects (logging, storage, metrics)
///
/// # Async Design
///
/// `process` is async to support I/O-bound operations (database writes,
/// HTTP calls). Processors should avoid blocking and use timeouts for
/// external calls.
///
/// # Example
///
/// ```ignore
/// pub struct LoggingProcessor;
///
/// #[async_trait]
/// impl EventProcessor for LoggingProcessor {
///     fn name(&self) -> &'static str { "logging" }
///
///     async fn process(&self, event: ProxyEvent, ctx: &ProcessContext) -> ProcessResult {
///         tracing::debug!("Event: {:?}", event);
///         ProcessResult::Continue(event)
///     }
/// }
/// ```
#[async_trait]
pub trait EventProcessor: Send + Sync {
    /// Human-readable name for logging and debugging
    fn name(&self) -> &'static str;

    /// Process an event, returning the result
    ///
    /// # Arguments
    /// * `event` - The event to process (owned for potential transformation)
    /// * `ctx` - Context about the current session/user
    ///
    /// # Returns
    /// * `ProcessResult::Continue(event)` - Pass event to next processor
    /// * `ProcessResult::Drop` - Remove event from pipeline
    async fn process(&self, event: ProxyEvent, ctx: &ProcessContext) -> ProcessResult;

    /// Called when the pipeline is shutting down
    ///
    /// Use this for cleanup: flush buffers, close connections, etc.
    /// Default implementation does nothing.
    async fn shutdown(&self) -> anyhow::Result<()> {
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
    /// Returns `Some(event)` if the event should be emitted,
    /// `None` if any processor filtered it out.
    pub async fn process(
        &self,
        mut event: ProxyEvent,
        ctx: &ProcessContext,
    ) -> Option<ProxyEvent> {
        for processor in &self.processors {
            match processor.process(event, ctx).await {
                ProcessResult::Continue(e) => {
                    event = e;
                }
                ProcessResult::Drop => {
                    tracing::trace!(
                        "Event dropped by processor '{}'",
                        processor.name()
                    );
                    return None;
                }
            }
        }
        Some(event)
    }

    /// Shutdown all processors gracefully
    pub async fn shutdown(&self) -> anyhow::Result<()> {
        for processor in &self.processors {
            if let Err(e) = processor.shutdown().await {
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

### 2. LifestatsProcessor

**Location:** `src/pipeline/lifestats.rs`

```rust
//! Lifetime statistics storage processor
//!
//! Stores events in SQLite for cross-session querying. Enables context
//! recovery via MCP tools.

use super::{EventProcessor, ProcessContext, ProcessResult};
use crate::events::ProxyEvent;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;
use tokio::sync::mpsc;

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
}

impl Default for LifestatsConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("./data/lifestats.db"),
            store_thinking: true,
            store_tool_io: true,
            max_thinking_size: 100_000, // ~100KB per thinking block
            retention_days: 90,
        }
    }
}

/// Lifetime statistics processor
///
/// Writes events to SQLite for cross-session queries.
/// Uses a background task to avoid blocking the event pipeline.
pub struct LifestatsProcessor {
    /// Channel to send events to background writer
    tx: mpsc::Sender<StorageCommand>,
    /// Config for reference
    config: LifestatsConfig,
}

enum StorageCommand {
    Store(ProxyEvent, ProcessContext),
    Shutdown,
}

impl LifestatsProcessor {
    /// Create a new lifestats processor
    ///
    /// Spawns a background task for database writes.
    pub fn new(config: LifestatsConfig) -> anyhow::Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = config.db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Initialize database
        let conn = Connection::open(&config.db_path)?;
        Self::init_schema(&conn)?;

        // Create channel for background writes
        let (tx, rx) = mpsc::channel::<StorageCommand>(1000);

        // Spawn background writer
        let db_path = config.db_path.clone();
        let store_thinking = config.store_thinking;
        let store_tool_io = config.store_tool_io;
        let max_thinking_size = config.max_thinking_size;

        tokio::spawn(async move {
            Self::background_writer(rx, db_path, store_thinking, store_tool_io, max_thinking_size)
                .await;
        });

        Ok(Self { tx, config })
    }

    /// Initialize database schema
    fn init_schema(conn: &Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            r#"
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

            -- Full-text search on thinking blocks
            CREATE VIRTUAL TABLE IF NOT EXISTS thinking_fts USING fts5(
                content,
                content=thinking_blocks,
                content_rowid=id
            );

            -- Triggers to keep FTS in sync
            CREATE TRIGGER IF NOT EXISTS thinking_ai AFTER INSERT ON thinking_blocks BEGIN
                INSERT INTO thinking_fts(rowid, content) VALUES (new.id, new.content);
            END;
            CREATE TRIGGER IF NOT EXISTS thinking_ad AFTER DELETE ON thinking_blocks BEGIN
                INSERT INTO thinking_fts(thinking_fts, rowid, content)
                VALUES('delete', old.id, old.content);
            END;

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

            -- User prompts (extracted from tool calls or requests)
            CREATE TABLE IF NOT EXISTS user_prompts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT,
                timestamp TEXT NOT NULL,
                content TEXT NOT NULL,

                FOREIGN KEY (session_id) REFERENCES sessions(id)
            );
            CREATE INDEX IF NOT EXISTS idx_prompts_session ON user_prompts(session_id);
            CREATE INDEX IF NOT EXISTS idx_prompts_timestamp ON user_prompts(timestamp);

            -- Full-text search on user prompts
            CREATE VIRTUAL TABLE IF NOT EXISTS prompts_fts USING fts5(
                content,
                content=user_prompts,
                content_rowid=id
            );

            CREATE TRIGGER IF NOT EXISTS prompts_ai AFTER INSERT ON user_prompts BEGIN
                INSERT INTO prompts_fts(rowid, content) VALUES (new.id, new.content);
            END;
            CREATE TRIGGER IF NOT EXISTS prompts_ad AFTER DELETE ON user_prompts BEGIN
                INSERT INTO prompts_fts(prompts_fts, rowid, content)
                VALUES('delete', old.id, old.content);
            END;

            -- Metadata table for schema versioning
            CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY,
                value TEXT
            );
            INSERT OR IGNORE INTO metadata (key, value) VALUES ('schema_version', '1');
            "#,
        )?;

        Ok(())
    }

    /// Background task that handles database writes
    async fn background_writer(
        mut rx: mpsc::Receiver<StorageCommand>,
        db_path: PathBuf,
        store_thinking: bool,
        store_tool_io: bool,
        max_thinking_size: usize,
    ) {
        // Open connection in background task
        let conn = match Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to open lifestats database: {}", e);
                return;
            }
        };

        while let Some(cmd) = rx.recv().await {
            match cmd {
                StorageCommand::Store(event, ctx) => {
                    if let Err(e) = Self::store_event(
                        &conn,
                        &event,
                        &ctx,
                        store_thinking,
                        store_tool_io,
                        max_thinking_size,
                    ) {
                        tracing::warn!("Failed to store event: {}", e);
                    }
                }
                StorageCommand::Shutdown => {
                    tracing::debug!("Lifestats background writer shutting down");
                    break;
                }
            }
        }
    }

    /// Store an event in the database
    fn store_event(
        conn: &Connection,
        event: &ProxyEvent,
        ctx: &ProcessContext,
        store_thinking: bool,
        store_tool_io: bool,
        max_thinking_size: usize,
    ) -> anyhow::Result<()> {
        let session_id = ctx.session_id.as_deref();

        match event {
            ProxyEvent::Thinking {
                timestamp,
                content,
                token_estimate,
            } if store_thinking => {
                // Truncate if too large
                let content = if content.len() > max_thinking_size {
                    format!(
                        "{}... [truncated, {} bytes total]",
                        &content[..max_thinking_size],
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
            }

            ProxyEvent::ToolCall {
                id,
                timestamp,
                tool_name,
                input,
            } => {
                let input_json = if store_tool_io {
                    Some(input.to_string())
                } else {
                    None
                };

                conn.execute(
                    "INSERT OR REPLACE INTO tool_calls (id, session_id, timestamp, tool_name, input_json)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, session_id, timestamp.to_rfc3339(), tool_name, input_json],
                )?;

                // Extract user prompt if this is a message submission
                // (Tool name patterns that indicate user input)
                if tool_name == "UserPrompt" || tool_name.contains("Submit") {
                    if let Some(content) = input.get("content").and_then(|v| v.as_str()) {
                        conn.execute(
                            "INSERT INTO user_prompts (session_id, timestamp, content)
                             VALUES (?1, ?2, ?3)",
                            params![session_id, timestamp.to_rfc3339(), content],
                        )?;
                    }
                }
            }

            ProxyEvent::ToolResult {
                id,
                timestamp,
                output,
                duration,
                success,
                ..
            } => {
                let output_json = if store_tool_io {
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
                // Calculate cost (simplified, should use pricing module)
                let cost_usd = 0.0; // TODO: integrate with pricing module

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

            _ => {
                // Other events not stored in lifestats
            }
        }

        Ok(())
    }
}

#[async_trait]
impl EventProcessor for LifestatsProcessor {
    fn name(&self) -> &'static str {
        "lifestats"
    }

    async fn process(&self, event: ProxyEvent, ctx: &ProcessContext) -> ProcessResult {
        // Send to background writer (non-blocking)
        // If channel is full, we drop the event (backpressure)
        if let Err(e) = self
            .tx
            .try_send(StorageCommand::Store(event.clone(), ctx.clone()))
        {
            tracing::trace!("Lifestats buffer full, dropping event: {}", e);
        }

        // Always pass through (side-effect only processor)
        ProcessResult::Continue(event)
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        // Signal background writer to stop
        let _ = self.tx.send(StorageCommand::Shutdown).await;
        Ok(())
    }
}
```

### 3. Lifestats Query Module

**Location:** `src/pipeline/lifestats_query.rs`

```rust
//! Query interface for lifestats database
//!
//! Provides structured queries for context recovery, used by MCP tools.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Query result for thinking block searches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingMatch {
    pub session_id: Option<String>,
    pub timestamp: String,
    pub content: String,
    pub tokens: Option<u32>,
    /// Relevance rank from FTS (lower = more relevant)
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

/// Query result for tool call history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub id: String,
    pub session_id: Option<String>,
    pub timestamp: String,
    pub tool_name: String,
    pub input_summary: Option<String>,
    pub duration_ms: Option<i64>,
    pub success: Option<bool>,
}

/// Session summary for overview queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub user_id: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub total_tokens: i64,
    pub total_cost_usd: f64,
    pub tool_calls: i64,
    pub thinking_blocks: i64,
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
    /// Tokens by model
    pub by_model: Vec<ModelStats>,
    /// Tool usage counts
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

/// Query interface for lifestats database
pub struct LifestatsQuery {
    conn: Connection,
}

impl LifestatsQuery {
    /// Open a connection to the lifestats database
    pub fn open(db_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let conn = Connection::open(db_path)?;
        Ok(Self { conn })
    }

    /// Search thinking blocks by keyword (FTS)
    pub fn search_thinking(
        &self,
        query: &str,
        limit: usize,
        sessions_back: Option<usize>,
    ) -> anyhow::Result<Vec<ThinkingMatch>> {
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

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![query, limit as i64], |row| {
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

    /// Search user prompts by keyword (FTS)
    pub fn search_prompts(
        &self,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<PromptMatch>> {
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

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![query, limit as i64], |row| {
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

    /// Get tool call history with optional filters
    pub fn get_tool_calls(
        &self,
        tool_name: Option<&str>,
        session_id: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<ToolCallRecord>> {
        let mut sql = String::from(
            r#"
            SELECT
                tc.id,
                tc.session_id,
                tc.timestamp,
                tc.tool_name,
                substr(tc.input_json, 1, 200) as input_summary,
                tr.duration_ms,
                tr.success
            FROM tool_calls tc
            LEFT JOIN tool_results tr ON tc.id = tr.call_id
            WHERE 1=1
        "#,
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(tool) = tool_name {
            sql.push_str(" AND tc.tool_name = ?");
            params_vec.push(Box::new(tool.to_string()));
        }

        if let Some(session) = session_id {
            sql.push_str(" AND tc.session_id = ?");
            params_vec.push(Box::new(session.to_string()));
        }

        sql.push_str(" ORDER BY tc.timestamp DESC LIMIT ?");
        params_vec.push(Box::new(limit as i64));

        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(ToolCallRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                timestamp: row.get(2)?,
                tool_name: row.get(3)?,
                input_summary: row.get(4)?,
                duration_ms: row.get(5)?,
                success: row.get::<_, Option<i32>>(6)?.map(|v| v != 0),
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Get session summaries
    pub fn get_sessions(&self, limit: usize) -> anyhow::Result<Vec<SessionSummary>> {
        let sql = r#"
            SELECT
                id,
                user_id,
                started_at,
                ended_at,
                total_tokens,
                total_cost_usd,
                tool_calls,
                thinking_blocks
            FROM sessions
            ORDER BY started_at DESC
            LIMIT ?
        "#;

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(SessionSummary {
                id: row.get(0)?,
                user_id: row.get(1)?,
                started_at: row.get(2)?,
                ended_at: row.get(3)?,
                total_tokens: row.get(4)?,
                total_cost_usd: row.get(5)?,
                tool_calls: row.get(6)?,
                thinking_blocks: row.get(7)?,
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
        // Aggregate stats
        let (total_sessions, total_tokens, total_cost, first_session, last_session): (
            i64,
            i64,
            f64,
            Option<String>,
            Option<String>,
        ) = self.conn.query_row(
            r#"
            SELECT
                COUNT(*) as sessions,
                COALESCE(SUM(total_tokens), 0) as tokens,
                COALESCE(SUM(total_cost_usd), 0) as cost,
                MIN(started_at) as first,
                MAX(started_at) as last
            FROM sessions
        "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )?;

        let total_tool_calls: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM tool_calls", [], |row| row.get(0))?;

        let total_thinking: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM thinking_blocks", [], |row| row.get(0))?;

        // By model
        let mut by_model = Vec::new();
        {
            let mut stmt = self.conn.prepare(
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
            let mut stmt = self.conn.prepare(
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
    /// Searches both thinking blocks and user prompts for a topic,
    /// returning chronologically ordered matches grouped by session.
    pub fn recover_context(
        &self,
        topic: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<ContextMatch>> {
        let mut results = Vec::new();

        // Search thinking blocks
        for m in self.search_thinking(topic, limit)? {
            results.push(ContextMatch {
                match_type: MatchType::Thinking,
                session_id: m.session_id,
                timestamp: m.timestamp,
                content: m.content,
                rank: m.rank,
            });
        }

        // Search user prompts
        for m in self.search_prompts(topic, limit)? {
            results.push(ContextMatch {
                match_type: MatchType::UserPrompt,
                session_id: m.session_id,
                timestamp: m.timestamp,
                content: m.content,
                rank: m.rank,
            });
        }

        // Sort by timestamp descending
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Limit total results
        results.truncate(limit);

        Ok(results)
    }
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
```

---

## MCP Tools Specification

**Location:** `mcp-server/src/lifestats-tools.ts`

```typescript
// MCP tools for lifestats queries
// These tools enable Claude to recover lost context from past sessions

import { z } from "zod";

// Tool: aspy_recall_context
// Primary context recovery tool
export const recallContextTool = {
  name: "aspy_recall_context",
  description: `Search past sessions for context about a topic.

Searches both Claude's thinking blocks and user prompts for relevant matches.
Use this when you need to recover lost context after session compaction.

Example queries:
- "theme color preferences"
- "authentication implementation"
- "solarized" (find discussions about solarized theme)`,

  parameters: z.object({
    query: z.string().describe("Topic or keywords to search for"),
    limit: z.number().default(10).describe("Maximum results to return"),
    include_thinking: z.boolean().default(true).describe("Include Claude's thinking blocks"),
    include_prompts: z.boolean().default(true).describe("Include user prompts"),
  }),

  handler: async ({ query, limit, include_thinking, include_prompts }) => {
    const response = await fetch(
      `${API_BASE}/api/lifestats/recover?` +
        `query=${encodeURIComponent(query)}&` +
        `limit=${limit}&` +
        `thinking=${include_thinking}&` +
        `prompts=${include_prompts}`
    );

    if (!response.ok) {
      throw new Error(`Lifestats query failed: ${response.status}`);
    }

    return await response.json();
  },
};

// Tool: aspy_lifetime_stats
// Overall usage statistics across all sessions
export const lifetimeStatsTool = {
  name: "aspy_lifetime_stats",
  description: `Get lifetime usage statistics across all Claude Code sessions.

Returns:
- Total tokens consumed (by model)
- Total cost incurred
- Tool usage patterns
- Session count and duration`,

  parameters: z.object({}),

  handler: async () => {
    const response = await fetch(`${API_BASE}/api/lifestats/summary`);
    if (!response.ok) {
      throw new Error(`Lifestats query failed: ${response.status}`);
    }
    return await response.json();
  },
};

// Tool: aspy_session_history
// Browse past sessions
export const sessionHistoryTool = {
  name: "aspy_session_history",
  description: `List past Claude Code sessions with summaries.

Use to find a specific session to query for context.`,

  parameters: z.object({
    limit: z.number().default(10).describe("Number of sessions to return"),
    user_id: z.string().optional().describe("Filter by user ID (API key hash)"),
  }),

  handler: async ({ limit, user_id }) => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (user_id) params.set("user_id", user_id);

    const response = await fetch(`${API_BASE}/api/lifestats/sessions?${params}`);
    if (!response.ok) {
      throw new Error(`Lifestats query failed: ${response.status}`);
    }
    return await response.json();
  },
};

// Tool: aspy_search_thinking
// Deep search into Claude's past reasoning
export const searchThinkingTool = {
  name: "aspy_search_thinking",
  description: `Search Claude's past thinking blocks for specific reasoning.

Thinking blocks contain Claude's internal reasoning process. This is useful
for understanding past decisions or recovering lost problem-solving context.

Uses full-text search with relevance ranking.`,

  parameters: z.object({
    query: z.string().describe("Search query (supports FTS5 syntax)"),
    limit: z.number().default(10).describe("Maximum results"),
    session_id: z.string().optional().describe("Limit to specific session"),
  }),

  handler: async ({ query, limit, session_id }) => {
    const params = new URLSearchParams({
      query,
      limit: String(limit),
    });
    if (session_id) params.set("session_id", session_id);

    const response = await fetch(`${API_BASE}/api/lifestats/thinking?${params}`);
    if (!response.ok) {
      throw new Error(`Lifestats query failed: ${response.status}`);
    }
    return await response.json();
  },
};

// Tool: aspy_tool_history
// Analyze past tool usage
export const toolHistoryTool = {
  name: "aspy_tool_history",
  description: `Get history of tool calls across sessions.

Useful for understanding what files were read/edited, what commands were run,
and how long operations took.`,

  parameters: z.object({
    tool_name: z.string().optional().describe("Filter by tool (Read, Edit, Bash, etc.)"),
    limit: z.number().default(20).describe("Maximum results"),
    session_id: z.string().optional().describe("Limit to specific session"),
  }),

  handler: async ({ tool_name, limit, session_id }) => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (tool_name) params.set("tool", tool_name);
    if (session_id) params.set("session_id", session_id);

    const response = await fetch(`${API_BASE}/api/lifestats/tools?${params}`);
    if (!response.ok) {
      throw new Error(`Lifestats query failed: ${response.status}`);
    }
    return await response.json();
  },
};
```

---

## Configuration

**Addition to `config.rs`:**

```rust
/// Lifestats configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lifestats {
    /// Enable lifetime statistics storage
    #[serde(default)]
    pub enabled: bool,

    /// Path to SQLite database (relative to config dir or absolute)
    #[serde(default = "default_db_path")]
    pub db_path: String,

    /// Store thinking blocks (can be large)
    #[serde(default = "default_true")]
    pub store_thinking: bool,

    /// Store full tool inputs/outputs
    #[serde(default = "default_true")]
    pub store_tool_io: bool,

    /// Maximum thinking block size to store (bytes)
    #[serde(default = "default_max_thinking")]
    pub max_thinking_size: usize,

    /// Retention period in days (0 = forever)
    #[serde(default)]
    pub retention_days: u32,
}

fn default_db_path() -> String {
    "./data/lifestats.db".to_string()
}

fn default_max_thinking() -> usize {
    100_000
}

fn default_true() -> bool {
    true
}

impl Default for Lifestats {
    fn default() -> Self {
        Self {
            enabled: false, // Opt-in
            db_path: default_db_path(),
            store_thinking: true,
            store_tool_io: true,
            max_thinking_size: default_max_thinking(),
            retention_days: 90,
        }
    }
}
```

**Example `config.toml`:**

```toml
[lifestats]
enabled = true
db_path = "./data/lifestats.db"
store_thinking = true
store_tool_io = true
max_thinking_size = 100000
retention_days = 90
```

---

## Integration Points

### 1. main.rs Changes

```rust
// After creating event channels, before spawning proxy:

// Create event pipeline
let mut event_pipeline = pipeline::EventPipeline::new();

// Register lifestats processor if enabled
if config.lifestats.enabled {
    let lifestats_config = pipeline::LifestatsConfig {
        db_path: PathBuf::from(&config.lifestats.db_path),
        store_thinking: config.lifestats.store_thinking,
        store_tool_io: config.lifestats.store_tool_io,
        max_thinking_size: config.lifestats.max_thinking_size,
        retention_days: config.lifestats.retention_days,
    };

    match pipeline::LifestatsProcessor::new(lifestats_config) {
        Ok(processor) => {
            event_pipeline.register(processor);
            tracing::info!("Lifestats processor enabled");
        }
        Err(e) => {
            tracing::error!("Failed to initialize lifestats: {}", e);
        }
    }
}

// Pass pipeline to proxy
let shared = proxy::SharedState {
    // ... existing fields ...
    event_pipeline: Arc::new(event_pipeline),
};
```

### 2. proxy/mod.rs Changes

```rust
impl ProxyState {
    async fn send_event(&self, event: ProxyEvent, user_id: Option<&str>) {
        // Build process context
        let ctx = pipeline::ProcessContext {
            session_id: self.current_session_id(),
            user_id: user_id.map(String::from),
            is_demo: false,
        };

        // Run through pipeline
        let event = match self.event_pipeline.process(event, &ctx).await {
            Some(e) => e,
            None => return, // Event was filtered
        };

        // Send to TUI and storage channels
        let _ = self.event_tx_tui.send(event.clone()).await;
        let _ = self.event_tx_storage.send(event.clone()).await;

        // Record to user's session
        if let Some(uid) = user_id {
            if let Ok(mut sessions) = self.sessions.lock() {
                sessions.record_event(&sessions::UserId::new(uid), event);
            }
        }
    }
}
```

### 3. API Endpoints for MCP

**Addition to `proxy/api.rs`:**

```rust
// Lifestats query endpoints

pub async fn lifestats_recover(
    Query(params): Query<RecoverParams>,
) -> Result<Json<Vec<ContextMatch>>, StatusCode> {
    let query = LifestatsQuery::open(&config.lifestats.db_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    query
        .recover_context(&params.query, params.limit.unwrap_or(10))
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn lifestats_summary() -> Result<Json<LifetimeStats>, StatusCode> {
    let query = LifestatsQuery::open(&config.lifestats.db_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    query
        .get_lifetime_stats()
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// Route registration
.route("/api/lifestats/recover", get(lifestats_recover))
.route("/api/lifestats/summary", get(lifestats_summary))
.route("/api/lifestats/sessions", get(lifestats_sessions))
.route("/api/lifestats/thinking", get(lifestats_thinking))
.route("/api/lifestats/tools", get(lifestats_tools))
```

---

## Migration Path

### Phase 1: Foundation (This RFC)
1. Add `pipeline` module with `EventProcessor` trait
2. Implement `LifestatsProcessor` with SQLite storage
3. Add configuration options
4. Wire into `send_event()`

### Phase 2: Query Interface
1. Implement `LifestatsQuery` module
2. Add HTTP API endpoints
3. Extend MCP server with lifestats tools

### Phase 3: Polish
1. Session aggregation (update stats on session end)
2. Retention policy enforcement (background cleanup)
3. Data export/import for backup
4. Query optimization (indices, query planning)

---

## Dependencies

**New Cargo dependencies:**

```toml
[dependencies]
rusqlite = { version = "0.31", features = ["bundled", "fts5"] }
async-trait = "0.1"
```

**Rationale:**
- `rusqlite`: SQLite bindings, `bundled` for easy deployment, `fts5` for full-text search
- `async-trait`: Required for async trait methods in `EventProcessor`

---

## Testing Strategy

### Unit Tests
- SQLite schema initialization
- Query result parsing
- FTS query syntax

### Integration Tests
- Event flow through pipeline
- Background writer task
- API endpoint responses

### Manual Testing
- "Solarized vomit green" recovery test (the benchmark)
- Multi-session context recovery
- Large thinking block handling

---

## Open Questions

1. **User prompt extraction**: How do we reliably identify user prompts in the event stream? The parser sees requests, but extracting the actual user message requires understanding the message format.

2. **Session boundaries**: Should we create sessions implicitly from first event, or require explicit SessionStart hooks?

3. **Cost calculation**: Should lifestats calculate costs, or just store token counts and let queries calculate?

4. **Privacy**: Should there be a config option to hash/redact sensitive content before storage?

---

## Appendix: File Structure

```
src/
├── pipeline/
│   ├── mod.rs              # EventPipeline trait and core types
│   ├── lifestats.rs        # LifestatsProcessor implementation
│   └── lifestats_query.rs  # Query interface for SQLite
├── proxy/
│   ├── api.rs              # Add lifestats endpoints
│   └── mod.rs              # Integrate pipeline into send_event
└── config.rs               # Add Lifestats config section

mcp-server/
└── src/
    ├── index.ts            # Register new tools
    └── lifestats-tools.ts  # Tool implementations

data/
└── lifestats.db            # SQLite database (created at runtime)
```
