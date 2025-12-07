/**
 * Lifetime Tools
 *
 * MCP tools for all-time statistics and configuration.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";

import { fetchApi, errorContent, successContent } from "../client/api.js";
import type { LifetimeStats, EmbeddingStatusResponse } from "../types/api.js";
import { getUserId } from "../utils/identity.js";
import { formatTokens, formatDate } from "../utils/format.js";

/**
 * Register all lifetime-related tools with the MCP server.
 */
export function registerLifetimeTools(server: McpServer): void {
  registerLifetime(server);
  registerEmbeddings(server);
}

// ============================================================================
// aspy_lifetime
// ============================================================================

function registerLifetime(server: McpServer): void {
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
        return errorContent(
          "Cannot determine user identity. Ensure ANTHROPIC_API_KEY is set."
        );
      }

      const result = await fetchApi<LifetimeStats>(
        `/api/lifestats/stats/user/${userId}`
      );

      if (!result.ok) {
        return errorContent(result.error.error);
      }

      const stats = result.data;

      const summaryParts = ["📊 **Your Claude Code Lifetime Stats**\n"];

      summaryParts.push(`**Sessions:** ${stats.total_sessions}`);
      summaryParts.push(`**Total Tokens:** ${formatTokens(stats.total_tokens)}`);
      summaryParts.push(`**Total Cost:** $${stats.total_cost_usd.toFixed(2)}`);
      summaryParts.push(`**Tool Calls:** ${stats.total_tool_calls.toLocaleString()}`);
      summaryParts.push(`**Thinking Blocks:** ${stats.total_thinking_blocks.toLocaleString()}`);

      if (stats.first_session && stats.last_session) {
        const first = formatDate(stats.first_session);
        const last = formatDate(stats.last_session);
        summaryParts.push(`\n**Time Range:** ${first} → ${last}`);
      }

      if (stats.by_model.length > 0) {
        summaryParts.push("\n**By Model:**");
        for (const m of stats.by_model.slice(0, 5)) {
          summaryParts.push(
            `  - ${m.model}: ${formatTokens(m.tokens)} tokens, $${m.cost_usd.toFixed(2)}`
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

      return successContent(summaryParts.join("\n"), stats);
    }
  );
}

// ============================================================================
// aspy_embeddings
// ============================================================================

function registerEmbeddings(server: McpServer): void {
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
        return errorContent(result.error.error);
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

      return successContent(summaryParts.join("\n"), data);
    }
  );
}
