/**
 * Formatting Utilities
 *
 * Helper functions for formatting output in tool responses.
 */

import type { MatchType } from "../types/api.js";

/**
 * Format match type with emoji for human-readable output.
 */
export function formatMatchType(matchType: MatchType): string {
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

/**
 * Truncate content to max length with ellipsis.
 */
export function truncateContent(content: string, maxLen: number = 200): string {
  if (content.length <= maxLen) return content;
  return content.slice(0, maxLen) + "...";
}

/**
 * Format context window status with emoji.
 */
export function formatWarningLevel(
  level: "normal" | "warning" | "high" | "critical"
): string {
  const statusEmoji = {
    normal: "🟢",
    warning: "🟡",
    high: "🟠",
    critical: "🔴",
  }[level];
  return statusEmoji;
}

/**
 * Create a visual progress bar.
 */
export function progressBar(percent: number, width: number = 20): string {
  const filled = Math.ceil(percent / (100 / width));
  const empty = width - filled;
  return "█".repeat(filled) + "░".repeat(empty);
}

/**
 * Format tokens as K or M.
 */
export function formatTokens(tokens: number): string {
  if (tokens >= 1_000_000) {
    return `${(tokens / 1_000_000).toFixed(2)}M`;
  }
  if (tokens >= 1000) {
    return `${Math.floor(tokens / 1000)}K`;
  }
  return String(tokens);
}

/**
 * Format a date string as date only (YYYY-MM-DD).
 */
export function formatDate(isoString: string): string {
  return isoString.split("T")[0];
}

/**
 * Format a date string as time only (HH:MM).
 */
export function formatTime(isoString: string): string {
  return isoString.split("T")[1]?.slice(0, 5) || "";
}
