// Session tracking for multi-user support
//
// This module tracks Claude Code sessions across multiple users. Each user
// is identified by a hash of their API key, and sessions are tracked either
// explicitly (via SessionStart/End hooks) or implicitly (via first-seen and
// warmup detection).
//
// Architecture:
// - Primary key: SessionKey (explicit session_id or implicit api_key_hash)
// - Sessions grouped by: UserId (api_key_hash)
// - Reverse index: active_by_user for quick "does this user have a session?"
//
// NOTE: Much of this module is scaffolding for planned features (idle detection,
// session aggregation, multi-user dashboards). Suppressing dead_code warnings
// until these features are wired up.
#![allow(dead_code)]

use crate::events::{ProxyEvent, Stats};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

// ─────────────────────────────────────────────────────────────────────────────
// Core Types
// ─────────────────────────────────────────────────────────────────────────────

/// User identity derived from API key hash (SHA-256, first 16 hex chars)
///
/// Example: "a3f2c91b4e8d7f01"
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct UserId(pub String);

impl UserId {
    /// Create from raw api_key_hash string
    pub fn new(hash: impl Into<String>) -> Self {
        Self(hash.into())
    }

    /// Short display format (first 8 chars)
    pub fn short(&self) -> &str {
        &self.0[..8.min(self.0.len())]
    }
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Session identifier - either explicit from hooks or implicit from API key
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum SessionKey {
    /// Explicit session ID from Claude Code's SessionStart hook
    Explicit(String),
    /// Implicit session derived from api_key_hash (fallback when no hooks)
    Implicit(String),
}

impl SessionKey {
    /// Create explicit key from hook-provided session_id
    pub fn explicit(session_id: impl Into<String>) -> Self {
        Self::Explicit(session_id.into())
    }

    /// Create implicit key for mid-restart sessions
    ///
    /// Generates a unique session ID like "foundry-a3f2" to distinguish
    /// from explicit sessions and make it clear this is a recovery session.
    pub fn implicit(api_key_hash: impl Into<String>) -> Self {
        let hash = api_key_hash.into();
        let short_user = &hash[..8.min(hash.len())];

        // Generate short random suffix for uniqueness
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u32)
            .unwrap_or(0);
        let suffix = format!("{:04x}", ts & 0xFFFF);

        Self::Implicit(format!("{}-{}", short_user, suffix))
    }

    /// Get the underlying string value
    pub fn as_str(&self) -> &str {
        match self {
            Self::Explicit(s) | Self::Implicit(s) => s,
        }
    }

    /// Check if this is an explicit (hook-provided) session
    pub fn is_explicit(&self) -> bool {
        matches!(self, Self::Explicit(_))
    }
}

impl std::fmt::Display for SessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Explicit(s) => write!(f, "{}", s),
            Self::Implicit(s) => write!(f, "~{}", s), // ~ prefix indicates recovery session
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Session Status
// ─────────────────────────────────────────────────────────────────────────────

/// Current status of a session
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum SessionStatus {
    /// Session is actively receiving events
    #[default]
    Active,
    /// Session hasn't received events recently
    Idle {
        /// When the session became idle
        since: DateTime<Utc>,
    },
    /// Session has ended
    Ended {
        /// Why the session ended
        reason: EndReason,
        /// When it ended
        ended: DateTime<Utc>,
    },
}

/// Reason a session ended
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EndReason {
    /// Explicit SessionEnd hook was received
    Hook,
    /// A new session started for the same user (superseded)
    Superseded,
    /// Session timed out due to inactivity
    Timeout,
}

impl std::fmt::Display for EndReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hook => write!(f, "hook"),
            Self::Superseded => write!(f, "superseded"),
            Self::Timeout => write!(f, "timeout"),
        }
    }
}

/// How the session was detected/started
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionSource {
    /// Explicit SessionStart hook
    Hook,
    /// Warmup pattern detected in request
    Warmup,
    /// First request seen from this user (no hook, no warmup)
    FirstSeen,
    /// Reconnected to historical session via transcript_path lookup
    Reconnected,
}

impl std::fmt::Display for SessionSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hook => write!(f, "hook"),
            Self::Warmup => write!(f, "warmup"),
            Self::FirstSeen => write!(f, "first-seen"),
            Self::Reconnected => write!(f, "reconnected"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Todo State (intercepted from TodoWrite tool calls)
// ─────────────────────────────────────────────────────────────────────────────

/// Status of a todo item (matches Claude Code's TodoWrite schema)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

impl std::fmt::Display for TodoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
        }
    }
}

/// A single todo item from Claude Code's TodoWrite tool
///
/// This matches the schema Claude Code uses:
/// - content: The imperative form ("Run tests", "Fix bug")
/// - activeForm: Present continuous ("Running tests", "Fixing bug")
/// - status: pending | in_progress | completed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    /// What needs to be done (imperative form)
    pub content: String,
    /// Current status of this todo
    pub status: TodoStatus,
    /// Active form shown during execution (present continuous)
    #[serde(rename = "activeForm")]
    pub active_form: String,
}

/// Parse todos from a TodoWrite tool call input
///
/// The input looks like:
/// ```json
/// {"todos": [{"content": "Fix bug", "status": "in_progress", "activeForm": "Fixing bug"}, ...]}
/// ```
pub fn parse_todos_from_input(input: &serde_json::Value) -> Option<Vec<TodoItem>> {
    let todos_array = input.get("todos")?.as_array()?;
    let mut result = Vec::with_capacity(todos_array.len());

    for item in todos_array {
        let content = item.get("content")?.as_str()?.to_string();
        let active_form = item.get("activeForm")?.as_str()?.to_string();
        let status_str = item.get("status")?.as_str()?;

        let status = match status_str {
            "pending" => TodoStatus::Pending,
            "in_progress" => TodoStatus::InProgress,
            "completed" => TodoStatus::Completed,
            _ => continue, // Skip unknown status
        };

        result.push(TodoItem {
            content,
            status,
            active_form,
        });
    }

    Some(result)
}

// ─────────────────────────────────────────────────────────────────────────────
// Context State
// ─────────────────────────────────────────────────────────────────────────────

/// Per-session context window state (not aggregatable)
///
/// This represents the current state of a session's context window - values that
/// get overwritten on each API call rather than accumulated. Unlike Stats which
/// can be meaningfully summed/merged across sessions, ContextState is inherently
/// per-session: each Claude Code conversation has its own context window.
///
/// # Why separate from Stats?
/// - `current_tokens` = "what is the context size RIGHT NOW" (snapshot)
/// - Stats fields = "how many tokens have we used TOTAL" (aggregate)
///
/// Mixing these in Stats caused semantic confusion in multi-user scenarios
/// where global "current context" was meaningless (just "whoever was last").
#[derive(Debug, Clone, Default)]
pub struct ContextState {
    /// Current context size from last ApiUsage event
    /// This is input + cache_creation + cache_read tokens
    pub current_tokens: u64,

    /// Cache tokens from last ApiUsage (for input vs cached breakdown)
    pub last_cached: u64,

    /// Context limit from config (copied per-session for convenience)
    pub limit: u64,
}

impl ContextState {
    /// Create with a specific context limit
    pub fn with_limit(limit: u64) -> Self {
        Self {
            current_tokens: 0,
            last_cached: 0,
            limit,
        }
    }

    /// Calculate context usage percentage
    pub fn percentage(&self) -> f64 {
        if self.limit == 0 {
            0.0
        } else {
            (self.current_tokens as f64 / self.limit as f64) * 100.0
        }
    }

    /// Get the input tokens (non-cached portion)
    pub fn input_tokens(&self) -> u64 {
        self.current_tokens.saturating_sub(self.last_cached)
    }

    /// Update from an ApiUsage event
    pub fn update_from_api_usage(
        &mut self,
        input_tokens: u32,
        cache_creation_tokens: u32,
        cache_read_tokens: u32,
    ) {
        self.current_tokens =
            input_tokens as u64 + cache_creation_tokens as u64 + cache_read_tokens as u64;
        self.last_cached = cache_read_tokens as u64;
    }

    /// Update after context compaction
    pub fn update_from_compact(&mut self, new_context: u64) {
        self.current_tokens = new_context;
        self.last_cached = 0;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Session
// ─────────────────────────────────────────────────────────────────────────────

/// Default idle timeout before marking session as idle
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

/// Maximum events to keep in session buffer
const MAX_SESSION_EVENTS: usize = 500;

/// A single Claude Code session
#[derive(Debug, Clone)]
pub struct Session {
    /// Session identifier (explicit or implicit)
    pub key: SessionKey,

    /// User this session belongs to
    pub user_id: UserId,

    /// Claude Code's session_id (only if from hook)
    pub claude_session_id: Option<String>,

    /// How this session was detected
    pub source: SessionSource,

    /// When the session started (wall clock)
    pub started: DateTime<Utc>,

    /// When we last saw activity (monotonic, for timeout detection)
    pub last_activity: Instant,

    /// Session-specific statistics (aggregatable metrics)
    pub stats: Stats,

    /// Context window state (per-session, not aggregatable)
    pub context: ContextState,

    /// Recent events buffer (most recent last)
    pub events: VecDeque<ProxyEvent>,

    /// Current session status
    pub status: SessionStatus,

    /// Current todo list state (intercepted from TodoWrite tool calls)
    ///
    /// Updated whenever Claude calls the TodoWrite tool. This captures
    /// what Claude is currently working on - useful for:
    /// - Context recovery after compaction (keywords!)
    /// - Session visualization in TUI
    /// - Understanding session progress
    pub todos: Vec<TodoItem>,

    /// When the todos were last updated
    pub todos_updated: Option<DateTime<Utc>>,

    /// Path to Claude Code's transcript file
    ///
    /// Captured from SessionStart hook. Maps user_id → session → transcript file.
    /// Enables cross-referencing aspy events with Claude's native storage.
    /// Example: ~/.claude/projects/.../abc123.jsonl
    pub transcript_path: Option<String>,
}

impl Session {
    /// Create a new session
    ///
    /// The `context_limit` is typically from config.context_limit.
    pub fn new(
        key: SessionKey,
        user_id: UserId,
        source: SessionSource,
        context_limit: u64,
    ) -> Self {
        let claude_session_id = match &key {
            SessionKey::Explicit(id) => Some(id.clone()),
            SessionKey::Implicit(_) => None,
        };

        Self {
            key,
            user_id,
            claude_session_id,
            source,
            started: Utc::now(),
            last_activity: Instant::now(),
            stats: Stats::default(),
            context: ContextState::with_limit(context_limit),
            events: VecDeque::with_capacity(MAX_SESSION_EVENTS),
            status: SessionStatus::Active,
            todos: Vec::new(),
            todos_updated: None,
            transcript_path: None,
        }
    }

    /// Record an event in this session
    pub fn record_event(&mut self, event: ProxyEvent) {
        self.last_activity = Instant::now();
        self.status = SessionStatus::Active;

        // Update stats based on event type
        self.stats.update(&event);

        // Update context state and intercept special tool calls
        match &event {
            ProxyEvent::ApiUsage {
                input_tokens,
                cache_creation_tokens,
                cache_read_tokens,
                model,
                ..
            } => {
                // Don't update context for Haiku (summarization, not main conversation)
                if !model.contains("haiku") {
                    self.context.update_from_api_usage(
                        *input_tokens,
                        *cache_creation_tokens,
                        *cache_read_tokens,
                    );
                }
            }
            ProxyEvent::ContextCompact { new_context, .. } => {
                self.context.update_from_compact(*new_context);
            }
            // Intercept TodoWrite tool calls to track Claude's current tasks
            ProxyEvent::ToolCall {
                tool_name, input, ..
            } if tool_name == "TodoWrite" => {
                if let Some(todos) = parse_todos_from_input(input) {
                    tracing::debug!(
                        user = %self.user_id.short(),
                        todo_count = todos.len(),
                        "TodoWrite intercepted: {} todos",
                        todos.len()
                    );
                    self.todos = todos;
                    self.todos_updated = Some(Utc::now());
                }
            }
            _ => {}
        }

        // Add to event buffer (bounded)
        if self.events.len() >= MAX_SESSION_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    /// Check if session should be marked idle
    pub fn check_idle(&mut self, timeout: Duration) {
        if matches!(self.status, SessionStatus::Active) && self.last_activity.elapsed() > timeout {
            self.status = SessionStatus::Idle { since: Utc::now() };
        }
    }

    /// Mark session as ended
    pub fn end(&mut self, reason: EndReason) {
        self.status = SessionStatus::Ended {
            reason,
            ended: Utc::now(),
        };
    }

    /// Duration since session started
    pub fn duration(&self) -> Duration {
        self.last_activity.saturating_duration_since(
            Instant::now()
                - Duration::from_secs((Utc::now() - self.started).num_seconds().max(0) as u64),
        )
    }

    /// Check if session is active (not ended)
    pub fn is_active(&self) -> bool {
        !matches!(self.status, SessionStatus::Ended { .. })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Session Manager
// ─────────────────────────────────────────────────────────────────────────────

/// Manages all sessions across users
#[derive(Debug)]
pub struct SessionManager {
    /// All sessions indexed by key
    sessions: HashMap<SessionKey, Session>,

    /// Reverse index: user_id -> their current active session key
    active_by_user: HashMap<UserId, SessionKey>,

    /// Ended sessions for history (bounded)
    history: VecDeque<Session>,

    /// Idle timeout configuration
    idle_timeout: Duration,

    /// Context limit from config (passed to new sessions)
    context_limit: u64,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new(DEFAULT_IDLE_TIMEOUT, 150_000) // Default context limit
    }
}

impl SessionManager {
    /// Create a new session manager with specified idle timeout and context limit
    pub fn new(idle_timeout: Duration, context_limit: u64) -> Self {
        Self {
            sessions: HashMap::new(),
            active_by_user: HashMap::new(),
            history: VecDeque::with_capacity(100),
            idle_timeout,
            context_limit,
        }
    }

    /// Start a new session (from hook or warmup detection)
    ///
    /// If the user already has an active session, it's superseded.
    pub fn start_session(
        &mut self,
        user_id: UserId,
        session_id: Option<String>,
        source: SessionSource,
        transcript_path: Option<String>,
    ) -> &Session {
        // Create session key
        let key = match session_id {
            Some(ref id) => {
                tracing::debug!(
                    session_id = %id,
                    user = %user_id.short(),
                    source = %source,
                    "Creating explicit session (from hook) for user {} with session_id {}",
                    user_id.short(),
                    id
                );
                SessionKey::explicit(id)
            }
            None => {
                let key = SessionKey::implicit(&user_id.0);
                tracing::debug!(
                    session_key = %key,
                    user = %user_id.short(),
                    source = %source,
                    "Creating implicit session (mid-restart recovery) for user {} with session_key {}",
                    user_id.short(),
                    key
                );
                key
            }
        };

        // Supersede existing session for this user
        if let Some(old_key) = self.active_by_user.remove(&user_id) {
            if let Some(mut old_session) = self.sessions.remove(&old_key) {
                tracing::debug!(
                    old_session = %old_key,
                    new_session = %key,
                    user = %user_id.short(),
                    "Superseding existing session for user {} with session_key {}",
                    user_id.short(),
                    old_key
                );
                old_session.end(EndReason::Superseded);
                self.archive_session(old_session);
            }
        }

        // Create and insert new session
        let mut session = Session::new(key.clone(), user_id.clone(), source, self.context_limit);
        session.transcript_path = transcript_path;
        self.sessions.insert(key.clone(), session);
        self.active_by_user.insert(user_id, key.clone());

        self.sessions.get(&key).unwrap()
    }

    /// End a session explicitly (from hook)
    pub fn end_session(&mut self, key: &SessionKey, reason: EndReason) {
        if let Some(mut session) = self.sessions.remove(key) {
            tracing::debug!(
                session = %key,
                user = %session.user_id.short(),
                reason = %reason,
                "Session ended for user {} with session_key {}",
                session.user_id.short(),
                key
            );
            self.active_by_user.remove(&session.user_id);
            session.end(reason);
            self.archive_session(session);
        }
    }

    /// End a session by user ID
    pub fn end_session_by_user(&mut self, user_id: &UserId, reason: EndReason) {
        if let Some(key) = self.active_by_user.remove(user_id) {
            if let Some(mut session) = self.sessions.remove(&key) {
                tracing::debug!(
                    session = %key,
                    user = %user_id.short(),
                    reason = %reason,
                    "Session ended by user {} with session_key {}",
                    user_id.short(),
                    key
                );
                session.end(reason);
                self.archive_session(session);
            }
        }
    }

    /// Reconnect a user to an existing session (from DB lookup)
    ///
    /// Called when UserPromptSubmit hook identifies that this user was previously
    /// working on this transcript_path. Replaces the current implicit session
    /// with the historical session_id to preserve continuity.
    ///
    /// Returns true if reconnection succeeded.
    pub fn reconnect_to_session(
        &mut self,
        user_id: &UserId,
        session_id: &str,
        transcript_path: String,
    ) -> bool {
        // Check if user already has a session
        if let Some(current_key) = self.active_by_user.get(user_id).cloned() {
            // If already using the target session, just ensure transcript_path is set
            if current_key.to_string().contains(session_id) {
                if let Some(session) = self.sessions.get_mut(&current_key) {
                    session.transcript_path = Some(transcript_path);
                }
                return true;
            }

            // Archive the current (implicit) session
            if let Some(mut old_session) = self.sessions.remove(&current_key) {
                tracing::debug!(
                    old_session = %current_key,
                    new_session = %session_id,
                    user = %user_id.short(),
                    "Replacing implicit session with reconnected session"
                );
                old_session.end(EndReason::Superseded);
                self.archive_session(old_session);
            }
            self.active_by_user.remove(user_id);
        }

        // Create session with the historical session_id
        let key = SessionKey::explicit(session_id);
        let mut session = Session::new(
            key.clone(),
            user_id.clone(),
            SessionSource::Reconnected,
            self.context_limit,
        );
        session.transcript_path = Some(transcript_path);

        self.sessions.insert(key.clone(), session);
        self.active_by_user.insert(user_id.clone(), key);

        true
    }

    /// Record an event for a user
    ///
    /// If no session exists, creates an implicit one (FirstSeen).
    pub fn record_event(&mut self, user_id: &UserId, event: ProxyEvent) {
        // Ensure user has a session
        let key = self.active_by_user.get(user_id).cloned();

        match key {
            Some(key) => {
                if let Some(session) = self.sessions.get_mut(&key) {
                    session.record_event(event);
                }
            }
            None => {
                // Create implicit session on first event (no transcript path available)
                self.start_session(user_id.clone(), None, SessionSource::FirstSeen, None);
                // Record the event in the newly created session
                if let Some(key) = self.active_by_user.get(user_id) {
                    if let Some(session) = self.sessions.get_mut(key) {
                        session.record_event(event);
                    }
                }
            }
        }
    }

    /// Get session by key
    pub fn get_session(&self, key: &SessionKey) -> Option<&Session> {
        self.sessions.get(key)
    }

    /// Get active session for a user
    pub fn get_user_session(&self, user_id: &UserId) -> Option<&Session> {
        self.active_by_user
            .get(user_id)
            .and_then(|key| self.sessions.get(key))
    }

    /// Get mutable session for a user
    pub fn get_user_session_mut(&mut self, user_id: &UserId) -> Option<&mut Session> {
        let key = self.active_by_user.get(user_id)?.clone();
        self.sessions.get_mut(&key)
    }

    /// Get session ID for a user (for ProcessContext)
    ///
    /// Returns the session ID string if the user has an active session.
    /// Uses Display format which includes `~` prefix for implicit (recovery) sessions.
    pub fn get_session_id(&self, user_id: &UserId) -> Option<String> {
        self.get_user_session(user_id)
            .map(|session| session.key.to_string()) // Use Display to include ~ prefix
    }

    /// List all active sessions
    pub fn active_sessions(&self) -> impl Iterator<Item = &Session> {
        self.sessions.values().filter(|s| s.is_active())
    }

    /// List all sessions (including idle but not ended)
    pub fn all_sessions(&self) -> impl Iterator<Item = &Session> {
        self.sessions.values()
    }

    /// List recent ended sessions
    pub fn session_history(&self) -> impl Iterator<Item = &Session> {
        self.history.iter()
    }

    /// Count active sessions
    pub fn active_count(&self) -> usize {
        self.sessions.values().filter(|s| s.is_active()).count()
    }

    /// Check for idle sessions and update their status
    pub fn check_idle_sessions(&mut self) {
        for session in self.sessions.values_mut() {
            session.check_idle(self.idle_timeout);
        }
    }

    /// Clean up timed-out sessions
    pub fn cleanup_timed_out(&mut self, timeout: Duration) {
        let timed_out: Vec<SessionKey> = self
            .sessions
            .iter()
            .filter(|(_, s)| {
                matches!(s.status, SessionStatus::Idle { .. })
                    && s.last_activity.elapsed() > timeout
            })
            .map(|(k, _)| k.clone())
            .collect();

        for key in timed_out {
            self.end_session(&key, EndReason::Timeout);
        }
    }

    /// Archive a session to history
    fn archive_session(&mut self, session: Session) {
        if self.history.len() >= 100 {
            self.history.pop_front();
        }
        self.history.push_back(session);
    }

    /// Get aggregate stats across all active sessions
    pub fn aggregate_stats(&self) -> Stats {
        let mut aggregate = Stats::default();
        for session in self.sessions.values() {
            aggregate.merge(&session.stats);
        }
        aggregate
    }

    /// Get stats for a specific user
    pub fn user_stats(&self, user_id: &UserId) -> Option<Stats> {
        self.get_user_session(user_id).map(|s| s.stats.clone())
    }

    /// Get current turn count for a user (without incrementing)
    pub fn get_turn_count(&self, user_id: &UserId) -> u64 {
        self.get_user_session(user_id)
            .map(|s| s.stats.turn_count)
            .unwrap_or(0)
    }

    /// Increment turn count for a user and return the new value
    ///
    /// Called when a fresh user prompt arrives (tool_result_count == 0).
    /// Returns the new turn count, or 1 if no session exists yet.
    pub fn increment_turn_count(&mut self, user_id: &UserId) -> u64 {
        if let Some(session) = self.get_user_session_mut(user_id) {
            session.stats.turn_count += 1;
            session.stats.turn_count
        } else {
            // No session yet - will be 1 when session starts
            1
        }
    }

    /// Backfill user_id for sessions with "unknown" user
    ///
    /// Called when we see a request with api_key_hash - updates any active
    /// session that has user_id="unknown" (from hooks that couldn't access API key)
    pub fn backfill_user_id(&mut self, api_key_hash: &str) {
        let new_user_id = UserId::new(api_key_hash);

        // Find sessions with unknown user_id and update them
        for session in self.sessions.values_mut() {
            if session.user_id.0 == "unknown" && session.is_active() {
                tracing::info!(
                    session_key = %session.key,
                    old_user = "unknown",
                    new_user = %new_user_id,
                    "Backfilled session user_id from request headers for user {} with session_key {}",
                    new_user_id.short(),
                    session.key
                );
                session.user_id = new_user_id.clone();

                // Also update the reverse index
                let key = session.key.clone();
                self.active_by_user.insert(new_user_id.clone(), key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_id_short() {
        let id = UserId::new("a3f2c91b4e8d7f01");
        assert_eq!(id.short(), "a3f2c91b");
    }

    #[test]
    fn test_session_key_display() {
        let explicit = SessionKey::explicit("abc123");
        let implicit = SessionKey::implicit("a3f2c91b4e8d7f01");

        // Explicit sessions display as-is
        assert_eq!(format!("{}", explicit), "abc123");

        // Implicit sessions have ~ prefix and generated suffix
        let implicit_str = format!("{}", implicit);
        assert!(
            implicit_str.starts_with("~a3f2c91b-"),
            "Implicit session should start with ~user_short-: {}",
            implicit_str
        );
    }

    #[test]
    fn test_session_manager_supersession() {
        let mut manager = SessionManager::default();
        let user = UserId::new("user1");

        // First session
        manager.start_session(
            user.clone(),
            Some("session1".to_string()),
            SessionSource::Hook,
            None,
        );
        assert_eq!(manager.active_count(), 1);

        // Second session supersedes first
        manager.start_session(
            user.clone(),
            Some("session2".to_string()),
            SessionSource::Hook,
            None,
        );
        assert_eq!(manager.active_count(), 1);
        assert_eq!(manager.history.len(), 1);

        // Check superseded session is in history
        let archived = manager.history.front().unwrap();
        assert!(matches!(
            archived.status,
            SessionStatus::Ended {
                reason: EndReason::Superseded,
                ..
            }
        ));
    }
}
