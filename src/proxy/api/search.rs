// Log search endpoint - Search session logs for past conversations

use super::ApiError;
use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Request body for POST /api/search
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Required: keyword to search for (case-insensitive)
    pub keyword: String,
    /// Filter by message role: "user" or "assistant"
    pub role: Option<String>,
    /// Specific session filename filter (partial match)
    pub session: Option<String>,
    /// Max results (default: 10, max: 100)
    #[serde(default = "default_search_limit")]
    pub limit: usize,
    /// Time range filter: "today", "before_today", "last_3_days", "last_7_days", "last_30_days"
    pub time_range: Option<String>,
}

fn default_search_limit() -> usize {
    10
}

/// Parse time_range string into (after, before) DateTime bounds
fn parse_time_range(time_range: &str) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    use chrono::{Duration, Timelike};

    let now = Utc::now();
    // Start of today (midnight UTC)
    let today_start = now
        .with_hour(0)
        .unwrap()
        .with_minute(0)
        .unwrap()
        .with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap();

    match time_range.to_lowercase().as_str() {
        "today" => (Some(today_start), None),
        "before_today" => (None, Some(today_start)),
        "last_3_days" => (Some(today_start - Duration::days(3)), None),
        "last_7_days" => (Some(today_start - Duration::days(7)), None),
        "last_30_days" => (Some(today_start - Duration::days(30)), None),
        _ => (None, None), // Unknown range, no filtering
    }
}

/// A single search result
#[derive(Debug, Serialize)]
pub struct SearchResult {
    /// Session filename
    pub session: String,
    /// Message timestamp
    pub timestamp: String,
    /// Role: "user" or "assistant"
    pub role: String,
    /// The matching text snippet (truncated around match)
    pub text: String,
}

/// Response for POST /api/search
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    /// The search query
    pub query: String,
    /// Number of session files searched
    pub sessions_searched: usize,
    /// Total matches found
    pub total_matches: usize,
    /// The results (most recent first)
    pub results: Vec<SearchResult>,
}

/// POST /api/search - Search session logs for past conversations
///
/// Searches through session log files for messages containing the keyword.
/// Useful for recovering context lost to compaction or finding previous decisions.
pub async fn search_logs(
    State(state): State<crate::proxy::ProxyState>,
    Json(query): Json<SearchQuery>,
) -> Result<Json<SearchResponse>, ApiError> {
    use std::fs;
    use std::io::{BufRead, BufReader};

    let keyword_lower = query.keyword.to_lowercase();
    let limit = query.limit.min(100); // Cap at 100 results
    let mut results = Vec::new();

    // Parse time range filter
    let (time_after, time_before) = query
        .time_range
        .as_deref()
        .map(parse_time_range)
        .unwrap_or((None, None));

    // List session files (newest first by filename)
    let mut sessions: Vec<_> = fs::read_dir(&state.log_dir)
        .map_err(|e| ApiError::Internal(format!("Failed to read log directory: {}", e)))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
        .collect();

    // Sort by filename descending (newest first, since filenames include timestamp)
    sessions.sort_by_key(|s| std::cmp::Reverse(s.file_name().to_os_string()));

    // Apply session filter if provided
    if let Some(ref session_filter) = query.session {
        let filter_lower = session_filter.to_lowercase();
        sessions.retain(|s| {
            s.file_name()
                .to_string_lossy()
                .to_lowercase()
                .contains(&filter_lower)
        });
    }

    let sessions_searched = sessions.len();

    'outer: for session_entry in &sessions {
        let file = match fs::File::open(session_entry.path()) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let reader = BufReader::new(file);
        let session_name = session_entry.file_name().to_string_lossy().to_string();

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            // Quick pre-filter before JSON parsing (performance optimization)
            if !line.to_lowercase().contains(&keyword_lower) {
                continue;
            }

            // Parse the event
            let event: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Only search Request events (they contain the messages array)
            if event.get("type").and_then(|t| t.as_str()) != Some("Request") {
                continue;
            }

            let timestamp_str = event
                .get("timestamp")
                .and_then(|t| t.as_str())
                .unwrap_or("");

            // Apply time range filter if specified
            if time_after.is_some() || time_before.is_some() {
                if let Ok(event_time) = timestamp_str.parse::<DateTime<Utc>>() {
                    if let Some(after) = time_after {
                        if event_time < after {
                            continue;
                        }
                    }
                    if let Some(before) = time_before {
                        if event_time >= before {
                            continue;
                        }
                    }
                }
            }

            let timestamp = timestamp_str.to_string();

            // Extract matching messages from body.messages[]
            if let Some(matches) =
                extract_matching_messages(&event, &keyword_lower, query.role.as_deref())
            {
                for (role, text) in matches {
                    results.push(SearchResult {
                        session: session_name.clone(),
                        timestamp: timestamp.clone(),
                        role,
                        text: truncate_around_match(&text, &keyword_lower, 500),
                    });

                    if results.len() >= limit {
                        break 'outer;
                    }
                }
            }
        }
    }

    Ok(Json(SearchResponse {
        query: query.keyword,
        sessions_searched,
        total_matches: results.len(),
        results,
    }))
}

/// Extract messages matching keyword and optional role filter
fn extract_matching_messages(
    event: &serde_json::Value,
    keyword: &str,
    role_filter: Option<&str>,
) -> Option<Vec<(String, String)>> {
    let messages = event.get("body")?.get("messages")?.as_array()?;

    let mut matches = Vec::new();

    for msg in messages {
        let role = msg.get("role")?.as_str()?;

        // Apply role filter
        if let Some(filter) = role_filter {
            if !role.eq_ignore_ascii_case(filter) {
                continue;
            }
        }

        // Extract text content from the message
        let content = msg.get("content")?;
        if let Some(text) = extract_text_content(content) {
            if text.to_lowercase().contains(keyword) {
                matches.push((role.to_string(), text));
            }
        }
    }

    if matches.is_empty() {
        None
    } else {
        Some(matches)
    }
}

/// Extract text from a content value (handles both string and array formats)
fn extract_text_content(content: &serde_json::Value) -> Option<String> {
    // Content can be a string directly
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }

    // Or an array of content blocks
    if let Some(blocks) = content.as_array() {
        let mut text_parts = Vec::new();
        for block in blocks {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    text_parts.push(text.to_string());
                }
            }
        }
        if !text_parts.is_empty() {
            return Some(text_parts.join("\n"));
        }
    }

    None
}

/// Truncate text around the first match of keyword, showing context
///
/// This function is defensive - it never panics even with malformed input.
/// If slicing fails, it logs a warning and gracefully degrades to showing more context.
fn truncate_around_match(text: &str, keyword: &str, max_len: usize) -> String {
    let text_lower = text.to_lowercase();

    // Find the position of the keyword
    if let Some(pos) = text_lower.find(keyword) {
        let half_context = max_len / 2;

        // Calculate start position (with some context before match)
        let start = if pos > half_context {
            // Find a word boundary near our desired start
            let desired_start = pos.saturating_sub(half_context);
            // Ensure we're on a character boundary before slicing
            let safe_start = text.floor_char_boundary(desired_start);

            // Use .get() instead of indexing - returns None if out of bounds
            match text.get(safe_start..) {
                Some(slice) => slice.find(' ').map_or(safe_start, |i| safe_start + i + 1),
                None => {
                    tracing::warn!(
                        "Failed to slice text at start position {} (text len: {})",
                        safe_start,
                        text.len()
                    );
                    0 // Fallback to beginning
                }
            }
        } else {
            0
        };

        // Calculate end position
        let end = (pos + keyword.len() + half_context).min(text.len());
        // Ensure we're on a character boundary before slicing
        let safe_end = text.floor_char_boundary(end);

        let end = match text.get(..safe_end) {
            Some(slice) => slice.rfind(' ').map_or(safe_end, |i| i),
            None => {
                tracing::warn!(
                    "Failed to slice text at end position {} (text len: {})",
                    safe_end,
                    text.len()
                );
                text.len() // Fallback to full length
            }
        };

        // Ensure end >= start (rfind can return position before start)
        let end = end.max(start);

        // Final slice with error handling - this is the critical extraction
        let extracted = match text.get(start..end) {
            Some(slice) => slice,
            None => {
                tracing::warn!(
                    "Failed to extract text[{}..{}] (text len: {}), using full text as fallback",
                    start,
                    end,
                    text.len()
                );
                text // Fallback to full text
            }
        };

        let mut result = String::new();
        if start > 0 {
            result.push_str("...");
        }
        result.push_str(extracted.trim());
        if end < text.len() {
            result.push_str("...");
        }
        result
    } else {
        // Keyword not found (shouldn't happen), just truncate
        if text.len() <= max_len {
            text.to_string()
        } else {
            // Ensure we're on a character boundary
            let safe_len = text.floor_char_boundary(max_len);

            // Safe slice with fallback
            match text.get(..safe_len) {
                Some(slice) => format!("{}...", slice),
                None => {
                    tracing::warn!(
                        "Failed to truncate text at {} (text len: {}), using char-based truncation",
                        safe_len,
                        text.len()
                    );
                    // Ultimate fallback: use char iteration which can't panic
                    text.chars().take(100).collect::<String>() + "..."
                }
            }
        }
    }
}
