---
layout: default
title: OpenTelemetry Guide
nav_order: 12
description: "Export Aspy telemetry to Azure Application Insights and other OTel backends"
---

# OpenTelemetry Guide

Export Aspy telemetry data to OpenTelemetry-compatible backends for enterprise observability, monitoring, and alerting.

## Quick Start

Add to your `~/.config/aspy/config.toml`:

```toml
[otel]
enabled = true
connection_string = "InstrumentationKey=xxx;IngestionEndpoint=https://..."
```

Or via environment variable:

```bash
export ASPY_OTEL_CONNECTION_STRING="InstrumentationKey=xxx;IngestionEndpoint=https://..."
```

## Azure Application Insights Setup

### 1. Create Application Insights Resource

1. Go to the Azure Portal
2. Create a new **Application Insights** resource
3. Choose a name (e.g., "aspy-telemetry") and region
4. After creation, go to the resource **Overview** page
5. Copy the **Connection String** (not the Instrumentation Key alone)

### 2. Configure Aspy

```toml
[otel]
enabled = true
connection_string = "InstrumentationKey=xxx;IngestionEndpoint=https://eastus-8.in.applicationinsights.azure.com/;..."
service_name = "aspy"           # Appears in Application Map
service_version = "0.2.0"       # Defaults to crate version
```

### 3. Verify in Azure Portal

After running Aspy with OTel enabled:

1. Go to **Application Insights** → **Transaction search**
2. Filter by operation name (e.g., `api.request`, `tool.Read`)
3. View traces and their attributes

## What Gets Exported

Aspy exports these events as OpenTelemetry spans:

### API Operations

| Event | Span Name | Key Attributes |
|-------|-----------|----------------|
| Request | `api.request` | `http.method`, `http.url`, `http.request.body.size` |
| Response | `api.response` | `http.status_code`, `http.ttfb_ms`, `http.duration_ms` |
| Error | `api.error` | `error.message`, `error.context` |

### Tool Calls

| Event | Span Name | Key Attributes |
|-------|-----------|----------------|
| ToolCall | `tool.<name>` | `tool.id`, `tool.name`, `tool.input.size` |
| ToolResult | `tool.<name>.result` | `tool.duration_ms`, `tool.success` |

### Token Usage

| Event | Span Name | Key Attributes |
|-------|-----------|----------------|
| ApiUsage | `api.usage` | `model`, `tokens.input`, `tokens.output`, `tokens.cache_*` |

### Pipeline Events

| Event | Span Name | Key Attributes |
|-------|-----------|----------------|
| RequestTransformed | `transform.request` | `transformer`, `tokens.before`, `tokens.after`, `tokens.delta` |
| ResponseAugmented | `augment.response` | `augmenter`, `tokens.injected` |
| ContextCompact | `context.compact` | `context.previous`, `context.new`, `context.reduction` |

### Common Attributes

All spans include:
- `session.id` - The Aspy session key (when available)
- `service.name` - "aspy" (configurable)
- `service.version` - The crate version

## Architecture

The OTel exporter uses a dedicated thread to avoid blocking the async runtime:

```
EventPipeline (sync)
    │
    └──→ OtelProcessor.process()
            │
            └──→ mpsc::SyncSender (bounded, 1000 events)
                    │
                    └──→ Dedicated Exporter Thread
                            │
                            └──→ Batch Exporter → Azure
```

**Backpressure handling:** If the channel fills up (1000 events), additional events are dropped silently. Telemetry is best-effort—it shouldn't impact proxy performance.

## Configuration Reference

```toml
[otel]
# Enable OpenTelemetry export (default: false)
enabled = true

# Azure Application Insights connection string (required)
# Format: InstrumentationKey=xxx;IngestionEndpoint=https://...
connection_string = "InstrumentationKey=..."

# Service name for telemetry (default: "aspy")
service_name = "aspy"

# Service version (default: crate version)
service_version = "0.2.0"
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `ASPY_OTEL_CONNECTION_STRING` | Azure connection string (overrides config) |

## Azure Workbook

Aspy includes an Azure Workbook template for visualizing telemetry. The workbook provides:

- **Token Usage Dashboard** — Input/output/cached tokens over time
- **Tool Call Analytics** — Most used tools, durations, success rates
- **Cost Tracking** — Estimated costs by model
- **Error Analysis** — Error rates and patterns

To import:

1. Go to **Azure Workbooks** in the Azure Portal
2. Click **New** → **Import**
3. Upload the workbook JSON from `examples/azure-workbook.json`
4. Select your Application Insights resource

## KQL Queries

Query your telemetry data with KQL:

### Token Usage by Model

```kql
traces
| where operation_Name == "api.usage"
| extend model = tostring(customDimensions.model)
| extend input_tokens = toint(customDimensions["tokens.input"])
| extend output_tokens = toint(customDimensions["tokens.output"])
| summarize
    total_input = sum(input_tokens),
    total_output = sum(output_tokens)
    by model, bin(timestamp, 1h)
| order by timestamp desc
```

### Tool Call Durations

```kql
traces
| where operation_Name startswith "tool." and operation_Name endswith ".result"
| extend tool_name = tostring(customDimensions["tool.name"])
| extend duration_ms = toint(customDimensions["tool.duration_ms"])
| summarize
    avg_duration = avg(duration_ms),
    p95_duration = percentile(duration_ms, 95),
    count = count()
    by tool_name
| order by count desc
```

### Error Rate

```kql
traces
| where operation_Name == "api.error"
| summarize errors = count() by bin(timestamp, 1h)
| join kind=leftouter (
    traces
    | where operation_Name == "api.request"
    | summarize requests = count() by bin(timestamp, 1h)
) on timestamp
| extend error_rate = todouble(errors) / todouble(requests) * 100
| project timestamp, errors, requests, error_rate
```

## Troubleshooting

### No Data in Application Insights

1. **Check connection string** — Ensure it includes both `InstrumentationKey` and `IngestionEndpoint`
2. **Verify enabled** — Check startup output for "OTel processor initialized"
3. **Wait for batch** — Spans are batched; wait ~30 seconds for first data
4. **Check logs** — Set `RUST_LOG=aspy::pipeline::otel=debug` for detailed logs

### High Memory Usage

The OTel exporter has a bounded channel (1000 events). If you see memory growth:

1. Events may be backing up — check for slow network to Azure
2. Consider reducing batch size or increasing flush interval

### Spans Not Correlating

Aspy creates independent spans (not a trace hierarchy). Each event is a separate span with `session.id` for correlation. Use session ID to group related spans in queries.

## Limitations

- **No trace hierarchy** — Events are independent spans, not parent-child traces
- **Best-effort delivery** — Backpressure drops events rather than blocking
- **Azure-optimized** — Currently uses `opentelemetry-application-insights` crate
- **No metrics/logs** — Only traces (spans) are exported

## Future Enhancements

Planned improvements:
- **Trace correlation** — Link request → tool → response in parent-child hierarchy
- **Metrics export** — Prometheus-compatible metrics
- **Generic OTLP** — Support for any OTLP endpoint (Jaeger, Tempo, etc.)
