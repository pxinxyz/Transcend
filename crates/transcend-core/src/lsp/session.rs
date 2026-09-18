//! Language Server Process Session
//!
//! Manages an asynchronous child process session communicating over stdio
//! using standard LSP JSON-RPC 2.0 framing and document synchronization.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use transcend_protocol::{
    DiagnosticSeverity, LspDiagnosticItem, LspDiagnosticsResponse, LspReferenceLocation,
    LspTargetLocation, SourceSpan,
};

use super::distiller::LspDistiller;
use super::protocol::{
    format_lsp_message, make_notification, make_request, path_to_uri, uri_to_path, LspMessageReader,
};
use super::registry::LspServerProfile;

/// A live language server child process session.
pub struct LspSession {
    workspace_root: PathBuf,
    language_id: String,
    outgoing_tx: mpsc::UnboundedSender<Vec<u8>>,
    pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>,
    diagnostics_cache: Arc<RwLock<HashMap<String, Vec<LspDiagnosticItem>>>>,
    open_documents: Arc<Mutex<HashMap<String, (std::time::SystemTime, i32)>>>,
    req_counter: AtomicU64,
    _child: Arc<Mutex<Child>>,
}

impl LspSession {
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

        let mut child = cmd.spawn().map_err(|e| {
            format!("Failed to spawn language server '{binary_path}': {e}")
        })?;

        let mut stdin = child.stdin.take().ok_or("Failed to open child process stdin")?;
        let mut stdout = child.stdout.take().ok_or("Failed to open child process stdout")?;

        let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
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
        let root_clone = workspace_root.clone();

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
                            else if msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics") {
                                if let Some(params) = msg.get("params") {
                                    if let Some(uri_str) = params.get("uri").and_then(|u| u.as_str()) {
                                        let file_path = uri_to_path(uri_str);
                                        let rel_path = file_path
                                            .strip_prefix(&root_clone)
                                            .unwrap_or(&file_path)
                                            .to_string_lossy()
                                            .replace('\\', "/");

                                        let mut items = Vec::new();
                                        if let Some(diags) = params.get("diagnostics").and_then(|d| d.as_array()) {
                                            for d in diags {
                                                let message = d.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();
                                                let code = d.get("code").map(|c| c.to_string().trim_matches('"').to_string());
                                                let source = d.get("source").and_then(|s| s.as_str()).map(|s| s.to_string());
                                                let severity = match d.get("severity").and_then(|s| s.as_u64()).unwrap_or(1) {
                                                    1 => DiagnosticSeverity::Error,
                                                    2 => DiagnosticSeverity::Warning,
                                                    3 => DiagnosticSeverity::Information,
                                                    _ => DiagnosticSeverity::Hint,
                                                };

                                                let range = d.get("range");
                                                let start_line = range.and_then(|r| r.get("start")).and_then(|s| s.get("line")).and_then(|l| l.as_u64()).unwrap_or(0) as usize + 1;
                                                let start_col = range.and_then(|r| r.get("start")).and_then(|s| s.get("character")).and_then(|c| c.as_u64()).unwrap_or(0) as usize + 1;
                                                let end_line = range.and_then(|r| r.get("end")).and_then(|e| e.get("line")).and_then(|l| l.as_u64()).unwrap_or(0) as usize + 1;
                                                let end_col = range.and_then(|r| r.get("end")).and_then(|e| e.get("character")).and_then(|c| c.as_u64()).unwrap_or(0) as usize + 1;

                                                items.push(LspDiagnosticItem {
                                                    file: rel_path.clone(),
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
                                        cache.insert(rel_path, items);
                                    }
                                }
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

        let _ = self.send_request("initialize", init_params, Duration::from_secs(10)).await?;
        self.send_notification("initialized", serde_json::json!({}))?;

        Ok(())
    }

    /// Send a request and await its response.
    pub async fn send_request(&self, method: &str, params: Value, timeout_dur: Duration) -> Result<Value, String> {
        let id = self.req_counter.fetch_add(1, Ordering::SeqCst);
        let (rx_tx, rx) = oneshot::channel();

        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(id, rx_tx);
        }

        let req = make_request(id, method, params);
        let framed = format_lsp_message(&req).map_err(|e| e.to_string())?;

        self.outgoing_tx.send(framed).map_err(|e| format!("Failed to send to LSP stdin: {e}"))?;

        match tokio::time::timeout(timeout_dur, rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err(format!("LSP server dropped request {id} ({method})")),
            Err(_) => {
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&id);
                Err(format!("LSP request {id} ({method}) timed out after {timeout_dur:?}"))
            }
        }
    }

    /// Send a notification (no response expected).
    pub fn send_notification(&self, method: &str, params: Value) -> Result<(), String> {
        let notif = make_notification(method, params);
        let framed = format_lsp_message(&notif).map_err(|e| e.to_string())?;
        self.outgoing_tx.send(framed).map_err(|e| format!("Failed to send notification: {e}"))?;
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
            let text = std::fs::read_to_string(file_path).map_err(|e| {
                format!("Failed to read file {}: {e}", file_path.display())
            })?;

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

        let text = std::fs::read_to_string(file_path).map_err(|e| {
            format!("Failed to read file {}: {e}", file_path.display())
        })?;

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
    pub async fn goto_definition(&self, file_path: &Path, line_0: u32, col_0: u32) -> Result<Vec<LspTargetLocation>, String> {
        self.ensure_document_open(file_path).await?;
        let uri = path_to_uri(file_path);

        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line_0, "character": col_0 }
        });

        let raw = self.send_request("textDocument/definition", params, Duration::from_secs(5)).await?;
        let targets = LspDistiller::distill_definition(&raw, &self.workspace_root);
        Ok(targets)
    }

    /// Query references for symbol or position.
    pub async fn find_references(&self, file_path: &Path, line_0: u32, col_0: u32, include_declaration: bool, limit: usize) -> Result<(Vec<LspReferenceLocation>, usize, bool), String> {
        self.ensure_document_open(file_path).await?;
        let uri = path_to_uri(file_path);

        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line_0, "character": col_0 },
            "context": { "includeDeclaration": include_declaration }
        });

        let raw = self.send_request("textDocument/references", params, Duration::from_secs(10)).await?;
        let result = LspDistiller::distill_references(&raw, &self.workspace_root, limit);
        Ok(result)
    }

    /// Query hover for symbol or position.
    pub async fn hover(&self, file_path: &Path, line_0: u32, col_0: u32) -> Result<(Option<String>, Option<String>, Option<SourceSpan>), String> {
        self.ensure_document_open(file_path).await?;
        let uri = path_to_uri(file_path);

        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line_0, "character": col_0 }
        });

        let raw = self.send_request("textDocument/hover", params, Duration::from_secs(5)).await?;
        let hover_data = LspDistiller::distill_hover(&raw);
        Ok(hover_data)
    }

    /// Retrieve active compiler diagnostics from the cache.
    pub async fn get_diagnostics(&self, path_filter: Option<&str>, severity_filter: Option<DiagnosticSeverity>) -> LspDiagnosticsResponse {
        let cache = self.diagnostics_cache.read().await;
        let mut all_diags = Vec::new();
        for items in cache.values() {
            all_diags.extend(items.clone());
        }
        LspDistiller::distill_diagnostics(&all_diags, path_filter, severity_filter)
    }
}

