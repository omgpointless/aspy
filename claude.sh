#!/usr/bin/env bash
#
# claude.sh - Launch Claude Code through Aspy proxy
#
# Usage:
#   ./claude.sh                    # Bare proxy (uses API key hash for identity)
#   ./claude.sh foundry            # Foundry provider via /foundry route
#   ./claude.sh dev-1              # Anthropic via /dev-1 route
#   ./claude.sh dev-2 anthropic    # Explicit: client + provider type
#   ./claude.sh openrouter         # OpenRouter provider via /openrouter route
#   ./claude.sh local              # Local Ollama via /local route
#   ./claude.sh zai                # Zai provider via /zai route
#   ./claude.sh --resume           # Resume most recent session
#   ./claude.sh dev-1 --resume     # Resume most recent with client
#   ./claude.sh dev-1 -r abc123    # Resume specific session
#
# Provider Types:
#   foundry    - Uses ANTHROPIC_FOUNDRY_BASE_URL (for Azure Foundry)
#   anthropic  - Uses ANTHROPIC_BASE_URL (default, for direct Anthropic API)
#   openrouter - Uses ANTHROPIC_BASE_URL, unsets ANTHROPIC_FOUNDRY_RESOURCE
#   ollama     - Uses ANTHROPIC_BASE_URL for local Ollama server
#   zai        - Uses ANTHROPIC_BASE_URL for Zai provider
#
# Resume Options:
#   --resume, -r              Resume most recent session
#   --resume <id>, -r <id>    Resume specific session by ID
#
# Examples:
#   ./claude.sh                    # → http://localhost:8080 (bare, API key hash)
#   ./claude.sh foundry            # → ANTHROPIC_FOUNDRY_BASE_URL=http://localhost:8080/foundry
#   ./claude.sh dev-1              # → ANTHROPIC_BASE_URL=http://localhost:8080/dev-1
#   ./claude.sh dev-2 anthropic    # → ANTHROPIC_BASE_URL=http://localhost:8080/dev-2
#   ./claude.sh openrouter         # → ANTHROPIC_BASE_URL=http://localhost:8080/openrouter
#   ./claude.sh local              # → ANTHROPIC_BASE_URL=http://localhost:8080/local (Ollama)
#   ./claude.sh zai                # → ANTHROPIC_BASE_URL=http://localhost:8080/zai
#   ./claude.sh --resume           # Resume most recent session (bare proxy)
#   ./claude.sh dev-1 --resume     # Resume most recent session via dev-1
#   ./claude.sh dev-1 -r abc123    # Resume session abc123 via dev-1
#
# Requirements:
#   - Aspy proxy running on localhost:8080
#   - Client configured in ~/.config/aspy/config.toml
#

set -e

PROXY_HOST="http://127.0.0.1:8080"

# Parse arguments - extract --resume/-r and session ID
CLIENT_ID=""
PROVIDER_TYPE=""
RESUME_FLAG=""
RESUME_SESSION=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --resume|-r)
            RESUME_FLAG="--resume"
            # Check if next arg is a session ID (not another flag or provider)
            if [[ -n "${2:-}" && ! "$2" =~ ^- && ! "$2" =~ ^(foundry|anthropic|openrouter|ollama|zai)$ ]]; then
                RESUME_SESSION="$2"
                shift
            fi
            shift
            ;;
        foundry|anthropic|openrouter|ollama|zai)
            # Explicit provider type
            if [[ -z "$CLIENT_ID" ]]; then
                CLIENT_ID="$1"
            else
                PROVIDER_TYPE="$1"
            fi
            shift
            ;;
        -*)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
        *)
            # Positional: client ID or provider type
            if [[ -z "$CLIENT_ID" ]]; then
                CLIENT_ID="$1"
            elif [[ -z "$PROVIDER_TYPE" ]]; then
                PROVIDER_TYPE="$1"
            fi
            shift
            ;;
    esac
done

# Auto-detect provider type from client name if not specified
if [[ -z "$PROVIDER_TYPE" ]]; then
    case "$CLIENT_ID" in
        foundry*)
            PROVIDER_TYPE="foundry"
            ;;
        openrouter*)
            PROVIDER_TYPE="openrouter"
            ;;
        zai*)
            PROVIDER_TYPE="zai"
            ;;
        local*|ollama*)
            PROVIDER_TYPE="ollama"
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

# Build claude args
CLAUDE_ARGS=()
if [[ -n "$RESUME_FLAG" ]]; then
    CLAUDE_ARGS+=("$RESUME_FLAG")
    if [[ -n "$RESUME_SESSION" ]]; then
        CLAUDE_ARGS+=("$RESUME_SESSION")
    fi
fi

echo "🚀 Launching Claude Code"
echo "   Client:   ${CLIENT_ID:-<bare>}"
echo "   Provider: ${PROVIDER_TYPE}"
echo "   Proxy:    ${PROXY_URL}"
if [[ -n "$RESUME_FLAG" ]]; then
    echo "   Resume:   ${RESUME_SESSION:-<most recent>}"
fi
echo ""

# Launch based on provider type
case "$PROVIDER_TYPE" in
    foundry)
        # Foundry uses different env vars
        ANTHROPIC_FOUNDRY_BASE_URL="${PROXY_URL}" \
        ANTHROPIC_FOUNDRY_RESOURCE= \
        ASPY_CLIENT_ID="${CLIENT_ID}" \
        claude "${CLAUDE_ARGS[@]}"
        ;;
    openrouter)
        # OpenRouter: use base URL, ensure foundry resource is unset
        ANTHROPIC_BASE_URL="${PROXY_URL}" \
        ANTHROPIC_FOUNDRY_RESOURCE= \
        ASPY_CLIENT_ID="${CLIENT_ID}" \
        claude "${CLAUDE_ARGS[@]}"
        ;;
    ollama)
        # Local Ollama: use base URL, ensure foundry resource is unset
        ANTHROPIC_BASE_URL="${PROXY_URL}" \
        ANTHROPIC_FOUNDRY_RESOURCE= \
        ASPY_CLIENT_ID="${CLIENT_ID}" \
        claude "${CLAUDE_ARGS[@]}"
        ;;
    zai)
        # Zai: use base URL, ensure foundry resource is unset
        ANTHROPIC_BASE_URL="${PROXY_URL}" \
        ANTHROPIC_FOUNDRY_RESOURCE= \
        ASPY_CLIENT_ID="${CLIENT_ID}" \
        claude "${CLAUDE_ARGS[@]}"
        ;;
    anthropic|*)
        # Standard Anthropic API
        ANTHROPIC_BASE_URL="${PROXY_URL}" \
        ASPY_CLIENT_ID="${CLIENT_ID}" \
        claude "${CLAUDE_ARGS[@]}"
        ;;
esac
