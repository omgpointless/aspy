#!/usr/bin/env node
/**
 * aspy-mcp: MCP server for Aspy
 *
 * Exposes Aspy's HTTP API as MCP tools for Claude Code integration.
 * This is a thin wrapper - all data comes from the running Aspy proxy.
 */

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { createHash } from "crypto";
import { z } from "zod";

// Configuration
const API_BASE = process.env.ASPY_API_URL ?? "http://127.0.0.1:8080";

// ============================================================================
// User Identity
// ============================================================================

/**
 * Get user ID for session isolation.
 *
 * Priority order:
 * 1. ASPY_CLIENT_ID - Explicit client ID (matches proxy's URL path routing)
 *    Use this when connecting via http://localhost:8080/foundry/ etc.
 * 2. ANTHROPIC_API_KEY/AUTH_TOKEN hash - Fallback for bare URL users
 *    Use this when connecting via http://localhost:8080/ without client path
 *
 * Returns null if no identity can be determined.
 */
let cachedUserId: string | null = null;

function getUserId(): string | null {
  if (cachedUserId !== null) return cachedUserId;

  // Priority 1: Explicit client ID (supports multi-client same-API-key setups)
  // This matches the proxy's routing: /foundry/v1/messages → user_id = "foundry"
  if (process.env.ASPY_CLIENT_ID) {
    cachedUserId = process.env.ASPY_CLIENT_ID;
    return cachedUserId;
  }

  // Priority 2: API key hash (fallback for users not using client routing)
  const authToken =
    process.env.ANTHROPIC_API_KEY || process.env.ANTHROPIC_AUTH_TOKEN;

  if (!authToken) {
    return null; // Can't determine identity
  }

  // SHA-256 hash, first 16 hex chars (matches Rust: format!("{:x}", hash)[..16])
  cachedUserId = createHash("sha256")
    .update(authToken, "utf8")
    .digest("hex")
    .slice(0, 16);

  return cachedUserId;
}

// ============================================================================
// API Client
// ============================================================================

interface ApiError {
  error: string;
  status: number;
}

type ApiResult<T> = { ok: true; data: T } | { ok: false; error: ApiError };

async function fetchApi<T>(endpoint: string): Promise<ApiResult<T>> {
  try {
    const response = await fetch(`${API_BASE}${endpoint}`);

    if (!response.ok) {
      return {
        ok: false,
        error: {
          error: `HTTP ${response.status}: ${response.statusText}`,
          status: response.status,
        },
      };
    }

    const data = (await response.json()) as T;
    return { ok: true, data };
  } catch (err) {
    const message = err instanceof Error ? err.message : "Unknown error";
    return {
      ok: false,
      error: {
        error: `Failed to connect to Aspy: ${message}`,
        status: 0,
      },
    };
  }
}

async function postApi<T>(endpoint: string, body: unknown): Promise<ApiResult<T>> {
  try {
    const response = await fetch(`${API_BASE}${endpoint}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });

    if (!response.ok) {
      return {
        ok: false,
        error: {
          error: `HTTP ${response.status}: ${response.statusText}`,
          status: response.status,
        },
      };
    }

    const data = (await response.json()) as T;
    return { ok: true, data };
  } catch (err) {
    const message = err instanceof Error ? err.message : "Unknown error";
    return {
      ok: false,
      error: {
        error: `Failed to connect to Aspy: ${message}`,
        status: 0,
      },
    };
  }
}

// ============================================================================
// Type Definitions (matching Rust API responses)
// ============================================================================

interface StatsResponse {
  session: {
    started: string | null;
    duration_secs: number;
    events_count: number;
  };
  tokens: {
    input: number;
    output: number;
    cached: number;
    cache_created: number;
    cache_ratio_pct: number;
  };
  cost: {
    total_usd: number;
    savings_usd: number;
    by_model: Record<string, number>;
  };
  requests: {
    total: number;
    failed: number;
    success_rate_pct: number;
    avg_ttfb_ms: number;
  };
  tools: {
    total_calls: number;
    failed_calls: number;
    by_tool: Record<string, number>;
  };
  thinking: {
    blocks: number;
    total_tokens: number;
  };
}

interface EventsResponse {
  [key: string]: unknown;
  total_in_buffer: number;
  returned: number;
  events: ProxyEvent[];
}

interface ProxyEvent {
  type: string;
  timestamp: string;
  [key: string]: unknown;
}

interface ContextResponse {
  [key: string]: unknown;
  current_tokens: number;
  limit_tokens: number;
  usage_pct: number;
  warning_level: "normal" | "warning" | "high" | "critical";
  compacts: number;
  breakdown: {
    input: number;
    cached: number;
  };
}

interface SearchResponse {
  [key: string]: unknown;
  query: string;
  sessions_searched: number;
  total_matches: number;
  results: Array<{
    session: string;
    timestamp: string;
    role: string;
    text: string;
  }>;
}

// ============================================================================
// Lifestats Type Definitions (matching Rust API responses)
// ============================================================================

interface ThinkingMatch {
  session_id: string | null;
  timestamp: string;
  content: string;
  tokens: number | null;
  rank: number;
}

interface PromptMatch {
  session_id: string | null;
  timestamp: string;
  content: string;
  rank: number;
}

interface ResponseMatch {
  session_id: string | null;
  timestamp: string;
  content: string;
  rank: number;
}

type MatchType = "thinking" | "user_prompt" | "assistant_response";

interface ContextMatch {
  match_type: MatchType;
  session_id: string | null;
  timestamp: string;
  content: string;
  rank: number;
}

interface ThinkingSearchResponse {
  [key: string]: unknown;
  query: string;
  mode: string;
  results: ThinkingMatch[];
}

interface PromptSearchResponse {
  [key: string]: unknown;
  query: string;
  mode: string;
  results: PromptMatch[];
}

interface ResponseSearchResponse {
  [key: string]: unknown;
  query: string;
  mode: string;
  results: ResponseMatch[];
}

interface ContextSearchResponse {
  [key: string]: unknown;
  topic: string;
  mode: string;
  results: ContextMatch[];
}

interface ModelStats {
  [key: string]: unknown;
  model: string;
  tokens: number;
  cost_usd: number;
  calls: number;
}

interface ToolStats {
  [key: string]: unknown;
  tool: string;
  calls: number;
  avg_duration_ms: number;
  success_rate: number;
  rejections: number;
  errors: number;
}

interface LifetimeStats {
  [key: string]: unknown;
  total_sessions: number;
  total_tokens: number;
  total_cost_usd: number;
  total_tool_calls: number;
  total_thinking_blocks: number;
  total_prompts: number;
  first_session: string | null;
  last_session: string | null;
  by_model: ModelStats[];
  by_tool: ToolStats[];
}

// ============================================================================
// MCP Server
// ============================================================================

const server = new McpServer({
  name: "aspy",
  version: "0.1.0",
});

// Tool: aspy_stats
server.registerTool(
  "aspy_stats",
  {
    title: "Session Statistics",
    description:
      "Get current session statistics including tokens, costs, tool calls, and thinking blocks from Aspy",
    inputSchema: {},
    outputSchema: {
      session: z.object({
        started: z.string().nullable(),
        duration_secs: z.number(),
        events_count: z.number(),
      }),
      tokens: z.object({
        input: z.number(),
        output: z.number(),
        cached: z.number(),
        cache_ratio_pct: z.number(),
      }),
      cost: z.object({
        total_usd: z.number(),
        savings_usd: z.number(),
      }),
      requests: z.object({
        total: z.number(),
        failed: z.number(),
        success_rate_pct: z.number(),
      }),
      tools: z.object({
        total_calls: z.number(),
        top_tools: z.record(z.number()),
      }),
      thinking: z.object({
        blocks: z.number(),
        total_tokens: z.number(),
      }),
    },
  },
  async () => {
    // Auto-scope to current user's session
    const userId = getUserId();
    const endpoint = userId ? `/api/stats?user=${userId}` : "/api/stats";
    const result = await fetchApi<StatsResponse>(endpoint);

    if (!result.ok) {
      return {
        content: [{ type: "text" as const, text: `Error: ${result.error.error}` }],
        isError: true,
      };
    }

    const stats = result.data;
    const output = {
      session: stats.session,
      tokens: {
        input: stats.tokens.input,
        output: stats.tokens.output,
        cached: stats.tokens.cached,
        cache_ratio_pct: stats.tokens.cache_ratio_pct,
      },
      cost: {
        total_usd: stats.cost.total_usd,
        savings_usd: stats.cost.savings_usd,
      },
      requests: {
        total: stats.requests.total,
        failed: stats.requests.failed,
        success_rate_pct: stats.requests.success_rate_pct,
      },
      tools: {
        total_calls: stats.tools.total_calls,
        top_tools: stats.tools.by_tool,
      },
      thinking: stats.thinking,
    };

    return {
      content: [{ type: "text" as const, text: JSON.stringify(output, null, 2) }],
      structuredContent: output,
    };
  }
);

// Tool: aspy_events
server.registerTool(
  "aspy_events",
  {
    title: "Session Events",
    description:
      "Get recent events from the Aspy session (tool calls, API usage, thinking blocks, etc.)",
    inputSchema: {
      limit: z
        .number()
        .min(1)
        .max(500)
        .default(10)
        .describe("Maximum number of events to return"),
      type: z
        .enum([
          "ToolCall",
          "ToolResult",
          "ApiUsage",
          "Thinking",
          "Request",
          "Response",
        ])
        .optional()
        .describe("Filter by event type"),
    },
    outputSchema: {
      total_in_buffer: z.number(),
      returned: z.number(),
      events: z.array(
        z.object({
          type: z.string(),
          timestamp: z.string(),
        })
      ),
    },
  },
  async ({ limit = 10, type }) => {
    const params = new URLSearchParams();
    params.set("limit", String(limit));
    if (type) {
      params.set("type", type);
    }
    // Auto-scope to current user's session
    const userId = getUserId();
    if (userId) {
      params.set("user", userId);
    }

    const result = await fetchApi<EventsResponse>(`/api/events?${params}`);

    if (!result.ok) {
      return {
        content: [{ type: "text" as const, text: `Error: ${result.error.error}` }],
        isError: true,
      };
    }

    const output = result.data;

    return {
      content: [{ type: "text" as const, text: JSON.stringify(output, null, 2) }],
      structuredContent: output,
    };
  }
);

// Tool: aspy_context
server.registerTool(
  "aspy_context",
  {
    title: "Context Window Status",
    description:
      "Get current context window usage, warning level, and compact count from Aspy",
    inputSchema: {},
    outputSchema: {
      current_tokens: z.number(),
      limit_tokens: z.number(),
      usage_pct: z.number(),
      warning_level: z.enum(["normal", "warning", "high", "critical"]),
      compacts: z.number(),
      breakdown: z.object({
        input: z.number(),
        cached: z.number(),
      }),
    },
  },
  async () => {
    // Auto-scope to current user's session
    const userId = getUserId();
    const endpoint = userId ? `/api/context?user=${userId}` : "/api/context";
    const result = await fetchApi<ContextResponse>(endpoint);

    if (!result.ok) {
      return {
        content: [{ type: "text" as const, text: `Error: ${result.error.error}` }],
        isError: true,
      };
    }

    const output = result.data;

    // Add human-readable status
    const statusEmoji = {
      normal: "🟢",
      warning: "🟡",
      high: "🟠",
      critical: "🔴",
    }[output.warning_level];

    const summary = `${statusEmoji} Context: ${Math.floor(output.usage_pct)}% (${Math.floor(output.current_tokens / 1000)}K / ${Math.floor(output.limit_tokens / 1000)}K)`;

    return {
      content: [
        { type: "text" as const, text: summary },
        { type: "text" as const, text: JSON.stringify(output, null, 2) },
      ],
      structuredContent: output,
    };
  }
);

// Tool: aspy_sessions
interface SessionsResponse {
  active_count: number;
  sessions: Array<{
    key: string;
    user_id: string;
    claude_session_id: string | null;
    source: string;
    started: string;
    status: string;
    event_count: number;
    stats: {
      requests: number;
      tool_calls: number;
      input_tokens: number;
      output_tokens: number;
      cost_usd: number;
    };
  }>;
}

server.registerTool(
  "aspy_sessions",
  {
    title: "Active Sessions",
    description:
      "List all active Claude Code sessions tracked by Aspy. Shows user IDs, session status, and per-session statistics.",
    inputSchema: {},
    outputSchema: {
      active_count: z.number(),
      my_user_id: z.string().nullable(),
      sessions: z.array(
        z.object({
          key: z.string(),
          user_id: z.string(),
          is_me: z.boolean(),
          source: z.string(),
          status: z.string(),
          event_count: z.number(),
        })
      ),
    },
  },
  async () => {
    const result = await fetchApi<SessionsResponse>("/api/sessions");

    if (!result.ok) {
      return {
        content: [{ type: "text" as const, text: `Error: ${result.error.error}` }],
        isError: true,
      };
    }

    const myUserId = getUserId();
    const sessions = result.data.sessions.map((s) => ({
      key: s.key,
      user_id: s.user_id,
      is_me: myUserId !== null && s.user_id === myUserId,
      source: s.source,
      status: s.status,
      event_count: s.event_count,
      stats: s.stats,
    }));

    // Find my session for summary
    const mySession = sessions.find((s) => s.is_me);
    const summaryParts = [`📊 ${result.data.active_count} active session(s)`];

    if (mySession) {
      summaryParts.push(
        `You: ${mySession.user_id.slice(0, 8)}... (${mySession.stats.tool_calls} tools, $${mySession.stats.cost_usd.toFixed(2)})`
      );
    } else if (myUserId) {
      summaryParts.push(`Your ID: ${myUserId.slice(0, 8)}... (session not found)`);
    }

    const output = {
      active_count: result.data.active_count,
      my_user_id: myUserId,
      sessions,
    };

    return {
      content: [
        { type: "text" as const, text: summaryParts.join("\n") },
        { type: "text" as const, text: JSON.stringify(output, null, 2) },
      ],
      structuredContent: output,
    };
  }
);

// Tool: aspy_search
server.registerTool(
  "aspy_search",
  {
    title: "Search Session Logs",
    description:
      "Search session logs for past conversations. Use to recover context lost to compaction or find previous decisions/discussions. Searches through all logged sessions for messages containing the keyword.",
    inputSchema: {
      keyword: z
        .string()
        .min(2)
        .describe("Search term (case-insensitive, min 2 characters)"),
      role: z
        .enum(["user", "assistant"])
        .optional()
        .describe("Filter by message author"),
      session: z
        .string()
        .optional()
        .describe("Filter to specific session (partial filename match)"),
      limit: z
        .number()
        .min(1)
        .max(100)
        .default(10)
        .describe("Maximum results to return (default: 10, max: 100)"),
      time_range: z
        .enum(["today", "before_today", "last_3_days", "last_7_days", "last_30_days"])
        .optional()
        .describe("Filter by time range (default: all time)"),
    },
    outputSchema: {
      query: z.string(),
      sessions_searched: z.number(),
      total_matches: z.number(),
      results: z.array(
        z.object({
          session: z.string(),
          timestamp: z.string(),
          role: z.string(),
          text: z.string(),
        })
      ),
    },
  },
  async ({ keyword, role, session, limit = 10, time_range }) => {
    const result = await postApi<SearchResponse>("/api/search", {
      keyword,
      role,
      session,
      limit,
      time_range,
    });

    if (!result.ok) {
      return {
        content: [{ type: "text" as const, text: `Error: ${result.error.error}` }],
        isError: true,
      };
    }

    const data = result.data;

    // Build human-readable summary
    const summaryParts = [
      `🔍 Found ${data.total_matches} match(es) for "${data.query}" across ${data.sessions_searched} session(s):`,
    ];

    if (data.results.length === 0) {
      summaryParts.push("\nNo matches found. Try a different keyword or check that logs exist.");
    } else {
      summaryParts.push("");
      for (const r of data.results) {
        // Extract just the date part from session filename
        const sessionDate = r.session.replace("aspy-", "").replace(".jsonl", "").slice(0, 8);
        summaryParts.push(`**${sessionDate}** [${r.role}]:`);
        summaryParts.push(`${r.text}\n`);
      }
    }

    return {
      content: [
        { type: "text" as const, text: summaryParts.join("\n") },
        { type: "text" as const, text: JSON.stringify(data, null, 2) },
      ],
      structuredContent: data,
    };
  }
);

// ============================================================================
// Lifestats Tools (FTS5 Search - Cross-Session Context Recovery)
// ============================================================================

// Helper: Format match type for display
function formatMatchType(matchType: MatchType): string {
  switch (matchType) {
    case "thinking":
      return "💭 Thinking";
    case "user_prompt":
      return "👤 User";
    case "assistant_response":
      return "🤖 Assistant";
    default:
      return matchType;
  }
}

// Helper: Truncate content for summary display
function truncateContent(content: string, maxLen: number = 200): string {
  if (content.length <= maxLen) return content;
  return content.slice(0, maxLen) + "...";
}

// Tool: aspy_lifestats_search_thinking
server.registerTool(
  "aspy_lifestats_search_thinking",
  {
    title: "Search Thinking Blocks (FTS5)",
    description:
      "Search Claude's thinking blocks across all your sessions using FTS5 full-text search. Returns results ranked by BM25 relevance. Use this to find past reasoning, analysis, and internal deliberation.",
    inputSchema: {
      q: z.string().min(2).describe("Search query (min 2 characters)"),
      limit: z
        .number()
        .min(1)
        .max(100)
        .default(10)
        .describe("Maximum results (default: 10, max: 100)"),
      mode: z
        .enum(["phrase", "natural", "raw"])
        .default("phrase")
        .describe(
          "Search mode: phrase (exact match), natural (AND/OR/NOT operators), raw (full FTS5 syntax)"
        ),
    },
    outputSchema: {
      query: z.string(),
      mode: z.string(),
      results: z.array(
        z.object({
          session_id: z.string().nullable(),
          timestamp: z.string(),
          content: z.string(),
          tokens: z.number().nullable(),
          rank: z.number(),
        })
      ),
    },
  },
  async ({ q, limit = 10, mode = "phrase" }) => {
    const userId = getUserId();
    if (!userId) {
      return {
        content: [
          {
            type: "text" as const,
            text: "Error: Cannot determine user identity. Ensure ANTHROPIC_API_KEY is set.",
          },
        ],
        isError: true,
      };
    }

    const params = new URLSearchParams();
    params.set("q", q);
    params.set("limit", String(limit));
    params.set("mode", mode);

    const result = await fetchApi<ThinkingSearchResponse>(
      `/api/lifestats/search/user/${userId}/thinking?${params}`
    );

    if (!result.ok) {
      return {
        content: [{ type: "text" as const, text: `Error: ${result.error.error}` }],
        isError: true,
      };
    }

    const data = result.data;

    // Build human-readable summary
    const summaryParts = [
      `💭 Found ${data.results.length} thinking block(s) for "${data.query}" (mode: ${data.mode}):`,
    ];

    if (data.results.length === 0) {
      summaryParts.push("\nNo matches found. Try different keywords or search mode.");
    } else {
      summaryParts.push("");
      for (const r of data.results) {
        const session = r.session_id?.slice(0, 8) ?? "unknown";
        const date = r.timestamp.split("T")[0];
        summaryParts.push(`**[${date}]** (session: ${session}, rank: ${r.rank.toFixed(2)})`);
        summaryParts.push(`${truncateContent(r.content)}\n`);
      }
    }

    return {
      content: [
        { type: "text" as const, text: summaryParts.join("\n") },
        { type: "text" as const, text: JSON.stringify(data, null, 2) },
      ],
      structuredContent: data,
    };
  }
);

// Tool: aspy_lifestats_search_prompts
server.registerTool(
  "aspy_lifestats_search_prompts",
  {
    title: "Search User Prompts (FTS5)",
    description:
      "Search your past prompts/messages across all sessions using FTS5 full-text search. Returns results ranked by BM25 relevance. Use this to find what you asked previously.",
    inputSchema: {
      q: z.string().min(2).describe("Search query (min 2 characters)"),
      limit: z
        .number()
        .min(1)
        .max(100)
        .default(10)
        .describe("Maximum results (default: 10, max: 100)"),
      mode: z
        .enum(["phrase", "natural", "raw"])
        .default("phrase")
        .describe(
          "Search mode: phrase (exact match), natural (AND/OR/NOT operators), raw (full FTS5 syntax)"
        ),
    },
    outputSchema: {
      query: z.string(),
      mode: z.string(),
      results: z.array(
        z.object({
          session_id: z.string().nullable(),
          timestamp: z.string(),
          content: z.string(),
          rank: z.number(),
        })
      ),
    },
  },
  async ({ q, limit = 10, mode = "phrase" }) => {
    const userId = getUserId();
    if (!userId) {
      return {
        content: [
          {
            type: "text" as const,
            text: "Error: Cannot determine user identity. Ensure ANTHROPIC_API_KEY is set.",
          },
        ],
        isError: true,
      };
    }

    const params = new URLSearchParams();
    params.set("q", q);
    params.set("limit", String(limit));
    params.set("mode", mode);

    const result = await fetchApi<PromptSearchResponse>(
      `/api/lifestats/search/user/${userId}/prompts?${params}`
    );

    if (!result.ok) {
      return {
        content: [{ type: "text" as const, text: `Error: ${result.error.error}` }],
        isError: true,
      };
    }

    const data = result.data;

    // Build human-readable summary
    const summaryParts = [
      `👤 Found ${data.results.length} user prompt(s) for "${data.query}" (mode: ${data.mode}):`,
    ];

    if (data.results.length === 0) {
      summaryParts.push("\nNo matches found. Try different keywords or search mode.");
    } else {
      summaryParts.push("");
      for (const r of data.results) {
        const session = r.session_id?.slice(0, 8) ?? "unknown";
        const date = r.timestamp.split("T")[0];
        summaryParts.push(`**[${date}]** (session: ${session}, rank: ${r.rank.toFixed(2)})`);
        summaryParts.push(`${truncateContent(r.content)}\n`);
      }
    }

    return {
      content: [
        { type: "text" as const, text: summaryParts.join("\n") },
        { type: "text" as const, text: JSON.stringify(data, null, 2) },
      ],
      structuredContent: data,
    };
  }
);

// Tool: aspy_lifestats_search_responses
server.registerTool(
  "aspy_lifestats_search_responses",
  {
    title: "Search Assistant Responses (FTS5)",
    description:
      "Search Claude's past responses across all your sessions using FTS5 full-text search. Returns results ranked by BM25 relevance. Use this to find previous explanations, code, or answers.",
    inputSchema: {
      q: z.string().min(2).describe("Search query (min 2 characters)"),
      limit: z
        .number()
        .min(1)
        .max(100)
        .default(10)
        .describe("Maximum results (default: 10, max: 100)"),
      mode: z
        .enum(["phrase", "natural", "raw"])
        .default("phrase")
        .describe(
          "Search mode: phrase (exact match), natural (AND/OR/NOT operators), raw (full FTS5 syntax)"
        ),
    },
    outputSchema: {
      query: z.string(),
      mode: z.string(),
      results: z.array(
        z.object({
          session_id: z.string().nullable(),
          timestamp: z.string(),
          content: z.string(),
          rank: z.number(),
        })
      ),
    },
  },
  async ({ q, limit = 10, mode = "phrase" }) => {
    const userId = getUserId();
    if (!userId) {
      return {
        content: [
          {
            type: "text" as const,
            text: "Error: Cannot determine user identity. Ensure ANTHROPIC_API_KEY is set.",
          },
        ],
        isError: true,
      };
    }

    const params = new URLSearchParams();
    params.set("q", q);
    params.set("limit", String(limit));
    params.set("mode", mode);

    const result = await fetchApi<ResponseSearchResponse>(
      `/api/lifestats/search/user/${userId}/responses?${params}`
    );

    if (!result.ok) {
      return {
        content: [{ type: "text" as const, text: `Error: ${result.error.error}` }],
        isError: true,
      };
    }

    const data = result.data;

    // Build human-readable summary
    const summaryParts = [
      `🤖 Found ${data.results.length} assistant response(s) for "${data.query}" (mode: ${data.mode}):`,
    ];

    if (data.results.length === 0) {
      summaryParts.push("\nNo matches found. Try different keywords or search mode.");
    } else {
      summaryParts.push("");
      for (const r of data.results) {
        const session = r.session_id?.slice(0, 8) ?? "unknown";
        const date = r.timestamp.split("T")[0];
        summaryParts.push(`**[${date}]** (session: ${session}, rank: ${r.rank.toFixed(2)})`);
        summaryParts.push(`${truncateContent(r.content)}\n`);
      }
    }

    return {
      content: [
        { type: "text" as const, text: summaryParts.join("\n") },
        { type: "text" as const, text: JSON.stringify(data, null, 2) },
      ],
      structuredContent: data,
    };
  }
);

// Tool: aspy_lifestats_context (MOST IMPORTANT - Combined Context Recovery)
server.registerTool(
  "aspy_lifestats_context",
  {
    title: "Context Recovery (FTS5)",
    description:
      "RECOMMENDED: Combined context recovery searching across thinking blocks, user prompts, AND assistant responses simultaneously. Returns unified results ranked by BM25 relevance. Best tool for recovering lost context after session compaction.",
    inputSchema: {
      topic: z.string().min(2).describe("Topic to search for (min 2 characters)"),
      limit: z
        .number()
        .min(1)
        .max(100)
        .default(10)
        .describe("Maximum total results (default: 10, max: 100)"),
      mode: z
        .enum(["phrase", "natural", "raw"])
        .default("phrase")
        .describe(
          "Search mode: phrase (exact match), natural (AND/OR/NOT operators), raw (full FTS5 syntax)"
        ),
    },
    outputSchema: {
      topic: z.string(),
      mode: z.string(),
      results: z.array(
        z.object({
          match_type: z.string(),
          session_id: z.string().nullable(),
          timestamp: z.string(),
          content: z.string(),
          rank: z.number(),
        })
      ),
    },
  },
  async ({ topic, limit = 10, mode = "phrase" }) => {
    const userId = getUserId();
    if (!userId) {
      return {
        content: [
          {
            type: "text" as const,
            text: "Error: Cannot determine user identity. Ensure ANTHROPIC_API_KEY is set.",
          },
        ],
        isError: true,
      };
    }

    const params = new URLSearchParams();
    params.set("topic", topic);
    params.set("limit", String(limit));
    params.set("mode", mode);

    const result = await fetchApi<ContextSearchResponse>(
      `/api/lifestats/context/user/${userId}?${params}`
    );

    if (!result.ok) {
      return {
        content: [{ type: "text" as const, text: `Error: ${result.error.error}` }],
        isError: true,
      };
    }

    const data = result.data;

    // Build human-readable summary with source type indicators
    const summaryParts = [
      `🔍 Context Recovery: Found ${data.results.length} match(es) for "${data.topic}" (mode: ${data.mode}):`,
    ];

    if (data.results.length === 0) {
      summaryParts.push("\nNo matches found. Try:");
      summaryParts.push("  - Different keywords");
      summaryParts.push('  - mode: "natural" with AND/OR operators');
      summaryParts.push("  - Broader search terms");
    } else {
      summaryParts.push("");
      for (const r of data.results) {
        const session = r.session_id?.slice(0, 8) ?? "unknown";
        const date = r.timestamp.split("T")[0];
        const typeLabel = formatMatchType(r.match_type);
        summaryParts.push(
          `${typeLabel} **[${date}]** (session: ${session}, rank: ${r.rank.toFixed(2)})`
        );
        summaryParts.push(`${truncateContent(r.content)}\n`);
      }
    }

    return {
      content: [
        { type: "text" as const, text: summaryParts.join("\n") },
        { type: "text" as const, text: JSON.stringify(data, null, 2) },
      ],
      structuredContent: data,
    };
  }
);

// Tool: aspy_lifestats_stats
server.registerTool(
  "aspy_lifestats_stats",
  {
    title: "Lifetime Statistics",
    description:
      "Get your lifetime usage statistics across all sessions: total tokens, costs, tool usage, model breakdown, and time range. Your personal Claude Code time machine summary.",
    inputSchema: {},
    outputSchema: {
      total_sessions: z.number(),
      total_tokens: z.number(),
      total_cost_usd: z.number(),
      total_tool_calls: z.number(),
      total_thinking_blocks: z.number(),
      total_prompts: z.number(),
      first_session: z.string().nullable(),
      last_session: z.string().nullable(),
      by_model: z.array(
        z.object({
          model: z.string(),
          tokens: z.number(),
          cost_usd: z.number(),
          calls: z.number(),
        })
      ),
      by_tool: z.array(
        z.object({
          tool: z.string(),
          calls: z.number(),
          avg_duration_ms: z.number(),
          success_rate: z.number(),
          rejections: z.number(),
          errors: z.number(),
        })
      ),
    },
  },
  async () => {
    const userId = getUserId();
    if (!userId) {
      return {
        content: [
          {
            type: "text" as const,
            text: "Error: Cannot determine user identity. Ensure ANTHROPIC_API_KEY is set.",
          },
        ],
        isError: true,
      };
    }

    const result = await fetchApi<LifetimeStats>(
      `/api/lifestats/stats/user/${userId}`
    );

    if (!result.ok) {
      return {
        content: [{ type: "text" as const, text: `Error: ${result.error.error}` }],
        isError: true,
      };
    }

    const stats = result.data;

    // Build human-readable summary
    const summaryParts = ["📊 **Your Claude Code Lifetime Stats**\n"];

    // Overview
    summaryParts.push(`**Sessions:** ${stats.total_sessions}`);
    summaryParts.push(`**Total Tokens:** ${(stats.total_tokens / 1_000_000).toFixed(2)}M`);
    summaryParts.push(`**Total Cost:** $${stats.total_cost_usd.toFixed(2)}`);
    summaryParts.push(`**Tool Calls:** ${stats.total_tool_calls.toLocaleString()}`);
    summaryParts.push(`**Thinking Blocks:** ${stats.total_thinking_blocks.toLocaleString()}`);

    // Time range
    if (stats.first_session && stats.last_session) {
      const first = stats.first_session.split("T")[0];
      const last = stats.last_session.split("T")[0];
      summaryParts.push(`\n**Time Range:** ${first} → ${last}`);
    }

    // Top models
    if (stats.by_model.length > 0) {
      summaryParts.push("\n**By Model:**");
      for (const m of stats.by_model.slice(0, 5)) {
        summaryParts.push(
          `  - ${m.model}: ${(m.tokens / 1_000_000).toFixed(2)}M tokens, $${m.cost_usd.toFixed(2)}`
        );
      }
    }

    // Top tools
    if (stats.by_tool.length > 0) {
      summaryParts.push("\n**Top Tools:**");
      for (const t of stats.by_tool.slice(0, 5)) {
        const failures = t.rejections + t.errors;
        const failureDetail = failures > 0
          ? ` [${t.rejections} rejected, ${t.errors} errors]`
          : "";
        summaryParts.push(
          `  - ${t.tool}: ${t.calls} calls (avg ${t.avg_duration_ms.toFixed(0)}ms, ${(t.success_rate * 100).toFixed(0)}% success)${failureDetail}`
        );
      }
    }

    return {
      content: [
        { type: "text" as const, text: summaryParts.join("\n") },
        { type: "text" as const, text: JSON.stringify(stats, null, 2) },
      ],
      structuredContent: stats,
    };
  }
);

// ============================================================================
// Main
// ============================================================================

async function main(): Promise<void> {
  const transport = new StdioServerTransport();
  await server.connect(transport);

  // Handle graceful shutdown
  process.on("SIGINT", async () => {
    await server.close();
    process.exit(0);
  });

  process.on("SIGTERM", async () => {
    await server.close();
    process.exit(0);
  });
}

main().catch((error: unknown) => {
  console.error("Fatal error:", error);
  process.exit(1);
});
