// Storage module - handles writing events to disk in JSON Lines format
//
// JSON Lines (JSONL) format writes one JSON object per line, making it easy to:
// - Stream process large files
// - Grep/search with standard tools
// - Parse with jq or other JSON tools
//
// Example: cat logs/2025-01-15.jsonl | jq '.tool_name'

use crate::events::ProxyEvent;
use anyhow::{Context, Result};
use chrono::Utc;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use tokio::sync::mpsc;

/// Handles writing events to JSON Lines files
pub struct Storage {
    log_dir: PathBuf,
    event_rx: mpsc::Receiver<ProxyEvent>,
}

impl Storage {
    /// Create a new storage handler
    pub fn new(log_dir: PathBuf, event_rx: mpsc::Receiver<ProxyEvent>) -> Result<Self> {
        // Create the log directory if it doesn't exist
        fs::create_dir_all(&log_dir).context("Failed to create log directory")?;

        Ok(Self { log_dir, event_rx })
    }

    /// Get the path to today's log file
    fn log_file_path(&self) -> PathBuf {
        let today = Utc::now().format("%Y-%m-%d");
        self.log_dir.join(format!("anthropic-spy-{}.jsonl", today))
    }

    /// Run the storage loop, writing events to disk as they arrive
    ///
    /// This runs in its own async task and continues until the channel is closed.
    /// In Rust, this pattern of "run until channel closes" is idiomatic for
    /// worker tasks that process a stream of events.
    pub async fn run(mut self) -> Result<()> {
        tracing::info!("Storage started, writing to: {:?}", self.log_dir);

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
