# demo.tape - VHS recording script for anthropic-spy
#
# Records a demo GIF showcasing:
# - Real-time streaming thinking (word-by-word)
# - Panel focus cycling with Tab
# - Event detail view
# - Stats view with 2x2 layout
# - Theme selector with live preview
# - Context bar filling up
# - Tool calls with realistic timings
#
# Run: vhs demo.tape
# Or via Docker: docker run --rm -v $(pwd):/work anthropic-spy-vhs /work/demo.tape
#
# Theme sync: Both VHS and anthropic-spy use iTerm2-Color-Schemes format.
# Set the same theme name for consistent appearance.

Output "/work/demo.gif"
Set FontSize 13
Set Width 1400
Set Height 850
Set Theme "Dracula"
Set Padding 15
Set Framerate 30

# Start demo mode with matching theme
Type "ANTHROPIC_SPY_DEMO=1 ANTHROPIC_SPY_THEME=Dracula /app/target/release/anthropic-spy"
Sleep 300ms
Enter

# === Phase 1: Watch thinking stream in ===
# Wait for initial topic and first streaming thinking block
Sleep 8s

# More thinking streaming
Sleep 4s

# === Phase 2: Navigate events ===
Down
Sleep 400ms
Down
Sleep 400ms
Down
Sleep 400ms

# === Phase 3: Show panel focus cycling (Tab) ===
# Tab to Thinking panel (border turns magenta)
Tab
Sleep 1.5s

# Tab to Logs panel (border turns green)
Tab
Sleep 1.5s

# Tab back to Events panel (border turns cyan)
Tab
Sleep 1s

# === Phase 4: Event detail view ===
# Navigate to a tool call event
Down
Sleep 300ms
Down
Sleep 300ms

# Open detail view (Enter)
Enter
Sleep 2s

# Scroll detail content
Down
Sleep 300ms
Down
Sleep 300ms
Down
Sleep 300ms

# Close detail view
Escape
Sleep 1s

# === Phase 5: Watch more activity ===
# Let events and thinking continue
Sleep 4s

# Navigate through events
Down
Sleep 400ms
Down
Sleep 400ms

# === Phase 6: Stats view ===
Type "s"
Sleep 4s

# Pause on stats - shows 2x2 grid:
# - Models panel (Opus/Haiku)
# - Tokens panel (Cached/Input/Output bars)
# - Tools panel (with durations)
# - Totals panel
Sleep 3s

# Back to Events
Type "e"
Sleep 2s

# === Phase 7: Theme selector with live preview ===
Type "t"
Sleep 1s

# Navigate themes - watch UI colors change live!
Down
Sleep 800ms
Down
Sleep 800ms
Down
Sleep 800ms

# Go back up
Up
Sleep 800ms
Up
Sleep 800ms

# Cancel without applying (Escape restores original)
Escape
Sleep 1.5s

# Open theme selector again
Type "t"
Sleep 800ms

# Pick a different theme and apply
Down
Sleep 600ms
Down
Sleep 600ms
Enter
Sleep 2s

# === Phase 8: Final activity ===
# Watch remaining thinking stream
Sleep 3s

# Scroll to see final state
Down
Sleep 300ms
Down
Sleep 300ms

# Final pause showing complete session
Sleep 2s

# === Clean exit ===
Type "q"
Sleep 500ms
