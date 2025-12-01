// Layout preset system for TUI
//
// Enables declarative, configurable layouts instead of hardcoded percentages.
// Core concepts:
// - Panel: a renderable UI component (Events, Thinking, Logs, etc.)
// - LayoutSlot: a panel with sizing constraints
// - Layout: a collection of slots arranged in a direction
// - Preset: a named configuration (shell + per-view layouts)

use super::layout::Breakpoint;
use ratatui::layout::Constraint;

/// All renderable panels in the TUI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    // Shell panels (always visible, frame the content)
    Title,
    Logs,
    ContextBar,
    Status,

    // Content panels (arranged by view layouts)
    Events,
    Thinking,

    // View-specific panels
    Stats,
    #[allow(dead_code)] // Future: settings panel in preset
    Settings,
}

/// Direction for laying out panels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutDirection {
    #[default]
    Vertical,
    Horizontal,
}

/// Responsive sizing rule - adjusts constraint based on breakpoint
#[derive(Debug, Clone)]
pub struct ResponsiveRule {
    /// Minimum breakpoint to show this panel
    pub min_breakpoint: Option<Breakpoint>,
    /// Constraint overrides per breakpoint
    pub overrides: Vec<(Breakpoint, SizeConstraint)>,
}

impl ResponsiveRule {
    /// Panel hidden below this breakpoint
    #[allow(dead_code)] // Future: responsive panel hiding
    pub fn hidden_below(bp: Breakpoint) -> Self {
        Self {
            min_breakpoint: Some(bp),
            overrides: Vec::new(),
        }
    }

    /// Different sizes at different breakpoints
    pub fn with_overrides(overrides: Vec<(Breakpoint, SizeConstraint)>) -> Self {
        Self {
            min_breakpoint: None,
            overrides,
        }
    }
}

/// Size constraint for a panel slot
#[derive(Debug, Clone, Copy)]
pub enum SizeConstraint {
    /// Fixed number of lines/columns
    Fixed(u16),
    /// Percentage of available space
    Percent(u16),
    /// Minimum size, grows to fill
    Min(u16),
    /// Fill remaining space equally with other Fill slots
    Fill,
}

impl SizeConstraint {
    /// Convert to ratatui Constraint
    pub fn to_constraint(self) -> Constraint {
        match self {
            SizeConstraint::Fixed(n) => Constraint::Length(n),
            SizeConstraint::Percent(p) => Constraint::Percentage(p),
            SizeConstraint::Min(n) => Constraint::Min(n),
            SizeConstraint::Fill => Constraint::Min(1), // Will be calculated
        }
    }
}

/// A slot in a layout - a panel with its sizing
#[derive(Debug, Clone)]
pub struct LayoutSlot {
    pub panel: Panel,
    pub size: SizeConstraint,
    pub responsive: Option<ResponsiveRule>,
}

impl LayoutSlot {
    pub fn new(panel: Panel, size: SizeConstraint) -> Self {
        Self {
            panel,
            size,
            responsive: None,
        }
    }

    pub fn with_responsive(mut self, rule: ResponsiveRule) -> Self {
        self.responsive = Some(rule);
        self
    }

    /// Check if this slot should be visible at the given breakpoint
    pub fn visible_at(&self, bp: Breakpoint) -> bool {
        match &self.responsive {
            Some(rule) => match rule.min_breakpoint {
                Some(min) => bp.at_least(min),
                None => true,
            },
            None => true,
        }
    }

    /// Get the effective constraint at a breakpoint
    pub fn constraint_at(&self, bp: Breakpoint) -> Constraint {
        if let Some(rule) = &self.responsive {
            // Check for breakpoint-specific override
            for (breakpoint, size) in &rule.overrides {
                if bp.at_least(*breakpoint) {
                    return size.to_constraint();
                }
            }
        }
        self.size.to_constraint()
    }
}

/// A layout defines how panels are arranged
#[derive(Debug, Clone)]
pub struct Layout {
    pub direction: LayoutDirection,
    pub slots: Vec<LayoutSlot>,
}

impl Layout {
    pub fn vertical(slots: Vec<LayoutSlot>) -> Self {
        Self {
            direction: LayoutDirection::Vertical,
            slots,
        }
    }

    pub fn horizontal(slots: Vec<LayoutSlot>) -> Self {
        Self {
            direction: LayoutDirection::Horizontal,
            slots,
        }
    }

    /// Get visible slots and their constraints for a breakpoint
    pub fn resolve(&self, bp: Breakpoint) -> Vec<(Panel, Constraint)> {
        self.slots
            .iter()
            .filter(|slot| slot.visible_at(bp))
            .map(|slot| (slot.panel, slot.constraint_at(bp)))
            .collect()
    }
}

/// Shell configuration - the outer frame that wraps all views
#[derive(Debug, Clone)]
pub struct ShellConfig {
    /// Panels above the content area
    pub header: Vec<LayoutSlot>,
    /// Panels below the content area
    pub footer: Vec<LayoutSlot>,
}

/// View-specific content layout
#[derive(Debug, Clone)]
pub struct ViewLayout {
    pub layout: Layout,
}

/// A complete preset - named configuration for the entire UI
#[derive(Debug, Clone)]
pub struct Preset {
    pub name: String,
    pub shell: ShellConfig,
    pub events_view: ViewLayout,
    #[allow(dead_code)] // Future: preset-driven stats layout
    pub stats_view: ViewLayout,
    #[allow(dead_code)] // Future: preset-driven settings layout
    pub settings_view: Option<ViewLayout>,
    /// Focus order for Tab cycling (follows visual layout order)
    pub focus_order: Vec<super::scroll::FocusablePanel>,
}

impl Preset {
    /// The "classic" preset - replicates current hardcoded layout
    pub fn classic() -> Self {
        use super::scroll::FocusablePanel;
        Self {
            name: "classic".to_string(),

            shell: ShellConfig {
                header: vec![LayoutSlot::new(Panel::Title, SizeConstraint::Fixed(3))],
                footer: vec![
                    LayoutSlot::new(Panel::Logs, SizeConstraint::Fixed(6)),
                    LayoutSlot::new(Panel::ContextBar, SizeConstraint::Fixed(1)),
                    LayoutSlot::new(Panel::Status, SizeConstraint::Fixed(2)),
                ],
            },

            events_view: ViewLayout {
                layout: Layout::horizontal(vec![
                    LayoutSlot::new(Panel::Events, SizeConstraint::Percent(65)),
                    // Thinking always visible - responsive overrides adjust size by breakpoint
                    LayoutSlot::new(Panel::Thinking, SizeConstraint::Percent(35)).with_responsive(
                        ResponsiveRule::with_overrides(vec![
                            (Breakpoint::UltraWide, SizeConstraint::Percent(30)),
                            (Breakpoint::Wide, SizeConstraint::Percent(35)),
                            (Breakpoint::Normal, SizeConstraint::Percent(40)),
                        ]),
                    ),
                ]),
            },

            stats_view: ViewLayout {
                layout: Layout::vertical(vec![LayoutSlot::new(Panel::Stats, SizeConstraint::Fill)]),
            },

            settings_view: None,

            // Focus order: left-to-right (Events | Thinking), then footer (Logs)
            focus_order: vec![
                FocusablePanel::Events,
                FocusablePanel::Thinking,
                FocusablePanel::Logs,
            ],
        }
    }

    /// "Reasoning" preset - thinking panel takes priority
    pub fn reasoning() -> Self {
        use super::scroll::FocusablePanel;
        Self {
            name: "reasoning".to_string(),

            shell: ShellConfig {
                header: vec![LayoutSlot::new(Panel::Title, SizeConstraint::Fixed(3))],
                footer: vec![
                    LayoutSlot::new(Panel::Logs, SizeConstraint::Fixed(4)), // Smaller logs
                    LayoutSlot::new(Panel::ContextBar, SizeConstraint::Fixed(1)),
                    LayoutSlot::new(Panel::Status, SizeConstraint::Fixed(2)),
                ],
            },

            events_view: ViewLayout {
                layout: Layout::vertical(vec![
                    // Thinking on top, takes most space
                    LayoutSlot::new(Panel::Thinking, SizeConstraint::Percent(65)),
                    // Events below, smaller
                    LayoutSlot::new(Panel::Events, SizeConstraint::Percent(35)),
                ]),
            },

            stats_view: ViewLayout {
                layout: Layout::vertical(vec![LayoutSlot::new(Panel::Stats, SizeConstraint::Fill)]),
            },

            settings_view: None,

            // Focus order: top-to-bottom (Thinking, Events), then footer (Logs)
            focus_order: vec![
                FocusablePanel::Thinking,
                FocusablePanel::Events,
                FocusablePanel::Logs,
            ],
        }
    }

    /// "Debug" preset - logs panel expanded
    pub fn debug() -> Self {
        use super::scroll::FocusablePanel;
        Self {
            name: "debug".to_string(),

            shell: ShellConfig {
                header: vec![LayoutSlot::new(Panel::Title, SizeConstraint::Fixed(3))],
                footer: vec![
                    // Much bigger logs panel
                    LayoutSlot::new(Panel::Logs, SizeConstraint::Min(12)),
                    LayoutSlot::new(Panel::ContextBar, SizeConstraint::Fixed(1)),
                    LayoutSlot::new(Panel::Status, SizeConstraint::Fixed(2)),
                ],
            },

            events_view: ViewLayout {
                layout: Layout::horizontal(vec![
                    LayoutSlot::new(Panel::Thinking, SizeConstraint::Percent(40)),
                    LayoutSlot::new(Panel::Events, SizeConstraint::Percent(60)),
                ]),
            },

            stats_view: ViewLayout {
                layout: Layout::vertical(vec![LayoutSlot::new(Panel::Stats, SizeConstraint::Fill)]),
            },

            settings_view: None,

            // Focus order: left-to-right (Thinking | Events), then footer (Logs)
            focus_order: vec![
                FocusablePanel::Thinking,
                FocusablePanel::Events,
                FocusablePanel::Logs,
            ],
        }
    }
}

/// Get preset by name
pub fn get_preset(name: &str) -> Preset {
    match name.to_lowercase().as_str() {
        "reasoning" => Preset::reasoning(),
        "debug" => Preset::debug(),
        _ => Preset::classic(), // Default fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_preset_structure() {
        let preset = Preset::classic();
        assert_eq!(preset.name, "classic");
        assert_eq!(preset.shell.header.len(), 1); // Title only
        assert_eq!(preset.shell.footer.len(), 3); // Logs, ContextBar, Status
    }

    #[test]
    fn responsive_visibility() {
        let slot = LayoutSlot::new(Panel::Thinking, SizeConstraint::Percent(35))
            .with_responsive(ResponsiveRule::hidden_below(Breakpoint::Normal));

        assert!(!slot.visible_at(Breakpoint::Compact));
        assert!(slot.visible_at(Breakpoint::Normal));
        assert!(slot.visible_at(Breakpoint::Wide));
    }

    #[test]
    fn layout_resolve() {
        let layout = Layout::horizontal(vec![
            LayoutSlot::new(Panel::Events, SizeConstraint::Percent(60)),
            LayoutSlot::new(Panel::Thinking, SizeConstraint::Percent(40))
                .with_responsive(ResponsiveRule::hidden_below(Breakpoint::Normal)),
        ]);

        let compact = layout.resolve(Breakpoint::Compact);
        assert_eq!(compact.len(), 1); // Only Events visible

        let wide = layout.resolve(Breakpoint::Wide);
        assert_eq!(wide.len(), 2); // Both visible
    }
}
