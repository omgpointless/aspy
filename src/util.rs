//! Shared utility functions

/// Safely truncate a string to at most `max_bytes` while respecting UTF-8 boundaries.
///
/// If the string is already shorter than `max_bytes`, returns it unchanged.
/// Otherwise, finds the last valid UTF-8 character boundary at or before `max_bytes`
/// and returns a slice up to that point.
///
/// # Examples
///
/// ```
/// use aspy::util::truncate_utf8_safe;
///
/// // ASCII: straightforward truncation
/// assert_eq!(truncate_utf8_safe("hello world", 5), "hello");
///
/// // UTF-8: respects character boundaries
/// // "cafe\u{0301}" is "café" where the accent is a combining character
/// let s = "cafe\u{0301}";  // 6 bytes total
/// let truncated = truncate_utf8_safe(s, 5);
/// assert!(truncated.len() <= 5);
/// assert!(truncated.is_char_boundary(truncated.len()));
/// ```
pub fn truncate_utf8_safe(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_shorter_than_max() {
        assert_eq!(truncate_utf8_safe("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_at_ascii_boundary() {
        assert_eq!(truncate_utf8_safe("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_at_utf8_boundary() {
        // 3-byte UTF-8 character: "日" = 0xE6 0x97 0xA5
        let s = "日本語";
        // Each character is 3 bytes, so 9 bytes total
        // Truncating at 4 should give us just "日" (3 bytes)
        assert_eq!(truncate_utf8_safe(s, 4), "日");
        assert_eq!(truncate_utf8_safe(s, 6), "日本");
    }

    #[test]
    fn test_truncate_empty_string() {
        assert_eq!(truncate_utf8_safe("", 5), "");
    }

    #[test]
    fn test_truncate_to_zero() {
        assert_eq!(truncate_utf8_safe("hello", 0), "");
    }
}
