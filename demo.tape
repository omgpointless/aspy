# demo.tape - VHS recording script for anthropic-spy
#
# Records a demo GIF showcasing:
# - Real-time streaming thinking (word-by-word, like ChatGPT)
# - Topic display in header (from Haiku summarization)
# - Context bar filling up
# - Tool calls with realistic timings
# - Stats view with gauges
#
# Run: vhs demo.tape
# Or via Docker: docker run --rm -v $(pwd):/vhs anthropic-spy-vhs /vhs/demo.tape

Output "/vhs/demo.gif"
Set FontSize 13
Set Width 1400
Set Height 850
Set Theme "Molokai"
Set Padding 15
Set Framerate 30

# Start demo mode
Type "ANTHROPIC_SPY_DEMO=1 /app/target/release/anthropic-spy"
Sleep 300ms
Enter

# Wait for initial topic ("Code Review Task") and first streaming thinking
# The demo has 1.5s delay, then topic response, then thinking starts streaming
Sleep 8s

# Watch more thinking stream in (second thinking block)
Sleep 5s

# Navigate down through events (shows scrollbar and more activity)
Down
Sleep 400ms
Down
Sleep 400ms
Down
Sleep 400ms

# Watch context bar grow as more events arrive
Sleep 3s

# Navigate to see tool calls
Down
Sleep 400ms
Down
Sleep 400ms
Down
Sleep 400ms

# Show event detail view (tool call with input JSON)
Enter
Sleep 2s

# Scroll detail content
Down
Sleep 300ms
Down
Sleep 300ms

# Back to list
Escape
Sleep 1s

# Continue watching events arrive and context grow
Down
Sleep 500ms
Down
Sleep 500ms

# Wait for more streaming thinking
Sleep 4s

# === Switch to Stats view ===
Type "s"
Sleep 4s

# Let user see the full Stats view with:
# - Models panel (Opus/Haiku distribution)
# - Cache & Context panel (gauges)
# - Tools panel (with durations)
# - Session Totals panel
Sleep 3s

# Back to Events view for finale
Type "e"
Sleep 2s

# Watch final thinking stream ("Implementation Complete" topic)
Sleep 4s

# Scroll to see completed state
Down
Sleep 300ms
Down
Sleep 300ms

# Final pause showing complete session
Sleep 2s

# Clean exit
Type "q"
Sleep 500ms
