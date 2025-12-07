//! Query interface for Cortex database
//!
//! Provides structured queries for context recovery, used by MCP tools.
//! Uses connection pooling for efficient concurrent access.
//!
//! # Architecture
//!
//! ```text
//! MCP Tools / HTTP API
//!         │
//!         └──→ CortexQuery (r2d2 pool)
//!                 │
//!                 ├──→ SQLite Reader Connection 1
//!                 ├──→ SQLite Reader Connection 2
//!                 └──→ SQLite Reader Connection N (max 4)
//!                         │
//!                         └──→ FTS5 Queries (BM25 ranking)
//! ```
//!
//! # WAL Mode Concurrency
//!
//! The cortex database uses WAL (Write-Ahead Logging) mode, which allows
//! multiple concurrent readers while the writer thread is active. The connection
//! pool manages up to 4 read-only connections for query parallelism.
//!
//! # Module Organization
//!
//! - `types` - Data types (DTOs) for query results and configuration
//! - `fts` - FTS5 full-text search methods (global and user-scoped)
//! - `stats` - Lifetime statistics aggregation
//! - `semantic` - Vector similarity search using embeddings
//! - `hybrid` - Reciprocal Rank Fusion combining FTS + vector search
//! - `sessions` - Session history and lookup queries

mod fts;
mod hybrid;
mod semantic;
mod sessions;
mod stats;
mod types;

// Re-export all public types for HTTP API serialization
#[allow(unused_imports)] // Used by REST API JSON serialization, not direct Rust imports
pub use types::{
    ContextMatch, EmbeddingStats, LifetimeStats, MatchType, ModelStats, PromptMatch, ResponseMatch,
    SearchMode, ThinkingMatch, TodoMatch, ToolStats,
};

use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use std::path::Path;

/// Query interface for cortex database
///
/// Uses connection pooling for efficient concurrent access.
///
/// # Example
///
/// ```rust,no_run
/// use aspy::pipeline::cortex_query::{CortexQuery, SearchMode};
///
/// # fn main() -> anyhow::Result<()> {
/// let query = CortexQuery::new("./data/cortex.db")?;
///
/// // Search thinking blocks
/// let results = query.search_thinking("solarized theme", 10, SearchMode::Phrase)?;
/// for m in results {
///     println!("[{}] {}", m.timestamp, m.content);
/// }
/// # Ok(())
/// # }
/// ```
pub struct CortexQuery {
    pool: Pool<SqliteConnectionManager>,
}

impl CortexQuery {
    /// Create a new query interface with connection pool
    ///
    /// # Arguments
    /// * `db_path` - Path to the SQLite database file
    ///
    /// # Errors
    /// Returns an error if the database cannot be opened or if a test
    /// connection cannot be established.
    pub fn new(db_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let manager = SqliteConnectionManager::file(db_path);
        let pool = Pool::builder()
            .max_size(4) // Read-only pool for concurrent queries
            .build(manager)?;

        // Verify connection works
        let conn = pool.get()?;
        conn.query_row("SELECT 1", [], |row| row.get::<_, i32>(0))?;

        Ok(Self { pool })
    }

    /// Get a connection from the pool
    fn conn(&self) -> anyhow::Result<PooledConnection<SqliteConnectionManager>> {
        Ok(self.pool.get()?)
    }
}
