# Extensions Vision: anthropic-spy as a Claude Code Reflexivity Layer

## Executive Summary

anthropic-spy can evolve from a **passive observer** to an **active participant** in the Claude Code ecosystem. By exposing observability data through standard extension mechanisms (HTTP API, MCP servers, hooks, slash commands), we enable:

1. **Claude self-introspection** - Claude queries its own token usage, tool history, and thinking blocks
2. **Proactive context management** - Auto-suggest compaction before hitting limits
3. **In-conversation observability** - Users get instant metrics without leaving the session
4. **Extensible workflows** - Hook scripts enable custom integrations (budgets, approvals, analytics)

This positions anthropic-spy as the **observability and reflexivity layer** for Claude Code sessions.

---

## The Strategic Opportunity

### Current State
anthropic-spy intercepts all Claude Code ↔ Anthropic API traffic and provides:
- Real-time TUI with event streams, thinking blocks, and stats
- JSON Lines session logs for analysis
- Token/cost tracking with cache efficiency metrics
- Context percentage warnings via request augmentation

### The Gap
Users cannot:
- Query session state from within the Claude Code conversation
- Enable Claude to introspect its own behavior (token usage, tool history)
- Build custom workflows on top of observability data (budgets, approvals, alerts)
- Access metrics without switching to the TUI or parsing logs

### The Vision
Expose anthropic-spy's observability data through **Claude Code's native extension points**:

```
┌─────────────────────────────────────────────────────────┐
│ Claude Code Session                                      │
│                                                          │
│  User: "How many tokens have I used today?"             │
│  Claude: [calls get_session_stats MCP tool]             │
│          "You've used 847K tokens (98% cached), $2.14"  │
│                                                          │
│  User: /spy:tools Read --recent 5                       │
│  [Shows top 5 Read tool calls with timing]              │
└─────────────────────────────────────────────────────────┘
         ↑                                    ↑
    MCP Tools                          Slash Commands
         │                                    │
         └──────────┬─────────────────────────┘
                    ↓
           ┌─────────────────┐
           │   HTTP API      │  ← New layer (kernel)
           │  :8080/api/*    │
           └─────────────────┘
                    ↓
           ┌─────────────────┐
           │  Event System   │  ← Existing (kernel)
           │  (mpsc channel) │
           └─────────────────┘
```

---

## Architectural Fit

### Layered Architecture Alignment

This vision aligns perfectly with our kernel/userland/user space model:

**Kernel (Core Infrastructure):**
- Event system (existing)
- HTTP API endpoints (NEW) - queries event history and stats
- Cannot be disabled, foundational capability

**Userland (Optional Features):**
- MCP server npm package (NEW) - wraps HTTP API in Claude-callable tools
- Augmentor hooks (existing) - can trigger external webhooks
- Config-toggleable, enhances UX but not required for operation

**User Space (Custom Extensions):**
- Slash command scripts (NEW) - user-provided `.claude/commands/spy/*.md`
- Hook scripts (NEW) - user-provided `~/.claude/hooks/*` that call HTTP API
- Completely external, users bring their own

**Decision Tree:**
```
Is this feature fundamental to anthropic-spy's operation?
├─ YES → Kernel (HTTP API, event system)
└─ NO  → Is it an official enhancement?
          ├─ YES → Userland (MCP server, toggleable)
          └─ NO  → User Space (custom scripts)
```

### Composition Over Inheritance

Extensions compose existing capabilities without modifying core systems:
- **HTTP API** queries the event channel (doesn't own new state)
- **MCP server** wraps HTTP API calls (doesn't touch proxy internals)
- **Slash commands** format HTTP responses (pure presentation layer)
- **Hooks** receive events via webhooks (passive listeners)

No god objects. No tight coupling. Features remain independent and toggleable.

---

## Integration Points

### 1. HTTP API (Foundation)

**Purpose:** Enable programmatic access to session state

**Endpoints:**
```
GET  /api/stats              → Session summary (tokens, costs, cache %)
GET  /api/events?limit=N     → Recent events (filterable by type)
GET  /api/tools?name=X       → Tool call history with timing
GET  /api/thinking?limit=N   → Thinking blocks
GET  /api/context            → Current context % and warnings
POST /api/hooks/trigger      → Webhook receiver for external systems
```

**Implementation Notes:**
- Axum routes alongside existing proxy handler
- Queries from existing event storage (no new data structures)
- JSON responses (easy consumption from scripts/MCP)
- Bind to `127.0.0.1` only (localhost security)
- Optional auth token via config (future: `ANTHROPIC_SPY_API_TOKEN`)

**Data Sources:**
- `Arc<Mutex<Vec<ProxyEvent>>>` - event history
- `Stats` struct - aggregated metrics
- Parser's `pending_calls` - in-flight tool calls

### 2. MCP Server

**Purpose:** Enable Claude to introspect its own session

**Package:** `anthropic-spy-mcp` (npm module)

**Tools:**
```typescript
{
  "get_session_stats": {
    description: "Get current session token usage, costs, and cache efficiency",
    parameters: {},
    returns: { tokens: {...}, cost: {...}, cache_ratio: "98%" }
  },

  "query_tool_calls": {
    description: "Search tool call history with optional filters",
    parameters: {
      tool_name?: string,
      limit?: number,
      success_only?: boolean
    },
    returns: [...tool calls with timing...]
  },

  "get_thinking_blocks": {
    description: "Retrieve Claude's recent thinking blocks",
    parameters: { limit?: number },
    returns: [...thinking content with token estimates...]
  },

  "analyze_token_usage": {
    description: "Breakdown of tokens by model, tool, and message type",
    parameters: {},
    returns: { by_model: {...}, by_tool: {...}, trends: [...] }
  },

  "get_context_state": {
    description: "Current context window usage and warnings",
    parameters: {},
    returns: { percentage: 87, warnings: [...], suggestions: [...] }
  }
}
```

**Configuration:**
```json
// .mcp.json
{
  "mcpServers": {
    "anthropic-spy": {
      "command": "npx",
      "args": ["anthropic-spy-mcp"],
      "env": {
        "ANTHROPIC_SPY_API_URL": "http://127.0.0.1:8080"
      }
    }
  }
}
```

**User Value:**
- Claude can answer "How many tokens have I used?" without user manually checking
- Cost awareness: "Am I approaching my budget?"
- Context optimization: "What should I compact to free up space?"
- Session archaeology: "What was I thinking when I wrote this function?" (query past thinking blocks)

### 3. Slash Commands

**Purpose:** User-facing convenience layer for quick queries

**Location:** `.claude/commands/spy/*.md` (shipped in repo, user symlinks or copies)

**Examples:**

**`.claude/commands/spy/stats.md`**
```markdown
---
description: "View anthropic-spy session statistics"
---
Query the anthropic-spy proxy for real-time session metrics.

```bash
curl -s http://127.0.0.1:8080/api/stats | jq '{
  tokens: .tokens,
  cost: .cost,
  cache_ratio: .cache_ratio,
  duration: .duration,
  requests: .requests
}'
```
```

**`.claude/commands/spy/tools.md`**
```markdown
---
description: "List recent tool calls with timing"
---
Shows the last 10 tool calls with execution times.

```bash
curl -s "http://127.0.0.1:8080/api/tools?limit=10" | jq -r '.[] |
  "\(.tool_name) | \(.duration)ms | \(if .success then "✓" else "✗" end)"'
```
```

**`.claude/commands/spy/thinking.md`**
```markdown
---
description: "Show Claude's recent thinking blocks"
---
Display the last 5 thinking blocks with token estimates.

```bash
curl -s "http://127.0.0.1:8080/api/thinking?limit=5" | jq -r '.[] |
  "[\(.timestamp)] \(.token_estimate) tokens\n\(.content[0:200])...\n"'
```
```

**`.claude/commands/spy/context.md`**
```markdown
---
description: "Check current context window usage"
---
Shows context percentage and any warnings.

```bash
curl -s http://127.0.0.1:8080/api/context | jq '{
  percentage: .percentage,
  warnings: .warnings,
  suggestions: .suggestions
}'
```
```

**User Value:**
- Instant observability without leaving conversation
- No need to switch to TUI or parse logs
- Shareable (team can commit commands to repo)
- Customizable (users edit scripts to format differently)

### 4. Hook Integration

**Purpose:** Bidirectional proxy ↔ Claude Code communication

**Hook Points:**
```
SessionStart     → Initialize proxy session, load preferences
PreToolUse       → Intercept tool calls (approval workflows, timing markers)
PostToolUse      → Enrich events with outcomes (success/failure, duration)
UserPromptSubmit → Context warnings, prompt complexity analysis
Stop             → Flush logs, generate summary, emit SessionEnded event
```

**Example Hook Script (User Space):**

**`~/.claude/hooks/pre-tool.sh`**
```bash
#!/bin/bash
# Read tool call from stdin (Claude Code provides JSON)
TOOL_CALL=$(cat)
TOOL_NAME=$(echo "$TOOL_CALL" | jq -r '.name')

# Notify proxy of incoming tool call
curl -s -X POST http://127.0.0.1:8080/api/hooks/pre-tool \
  -H "Content-Type: application/json" \
  -d "$TOOL_CALL"

# Approval workflow example: Block dangerous operations
if [[ "$TOOL_NAME" == "Bash" ]]; then
  COMMAND=$(echo "$TOOL_CALL" | jq -r '.input.command')
  if [[ "$COMMAND" =~ "rm -rf" ]]; then
    echo "⚠️  Dangerous command detected: $COMMAND"
    echo "Approve? (y/n)"
    read -r APPROVAL
    if [[ "$APPROVAL" != "y" ]]; then
      exit 1  # Block tool call
    fi
  fi
fi
```

**`~/.claude/hooks/stop.sh`**
```bash
#!/bin/bash
# Session cleanup
curl -s -X POST http://127.0.0.1:8080/api/hooks/stop

# Retrieve session summary
SUMMARY=$(curl -s http://127.0.0.1:8080/api/stats)
echo "Session Summary:"
echo "$SUMMARY" | jq '{tokens: .tokens, cost: .cost, duration: .duration}'
```

**Proxy Implementation:**
- Add `POST /api/hooks/pre-tool` endpoint
- Emit `HookTriggered` event to event channel
- Optionally block/modify tool calls (return modified JSON or error)

**User Value:**
- Custom workflows (cost budgets, approval gates, external logging)
- Team policies (block writes to production paths, require PR for deploys)
- Observability extensions (POST events to Slack, DataDog, etc.)

### 5. Context Warning Enhancement

**Current:** `maybe_inject_context_warning()` adds warnings to API response text

**Enhanced with Hooks:**
```rust
// In proxy/augmentation/context_warning.rs
async fn maybe_inject_context_warning(&self, request: &Request) -> Option<String> {
    let context_pct = self.calculate_context_percentage(request);

    match context_pct {
        80..=89 => {
            // Trigger UserPromptSubmit hook feedback
            self.trigger_hook("UserPromptSubmit", json!({
                "context_pct": context_pct,
                "warning_level": "moderate"
            })).await;
            Some("⚠️ Context at 80%...".to_string())
        },
        90..=94 => {
            // Stronger warning + suggest /compact
            self.trigger_hook("UserPromptSubmit", json!({
                "context_pct": context_pct,
                "warning_level": "high",
                "suggestion": "/compact"
            })).await;
            Some("🚨 Context at 90%! Consider /compact...".to_string())
        },
        95.. => {
            // Critical warning + expose via MCP for Claude to self-manage
            self.trigger_hook("UserPromptSubmit", json!({
                "context_pct": context_pct,
                "warning_level": "critical"
            })).await;
            // Could optionally block request here (return error)
            Some("🔴 CRITICAL: Context at 95%!...".to_string())
        },
        _ => None
    }
}
```

**User Value:**
- Proactive context management before hitting limits
- Claude can introspect context state via MCP and self-optimize
- Users notified via multiple channels (in-response warning + hook script)

---

## Prioritized Roadmap

### Phase 0: Document the Vision ✅
**Status:** Complete (this document)

**Deliverable:** `.claude/EXTENSIONS_VISION.md`

### Phase 1: HTTP API Foundation ✅
**Status:** Shipped in 0.1.0
**Goal:** Enable external queries of proxy state

**Implementation Completed:**
1. ✅ Added `/api/stats` endpoint with full session metrics
2. ✅ Shared state architecture (`Arc<Mutex<Stats>>`) for TUI + API access
3. ✅ JSON responses with proper error handling
4. ✅ Binds to `127.0.0.1:8080` (localhost only)
5. ✅ Documented in `README.md` with examples

**0.1.0 Proof-of-Concept Delivered:**
- `/api/stats` endpoint returns comprehensive session data
- Validated integration pattern works perfectly
- Foundation ready for additional endpoints in 0.2.0

**Deliverable:** `curl http://127.0.0.1:8080/api/stats` returns:
```json
{
  "session": {
    "started": "2025-11-30T14:23:00Z",
    "duration_secs": 7320,
    "events_count": 1847
  },
  "tokens": {
    "input": 124500,
    "output": 48200,
    "cached": 1456000,
    "cache_ratio_pct": 98
  },
  "cost": {
    "total_usd": 2.14,
    "by_model": {
      "claude-sonnet-4": 1.89,
      "claude-haiku-3-5": 0.25
    }
  },
  "requests": {
    "total": 79,
    "by_model": {
      "claude-sonnet-4": 37,
      "claude-haiku-3-5": 42
    }
  },
  "tools": {
    "total_calls": 142,
    "by_tool": {
      "Read": 58,
      "Edit": 32,
      "Bash": 24,
      "Glob": 18,
      "TodoWrite": 10
    }
  },
  "thinking": {
    "blocks": 23,
    "total_tokens": 8940
  }
}
```

### Phase 2: Slash Command Suite ✅ (Partial)
**Status:** `/spy:stats` shipped in 0.1.0, others pending 0.2.0

**Implementation Completed:**
1. ✅ Created `.claude/commands/spy/stats.md`
2. ✅ Command curls HTTP API and formats with `jq`
3. ✅ Tested and working in Claude Code

**Commands Status:**
- ✅ `/spy:stats` - Session summary (SHIPPED 0.1.0)
- ⏳ `/spy:tools [tool_name] [--recent N]` - Tool call history (0.2.0)
- ⏳ `/spy:thinking [--recent N]` - Thinking blocks (0.2.0)
- ⏳ `/spy:context` - Context percentage and warnings (0.2.0)
- ⏳ `/spy:export` - Download session JSONL path (0.2.0)

**0.1.0 Deliverable:** User types `/spy:stats`, sees formatted metrics in Claude Code ✅

### Phase 3: MCP Server (0.3.0)
**Goal:** Claude can introspect its own session

**Implementation:**
1. Create `anthropic-spy-mcp` npm package
2. Implement MCP protocol (stdin/stdout JSON-RPC)
3. Wrap HTTP API calls in MCP tool definitions
4. Ship with `.mcp.json` example config
5. Publish to npm registry

**Tools:**
- `get_session_stats` - Token/cost summary
- `query_tool_calls` - Searchable tool history
- `get_thinking_blocks` - Past reasoning
- `analyze_token_usage` - Breakdown by model/tool
- `get_context_state` - Context % and warnings

**Deliverable:**
- User configures MCP server
- Claude answers "How many tokens have I used?" by calling tool
- Demonstrates self-introspection capability

### Phase 4: Hook Integration ✅ (Partial)
**Status:** Cargo fmt hook shipped in 0.1.0, webhook receiver pending 0.3.0

**Goal:** Bidirectional proxy ↔ Claude Code communication

**Implementation Completed (0.1.0):**
1. ✅ Project-specific PostToolUse hook (`.claude/hooks/post-tool-use.sh`)
2. ✅ Automatic `cargo fmt` on Write/Edit of Rust files
3. ✅ JSON output with proper exit codes (non-blocking)
4. ✅ Documented in `.claude/hooks/README.md`
5. ✅ Handles missing `CLAUDE_PROJECT_DIR` gracefully

**Implementation Pending (0.3.0):**
1. ⏳ Add `POST /api/hooks/{hook_name}` webhook receiver
2. ⏳ Emit `HookTriggered` events to event channel
3. ⏳ Ship example hook scripts in `examples/hooks/`
4. ⏳ Optional: Implement hook response handling (block/modify tool calls)

**Hook Points:**
- `SessionStart` - Initialization
- `PreToolUse` - Intercept before execution
- `PostToolUse` - Enrich after completion
- `UserPromptSubmit` - Context warnings, analysis
- `Stop` - Cleanup, summary

**0.1.0 Deliverable:** ✅
- Project-specific cargo fmt hook working silently
- Formats all Rust files automatically on save
- Foundation for future hook-based workflows

**0.3.0 Deliverable:** ⏳
- Example hook scripts demonstrate approval workflow
- Proxy logs hook triggers in TUI
- Users can build custom integrations (budgets, alerts, external logging)

### Phase 5: Advanced Features (0.4.0+)
**Goal:** Polish and expand use cases

**Potential Features:**
- **Cost Budgeting:** Block requests when budget threshold exceeded
- **Session Recording/Replay:** Full session playback with timing
- **Team Analytics Dashboard:** Aggregate stats across users (web UI)
- **Prompt Templates:** Pre-built observability prompts as commands
- **API Key Rotation Detection:** Warn when key changes mid-session
- **Rate Limit Prediction:** Forecast hitting limits based on trends
- **Diff Visualization:** Show file changes in TUI (parse Edit tool results)
- **Security Audit Mode:** Flag sensitive file access, redact in logs
- **Multi-Session Analysis:** Compare sessions, identify patterns
- **Plugin System:** User-provided Rust/WASM plugins for custom augmentation

---

## Design Principles for Extensions

### 1. Composition, Not Modification
Extensions compose existing capabilities. They don't modify core systems (proxy, parser, event channel).

**Example:**
- ✅ MCP server queries HTTP API (composition)
- ❌ MCP server directly accesses event storage (tight coupling)

### 2. Kernel Provides Data, Userland Consumes
HTTP API is kernel (always available). MCP/commands are userland (optional, toggleable).

**Example:**
- ✅ MCP server can be disabled via config, HTTP API remains
- ❌ Disabling MCP breaks HTTP API

### 3. User Space is Sovereign
Users bring their own hook scripts and commands. We ship examples, not requirements.

**Example:**
- ✅ Ship `examples/hooks/approval-workflow.sh` (inspiration)
- ❌ Force users to use our approval logic

### 4. Security by Default
HTTP API binds to localhost only. Optional auth tokens for paranoid users.

**Example:**
- ✅ `ANTHROPIC_SPY_BIND=127.0.0.1:8080` (default)
- ❌ `ANTHROPIC_SPY_BIND=0.0.0.0:8080` (exposed to network)

### 5. Performance Aware
Querying session state should be fast (in-memory caching, limit result sizes).

**Example:**
- ✅ `/api/events?limit=100` (bounded)
- ❌ `/api/events` (returns all 10K events, slow)

### 6. Fail Gracefully
Extensions are optional. If HTTP API fails to bind, proxy still works (log warning, disable API).

**Example:**
- ✅ Port 8080 in use? Log warning, continue without API
- ❌ Port 8080 in use? Crash entire proxy

### 7. Discoverable and Documented
Every endpoint, tool, and command has examples in README.md.

**Example:**
- ✅ README shows `curl` examples for every endpoint
- ❌ "Figure out the API yourself"

---

## Success Metrics

How do we know this vision succeeded?

### User Adoption
- **Slash commands:** 30% of users run `/spy:stats` within first week
- **MCP server:** 10% of users configure MCP for self-introspection
- **Hook scripts:** 5% of users share custom hooks (approvals, budgets, alerts)

### Community Engagement
- **GitHub stars:** 2x increase (current baseline + attract extension developers)
- **External integrations:** Users build Slack bots, DataDog exporters, cost dashboards
- **Documentation PRs:** Users contribute command examples, hook recipes

### Technical Validation
- **HTTP API stability:** <1% error rate, <50ms p95 latency
- **MCP reliability:** Works across Windows/macOS/Linux without modification
- **Hook performance:** PreToolUse hooks add <10ms overhead

### Strategic Positioning
- **Referenced in Claude Code docs:** Anthropic mentions anthropic-spy as observability solution
- **Integration examples:** Other tools query anthropic-spy API (CI/CD, notebooks)
- **Teaching resource:** Used in tutorials for "understanding Claude Code behavior"

---

## Risks and Mitigations

### Risk 1: Scope Creep Before 0.1.0
**Impact:** Delays shipping core TUI improvements

**Mitigation:**
- This is a **0.2.0 vision**, not immediate work
- 0.1.0 includes only `/api/stats` as proof-of-concept (2-3 hours)
- Full HTTP API waits until 0.2.0

### Risk 2: HTTP API Security
**Impact:** Local processes could abuse unauthenticated endpoints

**Mitigation:**
- Bind to `127.0.0.1` only (localhost)
- Optional `ANTHROPIC_SPY_API_TOKEN` env var (future)
- Rate limiting (10 req/sec per endpoint)
- Document security model in README

### Risk 3: MCP Server Overhead
**Impact:** Querying proxy state slows down Claude responses

**Mitigation:**
- Cache recent stats in memory (don't recompute every call)
- Limit result sizes (`?limit=100` default)
- Make MCP toggleable (userland feature, disable if slow)
- Document performance characteristics

### Risk 4: Hook Script Complexity
**Impact:** Users struggle to write effective hooks

**Mitigation:**
- Ship polished examples in `examples/hooks/`
- Document hook payload formats with JSON schemas
- Create "hook recipes" wiki page
- Provide debugging tips (log hook stdin/stdout)

### Risk 5: Maintenance Burden
**Impact:** Supporting HTTP API, MCP, hooks increases complexity

**Mitigation:**
- HTTP API is thin layer over existing event system (low overhead)
- MCP server is separate npm package (independent versioning)
- Hooks are user space (we ship examples, users maintain their own)
- Clear boundaries prevent sprawl

### Risk 6: Claude Code Breaking Changes
**Impact:** Hook/MCP formats change, breaking integrations

**Mitigation:**
- Version HTTP API (`/v1/api/stats`)
- Document Claude Code version compatibility
- Monitor Claude Code changelogs for breaking changes
- Community helps report issues (benefit of open source)

---

## Call to Action

**For Maintainers:**
1. **Proof-of-Concept (0.1.0):** Add `/api/stats` endpoint (2-3 hours)
2. **Community Feedback:** Share vision on Discord/Reddit, gauge interest
3. **Roadmap Planning:** Prioritize 0.2.0 features based on feedback

**For Contributors:**
4. **HTTP API Design:** Propose endpoint schemas, error formats
5. **MCP Server:** Implement npm package (Node.js + MCP protocol)
6. **Slash Commands:** Write polished command scripts with `jq` formatting
7. **Hook Examples:** Create approval workflows, budget trackers, alerting scripts

**For Users:**
8. **Test `/api/stats`:** Validate proof-of-concept, report issues
9. **Request Features:** What observability data would you want as MCP tools?
10. **Share Use Cases:** How would you use hooks? (budgets, approvals, alerts?)

---

## Conclusion

anthropic-spy is positioned to become the **reflexivity layer** for Claude Code—enabling both users and Claude itself to understand, optimize, and extend AI-assisted development sessions.

By exposing observability data through standard extension mechanisms, we unlock:
- **Self-aware AI:** Claude introspects its own behavior
- **Proactive optimization:** Context warnings before hitting limits
- **Custom workflows:** Users build budgets, approvals, analytics
- **In-conversation observability:** Metrics without leaving the session

The architecture supports it. The integration points exist. The user value is clear.

Let's build it.

---

**Document Status:** Living document, updated as implementation progresses
**Last Updated:** 2025-11-30
**Next Review:** After 0.1.0 proof-of-concept ships
