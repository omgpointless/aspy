# demo.tape - VHS recording script for aspy
#
# Records a ~40 second demo GIF showcasing:
# - Real-time streaming thinking
# - Event navigation and detail view
# - Stats dashboard with widgets
# - Theme switching via Settings
#
# Run: docker run --rm -v $(pwd):/work aspy-vhs /work/demo.tape

Output "/work/demo.gif"
Set FontSize 14
Set Width 1400
Set Height 850
Set Theme "/app/themes/vhs/spy-dark.json"
Set Padding 15
Set Framerate 30

# Start demo mode
Type "ASPY_DEMO=1 /app/target/release/aspy"
Sleep 300ms
Enter

# === Phase 1: Watch thinking stream in (8s) ===
# Let the thinking panel fill with streaming content
Sleep 8s

# === Phase 2: Navigate events (5s) ===
# Scroll through some events
Down
Sleep 400ms
Down
Sleep 400ms
Down
Sleep 400ms

# Open detail view on an event
Enter
Sleep 2s

# Scroll detail content
Down
Sleep 300ms
Down
Sleep 300ms

# Close detail
Escape
Sleep 800ms

# === Phase 3: Stats view - the wow moment (7s) ===
# Press 's' for stats
Type "s"
Sleep 4s

# Tab through stats tabs to show different widgets
Tab
Sleep 1.5s
Tab
Sleep 1.5s

# === Phase 4: Settings & theme (6s) ===
# Back to events, then open settings
Type "e"
Sleep 500ms

# Open settings with F3
F3
Sleep 1s

# Navigate themes
Down
Sleep 600ms
Down
Sleep 600ms
Down
Sleep 600ms

# Apply theme
Enter
Sleep 2s

# === Phase 5: Final glimpse (3s) ===
# Back to events to show the new theme
Type "e"
Sleep 2s

# Navigate to show activity
Down
Sleep 300ms
Down
Sleep 300ms

# === Clean exit ===
Sleep 500ms
Type "q"
Sleep 300ms
