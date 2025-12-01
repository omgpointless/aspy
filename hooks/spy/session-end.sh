#!/bin/bash
# SessionEnd hook: Notify anthropic-spy proxy that session ended
#
# Called when Claude Code session ends (quit, clear, logout, etc).
# Archives the session in the proxy for history tracking.
#
# Input (stdin): JSON with session_id, reason, etc.
# Output: None needed (session is ending anyway)

# Read stdin (session info from Claude Code)
SESSION_DATA=$(cat)

# Extract session_id and reason from hook input
SESSION_ID=$(echo "$SESSION_DATA" | jq -r '.session_id // empty' 2>/dev/null)
REASON=$(echo "$SESSION_DATA" | jq -r '.reason // "other"' 2>/dev/null)

# Validate we got a session_id
if [ -z "$SESSION_ID" ] || [ "$SESSION_ID" = "null" ]; then
    exit 0
fi

# Compute user_id from API key (SHA-256, first 16 chars)
if [ -n "$ANTHROPIC_API_KEY" ]; then
    USER_ID=$(echo -n "$ANTHROPIC_API_KEY" | sha256sum | cut -c1-16)
else
    USER_ID="unknown"
fi

# Proxy API endpoint
ASPY_API_URL="${ASPY_API_URL:-http://127.0.0.1:8080}"

# Send session end to proxy (fire-and-forget)
# Very short timeout since session is ending anyway
curl -s -X POST "${ASPY_API_URL}/api/session/end" \
    -H "Content-Type: application/json" \
    -d "{\"session_id\": \"${SESSION_ID}\", \"user_id\": \"${USER_ID}\", \"reason\": \"${REASON}\"}" \
    --connect-timeout 1 \
    --max-time 2 \
    2>/dev/null &

# Don't wait for curl - session is ending, be fast
exit 0
