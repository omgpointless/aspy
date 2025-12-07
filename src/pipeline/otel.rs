//! OpenTelemetry export processor
//!
//! Exports Aspy events to OpenTelemetry-compatible backends like Azure Application Insights.
//! Uses a dedicated thread to avoid blocking the async runtime.
//!
//! # Architecture
//!
//! ```text
//! EventPipeline (sync)
//!     │
//!     └──→ OtelProcessor.process()
//!             │
//!             └──→ std::sync::mpsc::Sender (bounded)
//!                     │
//!                     └──→ Dedicated Exporter Thread
//!                             │
//!                             └──→ Azure Application Insights
//! ```
//!
//! # Feature Gate
//!
//! This module requires the `otel` feature to be enabled.

use super::{CompletionSignal, EventProcessor, ProcessContext, ProcessResult};
use crate::config::OtelConfig;
use crate::events::ProxyEvent;
use opentelemetry::trace::{Span, SpanKind, Status, Tracer, TracerProvider as _};
use opentelemetry::KeyValue;
use opentelemetry_application_insights::Exporter;
use opentelemetry_sdk::trace::TracerProvider;
use opentelemetry_sdk::Resource;
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Commands sent to the exporter thread
enum ExporterCommand {
    Export(Box<ProxyEvent>, ProcessContext),
    Shutdown,
}

/// OpenTelemetry export processor
///
/// Sends events to Azure Application Insights via a dedicated thread.
pub struct OtelProcessor {
    /// Channel to send events to exporter thread
    tx: SyncSender<ExporterCommand>,
    /// Handle to exporter thread (for join on shutdown)
    _exporter_handle: Option<JoinHandle<()>>,
    /// Completion signal for graceful shutdown
    completion: Arc<CompletionSignal>,
}

impl OtelProcessor {
    /// Create a new OTel processor
    ///
    /// # Arguments
    /// * `config` - OTel configuration including connection string
    ///
    /// # Returns
    /// * `Ok(OtelProcessor)` if initialization succeeds
    /// * `Err` if connection string is missing or exporter fails to initialize
    pub fn new(config: &OtelConfig) -> anyhow::Result<Self> {
        let connection_string = config
            .connection_string
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("OTel connection string required"))?
            .clone();

        let service_name = config.service_name.clone();
        let service_version = config.service_version.clone();

        // Create bounded channel for backpressure
        const CHANNEL_BUFFER: usize = 1000;
        let (tx, rx) = mpsc::sync_channel::<ExporterCommand>(CHANNEL_BUFFER);

        // Completion signal
        let completion = Arc::new(CompletionSignal::new());
        let exporter_completion = completion.clone();

        // Spawn dedicated exporter thread
        let exporter_handle =
            thread::Builder::new()
                .name("otel-exporter".into())
                .spawn(move || {
                    if let Err(e) = Self::exporter_thread(
                        rx,
                        &connection_string,
                        &service_name,
                        &service_version,
                    ) {
                        tracing::error!("OTel exporter thread error: {}", e);
                    }
                    exporter_completion.complete();
                })?;

        tracing::info!("OTel processor initialized (Azure Application Insights)");

        Ok(Self {
            tx,
            _exporter_handle: Some(exporter_handle),
            completion,
        })
    }

    /// Dedicated exporter thread - handles OTel span creation and export
    fn exporter_thread(
        rx: mpsc::Receiver<ExporterCommand>,
        connection_string: &str,
        service_name: &str,
        service_version: &str,
    ) -> anyhow::Result<()> {
        // Create a multi-threaded tokio runtime for async operations
        // The batch exporter spawns background tasks that need a runtime
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1) // Single worker is enough for telemetry
            .enable_all()
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create tokio runtime: {}", e))?;

        // CRITICAL: Enter the runtime context BEFORE creating the batch exporter
        // The batch exporter spawns async tasks immediately during construction
        let _guard = rt.enter();

        // Initialize the Azure Application Insights exporter with async client
        let exporter = Exporter::new_from_connection_string(
            connection_string,
            reqwest::Client::new(), // Async client, not blocking
        )
        .map_err(|e| anyhow::anyhow!("Failed to create Azure exporter: {}", e))?;

        // Build tracer provider with service metadata
        let provider = TracerProvider::builder()
            .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
            .with_resource(Resource::new([
                KeyValue::new("service.name", service_name.to_string()),
                KeyValue::new("service.version", service_version.to_string()),
            ]))
            .build();

        let tracer = provider.tracer("aspy");

        tracing::debug!("OTel exporter thread started");

        // Process events until shutdown
        loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(ExporterCommand::Export(event, ctx)) => {
                    Self::export_event(&tracer, &event, &ctx);
                }
                Ok(ExporterCommand::Shutdown) => {
                    tracing::debug!("OTel exporter received shutdown signal");
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {
                    // No events, continue waiting
                }
                Err(RecvTimeoutError::Disconnected) => {
                    tracing::warn!("OTel exporter channel disconnected");
                    break;
                }
            }
        }

        // Flush remaining spans before shutdown
        tracing::debug!("Flushing OTel spans...");
        if let Err(e) = provider.shutdown() {
            tracing::error!("OTel provider shutdown error: {:?}", e);
        }

        tracing::debug!("OTel exporter thread stopped");
        Ok(())
    }

    /// Export a single event as an OTel span
    fn export_event<T: Tracer>(tracer: &T, event: &ProxyEvent, ctx: &ProcessContext)
    where
        T::Span: Span,
    {
        match event {
            ProxyEvent::Request {
                id,
                method,
                path,
                body_size,
                ..
            } => {
                // Server kind: Claude Code is calling US (the proxy)
                let mut span = tracer
                    .span_builder("api.request")
                    .with_kind(SpanKind::Server)
                    .start(tracer);

                span.set_attribute(KeyValue::new("request.id", id.clone()));
                span.set_attribute(KeyValue::new("http.method", method.clone()));
                span.set_attribute(KeyValue::new("http.url", path.clone()));
                span.set_attribute(KeyValue::new("http.request.body.size", *body_size as i64));

                if let Some(session) = &ctx.session_id {
                    span.set_attribute(KeyValue::new("session.id", session.to_string()));
                }

                span.end();
            }

            ProxyEvent::Response {
                request_id,
                status,
                body_size,
                ttfb,
                duration,
                ..
            } => {
                // Server kind: We are responding TO Claude Code
                let mut span = tracer
                    .span_builder("api.response")
                    .with_kind(SpanKind::Server)
                    .start(tracer);

                span.set_attribute(KeyValue::new("request.id", request_id.clone()));
                span.set_attribute(KeyValue::new("http.status_code", *status as i64));
                span.set_attribute(KeyValue::new("http.response.body.size", *body_size as i64));
                span.set_attribute(KeyValue::new("http.ttfb_ms", ttfb.as_millis() as i64));
                span.set_attribute(KeyValue::new(
                    "http.duration_ms",
                    duration.as_millis() as i64,
                ));

                if let Some(session) = &ctx.session_id {
                    span.set_attribute(KeyValue::new("session.id", session.to_string()));
                }

                if *status >= 400 {
                    span.set_status(Status::error(format!("HTTP {}", status)));
                }

                span.end();
            }

            ProxyEvent::ToolCall {
                id,
                tool_name,
                input,
                ..
            } => {
                let mut span = tracer
                    .span_builder(format!("tool.{}", tool_name))
                    .with_kind(SpanKind::Internal)
                    .start(tracer);

                span.set_attribute(KeyValue::new("tool.id", id.clone()));
                span.set_attribute(KeyValue::new("tool.name", tool_name.clone()));
                span.set_attribute(KeyValue::new(
                    "tool.input.size",
                    input.to_string().len() as i64,
                ));

                if let Some(session) = &ctx.session_id {
                    span.set_attribute(KeyValue::new("session.id", session.to_string()));
                }

                span.end();
            }

            ProxyEvent::ToolResult {
                id,
                tool_name,
                duration,
                success,
                ..
            } => {
                let mut span = tracer
                    .span_builder(format!("tool.{}.result", tool_name))
                    .with_kind(SpanKind::Internal)
                    .start(tracer);

                span.set_attribute(KeyValue::new("tool.id", id.clone()));
                span.set_attribute(KeyValue::new("tool.name", tool_name.clone()));
                span.set_attribute(KeyValue::new(
                    "tool.duration_ms",
                    duration.as_millis() as i64,
                ));
                span.set_attribute(KeyValue::new("tool.success", *success));

                if !success {
                    span.set_status(Status::error("Tool execution failed"));
                }

                span.end();
            }

            ProxyEvent::ApiUsage {
                model,
                input_tokens,
                output_tokens,
                cache_creation_tokens,
                cache_read_tokens,
                ..
            } => {
                let mut span = tracer
                    .span_builder("api.usage")
                    .with_kind(SpanKind::Internal)
                    .start(tracer);

                span.set_attribute(KeyValue::new("model", model.clone()));
                span.set_attribute(KeyValue::new("tokens.input", *input_tokens as i64));
                span.set_attribute(KeyValue::new("tokens.output", *output_tokens as i64));
                span.set_attribute(KeyValue::new(
                    "tokens.cache_creation",
                    *cache_creation_tokens as i64,
                ));
                span.set_attribute(KeyValue::new(
                    "tokens.cache_read",
                    *cache_read_tokens as i64,
                ));
                span.set_attribute(KeyValue::new(
                    "tokens.total",
                    (*input_tokens + *output_tokens) as i64,
                ));

                if let Some(session) = &ctx.session_id {
                    span.set_attribute(KeyValue::new("session.id", session.to_string()));
                }

                span.end();
            }

            ProxyEvent::Error {
                message, context, ..
            } => {
                // Server kind: Error in handling client request
                let mut span = tracer
                    .span_builder("api.error")
                    .with_kind(SpanKind::Server)
                    .start(tracer);

                span.set_attribute(KeyValue::new("error.message", message.clone()));
                if let Some(ctx_str) = context {
                    span.set_attribute(KeyValue::new("error.context", ctx_str.clone()));
                }

                if let Some(session) = &ctx.session_id {
                    span.set_attribute(KeyValue::new("session.id", session.to_string()));
                }

                span.set_status(Status::error(message.clone()));

                span.end();
            }

            ProxyEvent::ContextCompact {
                previous_context,
                new_context,
                ..
            } => {
                let mut span = tracer
                    .span_builder("context.compact")
                    .with_kind(SpanKind::Internal)
                    .start(tracer);

                span.set_attribute(KeyValue::new("context.previous", *previous_context as i64));
                span.set_attribute(KeyValue::new("context.new", *new_context as i64));
                span.set_attribute(KeyValue::new(
                    "context.reduction",
                    (*previous_context - *new_context) as i64,
                ));

                span.end();
            }

            ProxyEvent::RequestTransformed {
                transformer,
                tokens_before,
                tokens_after,
                modifications,
                ..
            } => {
                // Internal: Aspy transforming the request before forwarding
                let mut span = tracer
                    .span_builder("transform.request")
                    .with_kind(SpanKind::Internal)
                    .start(tracer);

                span.set_attribute(KeyValue::new("transformer", transformer.clone()));
                span.set_attribute(KeyValue::new("tokens.before", *tokens_before as i64));
                span.set_attribute(KeyValue::new("tokens.after", *tokens_after as i64));
                span.set_attribute(KeyValue::new(
                    "tokens.delta",
                    (*tokens_after as i64) - (*tokens_before as i64),
                ));
                span.set_attribute(KeyValue::new(
                    "modifications.count",
                    modifications.len() as i64,
                ));

                if let Some(session) = &ctx.session_id {
                    span.set_attribute(KeyValue::new("session.id", session.to_string()));
                }

                span.end();
            }

            ProxyEvent::ResponseAugmented {
                augmenter,
                tokens_injected,
                ..
            } => {
                // Internal: Aspy augmenting the response before returning to client
                let mut span = tracer
                    .span_builder("augment.response")
                    .with_kind(SpanKind::Internal)
                    .start(tracer);

                span.set_attribute(KeyValue::new("augmenter", augmenter.clone()));
                span.set_attribute(KeyValue::new("tokens.injected", *tokens_injected as i64));

                if let Some(session) = &ctx.session_id {
                    span.set_attribute(KeyValue::new("session.id", session.to_string()));
                }

                span.end();
            }

            // Events we don't export (too verbose or not useful for telemetry)
            ProxyEvent::Thinking { .. }
            | ProxyEvent::ThinkingStarted { .. }
            | ProxyEvent::UserPrompt { .. }
            | ProxyEvent::AssistantResponse { .. }
            | ProxyEvent::HeadersCaptured { .. }
            | ProxyEvent::RateLimitUpdate { .. }
            | ProxyEvent::PreCompactHook { .. }
            | ProxyEvent::ContextRecovery { .. }
            | ProxyEvent::TodoSnapshot { .. } => {
                // Skip - these are either too verbose or internal to Aspy
            }
        }
    }
}

impl EventProcessor for OtelProcessor {
    fn name(&self) -> &'static str {
        "otel-exporter"
    }

    fn process(&self, event: &ProxyEvent, ctx: &ProcessContext) -> ProcessResult {
        // Try to send to exporter thread
        match self.tx.try_send(ExporterCommand::Export(
            Box::new(event.clone()),
            ctx.clone(),
        )) {
            Ok(()) => {
                // Successfully queued
            }
            Err(mpsc::TrySendError::Full(_)) => {
                // Backpressure: channel full, drop silently (telemetry is best-effort)
                tracing::trace!("OTel backpressure: dropped event");
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                tracing::warn!("OTel exporter thread disconnected");
            }
        }

        // Always pass through (side-effect only processor)
        ProcessResult::Continue
    }

    fn shutdown(&self) -> anyhow::Result<()> {
        // Signal exporter thread to stop
        let _ = self.tx.send(ExporterCommand::Shutdown);

        // Wait for completion signal (with timeout)
        const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
        if !self.completion.wait(SHUTDOWN_TIMEOUT) {
            tracing::warn!(
                "OTel exporter shutdown timed out after {:?}",
                SHUTDOWN_TIMEOUT
            );
        }

        Ok(())
    }
}
