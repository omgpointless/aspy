// Number formatters
//
// Shared formatting utilities for displaying numbers in the TUI.

/// Format a large number with commas for readability
///
/// # Examples
/// ```ignore
/// assert_eq!(format_number(1234567), "1,234,567");
/// assert_eq!(format_number(42), "42");
/// ```
pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();

    for (count, ch) in s.chars().rev().enumerate() {
        if count > 0 && count % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, ch);
    }

    result
}

/// Format a number compactly with K/M suffixes
///
/// # Examples
/// ```ignore
/// assert_eq!(format_compact_number(954356), "954K");
/// assert_eq!(format_compact_number(1_500_000), "1.5M");
/// assert_eq!(format_compact_number(42), "42");
/// ```
pub fn format_compact_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{}K", n / 1_000)
    } else {
        n.to_string()
    }
}
