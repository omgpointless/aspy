/**
 * Session Tools
 *
 * MCP tools for current session data: stats, events, context, sessions, identity.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";

import {
  API_BASE,
  fetchApi,
  errorContent,
  successContent,
} from "../client/api.js";
import type {
  StatsResponse,
  EventsResponse,
  ContextResponse,
  SessionsResponse,
  WhoamiResponse,
  SessionHistoryResponse,
  ContextSnapshotResponse,
  TodosResponse,
} from "../types/api.js";
import { getUserId, getIdentityLabel } from "../utils/identity.js";
import { formatWarningLevel, progressBar, formatTokens } from "../utils/format.js";

/**
 * Register all session-related tools with the MCP server.
 */
export function registerSessionTools(server: McpServer): void {
  registerStats(server);
  registerEvents(server);
  registerWindow(server);
  registerSessions(server);
  registerWhoami(server);
  registerSessionHistory(server);
  registerContextBreakdown(server);
  registerTodos(server);
}

// ============================================================================
// aspy_stats
// ============================================================================

function registerStats(server: McpServer): void {
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
        return errorContent(result.error.error);
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
}

// ============================================================================
// aspy_events
// ============================================================================

function registerEvents(server: McpServer): void {
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
        return errorContent(result.error.error);
      }

      return {
        content: [{ type: "text" as const, text: JSON.stringify(result.data, null, 2) }],
        structuredContent: result.data,
      };
    }
  );
}

// ============================================================================
// aspy_window
// ============================================================================

function registerWindow(server: McpServer): void {
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
        return errorContent(result.error.error);
      }

      const output = result.data;
      const statusEmoji = formatWarningLevel(output.warning_level);
      const summary = `${statusEmoji} Context Window: ${Math.floor(output.usage_pct)}% (${formatTokens(output.current_tokens)} / ${formatTokens(output.limit_tokens)})`;

      return successContent(summary, output);
    }
  );
}

// ============================================================================
// aspy_sessions
// ============================================================================

function registerSessions(server: McpServer): void {
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
        return errorContent(result.error.error);
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

      return successContent(summaryParts.join("\n"), output);
    }
  );
}

// ============================================================================
// aspy_whoami
// ============================================================================

function registerWhoami(server: McpServer): void {
  server.registerTool(
    "aspy_whoami",
    {
      title: "Who Am I",
      description:
        "Get your current user identity and active session info. Shows your user ID (client_id or API key hash), current session ID, session status, and when it started. Useful for debugging multi-user scenarios.",
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
      if (!userId) {
        return errorContent(
          "Cannot determine user identity. Ensure ASPY_CLIENT_ID or ANTHROPIC_API_KEY is set."
        );
      }

      const params = new URLSearchParams();
      params.set("user", userId);

      try {
        const response = await fetch(`${API_BASE}/api/whoami?${params}`);

        if (!response.ok) {
          return errorContent(`HTTP ${response.status}: ${response.statusText}`);
        }

        const data = (await response.json()) as WhoamiResponse;
        const idLabel = getIdentityLabel();
        const summaryParts = [`🔑 **You are:** ${data.user_id} (${idLabel})`];

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
          summaryParts.push(
            `\n💡 Session will be created on first API request through Aspy.`
          );
        }

        return successContent(summaryParts.join("\n"), data);
      } catch (err) {
        const message = err instanceof Error ? err.message : "Unknown error";
        return errorContent(`Failed to connect to Aspy: ${message}`);
      }
    }
  );
}

// ============================================================================
// aspy_session_history
// ============================================================================

function registerSessionHistory(server: McpServer): void {
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
            ended: z.string().nullish(),
            source: z.string(),
            end_reason: z.string().nullish(),
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
      const userId = getUserId();
      if (!userId) {
        return errorContent(
          "Cannot determine user identity. Ensure ASPY_CLIENT_ID or ANTHROPIC_API_KEY is set."
        );
      }

      const params = new URLSearchParams();
      params.set("user", userId);
      params.set("limit", String(limit));
      params.set("offset", String(offset));

      try {
        const response = await fetch(`${API_BASE}/api/session-history?${params}`);

        if (!response.ok) {
          return errorContent(`HTTP ${response.status}: ${response.statusText}`);
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

        return successContent(summaryParts.join("\n"), data);
      } catch (err) {
        const message = err instanceof Error ? err.message : "Unknown error";
        return errorContent(`Failed to connect to Aspy: ${message}`);
      }
    }
  );
}

// ============================================================================
// aspy_context_breakdown
// ============================================================================

function registerContextBreakdown(server: McpServer): void {
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
        return errorContent(
          "Cannot determine user identity. Ensure ANTHROPIC_API_KEY is set."
        );
      }

      const result = await fetchApi<ContextSnapshotResponse>(
        `/api/context/snapshot?user=${userId}`
      );

      if (!result.ok) {
        return errorContent(result.error.error);
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
        const bar = progressBar(cat.pct);
        const countStr = cat.count > 0 ? ` (${cat.count} items)` : "";
        summaryParts.push(
          `  ${cat.name.padEnd(13)} ${bar} ${cat.pct.toFixed(1)}% (~${formatTokens(cat.tokens)} tokens)${countStr}`
        );
      }

      return successContent(summaryParts.join("\n"), data);
    }
  );
}

// ============================================================================
// aspy_todos
// ============================================================================

function registerTodos(server: McpServer): void {
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
        return errorContent(
          "Cannot determine user identity. Ensure ANTHROPIC_API_KEY is set."
        );
      }

      const result = await fetchApi<TodosResponse>(`/api/session/${userId}/todos`);

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
        return errorContent(result.error.error);
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

      return successContent(summaryParts.join("\n"), data);
    }
  );
}
