# demo.tape - VHS recording script for anthropic-spy
#
# Records a demo GIF showing the TUI with thinking panel,
# tool calls, and token tracking.

Output "/vhs/demo.gif"
Set FontSize 14
Set Width 1400
Set Height 800
Set Theme "Dracula"
Set Padding 20

# Start demo mode
Type "ANTHROPIC_SPY_DEMO=1 /app/target/release/anthropic-spy"
Sleep 300ms
Enter

# Let events populate - thinking panel appears on right
Sleep 10s

# Navigate through events to show variety
Type "j"
Sleep 600ms
Type "j"
Sleep 600ms
Type "j"
Sleep 600ms
Type "j"
Sleep 600ms

# Show detail view for a tool call
Enter
Sleep 3s

# Back to list
Enter
Sleep 1s

# Navigate more to show thinking updates
Type "j"
Sleep 500ms
Type "j"
Sleep 500ms
Type "j"
Sleep 500ms

# Let final state settle - shows token accumulation
Sleep 3s

# Clean exit
Type "q"
Sleep 500ms
