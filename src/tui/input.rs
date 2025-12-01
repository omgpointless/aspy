// Input handling system with configurable key behaviors
//
// This module provides a flexible input handling system that supports:
// - State-change only keys (trigger once per press)
// - Repeatable keys (trigger on press, then repeat while held)

use crossterm::event::KeyCode;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Defines how a key should behave when pressed/held
#[derive(Debug, Clone, Copy)]
pub enum KeyBehavior {
    /// Trigger only on state change (press → release)
    /// Use for: Enter, Tab, single-action keys
    StateChange,

    /// Trigger on press, then repeat after initial delay
    /// Use for: Arrow keys, PageUp/Down, navigation
    Repeatable {
        /// Delay before starting to repeat (e.g., 500ms)
        initial_delay: Duration,
        /// Time between repeats (e.g., 50ms)
        repeat_interval: Duration,
    },
}

impl KeyBehavior {
    /// Standard navigation key behavior (like arrow keys)
    pub fn navigation() -> Self {
        Self::Repeatable {
            initial_delay: Duration::from_millis(500),
            repeat_interval: Duration::from_millis(50),
        }
    }

    /// Fast navigation (for PageUp/PageDown)
    pub fn fast_navigation() -> Self {
        Self::Repeatable {
            initial_delay: Duration::from_millis(300),
            repeat_interval: Duration::from_millis(30),
        }
    }
}

/// Tracks the state of a single key
#[derive(Debug)]
struct KeyState {
    /// Whether the key is currently pressed
    is_pressed: bool,
    /// When the key was first pressed
    press_started: Option<Instant>,
    /// When the action was last triggered
    last_triggered: Option<Instant>,
}

impl KeyState {
    fn new() -> Self {
        Self {
            is_pressed: false,
            press_started: None,
            last_triggered: None,
        }
    }

    /// Reset state when key is released
    fn release(&mut self) {
        self.is_pressed = false;
        self.press_started = None;
        self.last_triggered = None;
    }
}

/// Input handler that manages key behaviors
pub struct InputHandler {
    /// Map of key code to its current state
    key_states: HashMap<KeyCode, KeyState>,
    /// Map of key code to its behavior configuration
    key_behaviors: HashMap<KeyCode, KeyBehavior>,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            key_states: HashMap::new(),
            key_behaviors: HashMap::new(),
        }
    }

    /// Configure a key's behavior
    pub fn configure_key(&mut self, key: KeyCode, behavior: KeyBehavior) {
        self.key_behaviors.insert(key, behavior);
    }

    /// Configure multiple keys with the same behavior
    pub fn configure_keys(&mut self, keys: &[KeyCode], behavior: KeyBehavior) {
        for key in keys {
            self.configure_key(*key, behavior);
        }
    }

    /// Handle a key press event
    /// Returns true if the action should be triggered
    pub fn handle_key_press(&mut self, key: KeyCode) -> bool {
        let now = Instant::now();
        let behavior = self
            .key_behaviors
            .get(&key)
            .copied()
            .unwrap_or(KeyBehavior::StateChange);

        let state = self.key_states.entry(key).or_insert_with(KeyState::new);

        // If key was already pressed, check if we should repeat
        if state.is_pressed {
            match behavior {
                KeyBehavior::StateChange => {
                    // Debounce: only trigger if enough time passed since last trigger
                    // This handles terminals that don't send Release events
                    if let Some(last) = state.last_triggered {
                        if now.duration_since(last) >= Duration::from_millis(150) {
                            state.last_triggered = Some(now);
                            return true;
                        }
                    }
                    false
                }
                KeyBehavior::Repeatable {
                    initial_delay,
                    repeat_interval,
                } => {
                    // Check if we should trigger based on timing
                    if let (Some(press_start), Some(last_trigger)) =
                        (state.press_started, state.last_triggered)
                    {
                        let time_since_press = now.duration_since(press_start);
                        let time_since_last = now.duration_since(last_trigger);

                        // After initial delay, repeat at interval
                        if time_since_press >= initial_delay && time_since_last >= repeat_interval {
                            state.last_triggered = Some(now);
                            return true;
                        }
                    }
                    false
                }
            }
        } else {
            // New key press - always trigger
            state.is_pressed = true;
            state.press_started = Some(now);
            state.last_triggered = Some(now);
            true
        }
    }

    /// Handle a key release event
    pub fn handle_key_release(&mut self, key: KeyCode) {
        if let Some(state) = self.key_states.get_mut(&key) {
            state.release();
        }
    }

    /// Get default configuration for common keys
    pub fn with_default_config() -> Self {
        let mut handler = Self::new();

        // Navigation keys - repeatable
        handler.configure_keys(
            &[KeyCode::Up, KeyCode::Down, KeyCode::Left, KeyCode::Right],
            KeyBehavior::navigation(),
        );

        // Vim keys - repeatable
        handler.configure_keys(
            &[
                KeyCode::Char('j'),
                KeyCode::Char('k'),
                KeyCode::Char('h'),
                KeyCode::Char('l'),
            ],
            KeyBehavior::navigation(),
        );

        // Page navigation - fast repeatable
        handler.configure_keys(
            &[
                KeyCode::PageUp,
                KeyCode::PageDown,
                KeyCode::Home,
                KeyCode::End,
            ],
            KeyBehavior::fast_navigation(),
        );

        // Action keys - state change only (trigger once per press)
        handler.configure_keys(
            &[
                // Core actions
                KeyCode::Enter,
                KeyCode::Esc,
                KeyCode::Tab,
                KeyCode::BackTab,
                KeyCode::Char(' '),
                // Quit
                KeyCode::Char('q'),
                KeyCode::Char('Q'),
                // View switching
                KeyCode::F(1),
                KeyCode::F(2),
                KeyCode::F(3),
                KeyCode::Char('e'),
                KeyCode::Char('E'),
                KeyCode::Char('s'),
                KeyCode::Char('S'),
                // Clipboard
                KeyCode::Char('y'),
                KeyCode::Char('Y'),
                // Help
                KeyCode::Char('?'),
            ],
            KeyBehavior::StateChange,
        );

        handler
    }
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::with_default_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_state_change_no_repeat() {
        let mut handler = InputHandler::new();
        handler.configure_key(KeyCode::Enter, KeyBehavior::StateChange);

        // First press triggers
        assert!(handler.handle_key_press(KeyCode::Enter));

        // Subsequent presses while held don't trigger
        assert!(!handler.handle_key_press(KeyCode::Enter));
        assert!(!handler.handle_key_press(KeyCode::Enter));

        // Release
        handler.handle_key_release(KeyCode::Enter);

        // Next press triggers again
        assert!(handler.handle_key_press(KeyCode::Enter));
    }

    #[test]
    fn test_repeatable_with_delay() {
        let mut handler = InputHandler::new();
        handler.configure_key(
            KeyCode::Down,
            KeyBehavior::Repeatable {
                initial_delay: Duration::from_millis(100),
                repeat_interval: Duration::from_millis(50),
            },
        );

        // First press triggers immediately
        assert!(handler.handle_key_press(KeyCode::Down));

        // Immediate second call doesn't trigger (within initial delay)
        assert!(!handler.handle_key_press(KeyCode::Down));

        // Wait for initial delay
        thread::sleep(Duration::from_millis(110));

        // Should trigger now
        assert!(handler.handle_key_press(KeyCode::Down));

        // Wait for repeat interval
        thread::sleep(Duration::from_millis(60));

        // Should trigger again
        assert!(handler.handle_key_press(KeyCode::Down));
    }
}
