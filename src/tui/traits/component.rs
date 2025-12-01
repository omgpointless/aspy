//! Core component trait - the foundation of the UI system
//!
//! Every UI element that can be rendered implements `Component`.
//! This trait provides identity and rendering capability.

use crate::theme::Theme;
use crate::tui::streaming::StreamingState;
use ratatui::{layout::Rect, Frame};

/// Unique identifier for a component
///
/// Used for:
/// - Focus tracking (which component receives input)
/// - Theme lookups (panel-specific colors)
/// - Event routing
///
/// # Relationship to FocusablePanel
///
/// This replaces the `FocusablePanel` enum with a more extensible system.
/// New components can define their own IDs without modifying a central enum.
///
/// Note: Currently unused - intentional infrastructure for future component system migration
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentId {
    /// Main event list panel
    Events,
    /// Event detail view (expanded)
    Detail,
    /// Claude's thinking/reasoning panel
    Thinking,
    /// System logs panel
    Logs,
    /// Toast notification (non-focusable)
    Toast,
    /// Title bar (non-focusable)
    TitleBar,
    /// Status bar (non-focusable)
    StatusBar,
    /// Context usage bar (non-focusable)
    ContextBar,
}

impl ComponentId {
    /// Whether this component can receive focus
    #[allow(dead_code)]
    pub fn is_focusable(&self) -> bool {
        matches!(
            self,
            ComponentId::Events | ComponentId::Detail | ComponentId::Thinking | ComponentId::Logs
        )
    }

    /// Cycle to next focusable component (Tab behavior)
    #[allow(dead_code)]
    pub fn next_focus(self) -> Self {
        match self {
            Self::Events => Self::Thinking,
            Self::Thinking => Self::Logs,
            Self::Logs => Self::Events,
            Self::Detail => Self::Events, // Detail exits to Events
            other => other,               // Non-focusable stays put
        }
    }

    /// Cycle to previous focusable component (Shift+Tab behavior)
    #[allow(dead_code)]
    pub fn prev_focus(self) -> Self {
        match self {
            Self::Events => Self::Logs,
            Self::Thinking => Self::Events,
            Self::Logs => Self::Thinking,
            Self::Detail => Self::Events,
            other => other,
        }
    }
}

/// Immutable context passed to components during rendering
///
/// This replaces passing `&App` to every render function.
/// Components only see what they need - no access to mutable app state.
///
/// # Design Rationale
///
/// By constraining what components can see during render:
/// - Rendering becomes pure (no side effects)
/// - Components can't accidentally mutate app state
/// - Easier to test components in isolation
/// - Clear dependency injection pattern
///
/// Note: Currently unused - intentional infrastructure for future component system migration
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RenderContext<'a> {
    /// Color theme for styling
    pub theme: &'a Theme,

    /// Which component currently has focus
    pub focus: ComponentId,

    /// Animation frame counter (for spinners, blinking cursors)
    pub animation_frame: usize,

    /// Current streaming state (idle, thinking, generating)
    pub streaming_state: StreamingState,
}

impl<'a> RenderContext<'a> {
    /// Create a new render context
    #[allow(dead_code)]
    pub fn new(
        theme: &'a Theme,
        focus: ComponentId,
        animation_frame: usize,
        streaming_state: StreamingState,
    ) -> Self {
        Self {
            theme,
            focus,
            animation_frame,
            streaming_state,
        }
    }

    /// Check if a component is currently focused
    #[allow(dead_code)]
    pub fn is_focused(&self, id: ComponentId) -> bool {
        self.focus == id
    }

    /// Get spinner character for current animation frame
    #[allow(dead_code)]
    pub fn spinner_char(&self) -> char {
        const SPINNER: [char; 4] = ['◐', '◓', '◑', '◒'];
        SPINNER[self.animation_frame % SPINNER.len()]
    }

    /// Get animated dots for thinking indicator
    #[allow(dead_code)]
    pub fn thinking_dots(&self) -> &'static str {
        const DOTS: [&str; 4] = ["", ".", "..", "..."];
        DOTS[self.animation_frame % DOTS.len()]
    }
}

/// Base trait for all UI components
///
/// A component is anything that can render itself to the terminal.
/// This is the minimum contract - most components will also implement
/// additional traits like `Scrollable` or `Copyable`.
///
/// # Example
///
/// ```ignore
/// struct MyPanel {
///     data: Vec<String>,
/// }
///
/// impl Component for MyPanel {
///     fn id(&self) -> ComponentId {
///         ComponentId::Events // or a new variant
///     }
///
///     fn render(&self, f: &mut Frame, area: Rect, ctx: &RenderContext) {
///         let focused = ctx.is_focused(self.id());
///         // ... render logic
///     }
/// }
/// ```
pub trait Component {
    /// Unique identifier for this component
    #[allow(dead_code)]
    fn id(&self) -> ComponentId;

    /// Render the component to the given area
    ///
    /// # Arguments
    ///
    /// * `f` - The frame to render to
    /// * `area` - The rectangular area allocated for this component
    /// * `ctx` - Immutable render context (theme, focus, animations)
    #[allow(dead_code)]
    fn render(&self, f: &mut Frame, area: Rect, ctx: &RenderContext);
}
