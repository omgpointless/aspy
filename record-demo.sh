#!/bin/bash
# record-demo.sh - Build and record anthropic-spy demo with VHS
#
# Usage:
#   ./record-demo.sh                    # Build and record with default theme (Dracula)
#   ./record-demo.sh --theme Nord       # Record with specific theme
#   ./record-demo.sh --build            # Force rebuild (no cache)
#   ./record-demo.sh --build --theme "Solarized Light"

set -e

IMAGE_NAME="anthropic-spy-vhs"
THEME="Dracula"
BUILD=false
NO_CACHE=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --theme)
            THEME="$2"
            shift 2
            ;;
        --build)
            BUILD=true
            shift
            ;;
        --no-cache)
            NO_CACHE="--no-cache"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--build] [--no-cache] [--theme <name>]"
            exit 1
            ;;
    esac
done

echo "=== anthropic-spy VHS Demo Recorder ==="
echo "Theme: $THEME"

# Build if requested or image doesn't exist
if $BUILD || ! docker image inspect "$IMAGE_NAME" &>/dev/null; then
    echo ""
    echo "=== Building Docker image ==="
    docker build $NO_CACHE -t "$IMAGE_NAME" -f Dockerfile.vhs .
fi

# Create temp tape file with theme substituted
TEMP_TAPE=$(mktemp)
sed -e "s/Set Theme \".*\"/Set Theme \"$THEME\"/" \
    -e "s/ANTHROPIC_SPY_THEME=[^ ]*/ANTHROPIC_SPY_THEME=\"$THEME\"/" \
    demo.tape > "$TEMP_TAPE"

echo ""
echo "=== Recording demo ==="
docker run --rm -v "$(pwd):/work" -v "$TEMP_TAPE:/work/demo.tape:ro" "$IMAGE_NAME" /work/demo.tape

# Cleanup
rm -f "$TEMP_TAPE"

echo ""
echo "=== Done! ==="
echo "Output: demo.gif"
