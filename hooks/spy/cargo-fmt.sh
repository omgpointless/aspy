#!/bin/bash
# Post-tool-use hook: Automatic cargo fmt on Rust file modifications
#
# This hook runs after Write or Edit tool calls and formats Rust files using cargo fmt.
# It ensures all Rust code stays formatted according to project standards.

# DON'T use set -e - we want to handle errors gracefully and always exit 0

# Read stdin (tool use info from Claude Code)
TOOL_DATA=$(cat)

# Debug: Log what we receive
{
  echo "=== DEBUG ==="
  echo "TOOL_NAME: ${TOOL_NAME:-not set}"
  echo "TOOL_STDOUT: ${TOOL_STDOUT:-not set}"
  echo "TOOL_STDERR: ${TOOL_STDERR:-not set}"
  echo "TOOL_EXIT_CODE: ${TOOL_EXIT_CODE:-not set}"
  echo "STDIN: $TOOL_DATA"
  echo "============="
} >> /tmp/hook-debug.log

# Extract file path from tool call if it's an Edit or Write operation
FILE_PATH=$(echo "$TOOL_DATA" | jq -r '.input.file_path // empty' 2>/dev/null)

# Check if jq failed or no file path found
if [ -z "$FILE_PATH" ] || [ "$FILE_PATH" = "null" ]; then
    # Silently skip - likely not a Write/Edit tool or missing file_path
    exit 0
fi

# Check if it's a Rust file
if [[ "$FILE_PATH" == *.rs ]]; then
    # Run cargo fmt on the specific file
    if [ -n "$CLAUDE_PROJECT_DIR" ]; then
        FMT_OUTPUT=$(cargo fmt --manifest-path "$CLAUDE_PROJECT_DIR/Cargo.toml" -- "$FILE_PATH" 2>&1)
        FMT_EXIT=$?
    else
        FMT_OUTPUT=$(cargo fmt -- "$FILE_PATH" 2>&1)
        FMT_EXIT=$?
    fi

    if [ $FMT_EXIT -eq 0 ]; then
        # Success: Return JSON with system message
        jq -n --arg file "$FILE_PATH" '{
          systemMessage: ("✓ Formatted " + $file)
        }'
    else
        # Non-fatal error: Return JSON with warning
        jq -n --arg file "$FILE_PATH" --arg output "$FMT_OUTPUT" '{
          systemMessage: ("⚠ cargo fmt issues with " + $file + ": " + $output)
        }'
    fi
fi

# Exit 0 = non-blocking success
exit 0
