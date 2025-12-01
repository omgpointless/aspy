/// Responsive breakpoint system for TUI layout decisions.
///
/// Single source of truth for width thresholds - no magic numbers scattered in render code.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breakpoint {
    /// < 50 cols: Tiny terminal, minimal content
    Compact,
    /// 50-79 cols: Split pane, condensed format
    Normal,
    /// 80-119 cols: Standard terminal, full format
    Wide,
    /// 120+ cols: Wide/ultrawide monitor
    UltraWide,
}

impl Breakpoint {
    pub fn from_width(width: u16) -> Self {
        match width {
            0..=49 => Breakpoint::Compact,
            50..=79 => Breakpoint::Normal,
            80..=119 => Breakpoint::Wide,
            _ => Breakpoint::UltraWide,
        }
    }

    /// Check if at least this breakpoint (inclusive)
    pub fn at_least(&self, min: Breakpoint) -> bool {
        self.ordinal() >= min.ordinal()
    }

    fn ordinal(&self) -> u8 {
        match self {
            Breakpoint::Compact => 0,
            Breakpoint::Normal => 1,
            Breakpoint::Wide => 2,
            Breakpoint::UltraWide => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakpoint_thresholds() {
        assert_eq!(Breakpoint::from_width(40), Breakpoint::Compact);
        assert_eq!(Breakpoint::from_width(49), Breakpoint::Compact);
        assert_eq!(Breakpoint::from_width(50), Breakpoint::Normal);
        assert_eq!(Breakpoint::from_width(79), Breakpoint::Normal);
        assert_eq!(Breakpoint::from_width(80), Breakpoint::Wide);
        assert_eq!(Breakpoint::from_width(119), Breakpoint::Wide);
        assert_eq!(Breakpoint::from_width(120), Breakpoint::UltraWide);
    }

    #[test]
    fn at_least_comparisons() {
        let wide = Breakpoint::Wide;
        assert!(wide.at_least(Breakpoint::Compact));
        assert!(wide.at_least(Breakpoint::Normal));
        assert!(wide.at_least(Breakpoint::Wide));
        assert!(!wide.at_least(Breakpoint::UltraWide));
    }
}
