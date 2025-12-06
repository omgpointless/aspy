#!/usr/bin/env node
/**
 * SessionStart hook: Register session with aspy proxy
 *
 * Called when Claude Code starts a new session. Sends session info to the
 * proxy so it can track sessions per-user and provide filtered stats.
 *
 * Input (stdin): JSON with session_id, source, etc.
 * Output: JSON with optional systemMessage
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

  let sessionData;
  try {
    sessionData = JSON.parse(input);
  } catch {
    process.exit(0);
  }

  const sessionId = sessionData.session_id;
  const source = sessionData.source || 'startup';
  const transcriptPath = sessionData.transcript_path || null;

  if (!sessionId) {
    process.exit(0);
  }

  // Compute user_id from API key or OAuth token (SHA-256, first 16 chars)
  let userId = 'unknown';
  const apiKey = process.env.ANTHROPIC_API_KEY;
  const authToken = process.env.ANTHROPIC_AUTH_TOKEN;

  if (apiKey) {
    userId = createHash('sha256').update(apiKey).digest('hex').slice(0, 16);
  } else if (authToken) {
    userId = createHash('sha256').update(authToken).digest('hex').slice(0, 16);
  }

  // Send session start to proxy (fire-and-forget, don't block on failure)
  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);

    const response = await fetch(`${ASPY_API_URL}/api/session/start`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        session_id: sessionId,
        user_id: userId,
        source: 'hook',
        transcript_path: transcriptPath,
      }),
      signal: controller.signal,
    });

    clearTimeout(timeout);

    if (response.ok) {
      const data = await response.json();
      if (data.success) {
        // Return context for Claude
        console.log(JSON.stringify({
          hookSpecificOutput: {
            hookEventName: 'SessionStart',
            additionalContext: `Session tracked by aspy (user: ${userId.slice(0, 8)})`,
          },
        }));
      }
    }
  } catch {
    // Silently ignore - don't block Claude Code if proxy is down
  }

  process.exit(0);
}

main();
