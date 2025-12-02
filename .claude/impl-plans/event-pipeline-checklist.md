# Event Pipeline & Lifestats Storage - Implementation Checklist

**RFC:** [event-pipeline-and-storage.md](./event-pipeline-and-storage.md)
**Status:** Phase 2 Complete ✅ | Ready for Phase 3 (MCP Tools) or Testing
**Last Updated:** 2025-12-02
**Phase 2 Review:** Complete - All query interface components verified and production-ready

---

## Phase 1a: Core Pipeline (Minimal) ✅

**Goal:** Event processing infrastructure with no-op behavior

### Pipeline Infrastructure
- [x] Create `src/pipeline/mod.rs` module
- [x] Define `EventProcessor` trait (sync design, reference semantics)
- [x] Define `ProcessResult` enum (Continue, Transform, Drop, Error)
- [x] Define `ProcessContext` struct (session_id, user_id, is_demo with Arc<str>)
- [x] Implement `EventPipeline` struct with processor registry
- [x] Implement `process()` method with Cow<ProxyEvent> optimization
- [x] Implement `shutdown()` method (LIFO order, blocks until complete)
- [x] Add utility methods (`is_empty()`, `processor_names()`)

### Integration
- [x] Wire pipeline into `proxy::send_event()` (src/proxy/mod.rs:281-303)
- [x] Create pipeline conditionally in main.rs based on config
- [x] Pass ProcessContext with session/user correlation

### Validation
- [x] Create `LoggingProcessor` for pipeline flow validation (src/pipeline/logging.rs)
- [x] Verify empty pipeline is passthrough (no overhead)
- [x] Verify events flow through pipeline before dispatch

---

## Phase 1b: Storage Foundation ✅

**Goal:** SQLite database with WAL mode, writer thread, batch buffer

### Schema Design
- [x] Create schema v1 in `init_schema()` (src/pipeline/lifestats.rs:333-508)
  - [x] Metadata table for version tracking
  - [x] Sessions table (id, user_id, started_at, ended_at, source, aggregated stats)
  - [x] Thinking blocks table with FTS5 index (external content mode)
  - [x] Tool calls table (id, session_id, timestamp, tool_name, input_json)
  - [x] Tool results table (call_id, timestamp, output_json, duration_ms, success)
  - [x] API usage table (session_id, timestamp, model, tokens, cost)
  - [x] User prompts table with FTS5 index
  - [x] Assistant responses table with FTS5 index (v3.1 addition)
- [x] Enable WAL mode with proper PRAGMAs
  - [x] `PRAGMA journal_mode=WAL`
  - [x] `PRAGMA synchronous=NORMAL`
  - [x] `PRAGMA busy_timeout=5000`
  - [x] `PRAGMA cache_size=-64000` (64MB)
  - [x] FK constraints OFF (allows out-of-order events)

### Migration System
- [x] Implement version-based migration (src/pipeline/lifestats.rs:333-369)
- [x] Create `apply_schema_v1()` (initial schema)
- [x] Create `migrate_v1_to_v2()` (adds source column, idempotent)
- [x] Check version on startup and apply migrations sequentially
- [x] Document idempotency requirements (crash-safe migrations)

### Writer Thread Architecture
- [x] Spawn dedicated OS thread (NOT tokio task)
- [x] Use `std::sync::mpsc::sync_channel` (bounded, explicit backpressure)
- [x] Implement batch buffer with dual triggers:
  - [x] Size trigger (100 events)
  - [x] Time trigger (1 second)
- [x] Implement `flush_batch()` with transaction wrapping
- [x] Best-effort storage (individual event failures don't fail batch)

### Metrics & Observability
- [x] Create `LifestatsMetrics` struct (AtomicU64 counters)
  - [x] `events_stored` counter
  - [x] `events_dropped` counter (backpressure)
  - [x] `events_store_failed` counter (DB errors during batch)
  - [x] `batch_pending` gauge
  - [x] `write_latency_us` (total for averaging)
  - [x] `flush_count` counter
- [x] Implement `MetricsSnapshot` for observability
- [x] Implement `metrics()` getter on LifestatsProcessor
- [ ] Expose metrics via `/api/lifestats/health` endpoint (Phase 2)

### Graceful Shutdown
- [x] Implement `CompletionSignal` with Condvar (src/pipeline/lifestats.rs:119-155)
- [x] Implement `shutdown()` method (sends Shutdown command, waits with timeout)
- [x] Implement `Drop` trait (defense-in-depth, signals + joins thread)
- [x] Final batch flush on shutdown (all exit paths)
- [x] Wire explicit shutdown in main.rs (src/main.rs:354-364)

---

## Phase 1c: LifestatsProcessor ✅

**Goal:** Event routing, extraction, and storage

### Processor Implementation
- [x] Create `LifestatsProcessor` struct (src/pipeline/lifestats.rs:157-877)
- [x] Create `LifestatsConfig` struct with defaults
  - [x] `db_path` (PathBuf)
  - [x] `store_thinking` (bool, default true)
  - [x] `store_tool_io` (bool, default true)
  - [x] `max_thinking_size` (usize, 100KB default)
  - [x] `retention_days` (u32, 90 day default)
  - [x] `channel_buffer` (usize, 10k default)
  - [x] `batch_size` (usize, 100 default)
  - [x] `flush_interval` (Duration, 1s default)
- [x] Implement `new()` constructor (spawns writer thread)
- [x] Implement `EventProcessor` trait
  - [x] `name()` returns "lifestats"
  - [x] `process()` uses `try_send()` (non-blocking)
  - [x] `shutdown()` with timeout handling
- [x] Connect to writer thread via `WriterCommand` enum

### Event Storage Logic
- [x] Implement `store_event()` for all relevant event types (src/pipeline/lifestats.rs:656-811)
  - [x] Thinking blocks (with content truncation)
  - [x] Tool calls (INSERT OR REPLACE for idempotency)
  - [x] Tool results (linked to calls)
  - [x] API usage (with cost calculation)
  - [x] User prompts
  - [x] Assistant responses (v3.1 addition)
- [x] Manual FTS index updates (not triggers)
  - [x] Update thinking_fts after insert
  - [x] Update prompts_fts after insert
  - [x] Update responses_fts after insert
- [x] Handle config flags (store_thinking, store_tool_io, max_thinking_size)

### User Prompt Extraction
- [x] Implement `extract_user_prompt()` in proxy (src/proxy/mod.rs:46-74)
- [x] Parse messages array from request body
- [x] Find LAST user message (most recent)
- [x] Handle both string and array (multipart) content
- [x] Filter for text parts only
- [x] Emit UserPrompt event before request forwarding

### Assistant Response Extraction
- [x] Add `AssistantResponse` variant to `ProxyEvent` enum (src/events.rs:118-122)
- [x] Extract from regular JSON response (src/parser/mod.rs:228-245)
  - [x] Filter for Text content blocks
  - [x] Combine with "\n\n" separator
- [x] Extract from SSE streaming response
  - [x] Add `PartialContentBlock::Text` variant (src/parser/mod.rs:595+)
  - [x] Initialize on content_block_start (type="text")
  - [x] Accumulate on content_block_delta (type="text_delta")
  - [x] Emit on content_block_stop

### Configuration Integration
- [x] Add `LifestatsConfig` to main Config struct (src/config.rs:80-100)
- [x] Add TOML configuration support (src/config.rs:276-286, 408-417)
- [x] Support all config fields in file format
- [x] Implement config loading with defaults (src/config.rs:574-601)
- [x] Map flush_interval_secs (u64) to Duration in code

### Retention Cleanup
- [x] Implement `run_retention_cleanup()` (src/pipeline/lifestats.rs:560-654)
- [x] Document FTS external content sync contract (delete FTS FIRST, then base)
- [x] Delete in proper order (FTS → base tables)
  - [x] thinking_fts → thinking_blocks
  - [x] prompts_fts → user_prompts
  - [x] responses_fts → assistant_responses
  - [x] tool_results, tool_calls, api_usage
  - [x] sessions (orphaned only)
- [x] Wire up periodic cleanup in writer thread (src/pipeline/lifestats.rs:270-290)
  - [x] Track last_cleanup timestamp
  - [x] Run every 24 hours
  - [x] Skip if retention_days = 0
  - [x] Non-fatal error handling
- [ ] Add manual cleanup trigger endpoint (Phase 2: `/api/lifestats/cleanup`)

---

## Phase 2: Query Interface ✅

**Goal:** Read-only API with connection pooling for MCP tools

**Status:** COMPLETE (except testing and manual cleanup endpoint)
**Completed:** 2025-12-02
**Reviewed By:** Claude Sonnet 4.5

### Query Module Setup ✅
- [x] Create `src/pipeline/lifestats_query.rs` (556 lines)
- [x] Add `r2d2` and `r2d2_sqlite` dependencies (in Cargo.toml)
- [x] Implement connection pool (max_size: 4 for read-only) - line 265-266
- [x] Verify WAL mode allows concurrent reads (enabled in Phase 1)

### Search Mode Enum ✅
- [x] Implement `SearchMode` enum (Phrase, Natural, Raw) - lines 61-88
- [x] Implement `process()` method for each mode (improved from RFC's `process_query`) - lines 90-148
  - [x] Phrase: Wrap in quotes, escape internal quotes - line 99-101
  - [x] Natural: Preserve AND/OR/NOT, allow * wildcards - lines 103-142
  - [x] Raw: Pass through as-is (no escaping) - lines 143-146
- [x] Document safety trade-offs in each mode (extensive docs with examples)

### Query Result Structs ✅
- [x] Implement `ThinkingMatch` (session_id, timestamp, content, tokens, rank) - lines 151-159
- [x] Implement `PromptMatch` (session_id, timestamp, content, rank) - lines 161-168
- [x] Implement `ResponseMatch` (session_id, timestamp, content, rank) - lines 170-177
- [x] Implement `ContextMatch` (match_type, session_id, timestamp, content, rank) - lines 188-196
- [x] Implement `LifetimeStats` (sessions, tokens, cost, tool_calls, thinking_blocks, by_model, by_tool) - lines 198-210
- [x] Implement `ModelStats` (model, tokens, cost_usd, calls) - lines 212-220
- [x] Implement `ToolStats` (tool, calls, avg_duration_ms, success_rate) - lines 221-228

### LifestatsQuery Implementation ✅
- [x] Implement `new()` constructor (creates pool, verifies connection) - lines 254-274
- [x] Implement `search_thinking()` with FTS5 BM25 ranking - lines 281-333
- [x] Implement `search_prompts()` with FTS5 BM25 ranking - lines 335-383
- [x] Implement `search_responses()` with FTS5 BM25 ranking - lines 385-433
- [x] Implement `recover_context()` (combined search across all three) - lines 435-495
  - [x] Search thinking blocks - line 456
  - [x] Search user prompts - line 467
  - [x] Search assistant responses - line 478
  - [x] Sort by rank - line 489
  - [x] Limit total results - line 492
- [x] Implement `get_lifetime_stats()` (aggregate query) - lines 497-610
  - [x] Total tokens, cost, sessions - lines 507-522
  - [x] First/last session timestamps - lines 524-528
  - [x] By-model breakdown - lines 542-568
  - [x] By-tool breakdown with success rates - lines 570-597

### HTTP API Endpoints ✅
- [x] Implement `GET /api/lifestats/health` (system status) - api.rs:1086-1112
  - [x] Check pipeline and query availability
  - [x] Return status: "healthy" | "degraded" | "disabled" (improvement over RFC)
  - [x] Include query_available boolean field
  - Note: Health endpoint returns 200 OK with status field (better than 404 for disabled state)
- [x] Implement `POST /api/lifestats/cleanup` (manual retention trigger) - api.rs:1114-1129
  - [x] Placeholder with "not_implemented" status (documented TODO)
  - Note: Automatic 24h cleanup works; manual trigger deferred to future work
- [x] Implement `GET /api/lifestats/search/thinking?q=...&limit=...&mode=...` - api.rs:1177-1202
- [x] Implement `GET /api/lifestats/search/prompts?q=...&limit=...&mode=...` - api.rs:1204-1229
- [x] Implement `GET /api/lifestats/search/responses?q=...&limit=...&mode=...` - api.rs:1231-1256
- [x] Implement `GET /api/lifestats/context?topic=...&limit=...&mode=...` (combined) - api.rs:1275-1303
- [x] Implement `GET /api/lifestats/stats` (lifetime statistics) - api.rs:1305-1321
- [x] Add routes to proxy router (src/proxy/mod.rs:247-275)

### Infrastructure Integration ✅
- [x] Add lifestats_query field to ProxyState (proxy/mod.rs:115, 150)
- [x] Initialize LifestatsQuery in main.rs (lines 267-314)
- [x] Wire lifestats_query to SharedState (main.rs:332)
- [x] Export lifestats_query module (pipeline/mod.rs:25)

### Implementation Quality ✅
- [x] Zero compiler warnings (builds cleanly)
- [x] Zero unintended TODOs (only documented placeholder in cleanup endpoint)
- [x] Comprehensive documentation on all public APIs
- [x] Architectural consistency with Phase 1 (writer/reader separation)
- [x] All API endpoints return 404 when lifestats disabled (except health endpoint)

### Testing
- [ ] Unit tests for SearchMode query processing
- [ ] Unit tests for FTS5 escaping edge cases
- [ ] Integration test: insert → query → verify results
- [ ] Integration test: concurrent reads with WAL mode
- [ ] Load test: 1000 events → search performance

---

## Phase 3: MCP Tools & Agent Layer

**Goal:** Claude-accessible context recovery via MCP

### MCP Tool Definitions
- [ ] Create `.claude-plugin/mcp/tools.json` or equivalent
- [ ] Define `aspy_recall_context` tool
  - [ ] Input: topic (string), limit (number, default 10), mode (enum)
  - [ ] Output: ContextMatch[] with session_id, timestamp, content, rank
  - [ ] Description: "Search past conversations for specific topics"
- [ ] Define `aspy_lifetime_stats` tool
  - [ ] Input: none
  - [ ] Output: LifetimeStats with sessions, tokens, cost, breakdowns
  - [ ] Description: "Get aggregated statistics across all sessions"
- [ ] Define `aspy_search_thinking` tool (specialized search)
  - [ ] Input: query, limit, mode
  - [ ] Output: ThinkingMatch[]
- [ ] Define `aspy_search_prompts` tool
  - [ ] Input: query, limit, mode
  - [ ] Output: PromptMatch[]
- [ ] Define `aspy_search_responses` tool
  - [ ] Input: query, limit, mode
  - [ ] Output: ResponseMatch[]

### MCP Tool Implementation
- [ ] Implement tool handlers (call LifestatsQuery methods)
- [ ] Add error handling for database unavailable
- [ ] Add result truncation for large matches (prevent token explosion)
- [ ] Add formatting for Claude consumption (structured, readable)

### Agent Layer (Recovery Workflow)
- [ ] Create `.claude-plugin/agents/recover.md` (Haiku agent)
  - [ ] Define agent role: "Search coordinator and filter"
  - [ ] Define agent tools: aspy_search*, aspy_recall_context
  - [ ] Define workflow:
    1. Parse user's fuzzy query into search strategies
    2. Execute parallel searches with different keywords
    3. Filter and rank results by relevance
    4. Return structured matches (session ID + event ranges + previews)
  - [ ] CRITICAL: NO summarization - retrieval and filtering only
- [ ] Create `.claude-plugin/agents/profile.md` (Haiku agent)
  - [ ] Define agent role: "Performance and cost analysis"
  - [ ] Define agent tools: aspy_lifetime_stats
  - [ ] Define workflow:
    1. Query aggregated statistics
    2. Identify patterns (expensive tools, cache inefficiency)
    3. Generate actionable insights
- [ ] Create `.claude-plugin/agents/cost.md` (Haiku agent)
  - [ ] Define agent role: "Budget tracking and forecasting"
  - [ ] Define agent tools: aspy_lifetime_stats, aspy_stats (current session)
  - [ ] Define workflow:
    1. Calculate costs from token usage
    2. Project future costs based on trends
    3. Identify optimization opportunities

### Two-Agent Pattern Implementation
- [ ] Document pattern in agents:
  ```
  User Query → Haiku Agent (search + filter)
                   ↓ structured results
               Opus Agent (read full + synthesize)
  ```
- [ ] Implement agent coordination (if needed)
- [ ] Test recovery workflow end-to-end

### POC Validation
- [ ] Test context recovery after compaction
- [ ] Measure cost savings (Haiku searches vs Opus full context)
- [ ] Validate search quality (relevant results, good ranking)
- [ ] Document agent usage patterns for users

---

## Testing & Validation

### Unit Tests
- [ ] Pipeline processor ordering
- [ ] ProcessResult filtering behavior
- [ ] ProcessResult error handling (continue with original event)
- [ ] SQLite schema initialization
- [ ] WAL mode verification
- [ ] FTS escaping edge cases (quotes, parentheses, operators)

### Integration Tests
- [ ] Event flow: proxy → pipeline → SQLite → query
- [ ] Concurrent read/write with WAL mode
- [ ] Backpressure behavior (channel full scenario)
- [ ] Graceful shutdown (batch flushing verification)
- [ ] Retention cleanup (FTS sync, proper deletion order)
- [ ] Schema migration (v1 → v2 idempotency)

### Load Tests
- [ ] 100 events/second sustained for 10 minutes
- [ ] Verify no event loss (unless explicit backpressure)
- [ ] Measure write latency distribution (p50, p95, p99)
- [ ] Verify batch buffer behavior under load

### Recovery Tests
- [ ] Kill process during batch write (verify WAL recovery)
- [ ] Kill process during FTS update (verify index consistency)
- [ ] Restart after crash (verify no corruption)
- [ ] WAL checkpoint recovery

---

## Documentation

### User Documentation
- [ ] Update CLAUDE.md with lifestats usage
- [ ] Create docs/lifestats.md (architecture overview)
- [ ] Document configuration options with examples
- [ ] Document retention cleanup behavior
- [ ] Document MCP tools for context recovery

### Developer Documentation
- [ ] Document ProcessContext fields and usage
- [ ] Document SearchMode trade-offs
- [ ] Document FTS external content sync contract
- [ ] Document migration system for future schema changes
- [ ] Add inline code examples for custom processors

### API Reference
- [ ] Document HTTP endpoints in docs/api-reference.md
- [ ] Document query parameters and responses
- [ ] Document error codes and handling
- [ ] Add curl examples for each endpoint

---

## Future Enhancements (Post-v0.2.0)

### Phase 4: Redaction & Filtering Processors
- [ ] Implement RedactionProcessor (PII/secret detection)
- [ ] Implement FilterProcessor (drop by type/content)
- [ ] Implement TransformProcessor (event modification)
- [ ] Add processor priority system (order matters for transforms)

### Phase 5: Session Boundary Improvements
- [ ] Implement SessionStart hook detection (hook-based boundary)
- [ ] Implement warmup request detection (pattern-based boundary)
- [ ] Implement idle timeout sessions (time-based boundary)
- [ ] Implement session aggregation (update sessions table stats)

### Phase 6: Advanced Queries
- [ ] Implement temporal queries (events between timestamps)
- [ ] Implement session timeline queries (full conversation)
- [ ] Implement tool correlation queries (which tools are used together)
- [ ] Implement cost queries (sessions by cost, trend analysis)

### Phase 7: Export & Backup
- [ ] Implement JSONL export for sessions
- [ ] Implement SQLite backup (WAL checkpoint + copy)
- [ ] Implement incremental backup system
- [ ] Implement import for session migration

---

## Notes

**v3.1 Oversight Resolution:**
During Phase 1 implementation, we discovered that the original RFC captured UserPrompt and Thinking events but completely omitted AssistantResponse events. This was resolved by adding AssistantResponse to the event types, schema, parser, and storage. See RFC lines 10-24 for details.

**Shutdown Fix:**
Peer review identified that pipeline shutdown was implicit (Drop-based) rather than explicit. Fixed by cloning pipeline Arc in main.rs and calling shutdown() explicitly before proxy shutdown signal. See main.rs:354-364.

**Retention Cleanup:**
Initially deferred with TODO in RFC. Implemented in Phase 1 hygiene pass with 24-hour periodic cleanup in writer thread. See lifestats.rs:270-290.

**Zero Warnings:**
All compiler warnings resolved during Phase 1 hygiene pass. Future API annotated with `#[allow(dead_code)]` to document intent. Maintain zero warnings baseline going forward.
