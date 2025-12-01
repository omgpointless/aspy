#!/usr/bin/env node
/**
 * aspy-mcp: MCP server for anthropic-spy
 *
 * Exposes anthropic-spy's HTTP API as MCP tools for Claude Code integration.
 * This is a thin wrapper - all data comes from the running anthropic-spy proxy.
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
 * Compute user ID from API key/auth token (SHA-256, first 16 hex chars)
 * Matches the Rust proxy's hashing algorithm for consistency.
 *
 * Tries: ANTHROPIC_API_KEY, ANTHROPIC_AUTH_TOKEN (OAuth)
 * Returns null if no auth token available (MCP server sandboxed)
 */
let cachedUserId: string | null = null;

function getUserId(): string | null {
  if (cachedUserId !== null) return cachedUserId;

  // Try API key first, then OAuth token
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
        error: `Failed to connect to anthropic-spy: ${message}`,
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
        error: `Failed to connect to anthropic-spy: ${message}`,
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
      "Get current session statistics including tokens, costs, tool calls, and thinking blocks from anthropic-spy",
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
      "Get recent events from the anthropic-spy session (tool calls, API usage, thinking blocks, etc.)",
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
      "Get current context window usage, warning level, and compact count from anthropic-spy",
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
      "List all active Claude Code sessions tracked by anthropic-spy. Shows user IDs, session status, and per-session statistics.",
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
        const sessionDate = r.session.replace("anthropic-spy-", "").replace(".jsonl", "").slice(0, 8);
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
