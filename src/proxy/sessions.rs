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

    /// Create implicit key from api_key_hash
    pub fn implicit(api_key_hash: impl Into<String>) -> Self {
        Self::Implicit(api_key_hash.into())
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
            Self::Explicit(s) => write!(f, "session:{}", s),
            Self::Implicit(s) => write!(f, "user:{}", s),
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
}

impl std::fmt::Display for SessionSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hook => write!(f, "hook"),
            Self::Warmup => write!(f, "warmup"),
            Self::FirstSeen => write!(f, "first-seen"),
        }
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

    /// Session-specific statistics
    pub stats: Stats,

    /// Recent events buffer (most recent last)
    pub events: VecDeque<ProxyEvent>,

    /// Current session status
    pub status: SessionStatus,
}

impl Session {
    /// Create a new session
    pub fn new(key: SessionKey, user_id: UserId, source: SessionSource) -> Self {
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
            events: VecDeque::with_capacity(MAX_SESSION_EVENTS),
            status: SessionStatus::Active,
        }
    }

    /// Record an event in this session
    pub fn record_event(&mut self, event: ProxyEvent) {
        self.last_activity = Instant::now();
        self.status = SessionStatus::Active;

        // Update stats based on event type
        self.stats.update(&event);

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
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new(DEFAULT_IDLE_TIMEOUT)
    }
}

impl SessionManager {
    /// Create a new session manager with specified idle timeout
    pub fn new(idle_timeout: Duration) -> Self {
        Self {
            sessions: HashMap::new(),
            active_by_user: HashMap::new(),
            history: VecDeque::with_capacity(100),
            idle_timeout,
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
    ) -> &Session {
        // Create session key
        let key = match session_id {
            Some(ref id) => SessionKey::explicit(id),
            None => SessionKey::implicit(&user_id.0),
        };

        // Supersede existing session for this user
        if let Some(old_key) = self.active_by_user.remove(&user_id) {
            if let Some(mut old_session) = self.sessions.remove(&old_key) {
                old_session.end(EndReason::Superseded);
                self.archive_session(old_session);
            }
        }

        // Create and insert new session
        let session = Session::new(key.clone(), user_id.clone(), source);
        self.sessions.insert(key.clone(), session);
        self.active_by_user.insert(user_id, key.clone());

        self.sessions.get(&key).unwrap()
    }

    /// End a session explicitly (from hook)
    pub fn end_session(&mut self, key: &SessionKey, reason: EndReason) {
        if let Some(mut session) = self.sessions.remove(key) {
            self.active_by_user.remove(&session.user_id);
            session.end(reason);
            self.archive_session(session);
        }
    }

    /// End a session by user ID
    pub fn end_session_by_user(&mut self, user_id: &UserId, reason: EndReason) {
        if let Some(key) = self.active_by_user.remove(user_id) {
            if let Some(mut session) = self.sessions.remove(&key) {
                session.end(reason);
                self.archive_session(session);
            }
        }
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
                // Create implicit session on first event
                self.start_session(user_id.clone(), None, SessionSource::FirstSeen);
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
                    "Backfilled session user_id from request headers"
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
        let implicit = SessionKey::implicit("a3f2c91b");

        assert_eq!(format!("{}", explicit), "session:abc123");
        assert_eq!(format!("{}", implicit), "user:a3f2c91b");
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
        );
        assert_eq!(manager.active_count(), 1);

        // Second session supersedes first
        manager.start_session(
            user.clone(),
            Some("session2".to_string()),
            SessionSource::Hook,
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
