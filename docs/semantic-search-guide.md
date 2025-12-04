---
layout: default
title: Semantic Search Operations Guide
description: Configure, test, and operate Aspy's embedding system across different providers
---

# Semantic Search Operations Guide

A comprehensive guide to configuring, testing, and operating Aspy's semantic search embedding system across different providers.

## Configuration Overview

All embedding config lives in `~/.config/aspy/config.toml` under `[embeddings]`:

```toml
[embeddings]
provider = "none"           # "none" | "local" | "remote"
model = ""                  # Model name (provider-specific)
api_base = ""               # API endpoint (remote only)
auth_method = "bearer"      # "bearer" | "api-key"
api_key = ""                # Optional: API key (env var takes precedence)
batch_size = 10             # Documents per batch
poll_interval_secs = 30     # How often indexer checks for new content
```

**API Key Configuration:**

The API key can be set via environment variable or config file. Environment variable takes precedence:

```bash
# Recommended: Use dedicated environment variable
export ASPY_EMBEDDINGS_API_KEY="sk-..."
```

Or in config file (less secure, but convenient for testing):
```toml
[embeddings]
api_key = "sk-..."
```

---

## Provider Configurations

### 1. OpenAI API

```toml
[embeddings]
provider = "remote"
model = "text-embedding-3-small"    # or "text-embedding-3-large", "text-embedding-ada-002"
api_base = "https://api.openai.com/v1"
auth_method = "bearer"
batch_size = 20
poll_interval_secs = 30
```

**Environment:**
```bash
export ASPY_EMBEDDINGS_API_KEY="sk-..."
```

**Dimensions:**
- `text-embedding-3-small`: 1536 (default) or specify lower
- `text-embedding-3-large`: 3072 (default)
- `text-embedding-ada-002`: 1536 (legacy)

---

### 2. Azure OpenAI

```toml
[embeddings]
provider = "remote"
model = "text-embedding-3-small"    # Your deployment name
api_base = "https://<resource-name>.openai.azure.com/openai/deployments/<deployment-name>"
auth_method = "api-key"             # Azure uses api-key header, not Bearer
batch_size = 16
poll_interval_secs = 30
```

**Environment:**
```bash
export ASPY_EMBEDDINGS_API_KEY="your-azure-key"
```

**Note:** The `api_base` should include the deployment. Azure's endpoint structure:
```
https://{resource}.openai.azure.com/openai/deployments/{deployment}/embeddings?api-version=2023-05-15
```

The system appends `/embeddings` automatically.

---

### 3. Local Embeddings (MiniLM)

**Requires feature flag at build time:**
```bash
cargo build --release --features local-embeddings
```

```toml
[embeddings]
provider = "local"
model = "all-MiniLM-L6-v2"    # Downloaded automatically on first run
batch_size = 50               # Local can handle larger batches
poll_interval_secs = 10       # Faster since no API latency
```

**No environment variables needed.**

**Dimensions:** 384 (fixed for MiniLM-L6-v2)

**First run:** Model downloads (~25MB) to cache directory. Subsequent runs use cached model.

---

### 4. Other OpenAI-Compatible (Ollama, LM Studio, OpenRouter, etc.)

```toml
[embeddings]
provider = "remote"
model = "nomic-embed-text"          # Model name for your provider
api_base = "http://localhost:11434/v1"  # Ollama example
auth_method = "bearer"              # or "api-key" depending on provider
batch_size = 10
poll_interval_secs = 15
```

**Ollama setup:**
```bash
# Pull an embedding model
ollama pull nomic-embed-text

# Ollama serves OpenAI-compatible API at localhost:11434/v1
```

---

## Testing Protocol

### Step 1: Verify Configuration

```bash
# Check current config
aspy config --show

# Look for [embeddings] section
```

### Step 2: Check Status (Offline)

```bash
# Without proxy running - reads database directly
aspy embeddings --status
```

**Expected output (no embeddings yet):**
```
Embeddings Status (Offline)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Note: Proxy not running. Showing database snapshot.

  Provider:   remote
  Model:      text-embedding-3-small
  Dimensions: 1536

  Index Progress
  ──────────────────────────────────────────────────────────
  Thinking:   0/150 (0.0%)
  Prompts:    0/200 (0.0%)
  Responses:  0/180 (0.0%)
  ──────────────────────────────────────────────────────────
  Total:      0/530 (0.0%)
```

### Step 3: Start Proxy & Watch Indexing

```bash
# Terminal 1: Start aspy
aspy

# Terminal 2: Check live status
aspy embeddings --status
```

**Expected output (live):**
```
Embeddings Status (Live)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Indexer:    RUNNING (connected to running proxy)
  Provider:   remote
  Model:      text-embedding-3-small
  Dimensions: 1536

  Index Progress
  ──────────────────────────────────────────────────────────
  Indexed:    127 documents
  Pending:    403 documents
  Progress:   23.9%
```

### Step 4: Verify via API

```bash
# Direct API check (while proxy running)
curl http://127.0.0.1:8080/api/lifestats/embeddings/status | jq
```

**Expected:**
```json
{
  "enabled": true,
  "running": true,
  "provider": "remote",
  "model": "text-embedding-3-small",
  "dimensions": 1536,
  "documents_indexed": 127,
  "documents_pending": 403,
  "index_progress_pct": 23.9
}
```

### Step 5: Test Hybrid Search

```bash
# Once some embeddings exist, test hybrid context recovery
curl "http://127.0.0.1:8080/api/lifestats/context/hybrid/user/YOUR_USER_ID?topic=authentication&limit=5" | jq
```

**Response includes `search_type`:**
- `"hybrid"` = FTS + vector search combined via RRF
- `"fts_only"` = Embeddings not available, fell back to keyword search

---

## Switching Providers

### From OpenAI → Local

1. **Update config:**
```toml
[embeddings]
provider = "local"
model = "all-MiniLM-L6-v2"
```

2. **Rebuild with feature:**
```bash
cargo build --release --features local-embeddings
```

3. **Reindex (dimensions changed!):**
```bash
aspy embeddings --reindex
# Confirm with 'y'
```

4. **Restart aspy**

### From Remote → Different Remote

If dimensions are the same (e.g., both 1536), no reindex needed:
```toml
# Just change api_base
api_base = "https://new-provider.com/v1"
```

If dimensions differ, must reindex:
```bash
aspy embeddings --reindex
```

---

## Verification Checklist

| Check | Command | Expected |
|-------|---------|----------|
| Config loaded | `aspy config --show` | Shows `[embeddings]` with your values |
| DB exists | `ls ~/.local/share/aspy/lifestats.db` | File exists |
| Offline status | `aspy embeddings --status` | Shows "(Offline)", correct provider |
| Live status | `aspy embeddings --status` (proxy running) | Shows "(Live)", indexer RUNNING |
| API responds | `curl .../api/lifestats/embeddings/status` | JSON with `running: true` |
| Indexing progresses | Check status twice, 30s apart | `documents_indexed` increases |
| Hybrid search works | `curl .../context/hybrid/user/...` | `search_type: "hybrid"` |

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Provider: disabled` | `provider = "none"` in config | Set to `"local"` or `"remote"` |
| `Provider: remote` but 0% progress | Missing API key | Set `ASPY_EMBEDDINGS_API_KEY` env var or `api_key` in config |
| Local embeddings not available | Missing feature flag | Rebuild with `--features local-embeddings` |
| `search_type: "fts_only"` | No embeddings indexed yet | Wait for indexer, or check status |
| Dimensions mismatch error | Changed models without reindex | `aspy embeddings --reindex` |
| API returns 404 | Proxy not running or wrong port | Start `aspy`, check `bind_addr` |

---

## Quick Reference

```bash
# Status
aspy embeddings --status

# Force reindex
aspy embeddings --reindex

# API endpoints (while proxy running)
GET  /api/lifestats/embeddings/status     # Indexer status
POST /api/lifestats/embeddings/reindex    # Trigger reindex
POST /api/lifestats/embeddings/poll       # Force check for new content
GET  /api/lifestats/context/hybrid/user/:id?topic=X  # Hybrid search
```
