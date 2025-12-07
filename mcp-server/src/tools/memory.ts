/**
 * Memory Tools
 *
 * MCP tools for cross-session memory recall and search.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";

import { fetchApi, errorContent, successContent } from "../client/api.js";
import type {
  HybridContextResponse,
  ThinkingSearchResponse,
  PromptSearchResponse,
  ResponseSearchResponse,
  TodoSearchResponse,
} from "../types/api.js";
import { getUserId } from "../utils/identity.js";
import { formatMatchType, truncateContent, formatDate } from "../utils/format.js";

/**
 * Register all memory-related tools with the MCP server.
 */
export function registerMemoryTools(server: McpServer): void {
  registerRecall(server);
  registerRecallThinking(server);
  registerRecallPrompts(server);
  registerRecallResponses(server);
  registerTodosHistory(server);
}

// ============================================================================
// aspy_recall - PRIMARY memory search
// ============================================================================

function registerRecall(server: McpServer): void {
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
        return errorContent(
          "Cannot determine user identity. Ensure ANTHROPIC_API_KEY is set."
        );
      }

      const params = new URLSearchParams();
      params.set("topic", query);
      params.set("limit", String(limit));
      params.set("mode", "phrase");

      // Always use hybrid endpoint - it auto-falls back to FTS if no embeddings
      const result = await fetchApi<HybridContextResponse>(
        `/api/cortex/context/hybrid/user/${userId}?${params}`
      );

      if (!result.ok) {
        return errorContent(result.error.error);
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
          const date = formatDate(r.timestamp);
          const typeLabel = formatMatchType(r.match_type);
          summaryParts.push(`${typeLabel} **[${date}]** (session: ${session})`);
          summaryParts.push(`${truncateContent(r.content)}\n`);
        }
      }

      return successContent(summaryParts.join("\n"), data);
    }
  );
}

// ============================================================================
// aspy_recall_thinking
// ============================================================================

function registerRecallThinking(server: McpServer): void {
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
        return errorContent(
          "Cannot determine user identity. Ensure ANTHROPIC_API_KEY is set."
        );
      }

      const params = new URLSearchParams();
      params.set("q", query);
      params.set("limit", String(limit));
      params.set("mode", "phrase");

      const result = await fetchApi<ThinkingSearchResponse>(
        `/api/cortex/search/user/${userId}/thinking?${params}`
      );

      if (!result.ok) {
        return errorContent(result.error.error);
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
          const date = formatDate(r.timestamp);
          summaryParts.push(`**[${date}]** (session: ${session})`);
          summaryParts.push(`${truncateContent(r.content)}\n`);
        }
      }

      return successContent(summaryParts.join("\n"), data);
    }
  );
}

// ============================================================================
// aspy_recall_prompts
// ============================================================================

function registerRecallPrompts(server: McpServer): void {
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
        return errorContent(
          "Cannot determine user identity. Ensure ANTHROPIC_API_KEY is set."
        );
      }

      const params = new URLSearchParams();
      params.set("q", query);
      params.set("limit", String(limit));
      params.set("mode", "phrase");

      const result = await fetchApi<PromptSearchResponse>(
        `/api/cortex/search/user/${userId}/prompts?${params}`
      );

      if (!result.ok) {
        return errorContent(result.error.error);
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
          const date = formatDate(r.timestamp);
          summaryParts.push(`**[${date}]** (session: ${session})`);
          summaryParts.push(`${truncateContent(r.content)}\n`);
        }
      }

      return successContent(summaryParts.join("\n"), data);
    }
  );
}

// ============================================================================
// aspy_recall_responses
// ============================================================================

function registerRecallResponses(server: McpServer): void {
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
        return errorContent(
          "Cannot determine user identity. Ensure ANTHROPIC_API_KEY is set."
        );
      }

      const params = new URLSearchParams();
      params.set("q", query);
      params.set("limit", String(limit));
      params.set("mode", "phrase");

      const result = await fetchApi<ResponseSearchResponse>(
        `/api/cortex/search/user/${userId}/responses?${params}`
      );

      if (!result.ok) {
        return errorContent(result.error.error);
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
          const date = formatDate(r.timestamp);
          summaryParts.push(`**[${date}]** (session: ${session})`);
          summaryParts.push(`${truncateContent(r.content)}\n`);
        }
      }

      return successContent(summaryParts.join("\n"), data);
    }
  );
}

// ============================================================================
// aspy_todos_history
// ============================================================================

function registerTodosHistory(server: McpServer): void {
  server.registerTool(
    "aspy_todos_history",
    {
      title: "Todo History",
      description:
        "Search your todo history across sessions. Use for 'what was I working on?' queries. Shows past TodoWrite snapshots stored in cortex.",
      inputSchema: {
        query: z
          .string()
          .min(2)
          .optional()
          .describe("Optional search query to find specific todos"),
        days: z
          .number()
          .min(1)
          .max(365)
          .optional()
          .describe("Days to look back (e.g., 1 for today, 7 for this week)"),
        limit: z
          .number()
          .min(1)
          .max(50)
          .default(10)
          .describe("Maximum results (default: 10)"),
      },
      outputSchema: {
        query: z.string().nullable(),
        timeframe: z.string().nullable(),
        results: z.array(
          z.object({
            session_id: z.string().nullable(),
            timestamp: z.string(),
            content: z.string(),
            todos_json: z.string(),
            pending_count: z.number(),
            in_progress_count: z.number(),
            completed_count: z.number(),
            rank: z.number(),
          })
        ),
      },
    },
    async ({ query, days, limit = 10 }) => {
      const params = new URLSearchParams();
      if (query) params.set("q", query);
      if (days) params.set("days", String(days));
      params.set("limit", String(limit));

      const result = await fetchApi<TodoSearchResponse>(
        `/api/cortex/todos?${params}`
      );

      if (!result.ok) {
        return errorContent(result.error.error);
      }

      const data = result.data;

      const summaryParts: string[] = [];
      if (query) {
        summaryParts.push(`📋 Found ${data.results.length} todo snapshot(s) matching "${query}":`);
      } else if (days) {
        summaryParts.push(`📋 Found ${data.results.length} todo snapshot(s) from last ${days} day(s):`);
      } else {
        summaryParts.push(`📋 Found ${data.results.length} recent todo snapshot(s):`);
      }

      if (data.results.length === 0) {
        summaryParts.push("\nNo todo history found.");
      } else {
        summaryParts.push("");
        for (const r of data.results) {
          const session = r.session_id?.slice(0, 8) ?? "unknown";
          const date = formatDate(r.timestamp);
          const time = r.timestamp.split("T")[1]?.slice(0, 5) ?? "";
          summaryParts.push(`**[${date} ${time}]** (session: ${session})`);
          summaryParts.push(`  ⬜ ${r.pending_count} pending | 🔄 ${r.in_progress_count} in progress | ✅ ${r.completed_count} done`);
          if (r.content) {
            summaryParts.push(`  ${r.content.slice(0, 200)}${r.content.length > 200 ? "..." : ""}`);
          }
          summaryParts.push("");
        }
      }

      return successContent(summaryParts.join("\n"), data);
    }
  );
}
