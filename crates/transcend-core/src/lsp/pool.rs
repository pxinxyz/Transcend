//! Language Server Connection Pool
//!
//! Pools and reuses warm child process sessions keyed by language and workspace root.
//! Gracefully yields `None` if a language server is not installed, triggering fallback.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::registry::{LspRegistry, LspServerProfile};
use super::session::LspSession;

/// Pool of live, warm language server sessions.
#[derive(Clone, Default)]
pub struct LspSessionPool {
    sessions: Arc<Mutex<HashMap<(String, PathBuf), Arc<LspSession>>>>,
}

impl LspSessionPool {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Obtain an existing warm session or spawn a new one if the server binary is on PATH.
    /// Returns `Ok(None)` if no language server profile exists or binary is not installed.
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
        let key = (profile.language_id.to_string(), root.clone());

        let mut pool = self.sessions.lock().await;
        if let Some(existing) = pool.get(&key) {
            return Ok(Some((Arc::clone(existing), profile)));
        }

        // Spawn new session
        let session = LspSession::spawn(&binary, profile, root).await?;
        pool.insert(key, Arc::clone(&session));

        Ok(Some((session, profile)))
    }

    /// Get all active sessions currently running in the pool.
    pub async fn active_sessions(&self) -> Vec<Arc<LspSession>> {
        let pool = self.sessions.lock().await;
        pool.values().cloned().collect()
    }
}
