// Storage module - handles writing events to disk in JSON Lines format
//
// JSON Lines (JSONL) format writes one JSON object per line, making it easy to:
// - Stream process large files
// - Grep/search with standard tools
// - Parse with jq or other JSON tools
//
// Each session gets its own log file: anthropic-spy-YYYYMMDD-HHMMSS-XXXX.jsonl
// Example: jq '.tool_name' logs/anthropic-spy-20251127-143022-a7b3.jsonl

use crate::events::ProxyEvent;
use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use tokio::sync::mpsc;

/// Handles writing events to JSON Lines files
pub struct Storage {
    log_dir: PathBuf,
    session_id: String,
    event_rx: mpsc::Receiver<ProxyEvent>,
}

impl Storage {
    /// Create a new storage handler
    /// Each session gets its own log file based on session_id
    pub fn new(
        log_dir: PathBuf,
        session_id: String,
        event_rx: mpsc::Receiver<ProxyEvent>,
    ) -> Result<Self> {
        // Create the log directory if it doesn't exist
        fs::create_dir_all(&log_dir).context("Failed to create log directory")?;

        Ok(Self {
            log_dir,
            session_id,
            event_rx,
        })
    }

    /// Get the path to this session's log file
    /// Format: anthropic-spy-YYYYMMDD-HHMMSS-XXXX.jsonl
    fn log_file_path(&self) -> PathBuf {
        self.log_dir
            .join(format!("anthropic-spy-{}.jsonl", self.session_id))
    }

    /// Run the storage loop, writing events to disk as they arrive
    ///
    /// This runs in its own async task and continues until the channel is closed.
    /// In Rust, this pattern of "run until channel closes" is idiomatic for
    /// worker tasks that process a stream of events.
    pub async fn run(mut self) -> Result<()> {
        tracing::info!("Storage started, session log: {:?}", self.log_file_path());

        while let Some(event) = self.event_rx.recv().await {
            if let Err(e) = self.write_event(&event) {
                tracing::error!("Failed to write event: {:?}", e);
                // Continue processing even if one write fails
            }
        }

        tracing::info!("Storage shutting down");
        Ok(())
    }

    /// Write a single event to the log file
    fn write_event(&self, event: &ProxyEvent) -> Result<()> {
        let log_path = self.log_file_path();

        // Open file in append mode, create if it doesn't exist
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .context("Failed to open log file")?;

        // Serialize the event to JSON and write with newline
        let json = serde_json::to_string(event).context("Failed to serialize event")?;

        writeln!(file, "{}", json).context("Failed to write to log file")?;

        // Flush immediately so logs are visible even if process crashes
        file.flush().context("Failed to flush log file")?;

        Ok(())
    }
}
