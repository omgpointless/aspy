/**
 * User Identity Management
 *
 * Handles user identification for multi-user session isolation.
 */

import { createHash } from "crypto";

// Cached user ID to avoid repeated computation
let cachedUserId: string | null = null;

/**
 * Get user ID for session isolation.
 *
 * Priority order:
 * 1. ASPY_CLIENT_ID - Explicit client ID (matches proxy's URL path routing)
 * 2. ANTHROPIC_API_KEY/AUTH_TOKEN hash - Fallback for bare URL users
 *
 * @returns User ID string or null if identity cannot be determined
 */
export function getUserId(): string | null {
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

/**
 * Check if user ID was derived from explicit client ID vs API key hash.
 */
export function isExplicitClientId(): boolean {
  return Boolean(process.env.ASPY_CLIENT_ID);
}

/**
 * Get identity type label for display purposes.
 */
export function getIdentityLabel(): string {
  return isExplicitClientId() ? "client_id" : "api_key_hash";
}
