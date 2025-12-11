//! Proxy server setup and initialization

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{routing::any, Router};
use tokio::net::TcpListener;

use crate::config::Config;
use crate::parser::Parser;

use super::api;
use super::augmentation::AugmentationPipeline;
use super::count_tokens;
use super::proxy_handler;
use super::state::{EventChannels, ProxyState, SharedState};
use super::transformation;
use super::translation::TranslationPipeline;

/// Start the proxy server
pub async fn start_proxy(
    config: Config,
    channels: EventChannels,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    shared: SharedState,
) -> Result<()> {
    let bind_addr = config.bind_addr;
    let api_url = config.api_url.clone();

    // Build the HTTP client with timeout and connection pooling
    // NOTE: No default User-Agent set - we forward the original User-Agent from the client.
    // This is critical for Claude Max credentials which require the request to appear
    // as coming from Claude Code (Anthropic validates User-Agent for Claude Max auth).
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300)) // 5 minute timeout for API calls
        .pool_max_idle_per_host(10)
        // Force HTTP/1.1 to avoid HTTP/2 connection reset issues with some providers
        .http1_only()
        .build()
        .context("Failed to create HTTP client")?;

    // Create augmentation pipeline from config (opt-in augmenters)
    let augmentation = Arc::new(AugmentationPipeline::from_config(&config.augmentation));
    if augmentation.is_empty() {
        tracing::debug!("Augmentation pipeline: no augmenters enabled");
    } else {
        tracing::debug!(
            "Augmentation pipeline initialized with: {:?}",
            augmentation.augmenter_names()
        );
    }

    // Create translation pipeline from config (opt-in feature)
    let translation = Arc::new(TranslationPipeline::from_config(&config.translation));
    if translation.is_enabled() {
        tracing::info!("Translation pipeline enabled (OpenAI ↔ Anthropic)");
    } else {
        tracing::debug!("Translation pipeline: disabled");
    }

    // Create transformation pipeline from config (opt-in feature)
    let transformation = Arc::new(transformation::TransformationPipeline::from_config(
        &config.transformers,
    ));
    if !transformation.is_empty() {
        tracing::info!(
            "Transformation pipeline enabled with: {:?}",
            transformation.transformer_names()
        );
    } else {
        tracing::debug!("Transformation pipeline: no transformers enabled");
    }

    // Log client routing config if present
    if config.clients.is_configured() {
        tracing::info!(
            "Client routing enabled: {} client(s), {} provider(s)",
            config.clients.clients.len(),
            config.clients.providers.len()
        );
        for (id, client) in &config.clients.clients {
            tracing::info!(
                "  Client '{}': {} -> provider '{}'",
                id,
                client.name,
                client.provider
            );
        }
    }

    let state = ProxyState {
        client,
        parser: Parser::new(),
        event_tx_tui: channels.tui,
        event_tx_storage: channels.storage,
        api_url,
        streaming_thinking: shared.streaming_thinking,
        context_state: shared.context,
        augmentation,
        stats: shared.stats,
        events: shared.events,
        sessions: shared.sessions,
        log_dir: config.log_dir.clone(),
        clients: config.clients.clone(),
        pipeline: shared.pipeline,
        cortex_query: shared.cortex_query,
        embedding_indexer: shared.embedding_indexer,
        translation,
        transformation,
        transformers_config: config.transformers.clone(),
        count_tokens_cache: count_tokens::CountTokensCache::new_shared(
            count_tokens::CountTokensConfig {
                enabled: config.count_tokens.enabled,
                cache_ttl_seconds: config.count_tokens.cache_ttl_seconds,
                rate_limit_per_second: config.count_tokens.rate_limit_per_second,
            },
        ),
    };

    // Build the router - API endpoints + proxy handler
    let app = Router::new()
        // Stats and events endpoints
        .route("/api/stats", axum::routing::get(api::get_stats))
        .route("/api/events", axum::routing::get(api::get_events))
        .route("/api/context", axum::routing::get(api::get_context))
        .route(
            "/api/context/snapshot",
            axum::routing::get(api::get_context_snapshot),
        )
        // Whoami and session history endpoints
        .route("/api/whoami", axum::routing::get(api::get_whoami))
        .route(
            "/api/session-history",
            axum::routing::get(api::get_session_history),
        )
        // Session management endpoints
        .route("/api/sessions", axum::routing::get(api::get_sessions))
        .route(
            "/api/session/start",
            axum::routing::post(api::session_start),
        )
        .route("/api/session/end", axum::routing::post(api::session_end))
        .route(
            "/api/session/reconnect",
            axum::routing::post(api::session_reconnect),
        )
        // Session todos endpoint
        .route(
            "/api/session/:user_id/todos",
            axum::routing::get(api::get_session_todos),
        )
        // Hook endpoints
        .route(
            "/api/hook/precompact",
            axum::routing::post(api::hook_precompact),
        )
        // Log search endpoint
        .route("/api/search", axum::routing::post(api::search_logs))
        // cortex endpoints
        .route("/api/cortex/health", axum::routing::get(api::cortex_health))
        .route(
            "/api/cortex/cleanup",
            axum::routing::post(api::cortex_cleanup),
        )
        .route(
            "/api/cortex/search/thinking",
            axum::routing::get(api::cortex_search_thinking),
        )
        .route(
            "/api/cortex/search/prompts",
            axum::routing::get(api::cortex_search_prompts),
        )
        .route(
            "/api/cortex/search/responses",
            axum::routing::get(api::cortex_search_responses),
        )
        .route("/api/cortex/todos", axum::routing::get(api::cortex_todos))
        .route(
            "/api/cortex/context",
            axum::routing::get(api::cortex_context),
        )
        .route("/api/cortex/stats", axum::routing::get(api::cortex_stats))
        // User-scoped cortex endpoints
        .route(
            "/api/cortex/search/user/:user_id/thinking",
            axum::routing::get(api::cortex_search_user_thinking),
        )
        .route(
            "/api/cortex/search/user/:user_id/prompts",
            axum::routing::get(api::cortex_search_user_prompts),
        )
        .route(
            "/api/cortex/search/user/:user_id/responses",
            axum::routing::get(api::cortex_search_user_responses),
        )
        .route(
            "/api/cortex/context/user/:user_id",
            axum::routing::get(api::cortex_context_user),
        )
        .route(
            "/api/cortex/stats/user/:user_id",
            axum::routing::get(api::cortex_stats_user),
        )
        // Semantic search / hybrid endpoints
        .route(
            "/api/cortex/embeddings/status",
            axum::routing::get(api::cortex_embedding_status),
        )
        .route(
            "/api/cortex/embeddings/reindex",
            axum::routing::post(api::cortex_embedding_reindex),
        )
        .route(
            "/api/cortex/embeddings/poll",
            axum::routing::post(api::cortex_embedding_poll),
        )
        .route(
            "/api/cortex/context/hybrid/user/:user_id",
            axum::routing::get(api::cortex_context_hybrid_user),
        )
        // Proxy handler (catch-all)
        .route("/*path", any(proxy_handler))
        .with_state(state);

    tracing::info!("Starting proxy on {}", bind_addr);

    // Bind and serve
    let listener = TcpListener::bind(bind_addr)
        .await
        .context("Failed to bind to address")?;

    tracing::info!("Proxy listening on {}", bind_addr);

    // Start serving requests with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_rx.await.ok();
        })
        .await
        .context("Server error")?;

    tracing::info!("Proxy server shut down gracefully");
    Ok(())
}
