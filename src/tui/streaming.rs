// Streaming state machine for TUI header animation
//
// This module manages the streaming state displayed in the TUI header.
// It's extracted from App to:
// 1. Make state transitions explicit and documented
// 2. Enable unit testing of the state machine
// 3. Reduce complexity in the App struct
//
// State Diagram:
//
//                    ┌──────────────────────────────────────────┐
//                    │                                          │
//                    ▼                                          │
//   [Idle] ──Request──▶ [Generating] ──ThinkingStarted──▶ [Thinking]
//     ▲                      │                                  │
//     │                      │ Response                         │
//     │                      ▼                                  │
//     │◀─────────Response──[Idle]◀──────────────────────────────┘
//     │                      ▲                  (ApiUsage cleanup)
//     │                      │
//     │                      │ ToolCall(auto)
//     │                      │
//     │     Request          │ ToolCall(slow)
//     │        │             │        │
//     │        ▼             │        ▼
//     │   [Generating]───────┴──[Executing]
//     │        │
//     └────────┘ Response
//
// Note: ToolResult is informational only - no state change.
// The Request carrying the result sets Generating, Response sets Idle.
// "Executing" covers both approval wait and actual execution (proxy can't distinguish).
//

/// Streaming state for header animation
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StreamingState {
    #[default]
    Idle,
    /// Claude is thinking (extended thinking block active)
    Thinking,
    /// Claude is generating response
    Generating,
    /// Tool is executing (approval + execution phase)
    /// Note: Proxy can't distinguish between pending approval and active execution
    Executing,
}

/// State machine for streaming status
///
/// Encapsulates all state transition logic for the TUI header.
/// Each method represents an event that can trigger a transition.
#[derive(Debug, Default)]
pub struct StreamingStateMachine {
    state: StreamingState,
}

impl StreamingStateMachine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get current state
    pub fn state(&self) -> StreamingState {
        self.state
    }

    /// Request sent - Claude is about to generate
    pub fn on_request(&mut self) {
        self.state = StreamingState::Generating;
    }

    /// Response complete - back to idle
    pub fn on_response(&mut self) {
        self.state = StreamingState::Idle;
    }

    /// Tool call received - may need time to execute
    pub fn on_tool_call(&mut self, tool_name: &str) {
        if Self::tool_needs_execution_indicator(tool_name) {
            self.state = StreamingState::Executing;
        } else {
            self.state = StreamingState::Idle;
        }
    }

    /// Tool result received - informational only
    ///
    /// ToolResult tells us a tool finished executing, but doesn't change state.
    /// The Request that carried the tool_result already set us to Generating,
    /// and Response will set us to Idle when the API actually responds.
    pub fn on_tool_result(&mut self) {
        // Intentionally no state change - this is informational
        // State flow: Request → Generating → Response → Idle
    }

    /// Thinking block started (real-time, during stream)
    pub fn on_thinking_started(&mut self) {
        self.state = StreamingState::Thinking;
    }

    /// API usage received - terminal event, cleanup any transient state
    ///
    /// This acts as a safety net: if we're still in Generating or Thinking
    /// after the response completed, this resets to Idle.
    pub fn on_api_usage(&mut self) {
        if matches!(
            self.state,
            StreamingState::Generating | StreamingState::Thinking
        ) {
            self.state = StreamingState::Idle;
        }
    }

    /// Check if a tool needs an execution indicator (may take noticeable time)
    fn tool_needs_execution_indicator(tool_name: &str) -> bool {
        matches!(
            tool_name,
            "Edit" | "Write" | "Bash" | "NotebookEdit" | "Task"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_is_idle() {
        let sm = StreamingStateMachine::new();
        assert_eq!(sm.state(), StreamingState::Idle);
    }

    #[test]
    fn test_request_sets_generating() {
        let mut sm = StreamingStateMachine::new();
        sm.on_request();
        assert_eq!(sm.state(), StreamingState::Generating);
    }

    #[test]
    fn test_thinking_flow() {
        let mut sm = StreamingStateMachine::new();
        sm.on_request();
        assert_eq!(sm.state(), StreamingState::Generating);

        sm.on_thinking_started();
        assert_eq!(sm.state(), StreamingState::Thinking);

        // API usage cleans up thinking state
        sm.on_api_usage();
        assert_eq!(sm.state(), StreamingState::Idle);
    }

    #[test]
    fn test_tool_execution_flow() {
        let mut sm = StreamingStateMachine::new();
        sm.on_request();

        // Response arrives, then parser emits ToolCall
        sm.on_response();
        sm.on_tool_call("Edit");
        assert_eq!(sm.state(), StreamingState::Executing);

        // Tool executes → Claude Code sends new request with tool_result
        sm.on_request(); // New request carrying the result
        assert_eq!(sm.state(), StreamingState::Generating);

        // ToolResult is informational - doesn't change state
        sm.on_tool_result();
        assert_eq!(sm.state(), StreamingState::Generating); // Still generating!

        // Response completes the cycle
        sm.on_response();
        assert_eq!(sm.state(), StreamingState::Idle);
    }

    #[test]
    fn test_auto_tool_flow() {
        let mut sm = StreamingStateMachine::new();
        sm.on_request();

        // Read auto-executes (no approval needed)
        sm.on_tool_call("Read");
        assert_eq!(sm.state(), StreamingState::Idle);
    }

    #[test]
    fn test_response_sets_idle() {
        let mut sm = StreamingStateMachine::new();
        sm.on_request();
        sm.on_response();
        assert_eq!(sm.state(), StreamingState::Idle);
    }

    #[test]
    fn test_api_usage_only_clears_transient_states() {
        let mut sm = StreamingStateMachine::new();

        // From Executing - should NOT change (not transient)
        sm.on_request();
        sm.on_tool_call("Bash");
        assert_eq!(sm.state(), StreamingState::Executing);
        sm.on_api_usage();
        assert_eq!(sm.state(), StreamingState::Executing);
    }
}
