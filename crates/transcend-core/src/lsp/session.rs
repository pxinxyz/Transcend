//! Language Server Process Session
//!
//! Manages an asynchronous child process session communicating over stdio
//! using standard LSP JSON-RPC 2.0 framing and document synchronization.

use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock, mpsc, oneshot};
use transcend_protocol::{
    DiagnosticSeverity, LspDiagnosticItem, LspDiagnosticsResponse, LspReferenceLocation,
    LspTargetLocation, SourceSpan,
};

use super::distiller::LspDistiller;
use super::protocol::{
    LspMessageReader, format_lsp_message, make_notification, make_request, path_to_uri, uri_to_path,
};
use super::registry::LspServerProfile;

/// Outstanding JSON-RPC requests awaiting a response, keyed by request id.
type PendingRequests = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

/// Diagnostics published by the server, keyed by document URI.
type DiagnosticsCache = Arc<RwLock<HashMap<String, Vec<LspDiagnosticItem>>>>;

/// A live language server child process session.
pub struct LspSession {
    workspace_root: PathBuf,
    language_id: String,
    outgoing_tx: mpsc::UnboundedSender<Vec<u8>>,
    pending_requests: PendingRequests,
    diagnostics_cache: DiagnosticsCache,
    open_documents: Arc<Mutex<HashMap<String, (std::time::SystemTime, i32)>>>,
    req_counter: AtomicU64,
    _child: Arc<Mutex<Child>>,
    /// Monotonic timestamp of the last request served by this session, used by the
    /// pool to reclaim idle language servers.
    last_used_millis: AtomicU64,
    /// Set once the server has answered a semantic request, meaning its index is warm.
    warmed: AtomicBool,
}

/// Deadline for the first semantic request on a session.
///
/// Generous on purpose: rust-analyzer needs tens of seconds to index a fresh clone, and
/// answering late is strictly better than silently degrading to a textual heuristic.
const COLD_START_TIMEOUT: Duration = Duration::from_secs(120);

/// Milliseconds since an arbitrary process-local epoch.
fn now_millis() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(std::time::Instant::now);
    start.elapsed().as_millis() as u64
}

impl LspSession {
    /// Record that this session was just used.
    pub fn touch(&self) {
        self.last_used_millis.store(now_millis(), Ordering::Relaxed);
    }

    /// Instant of the last recorded use, for idle-reclaim decisions.
    pub fn last_used(&self) -> std::time::Instant {
        let millis = self.last_used_millis.load(Ordering::Relaxed);
        std::time::Instant::now()
            .checked_sub(Duration::from_millis(millis))
            .unwrap_or_else(std::time::Instant::now)
    }

    /// Ask the language server to exit, then drop the child.
    ///
    /// Sends the LSP `exit` notification so the server can flush its index; the
    /// underlying child is also `kill_on_drop`, so a server that ignores `exit` is
    /// still reaped when this session's last handle drops.
    pub async fn shutdown(&self) {
        let _ = self.send_notification("exit", serde_json::json!({}));
    }

    /// Spawn a new language server session and perform the initial LSP handshake.
    pub async fn spawn(
        binary_path: &str,
        profile: &LspServerProfile,
        workspace_root: PathBuf,
    ) -> Result<Arc<Self>, String> {
        let mut cmd = Command::new(binary_path);
        cmd.args(profile.args)
            .current_dir(&workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn language server '{binary_path}': {e}"))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or("Failed to open child process stdin")?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or("Failed to open child process stdout")?;

        let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let pending_requests: PendingRequests = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics_cache = Arc::new(RwLock::new(HashMap::new()));
        let open_documents = Arc::new(Mutex::new(HashMap::new()));

        // Background stdin writer task
        tokio::spawn(async move {
            while let Some(msg_bytes) = outgoing_rx.recv().await {
                if stdin.write_all(&msg_bytes).await.is_err() || stdin.flush().await.is_err() {
                    break;
                }
            }
        });

        // Background stdout reader task
        let pending_clone = Arc::clone(&pending_requests);
        let diags_clone = Arc::clone(&diagnostics_cache);

        tokio::spawn(async move {
            let mut reader = LspMessageReader::new();
            let mut buf = [0u8; 8192];

            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        reader.feed(&buf[..n]);
                        while let Ok(Some(msg)) = reader.next_message() {
                            // 1. Check if response to a pending request
                            if let Some(id) = msg.get("id").and_then(|id| id.as_u64()) {
                                let mut pending = pending_clone.lock().await;
                                if let Some(tx) = pending.remove(&id) {
                                    if let Some(err) = msg.get("error") {
                                        let _ = tx.send(Err(err.to_string()));
                                    } else {
                                        let res = msg.get("result").cloned().unwrap_or(Value::Null);
                                        let _ = tx.send(Ok(res));
                                    }
                                }
                            }
                            // 2. Check if textDocument/publishDiagnostics notification
                            else if msg.get("method").and_then(|m| m.as_str())
                                == Some("textDocument/publishDiagnostics")
                                && let Some(params) = msg.get("params")
                                && let Some(uri_str) = params.get("uri").and_then(|u| u.as_str())
                            {
                                let file_path = uri_to_path(uri_str);
                                // Store the ABSOLUTE path, normalised to forward slashes.
                                // These values are both the cache key and the `file` field of
                                // every LspDiagnosticItem, and the caller filters them against
                                // the caller's own (absolute, engine-resolved) path. Deriving a
                                // path relative to the session root here made the filter
                                // arithmetically incapable of matching: a relative key can
                                // never contain an absolute path, so every filter returned
                                // zero diagnostics and a broken file looked clean.
                                let file_key = file_path.to_string_lossy().replace('\\', "/");

                                let mut items = Vec::new();
                                if let Some(diags) =
                                    params.get("diagnostics").and_then(|d| d.as_array())
                                {
                                    for d in diags {
                                        let message = d
                                            .get("message")
                                            .and_then(|m| m.as_str())
                                            .unwrap_or("")
                                            .to_string();
                                        let code = d
                                            .get("code")
                                            .map(|c| c.to_string().trim_matches('"').to_string());
                                        let source = d
                                            .get("source")
                                            .and_then(|s| s.as_str())
                                            .map(|s| s.to_string());
                                        let severity = match d
                                            .get("severity")
                                            .and_then(|s| s.as_u64())
                                            .unwrap_or(1)
                                        {
                                            1 => DiagnosticSeverity::Error,
                                            2 => DiagnosticSeverity::Warning,
                                            3 => DiagnosticSeverity::Information,
                                            _ => DiagnosticSeverity::Hint,
                                        };

                                        let range = d.get("range");
                                        let start_line = range
                                            .and_then(|r| r.get("start"))
                                            .and_then(|s| s.get("line"))
                                            .and_then(|l| l.as_u64())
                                            .unwrap_or(0)
                                            as usize
                                            + 1;
                                        let start_col = range
                                            .and_then(|r| r.get("start"))
                                            .and_then(|s| s.get("character"))
                                            .and_then(|c| c.as_u64())
                                            .unwrap_or(0)
                                            as usize
                                            + 1;
                                        let end_line = range
                                            .and_then(|r| r.get("end"))
                                            .and_then(|e| e.get("line"))
                                            .and_then(|l| l.as_u64())
                                            .unwrap_or(0)
                                            as usize
                                            + 1;
                                        let end_col = range
                                            .and_then(|r| r.get("end"))
                                            .and_then(|e| e.get("character"))
                                            .and_then(|c| c.as_u64())
                                            .unwrap_or(0)
                                            as usize
                                            + 1;

                                        items.push(LspDiagnosticItem {
                                            file: file_key.clone(),
                                            severity,
                                            span: SourceSpan {
                                                start_line,
                                                start_col,
                                                end_line,
                                                end_col,
                                                start_byte: 0,
                                                end_byte: 0,
                                            },
                                            message,
                                            code,
                                            source,
                                        });
                                    }
                                }

                                let mut cache = diags_clone.write().await;
                                cache.insert(file_key, items);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let session = Arc::new(Self {
            workspace_root: workspace_root.clone(),
            language_id: profile.language_id.to_string(),
            outgoing_tx,
            pending_requests,
            diagnostics_cache,
            open_documents,
            req_counter: AtomicU64::new(1),
            _child: Arc::new(Mutex::new(child)),
            last_used_millis: AtomicU64::new(now_millis()),
            warmed: AtomicBool::new(false),
        });

        // Perform LSP handshake
        session.initialize().await?;

        Ok(session)
    }

    /// Complete LSP initialize handshake.
    async fn initialize(&self) -> Result<(), String> {
        let root_uri = path_to_uri(&self.workspace_root);
        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "definition": { "dynamicRegistration": false, "linkSupport": true },
                    "references": { "dynamicRegistration": false },
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "publishDiagnostics": { "relatedInformation": true }
                }
            }
        });

        let _ = self
            .send_request("initialize", init_params, Duration::from_secs(10))
            .await?;
        self.send_notification("initialized", serde_json::json!({}))?;

        Ok(())
    }

    /// Send a request and await its response.
    pub async fn send_request(
        &self,
        method: &str,
        params: Value,
        timeout_dur: Duration,
    ) -> Result<Value, String> {
        let id = self.req_counter.fetch_add(1, Ordering::SeqCst);
        let (rx_tx, rx) = oneshot::channel();

        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(id, rx_tx);
        }

        let req = make_request(id, method, params);
        let framed = format_lsp_message(&req).map_err(|e| e.to_string())?;

        self.outgoing_tx
            .send(framed)
            .map_err(|e| format!("Failed to send to LSP stdin: {e}"))?;

        match tokio::time::timeout(timeout_dur, rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err(format!("LSP server dropped request {id} ({method})")),
            Err(_) => {
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&id);
                Err(format!(
                    "LSP request {id} ({method}) timed out after {timeout_dur:?}"
                ))
            }
        }
    }

    /// Send a semantic request using the warm/cold deadline, marking the server warm
    /// once it answers.
    ///
    /// Only semantic requests go through here: the `initialize` handshake is answered
    /// immediately even by a server that has indexed nothing, so it must not be taken as
    /// evidence that the index is ready.
    async fn send_semantic_request(
        &self,
        method: &str,
        params: Value,
        warm_timeout: Duration,
    ) -> Result<Value, String> {
        let result = self
            .send_request(method, params, self.request_timeout(warm_timeout))
            .await;
        if result.is_ok() {
            self.mark_warmed();
        }
        result
    }

    /// Send a notification (no response expected).
    pub fn send_notification(&self, method: &str, params: Value) -> Result<(), String> {
        let notif = make_notification(method, params);
        let framed = format_lsp_message(&notif).map_err(|e| e.to_string())?;
        self.outgoing_tx
            .send(framed)
            .map_err(|e| format!("Failed to send notification: {e}"))?;
        Ok(())
    }

    /// Ensure the document is open and up-to-date in the language server session.
    pub async fn ensure_document_open(&self, file_path: &Path) -> Result<(), String> {
        let uri = path_to_uri(file_path);
        let current_mtime = std::fs::metadata(file_path)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

        let mut docs = self.open_documents.lock().await;
        if let Some((last_mtime, version)) = docs.get_mut(&uri) {
            if *last_mtime == current_mtime {
                return Ok(());
            }

            // Document on disk has changed: read updated text and notify LSP via didChange
            let text = std::fs::read_to_string(file_path)
                .map_err(|e| format!("Failed to read file {}: {e}", file_path.display()))?;

            *version += 1;
            let next_version = *version;
            *last_mtime = current_mtime;

            let change_params = serde_json::json!({
                "textDocument": {
                    "uri": uri.clone(),
                    "version": next_version
                },
                "contentChanges": [
                    { "text": text }
                ]
            });

            self.send_notification("textDocument/didChange", change_params)?;

            let save_params = serde_json::json!({
                "textDocument": { "uri": uri }
            });
            let _ = self.send_notification("textDocument/didSave", save_params);

            return Ok(());
        }

        let text = std::fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read file {}: {e}", file_path.display()))?;

        let params = serde_json::json!({
            "textDocument": {
                "uri": uri.clone(),
                "languageId": self.language_id,
                "version": 1,
                "text": text
            }
        });

        self.send_notification("textDocument/didOpen", params)?;
        docs.insert(uri, (current_mtime, 1));

        Ok(())
    }

    /// Query definition of symbol or position.
    pub async fn goto_definition(
        &self,
        file_path: &Path,
        line_0: u32,
        col_0: u32,
    ) -> Result<Vec<LspTargetLocation>, String> {
        self.ensure_document_open(file_path).await?;
        let uri = path_to_uri(file_path);

        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line_0, "character": col_0 }
        });

        let raw = self
            .send_semantic_request("textDocument/definition", params, Duration::from_secs(5))
            .await?;
        let targets = LspDistiller::distill_definition(&raw, &self.workspace_root);
        Ok(targets)
    }

    /// Query references for symbol or position.
    pub async fn find_references(
        &self,
        file_path: &Path,
        line_0: u32,
        col_0: u32,
        include_declaration: bool,
        limit: usize,
    ) -> Result<(Vec<LspReferenceLocation>, usize, bool), String> {
        self.ensure_document_open(file_path).await?;
        let uri = path_to_uri(file_path);

        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line_0, "character": col_0 },
            "context": { "includeDeclaration": include_declaration }
        });

        let raw = self
            .send_semantic_request("textDocument/references", params, Duration::from_secs(10))
            .await?;
        let result = LspDistiller::distill_references(&raw, &self.workspace_root, limit);
        Ok(result)
    }

    /// Query hover for symbol or position.
    pub async fn hover(
        &self,
        file_path: &Path,
        line_0: u32,
        col_0: u32,
    ) -> Result<(Option<String>, Option<String>, Option<SourceSpan>), String> {
        self.ensure_document_open(file_path).await?;
        let uri = path_to_uri(file_path);

        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line_0, "character": col_0 }
        });

        let raw = self
            .send_semantic_request("textDocument/hover", params, Duration::from_secs(5))
            .await?;
        let hover_data = LspDistiller::distill_hover(&raw);
        Ok(hover_data)
    }

    /// Deadline for a semantic request, widened while the server is still warming up.
    ///
    /// A freshly spawned server has to index the project before answering anything, so
    /// the first request can legitimately take far longer than a warm one. With only the
    /// warm deadline, a query against a fresh clone times out, the caller falls back to
    /// the Tree-sitter heuristic, and the caller silently loses compiler precision
    /// precisely when it was first asked for it.
    fn request_timeout(&self, warm: Duration) -> Duration {
        if self.warmed.load(Ordering::Relaxed) {
            warm
        } else {
            COLD_START_TIMEOUT
        }
    }

    /// Record that the server answered, so subsequent requests use the warm deadline.
    fn mark_warmed(&self) {
        self.warmed.store(true, Ordering::Relaxed);
    }

    /// Whether this server has answered a semantic request yet.
    ///
    /// While false the server is still indexing, so an empty or failed result is not
    /// evidence that the symbol has no definition.
    pub fn is_warmed(&self) -> bool {
        self.warmed.load(Ordering::Relaxed)
    }

    /// Whether any diagnostics are cached for `path_filter`.
    ///
    /// Diagnostics arrive as `textDocument/publishDiagnostics` notifications rather than as a
    /// response to a request, and nothing here sets `warmed` for them. Reading the cache
    /// therefore yields an empty set both when the file is genuinely clean AND when the
    /// server has not finished indexing -- and an empty set reads as "no problems". A caller
    /// asking "does this compile?" was told yes while `cargo check` reported two errors in
    /// the same file.
    ///
    /// The caller must distinguish the two cases with [`Self::is_warmed`]: a warmed server
    /// that published nothing has genuinely found nothing.
    pub async fn has_cached_diagnostics(&self, path_filter: Option<&str>) -> bool {
        let cache = self.diagnostics_cache.read().await;
        match path_filter {
            None => cache.values().any(|v| !v.is_empty()),
            Some(pf) => {
                let want = pf.replace('\\', "/");
                cache.iter().any(|(k, v)| {
                    let have = k.replace('\\', "/");
                    !v.is_empty()
                        && (have == want || have.ends_with(&want) || want.ends_with(&have))
                })
            }
        }
    }

    /// Retrieve active compiler diagnostics from the cache.
    pub async fn get_diagnostics(
        &self,
        path_filter: Option<&str>,
        severity_filter: Option<DiagnosticSeverity>,
    ) -> LspDiagnosticsResponse {
        let cache = self.diagnostics_cache.read().await;
        let mut all_diags = Vec::new();
        for items in cache.values() {
            all_diags.extend(items.clone());
        }
        LspDistiller::distill_diagnostics(&all_diags, path_filter, severity_filter)
    }
}
