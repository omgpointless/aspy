#!/usr/bin/env node
/**
 * UserPromptSubmit hook: Reconnect session on first message after proxy restart
 *
 * This hook fires on every user message. Its main purpose is to send the
 * transcript_path to Aspy, enabling session reconnection when the proxy
 * restarts while Claude Code is still running.
 *
 * Flow:
 * 1. Proxy restarts (CC still running, no SessionStart hook fires)
 * 2. User sends message
 * 3. This hook fires with transcript_path
 * 4. Aspy checks DB: "Have I seen this transcript before?"
 * 5. If yes: reconnect to existing session instead of creating new implicit one
 *
 * Input (stdin): JSON with session_id, transcript_path, prompt, etc.
 * Output: JSON (optional systemMessage)
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

  let hookData;
  try {
    hookData = JSON.parse(input);
  } catch {
    process.exit(0);
  }

  const transcriptPath = hookData.transcript_path;
  const sessionId = hookData.session_id;

  if (!transcriptPath) {
    // No transcript path, nothing to reconnect
    process.exit(0);
  }

  // Compute user_id from API key (same as session-start.js)
  let userId = 'unknown';
  const apiKey = process.env.ANTHROPIC_API_KEY;
  const authToken = process.env.ANTHROPIC_AUTH_TOKEN;

  if (apiKey) {
    userId = createHash('sha256').update(apiKey).digest('hex').slice(0, 16);
  } else if (authToken) {
    userId = createHash('sha256').update(authToken).digest('hex').slice(0, 16);
  }

  // Send reconnect request to Aspy (fire-and-forget, don't block Claude)
  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 3000);

    const response = await fetch(`${ASPY_API_URL}/api/session/reconnect`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        user_id: userId,
        transcript_path: transcriptPath,
        session_id: sessionId,
      }),
      signal: controller.signal,
    });

    clearTimeout(timeout);

    if (response.ok) {
      const data = await response.json();
      if (data.reconnected) {
        // Optionally inform Claude about reconnection
        console.log(JSON.stringify({
          hookSpecificOutput: {
            hookEventName: 'UserPromptSubmit',
            additionalContext: `Session reconnected: ${data.session_id} (${data.message})`,
          },
        }));
      }
    }
  } catch {
    // Silently ignore - don't block Claude if proxy is down or slow
  }

  process.exit(0);
}

main();
