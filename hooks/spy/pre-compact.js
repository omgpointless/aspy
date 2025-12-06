#!/usr/bin/env node
/**
 * PreCompact hook: Notify aspy proxy before context compaction
 *
 * Called when Claude Code is about to compact (manual /compact or auto).
 * Creates a timeline marker event in aspy's TUI for tracking compact flow.
 *
 * Input (stdin): JSON with trigger, session_id, transcript_path, etc.
 * Output: JSON (optional, not blocking)
 */

import { createHash } from 'crypto';

const ASPY_API_URL = process.env.ASPY_API_URL || 'http://127.0.0.1:8080';

async function main() {
  // Read stdin
  const chunks = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk);
  }
  const input = Buffer.concat(chunks).toString('utf8');

  let hookData = {};
  try {
    hookData = JSON.parse(input);
  } catch {
    // Use defaults if parse fails
  }

  const trigger = hookData.trigger || 'manual';

  // Compute user_id from API key or OAuth token (SHA-256, first 16 chars)
  let userId = process.env.ASPY_CLIENT_ID;

  if (!userId) {
    const apiKey = process.env.ANTHROPIC_API_KEY;
    const authToken = process.env.ANTHROPIC_AUTH_TOKEN;

    if (apiKey) {
      userId = createHash('sha256').update(apiKey).digest('hex').slice(0, 16);
    } else if (authToken) {
      userId = createHash('sha256').update(authToken).digest('hex').slice(0, 16);
    }
  }

  if (!userId) {
    // Can't determine user, skip silently
    process.exit(0);
  }

  // Send precompact notification to proxy
  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);

    const response = await fetch(`${ASPY_API_URL}/api/hook/precompact`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ user_id: userId, trigger }),
      signal: controller.signal,
    });

    clearTimeout(timeout);

    if (response.ok) {
      // Output message for user (shown in Claude Code)
      console.log(JSON.stringify({
        systemMessage: `Aspy: PreCompact (${trigger})`,
      }));
    }
  } catch {
    // Silently ignore - don't block compact if proxy is down
  }

  process.exit(0);
}

main();
