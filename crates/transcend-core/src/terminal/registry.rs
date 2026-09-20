//! Terminal Session Registry & Idle Reaper
//!
//! Stores active background sessions and enforces background resource reclamation
//! to prevent abandoned processes from lingering indefinitely.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

use super::session::TerminalSession;

/// Maximum idle time before session is reaped (default: 30 minutes).
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 30 * 60;

/// Maximum session lifetime before forced termination (default: 6 hours).
pub const DEFAULT_MAX_AGE_SECS: u64 = 6 * 60 * 60;

/// Thread-safe registry of live execution sessions.
#[derive(Clone)]
pub struct TerminalRegistry {
    sessions: Arc<RwLock<HashMap<String, Arc<TerminalSession>>>>,
}

impl Default for TerminalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalRegistry {
    /// Create a new registry and spawn the background idle session reaper.
    pub fn new() -> Self {
        let sessions = Arc::new(RwLock::new(HashMap::new()));
        let registry = Self { sessions };

        // Background reaper task if a Tokio runtime reactor is running
        let sessions_clone = Arc::clone(&registry.sessions);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(60));
                loop {
                    interval.tick().await;
                    Self::reap_expired(
                        &sessions_clone,
                        DEFAULT_IDLE_TIMEOUT_SECS,
                        DEFAULT_MAX_AGE_SECS,
                    )
                    .await;
                }
            });
        }

        registry
    }

    /// Insert an active session into the registry.
    pub async fn insert(&self, session: Arc<TerminalSession>) {
        let mut lock = self.sessions.write().await;
        lock.insert(session.session_id.clone(), session);
    }

    /// Retrieve an existing session by ID.
    pub async fn get(&self, session_id: &str) -> Option<Arc<TerminalSession>> {
        let lock = self.sessions.read().await;
        lock.get(session_id).cloned()
    }

    /// Remove and return a session from the registry.
    pub async fn remove(&self, session_id: &str) -> Option<Arc<TerminalSession>> {
        let mut lock = self.sessions.write().await;
        lock.remove(session_id)
    }

    /// Evicts expired or idle sessions.
    async fn reap_expired(
        sessions: &Arc<RwLock<HashMap<String, Arc<TerminalSession>>>>,
        max_idle_secs: u64,
        max_age_secs: u64,
    ) {
        let now_epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut to_kill = Vec::new();

        {
            let lock = sessions.read().await;
            for (id, session) in lock.iter() {
                let last_active = session.last_activity.load(Ordering::Relaxed);
                let started_epoch = session.started_at.timestamp().max(0) as u64;

                let idle_dur = now_epoch.saturating_sub(last_active);
                let age_dur = now_epoch.saturating_sub(started_epoch);

                // Reap if dead, idle for > max_idle, or older than max_age
                if !session.is_running() || idle_dur >= max_idle_secs || age_dur >= max_age_secs {
                    to_kill.push(id.clone());
                }
            }
        }

        if !to_kill.is_empty() {
            let mut lock = sessions.write().await;
            for id in to_kill {
                if let Some(session) = lock.remove(&id) {
                    session.kill().await;
                }
            }
        }
    }
}
