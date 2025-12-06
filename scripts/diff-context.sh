#!/bin/bash
# diff-context.sh - Compare two Anthropic API request bodies
# Usage: ./diff-context.sh [before.json] [after.json]

BEFORE_ORIG="${1:-/mnt/c/compactdiff/before.json}"
AFTER_ORIG="${2:-/mnt/c/compactdiff/after.json}"
BEFORE="$BEFORE_ORIG"
AFTER="$AFTER_ORIG"

# Auto-detect aspy wrapper format and extract body if needed
# Aspy exports have: {body, body_size, id, session_id, ...}
# Raw Anthropic API has: {messages, system, model, ...}
unwrap_if_aspy() {
    local file="$1"
    if jq -e '.body.messages' "$file" >/dev/null 2>&1; then
        # Aspy wrapper detected - extract body
        jq '.body' "$file"
    else
        # Already raw Anthropic API format
        cat "$file"
    fi
}

# Create temp files with unwrapped content
BEFORE_UNWRAPPED=$(mktemp)
AFTER_UNWRAPPED=$(mktemp)
trap "rm -f $BEFORE_UNWRAPPED $AFTER_UNWRAPPED" EXIT

unwrap_if_aspy "$BEFORE" > "$BEFORE_UNWRAPPED"
unwrap_if_aspy "$AFTER" > "$AFTER_UNWRAPPED"

# Use unwrapped versions for all analysis
BEFORE="$BEFORE_UNWRAPPED"
AFTER="$AFTER_UNWRAPPED"

echo "═══════════════════════════════════════════════════════════════════════════════"
echo "  CONTEXT DIFF: $(basename "$BEFORE_ORIG") → $(basename "$AFTER_ORIG")"
echo "═══════════════════════════════════════════════════════════════════════════════"
echo

# File sizes
SIZE_BEFORE=$(stat -c%s "$BEFORE" 2>/dev/null || stat -f%z "$BEFORE")
SIZE_AFTER=$(stat -c%s "$AFTER" 2>/dev/null || stat -f%z "$AFTER")
DELTA=$((SIZE_AFTER - SIZE_BEFORE))
echo "📦 FILE SIZE"
echo "   Before: $(numfmt --to=iec $SIZE_BEFORE 2>/dev/null || echo "$SIZE_BEFORE bytes")"
echo "   After:  $(numfmt --to=iec $SIZE_AFTER 2>/dev/null || echo "$SIZE_AFTER bytes")"
echo "   Delta:  $DELTA bytes (~$((DELTA / 4)) tokens)"
echo

# Message counts
echo "💬 MESSAGE COUNTS"
echo "   Before:"
jq -r '.messages | group_by(.role) | map("      \(.[0].role): \(length)") | .[]' "$BEFORE"
echo "   After:"
jq -r '.messages | group_by(.role) | map("      \(.[0].role): \(length)") | .[]' "$AFTER"
echo

# Total messages
MSG_BEFORE=$(jq '.messages | length' "$BEFORE")
MSG_AFTER=$(jq '.messages | length' "$AFTER")
echo "   Total: $MSG_BEFORE → $MSG_AFTER (Δ $((MSG_AFTER - MSG_BEFORE)))"
echo

# Tool results analysis
echo "🔧 TOOL RESULTS"
TR_BEFORE=$(jq '[.messages[] | select(.role=="user") | .content[]? | select(.type=="tool_result")] | length' "$BEFORE")
TR_AFTER=$(jq '[.messages[] | select(.role=="user") | .content[]? | select(.type=="tool_result")] | length' "$AFTER")
echo "   Count: $TR_BEFORE → $TR_AFTER (Δ $((TR_AFTER - TR_BEFORE)))"

TR_SIZE_BEFORE=$(jq '[.messages[] | select(.role=="user") | .content[]? | select(.type=="tool_result") | .content | tostring | length] | add // 0' "$BEFORE")
TR_SIZE_AFTER=$(jq '[.messages[] | select(.role=="user") | .content[]? | select(.type=="tool_result") | .content | tostring | length] | add // 0' "$AFTER")
echo "   Size:  $TR_SIZE_BEFORE → $TR_SIZE_AFTER chars (Δ $((TR_SIZE_AFTER - TR_SIZE_BEFORE)))"
echo

# Tool calls analysis
echo "🛠️  TOOL CALLS"
TC_BEFORE=$(jq '[.messages[] | select(.role=="assistant") | .content[]? | select(.type=="tool_use")] | length' "$BEFORE")
TC_AFTER=$(jq '[.messages[] | select(.role=="assistant") | .content[]? | select(.type=="tool_use")] | length' "$AFTER")
echo "   Count: $TC_BEFORE → $TC_AFTER (Δ $((TC_AFTER - TC_BEFORE)))"
echo

# System prompt
echo "📋 SYSTEM PROMPT"
SYS_BEFORE=$(jq '.system | tostring | length' "$BEFORE")
SYS_AFTER=$(jq '.system | tostring | length' "$AFTER")
echo "   Length: $SYS_BEFORE → $SYS_AFTER chars (Δ $((SYS_AFTER - SYS_BEFORE)))"
echo

# Thinking blocks
echo "🧠 THINKING BLOCKS"
TH_BEFORE=$(jq '[.messages[] | .content[]? | select(.type=="thinking")] | length' "$BEFORE")
TH_AFTER=$(jq '[.messages[] | .content[]? | select(.type=="thinking")] | length' "$AFTER")
echo "   Count: $TH_BEFORE → $TH_AFTER (Δ $((TH_AFTER - TH_BEFORE)))"

TH_SIZE_BEFORE=$(jq '[.messages[] | .content[]? | select(.type=="thinking") | .thinking | length] | add // 0' "$BEFORE")
TH_SIZE_AFTER=$(jq '[.messages[] | .content[]? | select(.type=="thinking") | .thinking | length] | add // 0' "$AFTER")
echo "   Size:  $TH_SIZE_BEFORE → $TH_SIZE_AFTER chars (Δ $((TH_SIZE_AFTER - TH_SIZE_BEFORE)))"
echo

# Text content (non-tool, non-thinking)
echo "📝 TEXT CONTENT"
TEXT_BEFORE=$(jq '[.messages[] | .content | if type == "string" then length elif type == "array" then [.[] | select(.type=="text") | .text | length] | add // 0 else 0 end] | add // 0' "$BEFORE")
TEXT_AFTER=$(jq '[.messages[] | .content | if type == "string" then length elif type == "array" then [.[] | select(.type=="text") | .text | length] | add // 0 else 0 end] | add // 0' "$AFTER")
echo "   Size:  $TEXT_BEFORE → $TEXT_AFTER chars (Δ $((TEXT_AFTER - TEXT_BEFORE)))"
echo

# Check for summary markers (compaction signature)
echo "🔍 COMPACTION MARKERS"
if jq -e '.messages[] | .content | tostring | test("summary of the conversation|This session is being continued")' "$AFTER" >/dev/null 2>&1; then
    echo "   ⚠️  Summary/continuation markers FOUND in after.json"
else
    echo "   ✓ No compaction markers detected"
fi
echo

# First/last message comparison
echo "📍 MESSAGE BOUNDARIES"
echo "   First user message (before): $(jq -r '[.messages[] | select(.role=="user")][0] | .content | if type == "string" then .[:80] else .[0].text // .[0].type | .[:80] end' "$BEFORE")..."
echo "   First user message (after):  $(jq -r '[.messages[] | select(.role=="user")][0] | .content | if type == "string" then .[:80] else .[0].text // .[0].type | .[:80] end' "$AFTER")..."
echo

echo "═══════════════════════════════════════════════════════════════════════════════"
# Provide appropriate diff command based on format
if jq -e '.body.messages' "$BEFORE_ORIG" >/dev/null 2>&1; then
    echo "  For full diff (aspy format): diff <(jq '.body.messages' $BEFORE_ORIG) <(jq '.body.messages' $AFTER_ORIG)"
else
    echo "  For full diff: diff <(jq .messages $BEFORE_ORIG) <(jq .messages $AFTER_ORIG)"
fi
echo "═══════════════════════════════════════════════════════════════════════════════"