#!/bin/bash
# SessionStart hook: Register session with anthropic-spy proxy
#
# Called when Claude Code starts a new session. Sends session info to the
# proxy so it can track sessions per-user and provide filtered stats.
#
# Input (stdin): JSON with session_id, source, etc.
# Output: JSON with optional systemMessage

# Read stdin (session info from Claude Code)
SESSION_DATA=$(cat)

# Extract session_id and source from hook input
SESSION_ID=$(echo "$SESSION_DATA" | jq -r '.session_id // empty' 2>/dev/null)
SOURCE=$(echo "$SESSION_DATA" | jq -r '.source // "startup"' 2>/dev/null)

# Validate we got a session_id
if [ -z "$SESSION_ID" ] || [ "$SESSION_ID" = "null" ]; then
    # No session_id - can't register
    exit 0
fi

# Compute user_id from API key or OAuth token (SHA-256, first 16 chars)
# This must match the proxy's hashing algorithm
if [ -n "$ANTHROPIC_API_KEY" ]; then
    USER_ID=$(echo -n "$ANTHROPIC_API_KEY" | sha256sum | cut -c1-16)
elif [ -n "$ANTHROPIC_AUTH_TOKEN" ]; then
    # Hash OAuth token for subscription users
    USER_ID=$(echo -n "$ANTHROPIC_AUTH_TOKEN" | sha256sum | cut -c1-16)
else
    # No identity available - proxy will backfill from headers
    USER_ID="unknown"
fi

# Proxy API endpoint (configurable via env, default localhost)
ASPY_API_URL="${ASPY_API_URL:-http://127.0.0.1:8080}"

# Send session start to proxy (fire-and-forget, don't block on failure)
RESPONSE=$(curl -s -X POST "${ASPY_API_URL}/api/session/start" \
    -H "Content-Type: application/json" \
    -d "{\"session_id\": \"${SESSION_ID}\", \"user_id\": \"${USER_ID}\", \"source\": \"hook\"}" \
    --connect-timeout 2 \
    --max-time 5 \
    2>/dev/null)

# Check if curl succeeded and got a response
if [ $? -eq 0 ] && [ -n "$RESPONSE" ]; then
    # Parse response to check success
    SUCCESS=$(echo "$RESPONSE" | jq -r '.success // false' 2>/dev/null)
    if [ "$SUCCESS" = "true" ]; then
        # Return context for Claude (optional)
        # IMPORTANT: hookEventName must be inside hookSpecificOutput for SessionStart hooks
        jq -n --arg user "${USER_ID:0:8}" '{
          hookSpecificOutput: {
            hookEventName: "SessionStart",
            additionalContext: ("Session tracked by anthropic-spy (user: " + $user + ")")
          }
        }'
    fi
fi

# Always exit 0 - don't block Claude Code if proxy is down
exit 0
