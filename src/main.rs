// Anthropic Spy - Observability Proxy for Claude Code
//
// This tool acts as an HTTP proxy between Claude Code and the Anthropic API,
// logging all tool calls and responses for analysis and debugging.
//
// Architecture:
// - Proxy server (axum): Intercepts HTTP traffic and forwards to Anthropic
// - Parser: Extracts tool calls from API request/response bodies
// - TUI (ratatui): Displays live tool calls in a terminal interface
// - Storage: Writes events to JSON Lines files for later analysis
// - Event system: mpsc channels connect all components

mod cli;
mod config;
mod demo;
mod events;
mod logging;
mod parser;
mod pipeline;
mod pricing;
mod proxy;
mod startup;
mod storage;
mod theme;
mod tui;

use anyhow::Result;
use chrono::Utc;
use config::Config;
use logging::{LogBuffer, TuiLogLayer};
use std::sync::{Arc, Mutex};
use storage::Storage;
use tokio::sync::mpsc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Shared buffer for streaming thinking content
/// The proxy writes to this as thinking_delta events arrive,
/// and the TUI reads from it each render frame for real-time display
/// Uses std::sync::Mutex for sync access in render loop
pub type StreamingThinking = Arc<Mutex<String>>;

/// Shared context state for interceptor injection
/// Parser updates this when ApiUsage arrives, interceptor reads when processing requests
#[derive(Debug, Default)]
pub struct ContextState {
    /// Current context size (input + cache_creation + cache_read tokens from last API call)
    pub current_tokens: u64,
    /// Configured context limit
    pub limit: u64,
    /// Last threshold percentage we warned at (80, 85, 90, 95) to avoid spam
    pub last_warned_threshold: Option<u8>,
}

impl ContextState {
    pub fn new(limit: u64) -> Self {
        Self {
            current_tokens: 0,
            limit,
            last_warned_threshold: None,
        }
    }

    /// Get context usage as percentage (0-100)
    pub fn usage_percent(&self) -> f64 {
        if self.limit == 0 {
            return 0.0;
        }
        (self.current_tokens as f64 / self.limit as f64) * 100.0
    }

    /// Check if we should warn at current level with configurable thresholds
    /// Returns Some(threshold) if we should warn, None if already warned or below thresholds
    pub fn should_warn_at(&self, thresholds: &[u8]) -> Option<u8> {
        let percent = self.usage_percent() as u8;

        // Find the highest threshold we've crossed
        // Thresholds should be sorted ascending: [60, 80, 85, 90, 95]
        let threshold = thresholds
            .iter()
            .rev() // Check highest first
            .find(|&&t| percent >= t)?;

        // Check if we already warned at this level or higher
        match self.last_warned_threshold {
            Some(last) if last >= *threshold => None, // Already warned
            _ => Some(*threshold),
        }
    }

    /// Check if we should warn at current level (default thresholds)
    /// Returns Some(threshold) if we should warn, None if already warned at this level
    pub fn should_warn(&self) -> Option<u8> {
        self.should_warn_at(&[60, 80, 85, 90, 95])
    }

    /// Update context tokens (called by parser on ApiUsage)
    ///
    /// Context size = input + cache_creation + cache_read because:
    /// - `input_tokens`: Fresh tokens (not cached, may include cache_creation subset)
    /// - `cache_creation_tokens`: Tokens being written to cache for future reads
    /// - `cache_read_tokens`: Tokens read from existing cache
    ///
    /// When cache is being created (first request after context grows), cache_creation
    /// is high and cache_read is low. Including cache_creation prevents false "green"
    /// status during cache warmup.
    pub fn update(
        &mut self,
        input_tokens: u64,
        cache_creation_tokens: u64,
        cache_read_tokens: u64,
    ) {
        self.current_tokens = input_tokens + cache_creation_tokens + cache_read_tokens;
    }

    /// Record that we warned at a threshold
    pub fn mark_warned(&mut self, threshold: u8) {
        self.last_warned_threshold = Some(threshold);
    }

    /// Reset warning state (called on context compact)
    pub fn reset_warnings(&mut self) {
        self.last_warned_threshold = None;
    }
}

/// Shared context state wrapped for thread-safe access
pub type SharedContextState = Arc<Mutex<ContextState>>;

/// Generate a unique session ID for log file naming
/// Format: YYYYMMDD-HHMMSS-XXXX (timestamp + 4 random hex chars)
fn generate_session_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    // Use RandomState to get a random value without adding a dependency
    let random = RandomState::new().build_hasher().finish();
    let short_hash = format!("{:04x}", random & 0xFFFF);

    format!("{}-{}", timestamp, short_hash)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Handle CLI commands first (config --show, --reset, --edit, --update)
    // If a command was handled, exit early
    if cli::handle_cli() {
        return Ok(());
    }

    // Ensure config template exists (helps users discover options)
    Config::ensure_config_exists();

    // Extract bundled themes on first run
    theme::ensure_themes_extracted();

    // Load configuration first to determine TUI vs headless mode
    let config = Config::from_env();

    // Create log buffer for TUI mode
    let log_buffer = LogBuffer::new();

    // Initialize tracing/logging with conditional output
    // In TUI mode: capture logs to buffer (prevents garbling the display)
    // In headless mode: output logs to stdout
    //
    // Precedence: RUST_LOG env var > config file > default "info"
    let default_filter = format!(
        "anthropic_spy={},tower_http=debug,axum=debug",
        config.logging.level
    );

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| default_filter.into());

    if config.enable_tui {
        // TUI mode: use custom layer that captures to buffer
        tracing_subscriber::registry()
            .with(filter)
            .with(TuiLogLayer::new(log_buffer.clone()))
            .init();
    } else {
        // Headless mode: use standard fmt layer for stdout
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    // Generate session ID for this run
    let session_id = generate_session_id();

    // Print startup banner (before TUI takes over screen)
    startup::print_startup(&config);
    startup::log_startup(&config);
    tracing::debug!("Session ID: {}", session_id);

    // Create event channels
    // We use bounded channels with a buffer size of 1000 events
    // If the buffer fills up, senders will wait (backpressure)
    // We create two separate channels: one for TUI, one for storage
    let (event_tx_tui, event_rx_tui) = mpsc::channel(1000);
    let (event_tx_storage, event_rx_storage) = mpsc::channel(1000);

    // Create shutdown channel for graceful proxy shutdown
    // This is a oneshot channel - it can only send one signal
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // Create shared buffer for streaming thinking content
    // Proxy writes thinking_delta content here, TUI reads it for real-time display
    let streaming_thinking: StreamingThinking = Arc::new(Mutex::new(String::new()));

    // Create shared context state for interceptor injection
    // Parser updates this on ApiUsage, interceptor reads to decide injection
    let context_state: SharedContextState =
        Arc::new(Mutex::new(ContextState::new(config.context_limit)));

    // Create shared statistics for HTTP API endpoints
    // TUI updates this as events arrive, API reads it for queries
    let shared_stats = Arc::new(Mutex::new(events::Stats {
        configured_context_limit: config.context_limit,
        ..Default::default()
    }));

    // Create shared events buffer for HTTP API endpoints
    // TUI syncs events here, API reads them for queries
    let shared_events = Arc::new(Mutex::new(proxy::api::EventBuffer::new()));

    // Create session manager for multi-user tracking
    // Tracks sessions by API key hash, manages session lifecycle
    let shared_sessions = Arc::new(Mutex::new(proxy::sessions::SessionManager::default()));

    // Spawn the storage task (if enabled)
    // This runs in the background, writing events to disk
    let storage_handle = if config.features.storage {
        let storage_config = config.clone();
        let storage_session_id = session_id.clone();
        Some(tokio::spawn(async move {
            let storage =
                Storage::new(storage_config.log_dir, storage_session_id, event_rx_storage)
                    .expect("Failed to create storage");
            storage.run().await
        }))
    } else {
        // Drop the receiver so senders don't block
        drop(event_rx_storage);
        None
    };

    // Spawn the proxy server task (or demo task in demo mode)
    // This runs in the background, handling HTTP requests
    let proxy_config = config.clone();
    let proxy_streaming_thinking = streaming_thinking.clone();

    // Pipeline reference for shutdown (only used in non-demo mode)
    let pipeline_for_shutdown: Option<std::sync::Arc<pipeline::EventPipeline>>;

    let proxy_handle = if config.demo_mode {
        // Demo mode: generate mock events instead of running real proxy
        // Drop storage sender since demo doesn't use it
        drop(event_tx_storage);
        tracing::info!("Running in DEMO MODE - generating mock events");
        pipeline_for_shutdown = None;
        tokio::spawn(async move {
            demo::run_demo(event_tx_tui, shutdown_rx, proxy_streaming_thinking).await;
        })
    } else {
        // Initialize event processing pipeline and query interface
        let (pipeline, lifestats_query) = if config.lifestats.enabled {
            use pipeline::{lifestats::LifestatsProcessor, lifestats_query::LifestatsQuery, EventPipeline};

            let mut pipeline = EventPipeline::new();

            // Create lifestats config from main config
            let lifestats_config = pipeline::lifestats::LifestatsConfig {
                db_path: config.lifestats.db_path.clone(),
                store_thinking: config.lifestats.store_thinking,
                store_tool_io: config.lifestats.store_tool_io,
                max_thinking_size: config.lifestats.max_thinking_size,
                retention_days: config.lifestats.retention_days,
                channel_buffer: config.lifestats.channel_buffer,
                batch_size: config.lifestats.batch_size,
                flush_interval: std::time::Duration::from_secs(
                    config.lifestats.flush_interval_secs,
                ),
            };

            match LifestatsProcessor::new(lifestats_config) {
                Ok(processor) => {
                    pipeline.register(processor);

                    // Initialize query interface (read-only connection pool)
                    match LifestatsQuery::new(&config.lifestats.db_path) {
                        Ok(query) => {
                            tracing::info!(
                                "Lifestats initialized (SQLite: {})",
                                config.lifestats.db_path.display()
                            );
                            (Some(std::sync::Arc::new(pipeline)), Some(std::sync::Arc::new(query)))
                        }
                        Err(e) => {
                            tracing::error!("Failed to initialize lifestats query interface: {}", e);
                            (Some(std::sync::Arc::new(pipeline)), None)
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to initialize lifestats processor: {}", e);
                    (None, None)
                }
            }
        } else {
            tracing::debug!("Lifestats processor disabled in config");
            (None, None)
        };

        // Bundle channels and shared state for the proxy
        let channels = proxy::EventChannels {
            tui: event_tx_tui,
            storage: event_tx_storage,
        };

        // Clone pipeline Arc before moving into shared (needed for shutdown)
        pipeline_for_shutdown = pipeline.clone();

        let shared = proxy::SharedState {
            stats: shared_stats.clone(),
            events: shared_events.clone(),
            sessions: shared_sessions.clone(),
            context: context_state.clone(),
            streaming_thinking: proxy_streaming_thinking,
            pipeline,
            lifestats_query,
        };
        tokio::spawn(async move {
            proxy::start_proxy(proxy_config, channels, shutdown_rx, shared)
                .await
                .expect("Proxy server failed");
        })
    };

    // Run the TUI in the main task
    // This blocks until the user quits (presses 'q')
    if config.enable_tui {
        tracing::info!("Starting TUI");
        if let Err(e) = tui::run_tui(
            event_rx_tui,
            log_buffer,
            config,
            streaming_thinking,
            shared_stats,
            shared_events,
        )
        .await
        {
            tracing::error!("TUI error: {:?}", e);
        }
    } else {
        tracing::info!("TUI disabled, running in headless mode");
        // In headless mode, just wait for Ctrl+C
        tokio::signal::ctrl_c().await?;
    }

    tracing::info!("Shutting down...");

    // Shutdown event pipeline explicitly (ensures batch flush before exit)
    // This must happen BEFORE signaling the proxy, so events in flight can be processed
    if let Some(pipeline) = pipeline_for_shutdown {
        tracing::debug!("Shutting down event pipeline...");
        if let Err(e) = pipeline.shutdown() {
            tracing::error!("Pipeline shutdown error: {}", e);
            // Continue shutdown despite pipeline error - other components need cleanup
        } else {
            tracing::debug!("Event pipeline shutdown complete");
        }
    }

    // Signal the proxy to shut down gracefully
    // If the send fails, the proxy has already shut down (which is fine)
    let _ = shutdown_tx.send(());

    // Wait for background tasks to finish
    // The channels will be automatically dropped when the proxy task completes
    let _ = proxy_handle.await;
    if let Some(handle) = storage_handle {
        let _ = handle.await;
    }

    tracing::info!("Shutdown complete");
    Ok(())
}
