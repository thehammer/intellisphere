use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::satellite::trust_budget::TrustBudget;

/// Represents an active satellite edge-execution session.
#[derive(Debug, Clone)]
pub struct SatelliteSession {
    /// Unique session identifier.
    pub session_id: String,
    /// Subject identifier of the authenticated caller that created the session.
    pub identity_sub: String,
    /// Trust budget for this session.
    pub trust_budget: TrustBudget,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session expires.
    pub expires_at: DateTime<Utc>,
}

/// Manages the lifecycle of satellite sessions.
pub struct SessionManager {
    sessions: HashMap<String, SatelliteSession>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Create and store a new satellite session. Returns the session id.
    pub fn create(
        &mut self,
        session_id: String,
        identity_sub: String,
        trust_budget: TrustBudget,
        expires_at: DateTime<Utc>,
    ) -> &SatelliteSession {
        let session = SatelliteSession {
            session_id: session_id.clone(),
            identity_sub,
            trust_budget,
            created_at: Utc::now(),
            expires_at,
        };
        self.sessions.insert(session_id.clone(), session);
        self.sessions.get(&session_id).unwrap()
    }

    /// Get a reference to a session by id.
    pub fn get(&self, session_id: &str) -> Option<&SatelliteSession> {
        self.sessions.get(session_id)
    }

    /// Get a mutable reference to a session by id.
    pub fn get_mut(&mut self, session_id: &str) -> Option<&mut SatelliteSession> {
        self.sessions.get_mut(session_id)
    }

    /// Remove a session by id. Returns the removed session if it existed.
    pub fn remove(&mut self, session_id: &str) -> Option<SatelliteSession> {
        self.sessions.remove(session_id)
    }

    /// Remove all expired sessions. Returns the number of sessions removed.
    pub fn cleanup_expired(&mut self) -> usize {
        let now = Utc::now();
        let before = self.sessions.len();
        self.sessions.retain(|_, s| s.expires_at > now);
        before - self.sessions.len()
    }

    /// Number of active sessions.
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Iterate over all sessions.
    pub fn sessions_iter(&self) -> impl Iterator<Item = (&String, &SatelliteSession)> {
        self.sessions.iter()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_budget() -> TrustBudget {
        TrustBudget::new(100.0, 1.0)
    }

    #[test]
    fn create_and_get() {
        let mut mgr = SessionManager::new();
        let expires = Utc::now() + Duration::hours(1);
        mgr.create("sess-1".into(), "user-a".into(), make_budget(), expires);
        let s = mgr.get("sess-1").unwrap();
        assert_eq!(s.identity_sub, "user-a");
        assert_eq!(mgr.len(), 1);
    }

    #[test]
    fn remove_session() {
        let mut mgr = SessionManager::new();
        let expires = Utc::now() + Duration::hours(1);
        mgr.create("sess-1".into(), "user-a".into(), make_budget(), expires);
        let removed = mgr.remove("sess-1");
        assert!(removed.is_some());
        assert!(mgr.is_empty());
    }

    #[test]
    fn cleanup_expired_removes_old_sessions() {
        let mut mgr = SessionManager::new();
        let past = Utc::now() - Duration::hours(1);
        let future = Utc::now() + Duration::hours(1);

        mgr.create("expired".into(), "user-a".into(), make_budget(), past);
        mgr.create("active".into(), "user-b".into(), make_budget(), future);

        let removed = mgr.cleanup_expired();
        assert_eq!(removed, 1);
        assert_eq!(mgr.len(), 1);
        assert!(mgr.get("active").is_some());
        assert!(mgr.get("expired").is_none());
    }

    #[test]
    fn get_mut_allows_budget_modification() {
        let mut mgr = SessionManager::new();
        let expires = Utc::now() + Duration::hours(1);
        mgr.create("sess-1".into(), "user-a".into(), make_budget(), expires);

        let s = mgr.get_mut("sess-1").unwrap();
        assert!(s.trust_budget.deduct(10.0));
        assert!((s.trust_budget.remaining - 90.0).abs() < f64::EPSILON);
    }
}
