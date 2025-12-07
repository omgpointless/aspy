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

import { registerSessionTools } from "./tools/session.js";
import { registerMemoryTools } from "./tools/memory.js";
import { registerLifetimeTools } from "./tools/lifetime.js";

// ============================================================================
// MCP Server
// ============================================================================

const server = new McpServer({
  name: "aspy",
  version: "0.2.0",
});

// Register all tools
registerSessionTools(server);
registerMemoryTools(server);
registerLifetimeTools(server);

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
