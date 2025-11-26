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

mod config;
mod demo;
mod events;
mod logging;
mod parser;
mod pricing;
mod proxy;
mod storage;
mod tui;

use anyhow::Result;
use config::Config;
use logging::{LogBuffer, TuiLogLayer};
use storage::Storage;
use tokio::sync::mpsc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration first to determine TUI vs headless mode
    let config = Config::from_env();

    // Create log buffer for TUI mode
    let log_buffer = LogBuffer::new();

    // Initialize tracing/logging with conditional output
    // In TUI mode: capture logs to buffer (prevents garbling the display)
    // In headless mode: output logs to stdout
    if config.enable_tui {
        // TUI mode: use custom layer that captures to buffer
        tracing_subscriber::registry()
            .with(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "anthropic_spy=info,tower_http=debug,axum=debug".into()),
            )
            .with(TuiLogLayer::new(log_buffer.clone()))
            .init();
    } else {
        // Headless mode: use standard fmt layer for stdout
        tracing_subscriber::registry()
            .with(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "anthropic_spy=debug,tower_http=debug,axum=debug".into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    tracing::info!("Starting Anthropic Spy");
    tracing::info!("Configuration: {:?}", config);

    // Create event channels
    // We use bounded channels with a buffer size of 1000 events
    // If the buffer fills up, senders will wait (backpressure)
    // We create two separate channels: one for TUI, one for storage
    let (event_tx_tui, event_rx_tui) = mpsc::channel(1000);
    let (event_tx_storage, event_rx_storage) = mpsc::channel(1000);

    // Create shutdown channel for graceful proxy shutdown
    // This is a oneshot channel - it can only send one signal
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // Spawn the storage task
    // This runs in the background, writing events to disk
    let storage_config = config.clone();
    let storage_handle = tokio::spawn(async move {
        let storage = Storage::new(storage_config.log_dir, event_rx_storage)
            .expect("Failed to create storage");
        storage.run().await
    });

    // Spawn the proxy server task (or demo task in demo mode)
    // This runs in the background, handling HTTP requests
    // We pass both event senders so the proxy can broadcast to TUI and storage
    // We also pass the shutdown receiver so the proxy can gracefully shut down
    let proxy_config = config.clone();
    let proxy_handle = if config.demo_mode {
        // Demo mode: generate mock events instead of running real proxy
        // Drop storage sender since demo doesn't use it
        drop(event_tx_storage);
        tracing::info!("Running in DEMO MODE - generating mock events");
        tokio::spawn(async move {
            demo::run_demo(event_tx_tui, shutdown_rx).await;
        })
    } else {
        tokio::spawn(async move {
            proxy::start_proxy(proxy_config, event_tx_tui, event_tx_storage, shutdown_rx)
                .await
                .expect("Proxy server failed");
        })
    };

    // Run the TUI in the main task
    // This blocks until the user quits (presses 'q')
    if config.enable_tui {
        tracing::info!("Starting TUI");
        if let Err(e) = tui::run_tui(event_rx_tui, log_buffer).await {
            tracing::error!("TUI error: {:?}", e);
        }
    } else {
        tracing::info!("TUI disabled, running in headless mode");
        // In headless mode, just wait for Ctrl+C
        tokio::signal::ctrl_c().await?;
    }

    tracing::info!("Shutting down...");

    // Signal the proxy to shut down gracefully
    // If the send fails, the proxy has already shut down (which is fine)
    let _ = shutdown_tx.send(());

    // Wait for background tasks to finish
    // The channels will be automatically dropped when the proxy task completes
    let _ = tokio::join!(storage_handle, proxy_handle);

    tracing::info!("Shutdown complete");
    Ok(())
}
