//! Language Server Connection Pool
//!
//! Pools and reuses warm child process sessions keyed by language and workspace root.
//! Gracefully yields `None` if a language server is not installed, triggering fallback.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use super::registry::{LspRegistry, LspServerProfile};
use super::session::LspSession;

/// Sessions untouched for longer than this are considered idle and reclaimed.
const IDLE_SESSION_TTL: Duration = Duration::from_secs(30 * 60);

type SessionKey = (String, PathBuf);

/// A slot in the pool: either a live session or an in-flight spawn that other
/// callers can await instead of starting a duplicate language server.
enum Slot {
    Live(Arc<LspSession>),
    Spawning(Arc<Mutex<Option<Arc<LspSession>>>>),
}

impl Slot {
    fn live_session(&self) -> Option<Arc<LspSession>> {
        match self {
            Slot::Live(session) => Some(Arc::clone(session)),
            Slot::Spawning(_) => None,
        }
    }

    fn last_used(&self) -> Option<Instant> {
        match self {
            Slot::Live(session) => Some(session.last_used()),
            Slot::Spawning(_) => Some(Instant::now()),
        }
    }
}

/// Pool of live, warm language server sessions.
#[derive(Clone, Default)]
pub struct LspSessionPool {
    sessions: Arc<Mutex<HashMap<SessionKey, Slot>>>,
}

impl LspSessionPool {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Obtain an existing warm session or spawn a new one if the server binary is on PATH.
    ///
    /// Returns `Ok(None)` if no language server profile exists or the binary is not
    /// installed.
    ///
    /// A cold spawn takes seconds, so the pool map is never held across it: the first
    /// caller publishes a `Spawning` placeholder and releases the lock, and concurrent
    /// callers for the same language await that placeholder rather than launching a
    /// second server. Sessions for *other* languages proceed immediately.
    pub async fn get_or_spawn(
        &self,
        file_path: &Path,
    ) -> Result<Option<(Arc<LspSession>, &'static LspServerProfile)>, String> {
        let Some(profile) = LspRegistry::profile_for_path(file_path) else {
            return Ok(None);
        };

        let Some(binary) = LspRegistry::resolve_binary(profile) else {
            return Ok(None);
        };

        let root = LspRegistry::find_workspace_root(file_path, Some(profile));
        let key: SessionKey = (profile.language_id.to_string(), root.clone());

        self.reclaim_idle().await;

        // Fast path: an existing live session needs no lock held during work.
        let placeholder = {
            let mut pool = self.sessions.lock().await;
            match pool.get(&key) {
                Some(Slot::Live(existing)) => {
                    let session = Arc::clone(existing);
                    drop(pool);
                    session.touch();
                    return Ok(Some((session, profile)));
                }
                Some(Slot::Spawning(slot)) => Arc::clone(slot),
                None => {
                    let slot = Arc::new(Mutex::new(None));
                    pool.insert(key.clone(), Slot::Spawning(Arc::clone(&slot)));
                    slot
                }
            }
        };

        // We either own the placeholder or are waiting on someone who does.
        let mut guard = placeholder.lock().await;
        if let Some(session) = guard.as_ref() {
            // Another caller finished the spawn while we waited.
            let session = Arc::clone(session);
            drop(guard);
            session.touch();
            return Ok(Some((session, profile)));
        }

        // Cold start, with the pool map lock released.
        let spawned = match LspSession::spawn(&binary, profile, root).await {
            Ok(session) => session,
            Err(e) => {
                // Remove the placeholder so a later call can retry.
                let mut pool = self.sessions.lock().await;
                pool.remove(&key);
                return Err(e);
            }
        };

        *guard = Some(Arc::clone(&spawned));
        drop(guard);

        let mut pool = self.sessions.lock().await;
        pool.insert(key, Slot::Live(Arc::clone(&spawned)));
        drop(pool);

        Ok(Some((spawned, profile)))
    }

    /// Get all active sessions currently running in the pool.
    pub async fn active_sessions(&self) -> Vec<Arc<LspSession>> {
        let pool = self.sessions.lock().await;
        pool.values().filter_map(Slot::live_session).collect()
    }

    /// Drop sessions that have been idle beyond [`IDLE_SESSION_TTL`].
    ///
    /// Without this, every language server touched during a daemon's lifetime stays
    /// resident (each holding hundreds of MB of index), since nothing else evicts.
    async fn reclaim_idle(&self) {
        let stale: Vec<SessionKey> = {
            let pool = self.sessions.lock().await;
            pool.iter()
                .filter(|(_, slot)| {
                    slot.last_used()
                        .is_some_and(|used| used.elapsed() > IDLE_SESSION_TTL)
                })
                .map(|(key, _)| key.clone())
                .collect()
        };

        if stale.is_empty() {
            return;
        }

        let mut evicted = Vec::new();
        {
            let mut pool = self.sessions.lock().await;
            for key in stale {
                if let Some(slot) = pool.remove(&key) {
                    evicted.push(slot);
                }
            }
        }

        for slot in evicted {
            if let Slot::Live(session) = slot {
                session.shutdown().await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_or_spawn_returns_none_without_binary() {
        let pool = LspSessionPool::new();
        // An extension with no registered profile must yield None, not an error.
        let res = pool
            .get_or_spawn(Path::new("unknown_file.zzz"))
            .await
            .expect("absent profile must not error");
        assert!(res.is_none());
    }

    #[tokio::test]
    async fn test_pool_starts_empty_and_concurrent_lookups_do_not_deadlock() {
        let pool = LspSessionPool::new();
        assert!(pool.active_sessions().await.is_empty());

        let mut handles = Vec::new();
        for _ in 0..8 {
            let pool = pool.clone();
            handles.push(tokio::spawn(async move {
                // `Cargo.toml` selects the rust profile; when rust-analyzer is absent
                // this returns None quickly, which is the interesting concurrent path.
                pool.get_or_spawn(Path::new("src/lib.rs")).await
            }));
        }
        for handle in handles {
            let _ = handle.await.expect("task should not panic");
        }
    }
}
