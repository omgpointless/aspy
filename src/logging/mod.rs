// Logging module - In-memory log capture for TUI display
//
// This module provides a custom tracing layer that captures log events
// in memory and forwards them to the TUI for display. This prevents logs
// from breaking through the TUI's alternate screen buffer and garbling the display.

use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{Level, Metadata, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// Maximum number of log entries to keep in memory
const MAX_LOG_ENTRIES: usize = 1000;

/// A single log entry captured from tracing
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    /// The tracing target (module path) - stored for future filtering support
    #[allow(dead_code)]
    pub target: String,
    pub message: String,
}

/// Log level for display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<&Level> for LogLevel {
    fn from(level: &Level) -> Self {
        match *level {
            Level::ERROR => LogLevel::Error,
            Level::WARN => LogLevel::Warn,
            Level::INFO => LogLevel::Info,
            Level::DEBUG => LogLevel::Debug,
            Level::TRACE => LogLevel::Trace,
        }
    }
}

impl LogLevel {
    /// Get the display string for this log level
    pub fn as_str(&self) -> &str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
            LogLevel::Trace => "TRACE",
        }
    }
}

/// In-memory log buffer with bounded size (ring buffer)
#[derive(Clone)]
pub struct LogBuffer {
    entries: Arc<Mutex<VecDeque<LogEntry>>>,
}

impl LogBuffer {
    /// Create a new log buffer
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(VecDeque::with_capacity(MAX_LOG_ENTRIES))),
        }
    }

    /// Add a log entry to the buffer
    /// If the buffer is full, removes the oldest entry
    pub fn add(&self, entry: LogEntry) {
        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= MAX_LOG_ENTRIES {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    /// Get all log entries (most recent last)
    #[allow(dead_code)]
    pub fn get_all(&self) -> Vec<LogEntry> {
        self.entries.lock().unwrap().iter().cloned().collect()
    }

    /// Clear all log entries
    #[allow(dead_code)]
    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Custom tracing layer that captures logs to a buffer
pub struct TuiLogLayer {
    buffer: LogBuffer,
    sender: Option<mpsc::Sender<LogEntry>>,
}

impl TuiLogLayer {
    /// Create a new TUI log layer with a log buffer
    pub fn new(buffer: LogBuffer) -> Self {
        Self {
            buffer,
            sender: None,
        }
    }

    /// Create a new TUI log layer with both buffer and channel sender
    #[allow(dead_code)]
    pub fn with_sender(buffer: LogBuffer, sender: mpsc::Sender<LogEntry>) -> Self {
        Self {
            buffer,
            sender: Some(sender),
        }
    }
}

impl<S> Layer<S> for TuiLogLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        // Extract log information
        let metadata = event.metadata();
        let level = LogLevel::from(metadata.level());
        let target = metadata.target().to_string();

        // Extract the message using a visitor
        let mut message = String::new();
        let mut visitor = MessageVisitor(&mut message);
        event.record(&mut visitor);

        let entry = LogEntry {
            timestamp: Utc::now(),
            level,
            target,
            message,
        };

        // Store in buffer
        self.buffer.add(entry.clone());

        // Send to channel if available
        if let Some(sender) = &self.sender {
            // Use try_send to avoid blocking if the channel is full
            let _ = sender.try_send(entry);
        }
    }

    fn enabled(&self, _metadata: &Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        // Enable all log levels - filtering happens at subscriber level
        true
    }
}

/// Visitor to extract the message from a tracing event
struct MessageVisitor<'a>(&'a mut String);

impl<'a> tracing::field::Visit for MessageVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            *self.0 = format!("{:?}", value);
            // Remove the quotes that Debug adds
            if self.0.starts_with('"') && self.0.ends_with('"') {
                *self.0 = self.0[1..self.0.len() - 1].to_string();
            }
        }
    }
}
