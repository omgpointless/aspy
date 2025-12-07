#!/usr/bin/env node
/**
 * aspy-mcp: MCP server for Aspy
 *
 * Exposes Aspy's HTTP API as MCP tools for Claude Code integration.
 * This is a thin wrapper - all data comes from the running Aspy proxy.
 *
 * Tool naming philosophy:
 * - SESSION tools: aspy_stats, aspy_events, aspy_window, aspy_sessions
 * - MEMORY tools: aspy_recall (primary), aspy_recall_* (specialized)
 * - LIFETIME tools: aspy_lifetime, aspy_embeddings
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
 * 2. ANTHROPIC_API_KEY/AUTH_TOKEN hash - Fallback for bare URL users
 */
let cachedUserId: string | null = null;

function getUserId(): string | null {
  if (cachedUserId !== null) return cachedUserId;

  // Priority 1: Explicit client ID
  if (process.env.ASPY_CLIENT_ID) {
    cachedUserId = process.env.ASPY_CLIENT_ID;
    return cachedUserId;
  }

  // Priority 2: API key hash
  const authToken =
    process.env.ANTHROPIC_API_KEY || process.env.ANTHROPIC_AUTH_TOKEN;

  if (!authToken) {
    return null;
  }

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

// Recall/Memory types
type MatchType = "thinking" | "user_prompt" | "assistant_response";

interface ContextMatch {
  match_type: MatchType;
  session_id: string | null;
  timestamp: string;
  content: string;
  rank: number;
}

interface HybridContextResponse {
  [key: string]: unknown;
  topic: string;
  mode: string;
  search_type: string; // "fts_only" or "hybrid"
  results: ContextMatch[];
}

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

// Lifetime types
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

interface EmbeddingStatusResponse {
  [key: string]: unknown;
  enabled: boolean;
  provider: string;
  model: string;
  dimensions: number;
  documents_embedded: number;
  documents_total: number;
  progress_pct: number;
}

// ============================================================================
// Helpers
// ============================================================================

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

function truncateContent(content: string, maxLen: number = 200): string {
  if (content.length <= maxLen) return content;
  return content.slice(0, maxLen) + "...";
}

// ============================================================================
// MCP Server
// ============================================================================

const server = new McpServer({
  name: "aspy",
  version: "0.2.0",
});

// ============================================================================
// SESSION TOOLS - Current session data
// ============================================================================

// Tool: aspy_stats - Session statistics
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

// Tool: aspy_events - Session events
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

    return {
      content: [{ type: "text" as const, text: JSON.stringify(result.data, null, 2) }],
      structuredContent: result.data,
    };
  }
);

// Tool: aspy_window - Context window gauge (renamed from aspy_context)
server.registerTool(
  "aspy_window",
  {
    title: "Context Window",
    description:
      "Check context window usage percentage, warning level, and compact count. Use this to monitor how full your context is.",
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

    const statusEmoji = {
      normal: "🟢",
      warning: "🟡",
      high: "🟠",
      critical: "🔴",
    }[output.warning_level];

    const summary = `${statusEmoji} Context Window: ${Math.floor(output.usage_pct)}% (${Math.floor(output.current_tokens / 1000)}K / ${Math.floor(output.limit_tokens / 1000)}K)`;

    return {
      content: [
        { type: "text" as const, text: summary },
        { type: "text" as const, text: JSON.stringify(output, null, 2) },
      ],
      structuredContent: output,
    };
  }
);

// Tool: aspy_sessions - List all sessions
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

// Tool: aspy_whoami - Current user identity and session
server.registerTool(
  "aspy_whoami",
  {
    title: "Who Am I",
    description:
      "Get your current user identity and active session info. Shows your API key hash, current session ID, session status, and when it started. Useful for debugging multi-user scenarios.",
    inputSchema: {},
    outputSchema: {
      user_id: z.string(),
      session_id: z.string().nullable(),
      claude_session_id: z.string().nullable(),
      session_started: z.string().nullable(),
      session_source: z.string().nullable(),
      session_status: z.string().nullable(),
      transcript_path: z.string().nullable(),
    },
  },
  async () => {
    const userId = getUserId();

    interface WhoamiResponse {
      [key: string]: unknown;
      user_id: string;
      session_id?: string;
      claude_session_id?: string;
      session_started?: string;
      session_source?: string;
      session_status?: string;
      transcript_path?: string;
    }

    // Build headers with authentication for user identification
    const headers: Record<string, string> = {};
    const authToken =
      process.env.ANTHROPIC_API_KEY || process.env.ANTHROPIC_AUTH_TOKEN;
    if (authToken) {
      if (process.env.ANTHROPIC_API_KEY) {
        headers["x-api-key"] = authToken;
      } else {
        headers["Authorization"] = `Bearer ${authToken}`;
      }
    }

    try {
      const response = await fetch(`${API_BASE}/api/whoami`, { headers });

      if (!response.ok) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Error: HTTP ${response.status}: ${response.statusText}`,
            },
          ],
          isError: true,
        };
      }

      const data = (await response.json()) as WhoamiResponse;

      const summaryParts = [`🔑 **You are:** ${data.user_id}`];

      if (data.session_id) {
        summaryParts.push(`📋 **Session:** ${data.session_id}`);
        summaryParts.push(`   Status: ${data.session_status || "unknown"}`);
        summaryParts.push(`   Source: ${data.session_source || "unknown"}`);
        if (data.session_started) {
          const started = new Date(data.session_started);
          summaryParts.push(`   Started: ${started.toLocaleString()}`);
        }
      } else {
        summaryParts.push("📋 **Session:** None active");
        if (userId) {
          summaryParts.push(
            `\n💡 Your user ID is ${userId.slice(0, 8)}... - session will be created on first API request.`
          );
        }
      }

      return {
        content: [
          { type: "text" as const, text: summaryParts.join("\n") },
          { type: "text" as const, text: JSON.stringify(data, null, 2) },
        ],
        structuredContent: data,
      };
    } catch (err) {
      const message = err instanceof Error ? err.message : "Unknown error";
      return {
        content: [
          { type: "text" as const, text: `Error: Failed to connect to Aspy: ${message}` },
        ],
        isError: true,
      };
    }
  }
);

// Tool: aspy_session_history - List past sessions
server.registerTool(
  "aspy_session_history",
  {
    title: "Session History",
    description:
      "Get a list of your past Claude Code sessions. Shows when sessions started/ended, their source, and statistics. Useful for finding previous work sessions or recovering context from past sessions.",
    inputSchema: {
      limit: z
        .number()
        .min(1)
        .max(100)
        .default(10)
        .describe("Maximum sessions to return (default: 10)"),
      offset: z
        .number()
        .min(0)
        .default(0)
        .describe("Skip first N sessions (for pagination)"),
    },
    outputSchema: {
      user_id: z.string(),
      count: z.number(),
      has_more: z.boolean(),
      sessions: z.array(
        z.object({
          session_id: z.string(),
          started: z.string(),
          ended: z.string().nullable(),
          source: z.string(),
          end_reason: z.string().nullable(),
          stats: z.object({
            requests: z.number(),
            tool_calls: z.number(),
            cost_usd: z.number(),
          }),
        })
      ),
    },
  },
  async ({ limit = 10, offset = 0 }) => {
    interface SessionHistoryItem {
      session_id: string;
      user_id: string;
      claude_session_id?: string;
      started: string;
      ended?: string;
      source: string;
      end_reason?: string;
      transcript_path?: string;
      stats: {
        requests: number;
        tool_calls: number;
        input_tokens: number;
        output_tokens: number;
        cost_usd: number;
      };
    }

    interface SessionHistoryResponse {
      [key: string]: unknown;
      user_id: string;
      count: number;
      has_more: boolean;
      sessions: SessionHistoryItem[];
    }

    // Build headers with authentication
    const headers: Record<string, string> = {};
    const authToken =
      process.env.ANTHROPIC_API_KEY || process.env.ANTHROPIC_AUTH_TOKEN;
    if (authToken) {
      if (process.env.ANTHROPIC_API_KEY) {
        headers["x-api-key"] = authToken;
      } else {
        headers["Authorization"] = `Bearer ${authToken}`;
      }
    }

    const params = new URLSearchParams();
    params.set("limit", String(limit));
    params.set("offset", String(offset));

    try {
      const response = await fetch(
        `${API_BASE}/api/session-history?${params}`,
        { headers }
      );

      if (!response.ok) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Error: HTTP ${response.status}: ${response.statusText}`,
            },
          ],
          isError: true,
        };
      }

      const data = (await response.json()) as SessionHistoryResponse;

      const summaryParts = [
        `📜 **Session History** (${data.count} session${data.count !== 1 ? "s" : ""})`,
      ];

      if (data.count === 0) {
        summaryParts.push("\nNo sessions found.");
      } else {
        summaryParts.push("");
        for (const s of data.sessions) {
          const date = s.started.split("T")[0];
          const time = s.started.split("T")[1]?.slice(0, 5) || "";
          const status = s.ended ? "✓" : "▶";
          const endInfo = s.ended
            ? ` (ended: ${s.end_reason || "unknown"})`
            : " (active)";

          summaryParts.push(
            `${status} **[${date} ${time}]** ${s.session_id.slice(0, 12)}...${endInfo}`
          );
          summaryParts.push(
            `   ${s.stats.tool_calls} tools, $${s.stats.cost_usd.toFixed(3)}`
          );
        }

        if (data.has_more) {
          summaryParts.push(
            `\n_More sessions available. Use offset=${offset + limit} to see next page._`
          );
        }
      }

      return {
        content: [
          { type: "text" as const, text: summaryParts.join("\n") },
          { type: "text" as const, text: JSON.stringify(data, null, 2) },
        ],
        structuredContent: data,
      };
    } catch (err) {
      const message = err instanceof Error ? err.message : "Unknown error";
      return {
        content: [
          { type: "text" as const, text: `Error: Failed to connect to Aspy: ${message}` },
        ],
        isError: true,
      };
    }
  }
);

// Tool: aspy_context_breakdown - Detailed context content analysis
server.registerTool(
  "aspy_context_breakdown",
  {
    title: "Context Breakdown",
    description:
      "Get a detailed breakdown of what's consuming your context window. Answers 'Why is my context so high?' by showing: tool_results (file contents, grep output), thinking (Claude's reasoning), text (conversation), and system prompt sizes.",
    inputSchema: {},
    outputSchema: {
      available: z.boolean(),
      message_count: z.number(),
      summary: z.string(),
      breakdown: z.object({
        tool_results: z.object({
          count: z.number(),
          chars: z.number(),
          estimated_tokens: z.number(),
          pct: z.number(),
        }),
        tool_inputs: z.object({
          count: z.number(),
          chars: z.number(),
          estimated_tokens: z.number(),
          pct: z.number(),
        }),
        thinking: z.object({
          chars: z.number(),
          estimated_tokens: z.number(),
          pct: z.number(),
        }),
        text: z.object({
          chars: z.number(),
          estimated_tokens: z.number(),
          pct: z.number(),
        }),
        system: z.object({
          chars: z.number(),
          estimated_tokens: z.number(),
          pct: z.number(),
        }),
      }),
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

    interface ContextSnapshotResponse {
      [key: string]: unknown;
      user_id: string;
      available: boolean;
      message_count: number;
      summary: string;
      breakdown: {
        tool_results: { count: number; chars: number; estimated_tokens: number; pct: number };
        tool_inputs: { count: number; chars: number; estimated_tokens: number; pct: number };
        thinking: { count: number; chars: number; estimated_tokens: number; pct: number };
        text: { count: number; chars: number; estimated_tokens: number; pct: number };
        system: { count: number; chars: number; estimated_tokens: number; pct: number };
      };
    }

    const result = await fetchApi<ContextSnapshotResponse>(
      `/api/context/snapshot?user=${userId}`
    );

    if (!result.ok) {
      return {
        content: [{ type: "text" as const, text: `Error: ${result.error.error}` }],
        isError: true,
      };
    }

    const data = result.data;

    if (!data.available) {
      return {
        content: [
          {
            type: "text" as const,
            text: "📊 **Context Breakdown**: No snapshot available yet. Try after the first API request.",
          },
        ],
        structuredContent: data,
      };
    }

    // Build human-readable summary with visual bars
    const b = data.breakdown;
    const summaryParts = [
      `📊 **Context Breakdown** (${data.message_count} messages)\n`,
      data.summary,
      "",
      "**By Category:**",
    ];

    // Sort categories by percentage for display
    const categories = [
      { name: "Tool Results", pct: b.tool_results.pct, tokens: b.tool_results.estimated_tokens, count: b.tool_results.count },
      { name: "Tool Inputs", pct: b.tool_inputs.pct, tokens: b.tool_inputs.estimated_tokens, count: b.tool_inputs.count },
      { name: "Thinking", pct: b.thinking.pct, tokens: b.thinking.estimated_tokens, count: 0 },
      { name: "Text", pct: b.text.pct, tokens: b.text.estimated_tokens, count: 0 },
      { name: "System", pct: b.system.pct, tokens: b.system.estimated_tokens, count: 0 },
    ].filter((c) => c.pct > 0.1);

    categories.sort((a, b) => b.pct - a.pct);

    for (const cat of categories) {
      const bar = "█".repeat(Math.ceil(cat.pct / 5)) + "░".repeat(20 - Math.ceil(cat.pct / 5));
      const countStr = cat.count > 0 ? ` (${cat.count} items)` : "";
      summaryParts.push(
        `  ${cat.name.padEnd(13)} ${bar} ${cat.pct.toFixed(1)}% (~${Math.round(cat.tokens / 1000)}K tokens)${countStr}`
      );
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

// Tool: aspy_todos - Get tracked todos from session
server.registerTool(
  "aspy_todos",
  {
    title: "Session Todos",
    description:
      "Get the current todo list state from your Claude Code session. Aspy intercepts TodoWrite tool calls to track what you're working on. Useful for context recovery - todo items are natural keywords!",
    inputSchema: {},
    outputSchema: {
      user_id: z.string(),
      updated: z.string().nullable(),
      count: z.number(),
      summary: z.object({
        pending: z.number(),
        in_progress: z.number(),
        completed: z.number(),
      }),
      todos: z.array(
        z.object({
          content: z.string(),
          status: z.string(),
          activeForm: z.string(),
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

    const result = await fetchApi<{
      user_id: string;
      updated: string | null;
      count: number;
      summary: { pending: number; in_progress: number; completed: number };
      todos: Array<{ content: string; status: string; activeForm: string }>;
    }>(`/api/session/${userId}/todos`);

    if (!result.ok) {
      // If 404, return empty todos (no session yet)
      if (result.error.status === 404) {
        return {
          content: [
            {
              type: "text" as const,
              text: "📋 No todos tracked yet (session not found or no TodoWrite calls intercepted)",
            },
          ],
          structuredContent: {
            user_id: userId,
            updated: null,
            count: 0,
            summary: { pending: 0, in_progress: 0, completed: 0 },
            todos: [],
          },
        };
      }
      return {
        content: [{ type: "text" as const, text: `Error: ${result.error.error}` }],
        isError: true,
      };
    }

    const data = result.data;

    // Build human-readable summary
    const summaryParts: string[] = [];

    if (data.count === 0) {
      summaryParts.push("📋 **No todos tracked** (no TodoWrite calls intercepted yet)");
    } else {
      summaryParts.push(`📋 **Todos** (${data.count} total)`);
      summaryParts.push(
        `   ⏳ Pending: ${data.summary.pending} | 🔄 In Progress: ${data.summary.in_progress} | ✅ Completed: ${data.summary.completed}`
      );

      // Show in-progress items prominently
      const inProgress = data.todos.filter((t) => t.status === "in_progress");
      if (inProgress.length > 0) {
        summaryParts.push("\n**Currently working on:**");
        for (const todo of inProgress) {
          summaryParts.push(`   🔄 ${todo.activeForm}`);
        }
      }

      // Show pending items
      const pending = data.todos.filter((t) => t.status === "pending");
      if (pending.length > 0) {
        summaryParts.push("\n**Pending:**");
        for (const todo of pending) {
          summaryParts.push(`   ⏳ ${todo.content}`);
        }
      }
    }

    if (data.updated) {
      const updatedTime = new Date(data.updated).toLocaleTimeString();
      summaryParts.push(`\n_Last updated: ${updatedTime}_`);
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
// MEMORY TOOLS - Cross-session recall (search past sessions)
// ============================================================================

// Tool: aspy_recall - PRIMARY memory search (semantic + keyword hybrid)
server.registerTool(
  "aspy_recall",
  {
    title: "Recall Memory",
    description:
      "Search your memory across all past sessions. Uses semantic search (if embeddings enabled) combined with keyword matching. This is THE tool for recovering lost context - handles fuzzy queries like 'that thing about golf and nature' as well as exact matches.",
    inputSchema: {
      query: z.string().min(2).describe("What to search for - can be fuzzy or exact"),
      limit: z
        .number()
        .min(1)
        .max(50)
        .default(10)
        .describe("Maximum results (default: 10)"),
    },
    outputSchema: {
      query: z.string(),
      search_type: z.string(),
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
  async ({ query, limit = 10 }) => {
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
    params.set("topic", query);
    params.set("limit", String(limit));
    params.set("mode", "phrase");

    // Always use hybrid endpoint - it auto-falls back to FTS if no embeddings
    const result = await fetchApi<HybridContextResponse>(
      `/api/lifestats/context/hybrid/user/${userId}?${params}`
    );

    if (!result.ok) {
      return {
        content: [{ type: "text" as const, text: `Error: ${result.error.error}` }],
        isError: true,
      };
    }

    const data = result.data;

    // Build human-readable summary
    const searchIcon = data.search_type === "hybrid" ? "🧠" : "📚";
    const searchLabel = data.search_type === "hybrid" ? "Semantic + Keyword" : "Keyword only";
    const summaryParts = [
      `${searchIcon} **Recall** (${searchLabel}): Found ${data.results.length} match(es) for "${data.topic}"`,
    ];

    if (data.results.length === 0) {
      summaryParts.push("\nNo matches found. Try different keywords or broader terms.");
    } else {
      summaryParts.push("");
      for (const r of data.results) {
        const session = r.session_id?.slice(0, 8) ?? "unknown";
        const date = r.timestamp.split("T")[0];
        const typeLabel = formatMatchType(r.match_type);
        summaryParts.push(
          `${typeLabel} **[${date}]** (session: ${session})`
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

// Tool: aspy_recall_thinking - Search Claude's reasoning only
server.registerTool(
  "aspy_recall_thinking",
  {
    title: "Recall Thinking",
    description:
      "Search Claude's past thinking blocks (internal reasoning). Use when you need to find WHY something was decided or HOW a problem was analyzed.",
    inputSchema: {
      query: z.string().min(2).describe("Search query"),
      limit: z
        .number()
        .min(1)
        .max(100)
        .default(10)
        .describe("Maximum results (default: 10)"),
    },
    outputSchema: {
      query: z.string(),
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
  async ({ query, limit = 10 }) => {
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
    params.set("q", query);
    params.set("limit", String(limit));
    params.set("mode", "phrase");

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

    const summaryParts = [
      `💭 Found ${data.results.length} thinking block(s) for "${data.query}":`,
    ];

    if (data.results.length === 0) {
      summaryParts.push("\nNo matches found.");
    } else {
      summaryParts.push("");
      for (const r of data.results) {
        const session = r.session_id?.slice(0, 8) ?? "unknown";
        const date = r.timestamp.split("T")[0];
        summaryParts.push(`**[${date}]** (session: ${session})`);
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

// Tool: aspy_recall_prompts - Search user questions only
server.registerTool(
  "aspy_recall_prompts",
  {
    title: "Recall Prompts",
    description:
      "Search your past prompts/questions. Use when you need to find what YOU asked previously.",
    inputSchema: {
      query: z.string().min(2).describe("Search query"),
      limit: z
        .number()
        .min(1)
        .max(100)
        .default(10)
        .describe("Maximum results (default: 10)"),
    },
    outputSchema: {
      query: z.string(),
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
  async ({ query, limit = 10 }) => {
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
    params.set("q", query);
    params.set("limit", String(limit));
    params.set("mode", "phrase");

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

    const summaryParts = [
      `👤 Found ${data.results.length} prompt(s) for "${data.query}":`,
    ];

    if (data.results.length === 0) {
      summaryParts.push("\nNo matches found.");
    } else {
      summaryParts.push("");
      for (const r of data.results) {
        const session = r.session_id?.slice(0, 8) ?? "unknown";
        const date = r.timestamp.split("T")[0];
        summaryParts.push(`**[${date}]** (session: ${session})`);
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

// Tool: aspy_recall_responses - Search Claude's answers only
server.registerTool(
  "aspy_recall_responses",
  {
    title: "Recall Responses",
    description:
      "Search Claude's past responses. Use when you need to find previous explanations, code, or answers.",
    inputSchema: {
      query: z.string().min(2).describe("Search query"),
      limit: z
        .number()
        .min(1)
        .max(100)
        .default(10)
        .describe("Maximum results (default: 10)"),
    },
    outputSchema: {
      query: z.string(),
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
  async ({ query, limit = 10 }) => {
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
    params.set("q", query);
    params.set("limit", String(limit));
    params.set("mode", "phrase");

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

    const summaryParts = [
      `🤖 Found ${data.results.length} response(s) for "${data.query}":`,
    ];

    if (data.results.length === 0) {
      summaryParts.push("\nNo matches found.");
    } else {
      summaryParts.push("");
      for (const r of data.results) {
        const session = r.session_id?.slice(0, 8) ?? "unknown";
        const date = r.timestamp.split("T")[0];
        summaryParts.push(`**[${date}]** (session: ${session})`);
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

// ============================================================================
// LIFETIME TOOLS - All-time statistics and configuration
// ============================================================================

// Tool: aspy_lifetime - All-time usage statistics
server.registerTool(
  "aspy_lifetime",
  {
    title: "Lifetime Statistics",
    description:
      "Get your all-time usage statistics across all sessions: total tokens, costs, tool usage, model breakdown. Your personal Claude Code history summary.",
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

    const summaryParts = ["📊 **Your Claude Code Lifetime Stats**\n"];

    summaryParts.push(`**Sessions:** ${stats.total_sessions}`);
    summaryParts.push(`**Total Tokens:** ${(stats.total_tokens / 1_000_000).toFixed(2)}M`);
    summaryParts.push(`**Total Cost:** $${stats.total_cost_usd.toFixed(2)}`);
    summaryParts.push(`**Tool Calls:** ${stats.total_tool_calls.toLocaleString()}`);
    summaryParts.push(`**Thinking Blocks:** ${stats.total_thinking_blocks.toLocaleString()}`);

    if (stats.first_session && stats.last_session) {
      const first = stats.first_session.split("T")[0];
      const last = stats.last_session.split("T")[0];
      summaryParts.push(`\n**Time Range:** ${first} → ${last}`);
    }

    if (stats.by_model.length > 0) {
      summaryParts.push("\n**By Model:**");
      for (const m of stats.by_model.slice(0, 5)) {
        summaryParts.push(
          `  - ${m.model}: ${(m.tokens / 1_000_000).toFixed(2)}M tokens, $${m.cost_usd.toFixed(2)}`
        );
      }
    }

    if (stats.by_tool.length > 0) {
      summaryParts.push("\n**Top Tools:**");
      for (const t of stats.by_tool.slice(0, 5)) {
        summaryParts.push(
          `  - ${t.tool}: ${t.calls} calls (${(t.success_rate * 100).toFixed(0)}% success)`
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

// Tool: aspy_embeddings - Embedding indexer status
server.registerTool(
  "aspy_embeddings",
  {
    title: "Embeddings Status",
    description:
      "Check if semantic search is enabled and indexing progress. Embeddings power fuzzy memory recall - 'that thing about golf?' works when embeddings are enabled.",
    inputSchema: {},
    outputSchema: {
      enabled: z.boolean(),
      provider: z.string(),
      model: z.string(),
      dimensions: z.number(),
      documents_embedded: z.number(),
      documents_total: z.number(),
      progress_pct: z.number(),
    },
  },
  async () => {
    const result = await fetchApi<EmbeddingStatusResponse>("/api/lifestats/embeddings/status");

    if (!result.ok) {
      return {
        content: [{ type: "text" as const, text: `Error: ${result.error.error}` }],
        isError: true,
      };
    }

    const data = result.data;

    const statusIcon = data.enabled ? "🧠" : "📚";
    const statusLabel = data.enabled ? "Enabled" : "Disabled (keyword-only)";
    const summaryParts = [`${statusIcon} **Semantic Search: ${statusLabel}**\n`];

    if (data.enabled) {
      summaryParts.push(`Provider: ${data.provider}`);
      summaryParts.push(`Model: ${data.model}`);
      summaryParts.push(`\n**Indexing:** ${data.documents_embedded} / ${data.documents_total} (${data.progress_pct.toFixed(1)}%)`);
    } else {
      summaryParts.push("Fuzzy queries like 'that golf thing?' won't work as well.");
      summaryParts.push("\n💡 To enable, add to config.toml:");
      summaryParts.push("```toml");
      summaryParts.push("[embeddings]");
      summaryParts.push('provider = "openai"');
      summaryParts.push("```");
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
