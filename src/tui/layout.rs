/// Responsive breakpoint system for TUI layout decisions.
///
/// Single source of truth for width thresholds - no magic numbers scattered in render code.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breakpoint {
    /// < 60 cols: Split pane, minimal terminal
    Compact,
    /// 60-99 cols: Half-screen
    Normal,
    /// 100-139 cols: Full terminal
    Wide,
    /// 140+ cols: Ultrawide monitor
    UltraWide,
}

impl Breakpoint {
    pub fn from_width(width: u16) -> Self {
        match width {
            0..=59 => Breakpoint::Compact,
            60..=99 => Breakpoint::Normal,
            100..=139 => Breakpoint::Wide,
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
        assert_eq!(Breakpoint::from_width(59), Breakpoint::Compact);
        assert_eq!(Breakpoint::from_width(60), Breakpoint::Normal);
        assert_eq!(Breakpoint::from_width(99), Breakpoint::Normal);
        assert_eq!(Breakpoint::from_width(100), Breakpoint::Wide);
        assert_eq!(Breakpoint::from_width(139), Breakpoint::Wide);
        assert_eq!(Breakpoint::from_width(140), Breakpoint::UltraWide);
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
