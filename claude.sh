#!/usr/bin/env bash
#
# claude.sh - Launch Claude Code through Aspy proxy
#
# Usage:
#   ./claude.sh                    # Bare proxy (uses API key hash for identity)
#   ./claude.sh foundry            # Foundry provider via /foundry route
#   ./claude.sh dev-1              # Anthropic via /dev-1 route
#   ./claude.sh dev-2 anthropic    # Explicit: client + provider type
#
# Provider Types:
#   foundry   - Uses ANTHROPIC_FOUNDRY_BASE_URL (for Azure Foundry)
#   anthropic - Uses ANTHROPIC_BASE_URL (default, for direct Anthropic API)
#
# Examples:
#   ./claude.sh                    # → http://localhost:8080 (bare, API key hash)
#   ./claude.sh foundry            # → ANTHROPIC_FOUNDRY_BASE_URL=http://localhost:8080/foundry
#   ./claude.sh dev-1              # → ANTHROPIC_BASE_URL=http://localhost:8080/dev-1
#   ./claude.sh dev-2 anthropic    # → ANTHROPIC_BASE_URL=http://localhost:8080/dev-2
#
# Requirements:
#   - Aspy proxy running on localhost:8080
#   - Client configured in ~/.config/aspy/config.toml
#

set -e

PROXY_HOST="http://127.0.0.1:8080"

# Parse arguments
CLIENT_ID="${1:-}"
PROVIDER_TYPE="${2:-}"

# Auto-detect provider type from client name if not specified
if [[ -z "$PROVIDER_TYPE" ]]; then
    case "$CLIENT_ID" in
        foundry*)
            PROVIDER_TYPE="foundry"
            ;;
        *)
            PROVIDER_TYPE="anthropic"
            ;;
    esac
fi

# Build proxy URL
if [[ -n "$CLIENT_ID" ]]; then
    PROXY_URL="${PROXY_HOST}/${CLIENT_ID}"
else
    PROXY_URL="${PROXY_HOST}"
fi

echo "🚀 Launching Claude Code"
echo "   Client:   ${CLIENT_ID:-<bare>}"
echo "   Provider: ${PROVIDER_TYPE}"
echo "   Proxy:    ${PROXY_URL}"
echo ""

# Launch based on provider type
case "$PROVIDER_TYPE" in
    foundry)
        # Foundry uses different env vars
        ANTHROPIC_FOUNDRY_BASE_URL="${PROXY_URL}" \
        ANTHROPIC_FOUNDRY_RESOURCE= \
        ASPY_CLIENT_ID="${CLIENT_ID}" \
        claude
        ;;
    anthropic|*)
        # Standard Anthropic API
        ANTHROPIC_BASE_URL="${PROXY_URL}" \
        ASPY_CLIENT_ID="${CLIENT_ID}" \
        claude
        ;;
esac
