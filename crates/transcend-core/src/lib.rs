//! Transcend Core Engine
//!
//! High-performance, in-process computational primitives for code search,
//! file discovery, AST outlining, and surgical transformations.

pub mod file_ops;
pub mod find;
pub mod find_symbol;
pub mod git_ops;
pub mod lsp;
pub mod outline;
pub mod patch;
pub mod search;
pub mod terminal;

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use transcend_protocol::{
    BatchPatchRequest, BatchPatchResponse, DeletePathRequest, DeletePathResponse, ExecRequest,
    ExecResponse, FindRequest, FindResponse, FindSymbolRequest, FindSymbolResponse,
    GitStatusRequest, GitStatusResponse, LspDefinitionRequest, LspDefinitionResponse,
    LspDiagnosticsRequest, LspDiagnosticsResponse, LspHoverRequest, LspHoverResponse,
    LspInstallRequest, LspInstallResponse, LspReferencesRequest, LspReferencesResponse,
    LspStatusRequest, LspStatusResponse, OutlineRequest, OutlineResponse, PatchRequest,
    PatchResponse, ReadFileRequest, ReadFileResponse, ReadSymbolRequest, ReadSymbolResponse,
    SearchRequest, SearchResponse, SetWorkspaceRequest, SetWorkspaceResponse, TerminalKillRequest,
    TerminalKillResponse, TerminalReadRequest, TerminalReadResponse, TerminalResizeRequest,
    TerminalResizeResponse, TerminalWriteRequest, TerminalWriteResponse, WriteFileRequest,
    WriteFileResponse,
};

use crate::file_ops::FileOps;
use crate::find::FindScanner;
use crate::find_symbol::SymbolFinder;
use crate::outline::scanner::OutlineScanner;
use crate::outline::symbol_reader::SymbolReader;
use crate::patch::Patcher;
use crate::search::SearchScanner;

/// Core engine errors.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Pattern error: {0}")]
    InvalidPattern(String),

    #[error("Parsing error in {file}: {message}")]
    ParseError { file: String, message: String },

    #[error("Operation failed: {0}")]
    General(String),
}

/// Result alias for Core operations.
pub type CoreResult<T> = Result<T, CoreError>;

/// Future type alias for object-safe asynchronous Engine trait methods.
pub type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// Core execution engine trait.
pub trait Engine: Send + Sync {
    /// Execute a code search.
    fn search(&self, req: &SearchRequest) -> CoreResult<SearchResponse>;

    /// Execute a file discovery traversal.
    fn find(&self, req: &FindRequest) -> CoreResult<FindResponse>;

    /// Extract an AST outline of a source file.
    fn outline(&self, req: &OutlineRequest) -> CoreResult<OutlineResponse>;

    /// Surgically extract a specific symbol by name or qualified locator.
    fn read_symbol(&self, req: &ReadSymbolRequest) -> CoreResult<ReadSymbolResponse>;

    /// Surgically patch code with in-memory AST validation.
    fn patch(&self, req: &PatchRequest) -> CoreResult<PatchResponse>;

    /// Globally find code symbol definitions across the workspace.
    fn find_symbol(&self, req: &FindSymbolRequest) -> CoreResult<FindSymbolResponse>;

    /// Read file content with line/byte boundaries and binary safety checks.
    fn read_file(&self, req: &ReadFileRequest) -> CoreResult<ReadFileResponse>;

    /// Write text content to file atomically, with collision and parent directory guards.
    fn write_file(&self, req: &WriteFileRequest) -> CoreResult<WriteFileResponse>;

    /// Delete file or directory within workspace safety boundaries.
    fn delete_path(&self, req: &DeletePathRequest) -> CoreResult<DeletePathResponse>;

    /// Transactionally apply multiple patches across files with AST preflight and rollback guarantees.
    fn batch_patch(&self, req: &BatchPatchRequest) -> CoreResult<BatchPatchResponse>;

    /// Go to compiler-resolved definition of a symbol or position.
    fn lsp_definition<'a>(
        &'a self,
        req: &'a LspDefinitionRequest,
    ) -> BoxFuture<'a, CoreResult<LspDefinitionResponse>>;

    /// Find all compiler-resolved references and call sites across the workspace.
    fn lsp_references<'a>(
        &'a self,
        req: &'a LspReferencesRequest,
    ) -> BoxFuture<'a, CoreResult<LspReferencesResponse>>;

    /// Inspect inferred type signature and documentation.
    fn lsp_hover<'a>(
        &'a self,
        req: &'a LspHoverRequest,
    ) -> BoxFuture<'a, CoreResult<LspHoverResponse>>;

    /// Retrieve active compiler diagnostics for file or workspace.
    fn lsp_diagnostics<'a>(
        &'a self,
        req: &'a LspDiagnosticsRequest,
    ) -> BoxFuture<'a, CoreResult<LspDiagnosticsResponse>>;

    /// Check current installation status and recipes for language servers.
    fn lsp_status<'a>(
        &'a self,
        req: &'a LspStatusRequest,
    ) -> BoxFuture<'a, CoreResult<LspStatusResponse>>;

    /// Automatically install a language server using host package managers.
    fn lsp_install<'a>(
        &'a self,
        req: &'a LspInstallRequest,
    ) -> BoxFuture<'a, CoreResult<LspInstallResponse>>;

    /// Execute a command using the hybrid terminal runner.
    fn exec<'a>(&'a self, req: &'a ExecRequest) -> BoxFuture<'a, CoreResult<ExecResponse>>;

    /// Read incremental output from an active terminal session.
    fn terminal_read<'a>(
        &'a self,
        req: &'a TerminalReadRequest,
    ) -> BoxFuture<'a, CoreResult<TerminalReadResponse>>;

    /// Send interactive input to an active terminal session.
    fn terminal_write<'a>(
        &'a self,
        req: &'a TerminalWriteRequest,
    ) -> BoxFuture<'a, CoreResult<TerminalWriteResponse>>;

    /// Resize terminal dimensions.
    fn terminal_resize<'a>(
        &'a self,
        req: &'a TerminalResizeRequest,
    ) -> BoxFuture<'a, CoreResult<TerminalResizeResponse>>;

    /// Terminate an active terminal session and its process tree.
    fn terminal_kill<'a>(
        &'a self,
        req: &'a TerminalKillRequest,
    ) -> BoxFuture<'a, CoreResult<TerminalKillResponse>>;

    /// Configure or update the active workspace root path.
    fn set_workspace(&self, req: &SetWorkspaceRequest) -> CoreResult<SetWorkspaceResponse>;

    /// Retrieve the currently active or auto-detected workspace root.
    fn get_workspace(&self) -> PathBuf;

    /// Inspect structured git status for a repository directory.
    fn git_status<'a>(
        &'a self,
        req: &'a GitStatusRequest,
    ) -> BoxFuture<'a, CoreResult<GitStatusResponse>>;
}

/// Default in-process engine implementation.
#[derive(Clone)]
pub struct NativeEngine {
    lsp: Arc<lsp::LspEngine>,
    terminal: Arc<terminal::TerminalEngine>,
    workspace_root: Arc<RwLock<Option<PathBuf>>>,
}

impl Default for NativeEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Largest char boundary in `s` that is `<= index` (clamped to `s.len()`).
///
/// Byte budgets are not valid `String` indices: slicing or truncating at a non-boundary
/// panics. Every place a caller-supplied byte budget is applied to text must walk the cut
/// back to a real boundary first.
pub fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Smallest char boundary in `s` that is `>= index` (clamped to `s.len()`).
pub fn ceil_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Normalize a path by stripping Windows verbatim device prefixes (`\\?\` or `//?/`).
pub fn clean_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else if let Some(stripped) = s.strip_prefix("//?/") {
        PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}

/// Find the outermost ancestor of `start` carrying a project root marker.
///
/// Climbing to the outermost (rather than nearest) marker matters in workspaces where
/// every crate has its own `Cargo.toml`: a nearest-match rule would scope every
/// operation to a single crate and silently hide the rest of the repository.
pub fn outermost_anchor(start: &Path) -> Option<&Path> {
    let mut found: Option<&Path> = None;
    let mut curr = Some(start);
    while let Some(dir) = curr {
        if dir.join("Cargo.toml").exists()
            || dir.join(".git").exists()
            || dir.join("package.json").exists()
        {
            found = Some(dir);
        }
        curr = dir.parent();
    }
    found
}

impl NativeEngine {
    pub fn new() -> Self {
        Self {
            lsp: Arc::new(lsp::LspEngine::new()),
            terminal: Arc::new(terminal::TerminalEngine::new()),
            workspace_root: Arc::new(RwLock::new(None)),
        }
    }

    /// Retrieve the current workspace root, consulting explicit config, environment, or root anchors.
    ///
    /// Anchor discovery climbs to the *outermost* directory carrying a project marker
    /// rather than stopping at the first one. In a Cargo workspace every crate has its
    /// own `Cargo.toml`, so a nearest-match rule would scope every operation to the one
    /// crate the daemon happened to start in, silently hiding the rest of the repo.
    pub fn get_workspace(&self) -> PathBuf {
        if let Ok(guard) = self.workspace_root.read()
            && let Some(ref root) = *guard
        {
            return clean_path(root);
        }
        if let Ok(env_root) =
            std::env::var("TRANSCEND_WORKSPACE").or_else(|_| std::env::var("WORKSPACE_ROOT"))
        {
            let p = PathBuf::from(env_root);
            if p.exists() {
                return clean_path(&p);
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            return clean_path(outermost_anchor(&cwd).unwrap_or(cwd.as_path()));
        }
        PathBuf::from(".")
    }

    /// Resolve an optional or relative path against the active workspace root.
    pub fn resolve_path(&self, raw: Option<&str>) -> PathBuf {
        let root = self.get_workspace();
        let target = match raw {
            None => root,
            Some(p) => {
                let s = p.trim();
                if s.is_empty() || s == "." {
                    root
                } else {
                    let path = Path::new(s);
                    if path.is_absolute() {
                        path.to_path_buf()
                    } else {
                        root.join(path)
                    }
                }
            }
        };
        clean_path(&target)
    }

    /// Enforce that `target` cannot escape the workspace boundary.
    ///
    /// Used by every mutating primitive (`write_file`, `patch`, `batch_patch`,
    /// `delete_path`) so that a relative path containing `..`, or an absolute path
    /// pointing elsewhere, cannot modify files outside the active workspace.
    ///
    /// `explicit_root` is a caller-supplied boundary (used by tests and by callers
    /// holding several workspaces); when absent the engine's active root applies.
    /// Comparison is done on canonicalized paths, which resolves symlinks so that a
    /// symlink farm cannot be used to hop the boundary. If the target does not exist
    /// yet (a `write_file` creating a new file), its nearest existing ancestor is
    /// canonicalized instead.
    pub fn ensure_within_workspace(
        &self,
        target: &Path,
        explicit_root: Option<&str>,
    ) -> CoreResult<PathBuf> {
        let root_raw = match explicit_root {
            Some(r) if !r.trim().is_empty() => PathBuf::from(r.trim()),
            _ => self.get_workspace(),
        };
        let root = clean_path(&root_raw);

        // Canonicalize the root; a boundary we cannot resolve is treated as a hard
        // error rather than an implicit "allow everything".
        let canonical_root = root.canonicalize().map_err(|e| {
            CoreError::General(format!(
                "Cannot resolve workspace boundary '{}': {e}",
                root.display()
            ))
        })?;
        let canonical_root = clean_path(&canonical_root);

        let resolved_target = Self::canonicalize_existing_ancestor(target)?;
        if !resolved_target.starts_with(&canonical_root) {
            return Err(CoreError::General(format!(
                "Access denied: path '{}' escapes workspace boundary '{}'. \
                 Pass an absolute path inside the workspace, or call set_workspace first.",
                target.display(),
                canonical_root.display()
            )));
        }
        Ok(canonical_root)
    }

    /// Canonicalize `path`, walking up to the nearest existing ancestor when the leaf
    /// (or an intermediate directory) does not exist yet.
    fn canonicalize_existing_ancestor(path: &Path) -> CoreResult<PathBuf> {
        if let Ok(c) = path.canonicalize() {
            return Ok(clean_path(&c));
        }

        let mut ancestor = path.to_path_buf();
        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        loop {
            match ancestor.parent() {
                Some(parent) if parent.as_os_str().is_empty() => break,
                Some(parent) => {
                    let file_name = ancestor
                        .file_name()
                        .map(|n| n.to_os_string())
                        .unwrap_or_default();
                    if file_name.is_empty() {
                        break;
                    }
                    tail.push(file_name);
                    ancestor = parent.to_path_buf();
                }
                None => break,
            }
            if let Ok(c) = ancestor.canonicalize() {
                let mut resolved = clean_path(&c);
                for part in tail.iter().rev() {
                    resolved.push(part);
                }
                return Ok(resolved);
            }
        }

        // Nothing along the chain exists: fall back to the lexical absolute form.
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| CoreError::General(format!("Cannot resolve current directory: {e}")))?
                .join(path)
        };
        Ok(clean_path(&absolute))
    }
}

impl Engine for NativeEngine {
    fn set_workspace(&self, req: &SetWorkspaceRequest) -> CoreResult<SetWorkspaceResponse> {
        let p = Path::new(&req.path);
        if !p.exists() {
            return Err(CoreError::General(format!(
                "Workspace directory does not exist: {}",
                p.display()
            )));
        }
        if !p.is_dir() {
            return Err(CoreError::General(format!(
                "Workspace path is not a directory: {}",
                p.display()
            )));
        }
        let canonical = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let clean = clean_path(&canonical);
        if let Ok(mut guard) = self.workspace_root.write() {
            *guard = Some(clean.clone());
        }
        Ok(SetWorkspaceResponse {
            success: true,
            workspace_root: clean.to_string_lossy().to_string(),
            message: format!("Workspace root configured to {}", clean.display()),
        })
    }

    fn get_workspace(&self) -> PathBuf {
        NativeEngine::get_workspace(self)
    }

    fn search(&self, req: &SearchRequest) -> CoreResult<SearchResponse> {
        let mut resolved = req.clone();
        resolved.path = Some(
            self.resolve_path(req.path.as_deref())
                .to_string_lossy()
                .to_string(),
        );
        SearchScanner::scan(&resolved)
    }

    fn find(&self, req: &FindRequest) -> CoreResult<FindResponse> {
        let mut resolved = req.clone();
        resolved.path = Some(
            self.resolve_path(req.path.as_deref())
                .to_string_lossy()
                .to_string(),
        );
        FindScanner::scan(&resolved)
    }

    fn outline(&self, req: &OutlineRequest) -> CoreResult<OutlineResponse> {
        let mut resolved = req.clone();
        resolved.path = Some(
            self.resolve_path(req.path.as_deref())
                .to_string_lossy()
                .to_string(),
        );
        OutlineScanner::scan(&resolved)
    }

    fn read_symbol(&self, req: &ReadSymbolRequest) -> CoreResult<ReadSymbolResponse> {
        let mut resolved = req.clone();
        resolved.path = Some(
            self.resolve_path(req.path.as_deref())
                .to_string_lossy()
                .to_string(),
        );
        SymbolReader::read(&resolved)
    }

    fn patch(&self, req: &PatchRequest) -> CoreResult<PatchResponse> {
        let mut resolved = req.clone();
        resolved.path = self
            .resolve_path(Some(&req.path))
            .to_string_lossy()
            .to_string();
        resolved.workspace_root = Some(
            self.ensure_within_workspace(Path::new(&resolved.path), req.workspace_root.as_deref())?
                .to_string_lossy()
                .to_string(),
        );
        Patcher::patch(&resolved)
    }

    fn find_symbol(&self, req: &FindSymbolRequest) -> CoreResult<FindSymbolResponse> {
        let mut resolved = req.clone();
        resolved.path = Some(
            self.resolve_path(req.path.as_deref())
                .to_string_lossy()
                .to_string(),
        );
        SymbolFinder::find(&resolved)
    }

    fn read_file(&self, req: &ReadFileRequest) -> CoreResult<ReadFileResponse> {
        let mut resolved = req.clone();
        resolved.path = self
            .resolve_path(Some(&req.path))
            .to_string_lossy()
            .to_string();
        FileOps::read_file(&resolved)
    }

    fn write_file(&self, req: &WriteFileRequest) -> CoreResult<WriteFileResponse> {
        let mut resolved = req.clone();
        resolved.path = self
            .resolve_path(Some(&req.path))
            .to_string_lossy()
            .to_string();
        resolved.workspace_root = Some(
            self.ensure_within_workspace(Path::new(&resolved.path), req.workspace_root.as_deref())?
                .to_string_lossy()
                .to_string(),
        );
        FileOps::write_file(&resolved)
    }

    fn delete_path(&self, req: &DeletePathRequest) -> CoreResult<DeletePathResponse> {
        let mut resolved = req.clone();
        resolved.path = self
            .resolve_path(Some(&req.path))
            .to_string_lossy()
            .to_string();
        resolved.workspace_root = Some(
            self.ensure_within_workspace(Path::new(&resolved.path), req.workspace_root.as_deref())?
                .to_string_lossy()
                .to_string(),
        );
        FileOps::delete_path(&resolved)
    }

    fn batch_patch(&self, req: &BatchPatchRequest) -> CoreResult<BatchPatchResponse> {
        let mut resolved = req.clone();
        for p in &mut resolved.patches {
            p.path = self
                .resolve_path(Some(&p.path))
                .to_string_lossy()
                .to_string();
            p.workspace_root = Some(
                self.ensure_within_workspace(Path::new(&p.path), req.workspace_root.as_deref())?
                    .to_string_lossy()
                    .to_string(),
            );
        }
        resolved.workspace_root = req.workspace_root.clone();
        Patcher::batch_patch(&resolved)
    }

    fn lsp_definition<'a>(
        &'a self,
        req: &'a LspDefinitionRequest,
    ) -> BoxFuture<'a, CoreResult<LspDefinitionResponse>> {
        let mut resolved = req.clone();
        resolved.path = self
            .resolve_path(Some(&req.path))
            .to_string_lossy()
            .to_string();
        Box::pin(async move { self.lsp.goto_definition(self, &resolved).await })
    }

    fn lsp_references<'a>(
        &'a self,
        req: &'a LspReferencesRequest,
    ) -> BoxFuture<'a, CoreResult<LspReferencesResponse>> {
        let mut resolved = req.clone();
        resolved.path = self
            .resolve_path(Some(&req.path))
            .to_string_lossy()
            .to_string();
        Box::pin(async move { self.lsp.find_references(self, &resolved).await })
    }

    fn lsp_hover<'a>(
        &'a self,
        req: &'a LspHoverRequest,
    ) -> BoxFuture<'a, CoreResult<LspHoverResponse>> {
        let mut resolved = req.clone();
        resolved.path = self
            .resolve_path(Some(&req.path))
            .to_string_lossy()
            .to_string();
        Box::pin(async move { self.lsp.hover(self, &resolved).await })
    }

    fn lsp_diagnostics<'a>(
        &'a self,
        req: &'a LspDiagnosticsRequest,
    ) -> BoxFuture<'a, CoreResult<LspDiagnosticsResponse>> {
        let mut resolved = req.clone();
        if let Some(ref p) = req.path {
            resolved.path = Some(self.resolve_path(Some(p)).to_string_lossy().to_string());
        }
        Box::pin(async move { self.lsp.diagnostics(self, &resolved).await })
    }

    fn lsp_status<'a>(
        &'a self,
        req: &'a LspStatusRequest,
    ) -> BoxFuture<'a, CoreResult<LspStatusResponse>> {
        let req = req.clone();
        Box::pin(async move { self.lsp.status(&req).await })
    }

    fn lsp_install<'a>(
        &'a self,
        req: &'a LspInstallRequest,
    ) -> BoxFuture<'a, CoreResult<LspInstallResponse>> {
        let req = req.clone();
        Box::pin(async move { self.lsp.install(&req).await })
    }

    fn exec<'a>(&'a self, req: &'a ExecRequest) -> BoxFuture<'a, CoreResult<ExecResponse>> {
        let mut resolved = req.clone();
        resolved.cwd = Some(
            self.resolve_path(req.cwd.as_deref())
                .to_string_lossy()
                .to_string(),
        );
        Box::pin(async move { self.terminal.exec(&resolved).await })
    }

    fn terminal_read<'a>(
        &'a self,
        req: &'a TerminalReadRequest,
    ) -> BoxFuture<'a, CoreResult<TerminalReadResponse>> {
        Box::pin(async move { self.terminal.read(req).await })
    }

    fn terminal_write<'a>(
        &'a self,
        req: &'a TerminalWriteRequest,
    ) -> BoxFuture<'a, CoreResult<TerminalWriteResponse>> {
        Box::pin(async move { self.terminal.write(req).await })
    }

    fn terminal_resize<'a>(
        &'a self,
        req: &'a TerminalResizeRequest,
    ) -> BoxFuture<'a, CoreResult<TerminalResizeResponse>> {
        Box::pin(async move { self.terminal.resize(req).await })
    }

    fn terminal_kill<'a>(
        &'a self,
        req: &'a TerminalKillRequest,
    ) -> BoxFuture<'a, CoreResult<TerminalKillResponse>> {
        Box::pin(async move { self.terminal.kill(req).await })
    }

    fn git_status<'a>(
        &'a self,
        req: &'a GitStatusRequest,
    ) -> BoxFuture<'a, CoreResult<GitStatusResponse>> {
        let target_dir = self.resolve_path(req.path.as_deref());
        Box::pin(async move {
            tokio::task::spawn_blocking(move || crate::git_ops::GitEngine::status(&target_dir))
                .await
                .map_err(|e| CoreError::General(format!("Git task join error: {e}")))?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use transcend_protocol::{
        FindOptions, FindSymbolRequest, OutlineFormat, OutlineOptions, OutlineRequest, ParseStatus,
        PatchRequest, SearchOptions, SymbolKind,
    };

    static SANDBOX_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    struct TestSandbox {
        dir: PathBuf,
    }

    impl TestSandbox {
        /// Workspace boundary covering this sandbox, so the mutating-op guard accepts
        /// the absolute temp paths the tests use.
        fn boundary(&self) -> Option<String> {
            Some(self.dir.to_string_lossy().to_string())
        }

        fn create() -> Self {
            let pid = std::process::id();
            let count = SANDBOX_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let thread_id = format!("{:?}", std::thread::current().id())
                .replace(|c: char| !c.is_alphanumeric(), "");
            let dir = std::env::temp_dir()
                .join(format!("transcend_test_{}_{}_{}", pid, thread_id, count));
            fs::create_dir_all(&dir).unwrap();

            // 1. Regular UTF-8 file
            let src_dir = dir.join("src");
            fs::create_dir_all(&src_dir).unwrap();
            fs::write(
                src_dir.join("main.rs"),
                "fn hello_world() {\n    println!(\"Hello\");\n}\n",
            )
            .unwrap();

            // 2. Binary file with NUL byte
            fs::write(
                dir.join("image.bin"),
                [b'f', b'n', 0x00, b'b', b'i', b'n', 0x00],
            )
            .unwrap();

            // 3. Non-UTF-8 Latin-1 text file with 0xE9 (é in Latin-1, invalid standalone in UTF-8)
            fs::write(
                dir.join("latin1.txt"),
                [
                    b'f', b'n', b' ', b'c', 0xE9, b'l', b'e', b'b', b'r', b'e', b'\n',
                ],
            )
            .unwrap();

            // 4. Repeated match file for budget testing
            let mut budget_content = String::new();
            for i in 0..10 {
                budget_content.push_str(&format!("item_{}: target_hit\n", i));
            }
            fs::write(dir.join("budget.txt"), budget_content).unwrap();

            Self { dir }
        }

        fn path_str(&self) -> String {
            self.dir.to_string_lossy().to_string()
        }
    }

    impl Drop for TestSandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn test_search_basic_and_line_mapping() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .search(&SearchRequest {
                pattern: "hello_world".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    case_sensitive: Some(true),
                    max_matches: Some(50),
                    ..Default::default()
                }),
            })
            .expect("search should succeed");

        assert_eq!(res.total_matches, 1);
        assert_eq!(res.files.len(), 1);
        assert_eq!(res.files[0].matches.len(), 1);
        assert_eq!(res.files[0].matches[0].line_number, 1);
        assert!(
            res.files[0].matches[0]
                .line_text
                .contains("fn hello_world()")
        );
        assert!(!res.truncated);
    }

    #[test]
    fn test_search_case_sensitivity() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Case-insensitive (should find)
        let res_ci = engine
            .search(&SearchRequest {
                pattern: "HELLO_WORLD".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    case_sensitive: Some(false),
                    ..Default::default()
                }),
            })
            .unwrap();
        assert_eq!(res_ci.total_matches, 1);

        // Case-sensitive (should not find)
        let res_cs = engine
            .search(&SearchRequest {
                pattern: "HELLO_WORLD".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    case_sensitive: Some(true),
                    ..Default::default()
                }),
            })
            .unwrap();
        assert_eq!(res_cs.total_matches, 0);
    }

    #[test]
    fn test_search_skips_binary_with_nul_bytes() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Search for 'bin' which exists only in image.bin
        let res = engine
            .search(&SearchRequest {
                pattern: "bin".to_string(),
                path: Some(sandbox.path_str()),
                options: None,
            })
            .unwrap();

        // Binary file must have been skipped by quit(0x00)
        assert_eq!(res.total_matches, 0);
    }

    #[test]
    fn test_search_handles_invalid_utf8_without_panic() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Search for 'fn' in latin1.txt which contains 0xE9
        let res = engine
            .search(&SearchRequest {
                pattern: "fn".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    file_pattern: Some("latin1.txt".to_string()),
                    ..Default::default()
                }),
            })
            .expect("should not panic on invalid UTF-8 bytes");

        assert_eq!(res.total_matches, 1);
        assert_eq!(res.files.len(), 1);
        assert_eq!(res.files[0].matches.len(), 1);
        // Lossy UTF-8 turns 0xE9 into replacement char
        assert!(res.files[0].matches[0].line_text.contains("c\u{FFFD}lebre"));
    }

    #[test]
    fn test_search_budget_clipping_and_clustering() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .search(&SearchRequest {
                pattern: "target_hit".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    max_matches: Some(3),
                    ..Default::default()
                }),
            })
            .unwrap();

        // Total matches is 10, but returned items is capped at 3
        assert_eq!(res.total_matches, 10);
        assert_eq!(res.files.len(), 1);
        assert_eq!(res.files[0].match_count, 10);
        assert_eq!(res.files[0].matches.len(), 3);
        assert!(res.truncated);
    }

    #[test]
    fn test_search_concurrency_and_deterministic_ordering() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Create 20 files across subdirectories with variable hit counts
        let mut expected_total = 0;
        for i in 1..=20 {
            let sub = sandbox.dir.join(format!("sub_{}", i % 4));
            fs::create_dir_all(&sub).unwrap();
            let mut file_content = String::new();
            for line_idx in 0..i {
                file_content.push_str(&format!("noise line {}\n", line_idx));
                file_content.push_str("concurrent_needle_marker\n");
                expected_total += 1;
            }
            fs::write(sub.join(format!("file_{:02}.txt", i)), file_content).unwrap();
        }

        let res = engine
            .search(&SearchRequest {
                pattern: "concurrent_needle_marker".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    case_sensitive: Some(true),
                    max_matches: Some(25),
                    ..Default::default()
                }),
            })
            .expect("concurrent search should succeed");

        assert_eq!(res.total_matches, expected_total);
        assert!(res.truncated);

        // Verify total matches across files is capped at 25
        let total_inlined_matches: usize = res.files.iter().map(|f| f.matches.len()).sum();
        assert_eq!(total_inlined_matches, 25);

        // Verify files are sorted by match_count descending
        assert_eq!(res.files.len(), 20);
        for window in res.files.windows(2) {
            let a = &window[0];
            let b = &window[1];
            assert!(
                a.match_count >= b.match_count,
                "Files must be sorted by match_count descending: {} vs {}",
                a.match_count,
                b.match_count
            );
        }
        // Top file cluster must have 20 matches (from file_20)
        assert_eq!(res.files[0].match_count, 20);

        // Verify line matches in each file are sorted by line_number ascending
        for file in &res.files {
            for window in file.matches.windows(2) {
                assert!(window[0].line_number <= window[1].line_number);
            }
        }
    }

    #[test]
    fn test_search_max_per_file_diversity() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // monster file with 20 hits
        let mut monster_content = String::new();
        for i in 0..20 {
            monster_content.push_str(&format!("monster_line_{}: diversity_needle\n", i));
        }
        fs::write(sandbox.dir.join("monster.txt"), monster_content).unwrap();

        // regular file with 5 hits
        let mut regular_content = String::new();
        for i in 0..5 {
            regular_content.push_str(&format!("regular_line_{}: diversity_needle\n", i));
        }
        fs::write(sandbox.dir.join("regular.txt"), regular_content).unwrap();

        let res = engine
            .search(&SearchRequest {
                pattern: "diversity_needle".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    max_matches: Some(10),
                    max_per_file: Some(3),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.total_matches, 25);
        assert_eq!(res.total_files, 2);

        let monster = res
            .files
            .iter()
            .find(|f| f.file.contains("monster.txt"))
            .unwrap();
        let regular = res
            .files
            .iter()
            .find(|f| f.file.contains("regular.txt"))
            .unwrap();
        assert_eq!(monster.match_count, 20);
        assert_eq!(monster.matches.len(), 3);
        assert_eq!(regular.match_count, 5);
        assert_eq!(regular.matches.len(), 3);
    }

    #[test]
    fn test_search_line_length_truncation() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Generate 1000 char line with needle
        let long_line = format!("long_prefix_{}_long_suffix\n", "x".repeat(1000));
        fs::write(sandbox.dir.join("long.txt"), long_line).unwrap();

        let res = engine
            .search(&SearchRequest {
                pattern: "long_prefix".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    file_pattern: Some("long.txt".to_string()),
                    max_line_length: Some(40),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.files.len(), 1);
        assert_eq!(res.files[0].matches.len(), 1);
        assert!(res.files[0].matches[0].line_text.contains("[truncated"));
    }

    /// Regression: `include_declaration` defaults to false, meaning the declaration itself
    /// must be excluded from references. The heuristic fallback dropped the flag entirely, so
    /// the default and `include_declaration: true` returned byte-identical sets containing
    /// the definition line -- the caller got no indication the filter had been skipped.
    #[test]
    fn test_heuristic_find_references_respects_include_declaration() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let src = sandbox.dir.join("widget.rs");
        // Line 1 is the declaration; line 3 is a call site.
        fs::write(
            &src,
            "pub fn widget_build() -> u32 { 1 }\n\nfn caller() -> u32 { widget_build() }\n",
        )
        .unwrap();

        let without = crate::lsp::fallback::HeuristicFallback::find_references(
            &engine,
            &src,
            "widget_build",
            false,
            50,
        );
        let with = crate::lsp::fallback::HeuristicFallback::find_references(
            &engine,
            &src,
            "widget_build",
            true,
            50,
        );

        let lines_without: Vec<usize> = without
            .references
            .iter()
            .map(|r| r.span.start_line)
            .collect();
        let lines_with: Vec<usize> = with.references.iter().map(|r| r.span.start_line).collect();

        assert!(
            !lines_with.is_empty(),
            "the call site must be found, got {lines_with:?}"
        );
        assert!(
            !lines_without.contains(&1),
            "default (include_declaration: false) must exclude the declaration on line 1, got {lines_without:?}"
        );
        assert!(
            lines_with.contains(&1),
            "include_declaration: true must include the declaration, got {lines_with:?}"
        );
        assert!(
            lines_with.len() > lines_without.len(),
            "the flag must change the result: {lines_without:?} vs {lines_with:?}"
        );
        assert_eq!(
            without.total_found,
            without.references.len(),
            "total_found must agree with the returned references"
        );
    }

    /// The boundary sent to `FileOps` comes from `ensure_within_workspace` against the
    /// ENGINE's root, not from `FileOps`' own environment/cwd walk. Pinning this makes the
    /// layering explicit: if the engine stops filling `workspace_root`, `FileOps::ensure_within`
    /// silently allows the write instead of failing closed.
    #[test]
    fn mutating_ops_bound_writes_to_the_engine_root_not_the_process_cwd() {
        let outer = TestSandbox::create();
        let inner = outer.dir.join("inner_workspace");
        fs::create_dir_all(&inner).unwrap();

        let engine = NativeEngine::new();
        engine
            .set_workspace(&SetWorkspaceRequest {
                path: inner.to_string_lossy().to_string(),
            })
            .expect("set_workspace should succeed");

        // A relative write resolves and is bounded against the engine root.
        let ok = engine
            .write_file(&WriteFileRequest {
                path: "inside.txt".to_string(),
                content: "ok".to_string(),
                ..Default::default()
            })
            .expect("write inside the engine root must be allowed");
        assert!(ok.success, "{}", ok.message);
        assert!(
            inner.join("inside.txt").exists(),
            "the file belongs under the engine root"
        );

        // An absolute path outside the engine root must be refused even though it sits
        // inside a directory the process could reach.
        let outside = outer.dir.join("escape.txt");
        let err = engine
            .write_file(&WriteFileRequest {
                path: outside.to_string_lossy().to_string(),
                content: "nope".to_string(),
                ..Default::default()
            })
            .expect_err("writing outside the engine root must be refused");
        assert!(
            err.to_string().to_lowercase().contains("access denied")
                || err.to_string().to_lowercase().contains("boundary"),
            "expected a boundary refusal, got: {err}"
        );
        assert!(
            !outside.exists(),
            "the refused write must not have created anything"
        );
    }

    /// Pins the current `read_file` failure contract so its weakness is visible in the code
    /// rather than only in a review.
    ///
    /// `ReadFileResponse` has no success discriminator: a missing file, a directory, and a
    /// genuinely empty file all return `content: ""`, `truncated: false`, `total_lines: 0`.
    /// The only failure signal is the free-text `message`, and the only way to test it is to
    /// parse prose. Every sibling mutating tool returns `success`; `read_file` is the outlier.
    ///
    /// Verified live: an agent gating on `content`/`truncated` cannot tell these apart.
    /// Fixing it means adding a field to a public response contract, so this test records the
    /// behaviour and will need updating when that decision is taken.
    #[test]
    fn read_file_failures_are_indistinguishable_without_parsing_the_message() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let empty = sandbox.dir.join("empty_probe.txt");
        fs::write(&empty, "").unwrap();

        let missing = engine
            .read_file(&ReadFileRequest {
                path: sandbox
                    .dir
                    .join("absent_probe.txt")
                    .to_string_lossy()
                    .to_string(),
                ..Default::default()
            })
            .expect("missing file returns Ok, not Err");
        let blank = engine
            .read_file(&ReadFileRequest {
                path: empty.to_string_lossy().to_string(),
                ..Default::default()
            })
            .expect("empty file reads fine");
        let directory = engine
            .read_file(&ReadFileRequest {
                path: sandbox.dir.to_string_lossy().to_string(),
                ..Default::default()
            })
            .expect("directory returns Ok, not Err");

        // The fields a caller would naturally gate on are identical across all three.
        for (label, res) in [
            ("missing", &missing),
            ("empty", &blank),
            ("directory", &directory),
        ] {
            assert_eq!(res.content, "", "{label}: content");
            assert!(!res.truncated, "{label}: truncated");
            assert_eq!(res.total_lines, 0, "{label}: total_lines");
        }

        // Only prose separates them, which is the whole problem.
        assert!(
            missing.message.is_some(),
            "missing file must explain itself"
        );
        assert!(directory.message.is_some(), "directory must explain itself");
        assert!(
            blank.message.is_none(),
            "a successful read of an empty file carries no message"
        );

        // And the missing-file message says so, in prose rather than a machine-readable field.
        let msg = missing.message.unwrap_or_default();
        assert!(
            msg.contains("does not exist"),
            "expected a prose failure reason, got {msg:?}"
        );
    }

    /// Pins how a misspelled option is handled, so the behaviour is a known choice rather
    /// than a surprise.
    ///
    /// Requests deserialize with unknown fields ignored, so a typo'd option name is silently
    /// dropped and the DEFAULT applies. Verified live on a directory of 150 files:
    ///
    ///   find   max_results: 5   -> 5 entries
    ///   find   max_result: 5    -> 100 entries   (typo: default cap applied)
    ///   search max_matches: 3   -> 3 matches
    ///   search max_match: 3     -> 50 matches    (typo: default cap applied)
    ///
    /// The direction matters. A caller asking for a SMALLER budget than the default silently
    /// receives up to 20x more output than intended -- a token and context problem. It is
    /// worse in the other direction: a caller asking for more results than the default
    /// receives fewer, so relevant results are missing with no signal at all.
    ///
    /// The protocol carries 56 `alias` attributes precisely because option names are easy to
    /// get wrong, so being forgiving about naming is deliberate. Forgiving silently is not the
    /// same as forgiving usefully, though: making it loud needs `deny_unknown_fields` on the
    /// option structs, which rejects any unexpected key and is therefore a breaking change for
    /// hosts that send extra metadata.
    #[test]
    fn misspelled_option_names_are_silently_ignored() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let many = sandbox.dir.join("many");
        fs::create_dir_all(&many).unwrap();
        for i in 0..150 {
            fs::write(many.join(format!("f{i:03}.rs")), "pub fn t() {}\n").unwrap();
        }
        let path = many.to_string_lossy().to_string();

        // The typo is not a field, so it never reaches the engine; the default applies.
        let typo: FindRequest = serde_json::from_value(serde_json::json!({
            "pattern": "*.rs", "path": path, "options": { "max_result": 5 }
        }))
        .expect("an unknown option is accepted, not rejected");
        let res = engine.find(&typo).expect("find should succeed");
        assert_eq!(res.total_count, 150);
        assert!(
            res.entries.len() > 5,
            "the typo fell back to the default cap and returned {} entries",
            res.entries.len()
        );

        // The correctly-spelled option does bound the result, so the contrast is the typo
        // alone rather than a broken cap.
        let good: FindRequest = serde_json::from_value(serde_json::json!({
            "pattern": "*.rs", "path": path, "options": { "max_results": 5 }
        }))
        .expect("valid request");
        assert_eq!(
            engine
                .find(&good)
                .expect("find should succeed")
                .entries
                .len(),
            5
        );
    }

    /// Zero and inverted budgets are rejected rather than silently reinterpreted.
    ///
    /// All three were verified live before the fix:
    /// - `read_file {start_line: 8, end_line: 3}` returned `start_line: 8, end_line: 8` with
    ///   empty content for a 10-line file: the reported range claimed a line it had not
    ///   returned, with no explanation.
    /// - `outline {max_files: 0}` returned an empty census whose `summary.total_files` was 0
    ///   for a directory that plainly contained a file -- indistinguishable from an empty
    ///   directory.
    /// - `search {max_matches: 0}` silently ignored the budget and returned every match,
    ///   while `find_symbol {limit: 0}` returned none: the same input meant two opposite
    ///   things in sibling tools.
    #[test]
    fn zero_and_inverted_budgets_are_rejected() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let file = sandbox.dir.join("ten.txt");
        fs::write(
            &file,
            (1..=10).map(|i| format!("line{i}\n")).collect::<String>(),
        )
        .unwrap();
        let p = file.to_string_lossy().to_string();

        let inverted = engine.read_file(&ReadFileRequest {
            path: p.clone(),
            start_line: Some(8),
            end_line: Some(3),
            ..Default::default()
        });
        assert!(inverted.is_err(), "an inverted range must be refused");
        let msg = inverted.unwrap_err().to_string();
        assert!(
            msg.contains("end_line") && msg.contains("start_line"),
            "got: {msg}"
        );

        // A sane range still works, so the guard is narrow.
        assert!(
            engine
                .read_file(&ReadFileRequest {
                    path: p.clone(),
                    start_line: Some(3),
                    end_line: Some(5),
                    ..Default::default()
                })
                .is_ok()
        );

        for opts in [
            SearchOptions {
                max_matches: Some(0),
                ..Default::default()
            },
            SearchOptions {
                max_per_file: Some(0),
                ..Default::default()
            },
        ] {
            assert!(
                engine
                    .search(&SearchRequest {
                        pattern: "line".to_string(),
                        path: Some(sandbox.path_str()),
                        options: Some(opts),
                    })
                    .is_err(),
                "a zero search budget must be refused"
            );
        }

        for opts in [
            OutlineOptions {
                max_files: Some(0),
                ..Default::default()
            },
            OutlineOptions {
                max_symbols: Some(0),
                ..Default::default()
            },
        ] {
            assert!(
                engine
                    .outline(&OutlineRequest {
                        path: Some(sandbox.path_str()),
                        content: None,
                        options: Some(opts),
                    })
                    .is_err(),
                "a zero outline budget must be refused"
            );
        }

        assert!(
            engine
                .find_symbol(&transcend_protocol::FindSymbolRequest {
                    name: "anything".to_string(),
                    path: Some(sandbox.path_str()),
                    limit: Some(0),
                    ..Default::default()
                })
                .is_err(),
            "a zero limit must be refused"
        );
    }

    /// The outline summary counters must describe what the response carries, and mean the same
    /// thing whichever budget was binding.
    ///
    /// They previously reported different stages: `total_symbols` counted every symbol parsed
    /// before the budget ran, while `total_files` counted files after the file cap but before
    /// the same budget. An 8-file, 16-symbol directory therefore reported `total_files: 1` with
    /// `total_symbols: 16` under `max_files: 1` -- an impossible census, and one that shifted
    /// meaning depending on which cap bit. A reader doing an architecture survey takes these
    /// as the size of the codebase.
    #[test]
    fn outline_summary_reports_what_was_returned() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let dir = sandbox.dir.join("census");
        fs::create_dir_all(&dir).unwrap();
        for i in 0..8 {
            fs::write(
                dir.join(format!("m{i}.rs")),
                format!("pub fn alpha_{i}() {{}}\npub fn beta_{i}() {{}}\n"),
            )
            .unwrap();
        }
        let path = dir.to_string_lossy().to_string();

        let count = |res: &transcend_protocol::OutlineResponse| {
            let shown_symbols: usize = res.files.iter().map(|f| f.symbols.len()).sum();
            (
                res.files.len(),
                shown_symbols,
                res.summary.total_files,
                res.summary.total_symbols,
                res.truncated,
            )
        };

        // Nothing capped: the census is the truth, 8 files and 16 symbols.
        let full = engine
            .outline(&OutlineRequest {
                path: Some(path.clone()),
                content: None,
                options: None,
            })
            .expect("outline should succeed");
        assert_eq!(count(&full), (8, 16, 8, 16, false), "uncapped census");

        // Capped by files, by symbols, and by both: in every case the summary must match the
        // returned payload, and the result must be flagged as partial.
        for (label, opts) in [
            (
                "max_files",
                OutlineOptions {
                    max_files: Some(1),
                    ..Default::default()
                },
            ),
            (
                "max_symbols",
                OutlineOptions {
                    max_symbols: Some(1),
                    ..Default::default()
                },
            ),
            (
                "both",
                OutlineOptions {
                    max_files: Some(1),
                    max_symbols: Some(1),
                    ..Default::default()
                },
            ),
        ] {
            let res = engine
                .outline(&OutlineRequest {
                    path: Some(path.clone()),
                    content: None,
                    options: Some(opts),
                })
                .expect("outline should succeed");
            let (files, symbols, sum_files, sum_symbols, truncated) = count(&res);
            assert_eq!(
                (sum_files, sum_symbols),
                (files, symbols),
                "{label}: summary must describe the returned payload"
            );
            assert!(
                sum_symbols <= sum_files * 2,
                "{label}: {sum_symbols} symbols across {sum_files} files is impossible"
            );
            assert!(
                truncated,
                "{label}: a capped result must be flagged truncated"
            );
        }
    }

    /// The same empty-relative-path defect existed in three tools, found by comparing how each
    /// reports a file path for the same root. With a FILE as the root:
    ///
    ///   search      -> ""                              (fixed)
    ///   find_symbol -> ""                              (fixed here)
    ///   outline     -> "C:/.../root.rs", an absolute path   (fixed here)
    ///
    /// while with a directory root all three agree on a relative path. Since the "relative to
    /// search root" forms are documented, and three tools disagreeing about the same file would
    /// force callers to special-case each, all three now report the file's own name -- the only
    /// relative form when the root is the file.
    #[test]
    fn file_root_paths_are_relative_across_tools() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let dir = sandbox.dir.join("proj");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("root.rs");
        fs::write(&file, "pub fn root_fn() -> u32 { 1 }\npath_marker\n").unwrap();
        let file_str = file.to_string_lossy().to_string();

        // search
        let sr = engine
            .search(&SearchRequest {
                pattern: "path_marker".to_string(),
                path: Some(file_str.clone()),
                options: None,
            })
            .expect("search should succeed");
        assert_eq!(sr.files[0].file, "root.rs", "search");

        // find_symbol
        let fr = engine
            .find_symbol(&transcend_protocol::FindSymbolRequest {
                name: "root_fn".to_string(),
                path: Some(file_str.clone()),
                exact: Some(true),
                ..Default::default()
            })
            .expect("find_symbol should succeed");
        assert!(!fr.symbols.is_empty(), "symbol should be found");
        assert_eq!(fr.symbols[0].file, "root.rs", "find_symbol");

        // outline
        let or = engine
            .outline(&OutlineRequest {
                path: Some(file_str.clone()),
                content: None,
                options: None,
            })
            .expect("outline should succeed");
        assert_eq!(or.files[0].file, "root.rs", "outline");

        // A directory root keeps the relative-with-subdirectory form for all three.
        let dr = engine
            .outline(&OutlineRequest {
                path: Some(dir.to_string_lossy().to_string()),
                content: None,
                options: None,
            })
            .expect("outline should succeed");
        assert_eq!(
            dr.files[0].file, "root.rs",
            "outline under a directory root"
        );
    }

    /// The engine's workspace root is process-global shared state, and `set_workspace` is a
    /// tool any caller can invoke. A single operation reads it more than once: the target path
    /// is resolved with one read and the boundary is enforced with another. If those two reads
    /// straddle a `set_workspace`, one operation can mix two roots.
    ///
    /// This drives that with real threads on a cloned engine (which shares the root by `Arc`)
    /// rather than through a client, because a client serialises requests over one session and
    /// would only exercise ordering. The assertions are the properties that must hold under any
    /// interleaving:
    ///
    /// 1. a write never lands outside BOTH roots -- otherwise the boundary is defeatable by
    ///    racing it, which is a security property, not a cosmetic one;
    /// 2. a successful write reports the path it actually wrote, so the response cannot name a
    ///    root the bytes did not go to.
    #[test]
    fn set_workspace_racing_a_write_never_escapes_both_roots() {
        let sandbox = TestSandbox::create();
        let root_a = sandbox.dir.join("root_a");
        let root_b = sandbox.dir.join("root_b");
        fs::create_dir_all(&root_a).unwrap();
        fs::create_dir_all(&root_b).unwrap();

        let engine = NativeEngine::new();
        engine
            .set_workspace(&SetWorkspaceRequest {
                path: root_a.to_string_lossy().to_string(),
            })
            .expect("set_workspace");

        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_flip = std::sync::Arc::clone(&stop);
        let flipper = {
            let engine = engine.clone();
            let a = root_a.to_string_lossy().to_string();
            let b = root_b.to_string_lossy().to_string();
            std::thread::spawn(move || {
                while !stop_flip.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = engine.set_workspace(&SetWorkspaceRequest { path: a.clone() });
                    let _ = engine.set_workspace(&SetWorkspaceRequest { path: b.clone() });
                }
            })
        };

        let mut escapes = Vec::new();
        let mut mismatches = Vec::new();
        let mut writes = 0usize;
        let mut in_a_count = 0usize;
        let mut in_b_count = 0usize;
        for i in 0..300 {
            let name = format!("race_{i}.txt");
            if let Ok(res) = engine.write_file(&WriteFileRequest {
                path: name.clone(),
                content: "x".to_string(),
                ..Default::default()
            }) && res.success
            {
                writes += 1;
                let in_a = root_a.join(&name).exists();
                let in_b = root_b.join(&name).exists();
                if in_a {
                    in_a_count += 1;
                }
                if in_b {
                    in_b_count += 1;
                }
                if !in_a && !in_b {
                    escapes.push(format!("{name}: wrote outside both roots"));
                }
                let reported = res.file.replace('\\', "/");
                if in_a && !reported.contains("/root_a/") {
                    mismatches.push(format!("{name}: landed in root_a, reported {reported}"));
                }
                if in_b && !reported.contains("/root_b/") {
                    mismatches.push(format!("{name}: landed in root_b, reported {reported}"));
                }
            }
        }

        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        flipper.join().expect("flipper thread");

        // Guard the test itself: if the flipper had not interleaved, every write would land in
        // one root and the escape assertion would be vacuous. Both roots must have received
        // writes for this to have exercised the race at all.
        assert!(writes > 0, "no write succeeded, so nothing was tested");
        assert!(
            in_a_count > 0 && in_b_count > 0,
            "writes reached only one root ({in_a_count} in root_a, {in_b_count} in root_b), so \
             the flipper never interleaved and the escape check proved nothing"
        );
        assert!(
            escapes.is_empty(),
            "writes escaped both roots {} time(s): {:?}",
            escapes.len(),
            &escapes[..escapes.len().min(5)]
        );
        assert!(
            mismatches.is_empty(),
            "the response named a root the bytes did not reach {} time(s): {:?}",
            mismatches.len(),
            &mismatches[..mismatches.len().min(5)]
        );
    }

    /// When `path` names a single file, the search root *is* that file, so
    /// `strip_prefix` produced an empty string and every cluster reported `file: ""`.
    #[test]
    fn test_search_single_file_reports_its_path() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let hits = sandbox.dir.join("single_file_hits.txt");
        fs::write(&hits, "target_hit\ntarget_hit\ntarget_hit\n").unwrap();

        let res = engine
            .search(&SearchRequest {
                pattern: "target_hit".to_string(),
                path: Some(hits.to_string_lossy().to_string()),
                options: None,
            })
            .unwrap();

        assert_eq!(res.total_matches, 3);
        assert_eq!(res.total_files, 1);
        let cluster = res.files.first().expect("one cluster expected");
        assert!(
            !cluster.file.is_empty(),
            "cluster must name the file it describes, got an empty string"
        );
        assert_eq!(
            cluster.file, "single_file_hits.txt",
            "FileCluster.file is documented as relative to the search root; with a file root \
             the only relative form is the file's own name"
        );
        assert!(
            !std::path::Path::new(&cluster.file).is_absolute(),
            "an absolute path is not relative to the search root, and made search disagree \
             with find and outline for the same file"
        );
    }

    /// Regression: `truncated` is documented as "matches were capped due to the match
    /// budget", but it only compared the global total against `max_matches`, so a
    /// `max_per_file` cap dropped line text while the top-level flag still said false.
    #[test]
    fn test_search_truncated_flag_reflects_max_per_file_cap() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let many = sandbox.dir.join("many_hits.txt");
        fs::write(
            &many,
            "cap_marker\ncap_marker\ncap_marker\ncap_marker\ncap_marker\n",
        )
        .unwrap();

        let res = engine
            .search(&SearchRequest {
                pattern: "cap_marker".to_string(),
                path: Some(many.to_string_lossy().to_string()),
                options: Some(SearchOptions {
                    // Global budget is generous; only the per-file cap bites.
                    max_matches: Some(50),
                    max_per_file: Some(2),
                    ..Default::default()
                }),
            })
            .unwrap();

        let cluster = res.files.first().expect("one cluster expected");
        assert_eq!(cluster.match_count, 5, "all 5 matches were counted");
        assert_eq!(cluster.matches.len(), 2, "only 2 returned text");
        assert!(
            cluster.matches_truncated,
            "the cluster itself must report the cap"
        );
        assert!(
            res.truncated,
            "top-level truncated must agree with the cluster: the budget dropped matches"
        );
    }

    /// The global budget must still set `truncated` on its own.
    #[test]
    fn test_search_truncated_flag_reflects_global_budget() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        for i in 0..4 {
            fs::write(
                sandbox.dir.join(format!("global_{i}.txt")),
                "global_marker\nglobal_marker\n",
            )
            .unwrap();
        }

        let res = engine
            .search(&SearchRequest {
                pattern: "global_marker".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    max_matches: Some(3),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert!(res.total_matches > 3, "budget should have dropped matches");
        assert!(res.truncated);
    }

    #[test]
    fn test_search_directory_clusters() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let billing = sandbox.dir.join("billing");
        let auth = sandbox.dir.join("auth");
        fs::create_dir_all(&billing).unwrap();
        fs::create_dir_all(&auth).unwrap();

        fs::write(billing.join("invoice.rs"), "cluster_radar\ncluster_radar\n").unwrap();
        fs::write(billing.join("payment.rs"), "cluster_radar\n").unwrap();
        fs::write(auth.join("token.rs"), "cluster_radar\n").unwrap();

        let res = engine
            .search(&SearchRequest {
                pattern: "cluster_radar".to_string(),
                path: Some(sandbox.path_str()),
                options: None,
            })
            .unwrap();

        assert_eq!(res.total_matches, 4);
        assert_eq!(res.total_files, 3);

        // billing has 2 files and 3 matches; auth has 1 file and 1 match
        assert!(res.directory_radar.len() >= 2);
        let billing_radar = res
            .directory_radar
            .iter()
            .find(|d| d.directory.contains("billing"))
            .unwrap();
        assert_eq!(billing_radar.file_count, 2);
        assert_eq!(billing_radar.match_count, 3);

        let auth_radar = res
            .directory_radar
            .iter()
            .find(|d| d.directory.contains("auth"))
            .unwrap();
        assert_eq!(auth_radar.file_count, 1);
        assert_eq!(auth_radar.match_count, 1);
    }

    #[test]
    fn test_search_context_lines() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let content =
            "line 1 before\nline 2 before\nmatched_needle_target\nline 4 after\nline 5 after\n";
        fs::write(sandbox.dir.join("context.txt"), content).unwrap();

        let res = engine
            .search(&SearchRequest {
                pattern: "matched_needle_target".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    file_pattern: Some("context.txt".to_string()),
                    context_lines: Some(2),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.files.len(), 1);
        assert_eq!(res.files[0].matches.len(), 1);
        let m = &res.files[0].matches[0];
        assert_eq!(m.context_before, vec!["line 1 before", "line 2 before"]);
        assert_eq!(m.context_after, vec!["line 4 after", "line 5 after"]);
    }

    #[test]
    fn test_find_all_files() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: None,
            })
            .expect("find all files should succeed");

        assert_eq!(res.total_count, 4);
        assert_eq!(res.entries.len(), 4);
        assert!(!res.truncated);
        assert!(res.entries.iter().any(|e| e.path.ends_with("main.rs")));
        assert!(res.entries.iter().any(|e| e.path.ends_with("budget.txt")));

        // Metadata check
        for entry in &res.entries {
            assert!(entry.size_bytes > 0);
            assert!(entry.modified.is_some());
        }

        // Extension breakdown check
        assert_eq!(res.extension_breakdown.get("rs"), Some(&1));
        assert_eq!(res.extension_breakdown.get("txt"), Some(&2));
        assert_eq!(res.extension_breakdown.get("bin"), Some(&1));
    }

    /// `respect_gitignore: false` must reach gitignored paths without also pulling in
    /// hidden directories — the two concerns are independent.
    #[test]
    fn test_search_and_find_respect_gitignore_toggle() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Build an ignored directory plus a hidden one inside a real git work tree.
        let repo = sandbox.dir.join("repo");
        let ignored = repo.join("vendored");
        let hidden = repo.join(".hidden_dir");
        fs::create_dir_all(&ignored).unwrap();
        fs::create_dir_all(&hidden).unwrap();
        fs::write(ignored.join("lib.rs"), "pub fn vendored_marker() {}\n").unwrap();
        fs::write(hidden.join("secret.rs"), "pub fn hidden_marker() {}\n").unwrap();
        fs::write(repo.join(".gitignore"), "vendored/\n").unwrap();
        fs::write(repo.join("src.rs"), "pub fn visible_marker() {}\n").unwrap();
        // A `.git` directory makes `ignore` treat this as a work tree root.
        fs::create_dir_all(repo.join(".git")).unwrap();

        let repo_str = repo.to_string_lossy().to_string();

        // Default: gitignore honoured, so the vendored file is invisible.
        let default_search = engine
            .search(&SearchRequest {
                pattern: "vendored_marker".to_string(),
                path: Some(repo_str.clone()),
                options: None,
            })
            .unwrap();
        assert_eq!(
            default_search.total_matches, 0,
            "gitignored file must not be searched by default"
        );

        // Opting out must find it.
        let opted_out = engine
            .search(&SearchRequest {
                pattern: "vendored_marker".to_string(),
                path: Some(repo_str.clone()),
                options: Some(SearchOptions {
                    respect_gitignore: Some(false),
                    ..Default::default()
                }),
            })
            .unwrap();
        assert_eq!(
            opted_out.total_matches, 1,
            "respect_gitignore:false must search ignored paths"
        );

        // ...but must NOT drag in hidden directories.
        let hidden_search = engine
            .search(&SearchRequest {
                pattern: "hidden_marker".to_string(),
                path: Some(repo_str.clone()),
                options: Some(SearchOptions {
                    respect_gitignore: Some(false),
                    ..Default::default()
                }),
            })
            .unwrap();
        assert_eq!(
            hidden_search.total_matches, 0,
            "respect_gitignore must be independent of include_hidden"
        );

        // Both flags together see everything.
        let everything = engine
            .search(&SearchRequest {
                pattern: "marker".to_string(),
                path: Some(repo_str.clone()),
                options: Some(SearchOptions {
                    respect_gitignore: Some(false),
                    include_hidden: Some(true),
                    ..Default::default()
                }),
            })
            .unwrap();
        assert_eq!(everything.total_matches, 3);

        // `find` must behave identically.
        let find_default = engine
            .find(&FindRequest {
                pattern: Some("*.rs".to_string()),
                path: Some(repo_str.clone()),
                options: None,
            })
            .unwrap();
        assert!(
            !find_default
                .entries
                .iter()
                .any(|e| e.path.contains("vendored")),
            "find must skip gitignored paths by default: {:?}",
            find_default.entries
        );

        let find_opted_out = engine
            .find(&FindRequest {
                pattern: Some("*.rs".to_string()),
                path: Some(repo_str),
                options: Some(FindOptions {
                    respect_gitignore: Some(false),
                    ..Default::default()
                }),
            })
            .unwrap();
        assert!(
            find_opted_out
                .entries
                .iter()
                .any(|e| e.path.contains("vendored")),
            "find must reach gitignored paths when asked: {:?}",
            find_opted_out.entries
        );
    }

    #[test]
    fn test_find_glob_pattern() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .find(&FindRequest {
                pattern: Some("*.rs".to_string()),
                path: Some(sandbox.path_str()),
                options: None,
            })
            .unwrap();

        assert_eq!(res.total_count, 1);
        assert_eq!(res.entries.len(), 1);
        assert_eq!(res.entries[0].path, "src/main.rs");
    }

    #[test]
    fn test_find_extension_filter() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: Some(FindOptions {
                    extension: Some("txt".to_string()),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.total_count, 2);
        assert!(res.entries.iter().all(|e| e.path.ends_with(".txt")));
    }

    #[test]
    fn test_find_max_depth() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // max_depth 1 should only return files in root sandbox, not in src/main.rs
        let res = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: Some(FindOptions {
                    max_depth: Some(1),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.total_count, 3);
        assert!(!res.entries.iter().any(|e| e.path.contains("main.rs")));
    }

    #[test]
    fn test_find_directory_type_filter() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: Some(FindOptions {
                    file_type: Some("directory".to_string()),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.total_count, 1);
        assert_eq!(res.entries[0].path, "src");
    }

    #[test]
    fn test_find_dynamic_excludes() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: Some(FindOptions {
                    exclude: Some(vec!["*.txt".to_string(), "*.bin".to_string()]),
                    ..Default::default()
                }),
            })
            .unwrap();

        // Only src/main.rs remains
        assert_eq!(res.total_count, 1);
        assert_eq!(res.entries[0].path, "src/main.rs");
    }

    #[test]
    fn test_find_and_search_hidden_files() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Create a hidden directory and a hidden file
        let github_dir = sandbox.dir.join(".github");
        fs::create_dir_all(&github_dir).unwrap();
        fs::write(github_dir.join("ci.yml"), "name: CI hidden workflow\n").unwrap();
        fs::write(sandbox.dir.join(".env"), "SECRET_KEY=hidden_secret\n").unwrap();

        // 1. Find default (should omit hidden files)
        let res_default = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: None,
            })
            .unwrap();
        assert!(
            !res_default
                .entries
                .iter()
                .any(|e| e.path.contains(".github") || e.path.contains(".env"))
        );

        // 2. Find with include_hidden: true
        let res_hidden = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: Some(FindOptions {
                    include_hidden: Some(true),
                    ..Default::default()
                }),
            })
            .unwrap();
        assert!(
            res_hidden
                .entries
                .iter()
                .any(|e| e.path.contains(".github") || e.path.contains(".env"))
        );

        // 3. Search default (should omit hidden files)
        let search_default = engine
            .search(&SearchRequest {
                pattern: "hidden_secret".to_string(),
                path: Some(sandbox.path_str()),
                options: None,
            })
            .unwrap();
        assert_eq!(search_default.total_matches, 0);

        // 4. Search with include_hidden: true
        let search_hidden = engine
            .search(&SearchRequest {
                pattern: "hidden_secret".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    include_hidden: Some(true),
                    ..Default::default()
                }),
            })
            .unwrap();
        assert_eq!(search_hidden.total_matches, 1);
        assert!(search_hidden.files[0].file.contains(".env"));
    }

    #[test]
    fn test_find_max_per_dir_diversity() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Create heavy folder with 5 files
        let heavy = sandbox.dir.join("heavy");
        fs::create_dir_all(&heavy).unwrap();
        for i in 0..5 {
            fs::write(heavy.join(format!("item_{}.rs", i)), "fn x() {}\n").unwrap();
        }

        let res = engine
            .find(&FindRequest {
                pattern: Some("*.rs".to_string()),
                path: Some(sandbox.path_str()),
                options: Some(FindOptions {
                    max_per_dir: Some(2),
                    ..Default::default()
                }),
            })
            .unwrap();

        // Total .rs files is 6 (1 in src + 5 in heavy)
        assert_eq!(res.total_count, 6);
        // But heavy contributed at most 2, src contributed 1 -> 3 entries returned
        assert_eq!(res.entries.len(), 3);
        let heavy_count = res
            .entries
            .iter()
            .filter(|e| e.path.starts_with("heavy/"))
            .count();
        assert_eq!(heavy_count, 2);
    }

    #[test]
    fn test_find_sort_by_size() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: Some(FindOptions {
                    sort_by: Some("size".to_string()),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.entries.len(), 4);
        for window in res.entries.windows(2) {
            assert!(
                window[0].size_bytes >= window[1].size_bytes,
                "Entries must be sorted by size descending"
            );
        }
    }

    #[test]
    fn test_find_budget_capping_and_radar() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: Some(FindOptions {
                    max_results: Some(2),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.total_count, 4);
        assert_eq!(res.entries.len(), 2);
        assert!(res.truncated);
        assert!(!res.directory_radar.is_empty());
    }

    #[test]
    fn test_outline_rust_symbols_and_hierarchy() {
        let engine = NativeEngine::new();
        let code = r#"
/// Primary user entity.
pub struct User {
    pub id: u64,
    secret: String,
}

pub enum Role {
    Admin,
    Member,
}

pub trait Authenticator {
    fn authenticate(&self) -> bool;
}

impl Authenticator for User {
    fn authenticate(&self) -> bool {
        true
    }
}

impl User {
    pub fn new(id: u64) -> Self {
        Self { id, secret: "".into() }
    }
}
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("user.rs".to_string()),
                content: Some(code.to_string()),
                options: None,
            })
            .expect("outline should succeed");

        assert_eq!(res.files.len(), 1);
        let file = &res.files[0];
        assert_eq!(file.language, "rust");
        assert_eq!(file.parse_status, ParseStatus::Complete);

        // 1. Struct User
        let user_struct = file
            .symbols
            .iter()
            .find(|s| s.name == "User" && s.kind == SymbolKind::Struct)
            .unwrap();
        assert_eq!(user_struct.visibility.as_deref(), Some("pub"));
        assert_eq!(
            user_struct.doc_comment.as_deref(),
            Some("Primary user entity.")
        );
        assert_eq!(user_struct.children.len(), 2);
        assert_eq!(user_struct.children[0].name, "id");
        assert_eq!(user_struct.children[0].kind, SymbolKind::Field);
        assert_eq!(user_struct.children[0].visibility.as_deref(), Some("pub"));
        assert_eq!(user_struct.children[1].name, "secret");
        assert_eq!(user_struct.children[1].visibility, None);

        // Check SourceSpan coordinate preservation
        assert!(user_struct.span.start_line >= 2);
        assert!(user_struct.span.end_line > user_struct.span.start_line);
        assert!(user_struct.span.end_byte > user_struct.span.start_byte);

        // 2. Enum Role
        let role_enum = file
            .symbols
            .iter()
            .find(|s| s.name == "Role" && s.kind == SymbolKind::Enum)
            .unwrap();
        assert_eq!(role_enum.children.len(), 2);
        assert_eq!(role_enum.children[0].name, "Admin");

        // 3. Trait Authenticator
        let auth_trait = file
            .symbols
            .iter()
            .find(|s| s.name == "Authenticator" && s.kind == SymbolKind::Trait)
            .unwrap();
        assert_eq!(auth_trait.children.len(), 1);
        assert_eq!(auth_trait.children[0].name, "authenticate");

        // 4. Impl Authenticator for User
        let impl_auth = file
            .symbols
            .iter()
            .find(|s| s.name.contains("Authenticator for User"))
            .unwrap();
        assert_eq!(impl_auth.kind, SymbolKind::Implementation);
        assert_eq!(impl_auth.relationships.len(), 2);
        assert!(
            impl_auth
                .relationships
                .iter()
                .any(|r| r.relation == "implements" && r.target == "Authenticator")
        );
        assert!(
            impl_auth
                .relationships
                .iter()
                .any(|r| r.relation == "targets" && r.target == "User")
        );
        assert_eq!(impl_auth.children.len(), 1);
        assert_eq!(impl_auth.children[0].name, "authenticate");

        // 5. Impl User
        let impl_user = file.symbols.iter().find(|s| s.name == "impl User").unwrap();
        assert_eq!(impl_user.children.len(), 1);
        assert_eq!(impl_user.children[0].name, "new");
        assert_eq!(impl_user.children[0].visibility.as_deref(), Some("pub"));
    }

    #[test]
    fn test_outline_rust_doc_comments_with_attributes() {
        let engine = NativeEngine::new();
        let code = r#"
/// Model representing a database record.
#[derive(Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Record {
    pub id: u64,
}

/// Dispatches an asynchronous event.
#[tokio::main]
#[inline]
pub async fn dispatch_event() {}
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("record.rs".to_string()),
                content: Some(code.to_string()),
                options: None,
            })
            .expect("outline should succeed");

        let file = &res.files[0];
        let record = file.symbols.iter().find(|s| s.name == "Record").unwrap();
        assert_eq!(
            record.doc_comment.as_deref(),
            Some("Model representing a database record.")
        );

        let dispatch = file
            .symbols
            .iter()
            .find(|s| s.name == "dispatch_event")
            .unwrap();
        assert_eq!(
            dispatch.doc_comment.as_deref(),
            Some("Dispatches an asynchronous event.")
        );
    }

    #[test]
    fn test_outline_typescript_classes_and_interfaces() {
        let engine = NativeEngine::new();
        let code = r#"
/** Service contract interface */
export interface IService {
    port: number;
    start(): Promise<void>;
}

export class WebServer implements IService {
    port: number;
    private running: boolean;

    constructor(port: number) {
        this.port = port;
        this.running = false;
    }

    async start(): Promise<void> {
        console.log("Started");
    }
}
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("server.ts".to_string()),
                content: Some(code.to_string()),
                options: None,
            })
            .expect("typescript outline should succeed");

        assert_eq!(res.files.len(), 1);
        let file = &res.files[0];
        assert_eq!(file.language, "typescript");

        // Interface IService
        let iface = file.symbols.iter().find(|s| s.name == "IService").unwrap();
        assert_eq!(iface.kind, SymbolKind::Interface);
        assert_eq!(iface.visibility.as_deref(), Some("exported"));
        assert!(
            iface
                .doc_comment
                .as_deref()
                .unwrap()
                .contains("Service contract")
        );
        assert_eq!(iface.children.len(), 2);

        // Class WebServer
        let cls = file.symbols.iter().find(|s| s.name == "WebServer").unwrap();
        assert_eq!(cls.kind, SymbolKind::Class);
        assert_eq!(cls.visibility.as_deref(), Some("exported"));
        assert!(
            cls.relationships
                .iter()
                .any(|r| r.relation == "implements" && r.target == "IService")
        );
        assert!(cls.children.iter().any(|c| c.name == "constructor"));
        assert!(
            cls.children
                .iter()
                .any(|c| c.name == "start" && c.kind == SymbolKind::Method)
        );
    }

    #[test]
    fn test_outline_typescript_multi_export_and_constants() {
        let engine = NativeEngine::new();
        let code = r#"
export const API_URL = "https://api.example.com", TIMEOUT = 5000;
export type Handler = () => void;
const INTERNAL_SECRET = 42;
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("config.ts".to_string()),
                content: Some(code.to_string()),
                options: None,
            })
            .expect("typescript outline should succeed");

        assert_eq!(res.files.len(), 1);
        let file = &res.files[0];
        assert_eq!(file.language, "typescript");

        let api_url = file.symbols.iter().find(|s| s.name == "API_URL").unwrap();
        assert_eq!(api_url.kind, SymbolKind::Constant);
        assert_eq!(api_url.visibility.as_deref(), Some("exported"));

        let timeout = file.symbols.iter().find(|s| s.name == "TIMEOUT").unwrap();
        assert_eq!(timeout.kind, SymbolKind::Constant);
        assert_eq!(timeout.visibility.as_deref(), Some("exported"));

        let handler = file.symbols.iter().find(|s| s.name == "Handler").unwrap();
        assert_eq!(handler.kind, SymbolKind::TypeAlias);
        assert_eq!(handler.visibility.as_deref(), Some("exported"));

        let secret = file
            .symbols
            .iter()
            .find(|s| s.name == "INTERNAL_SECRET")
            .unwrap();
        assert_eq!(secret.kind, SymbolKind::Constant);
        assert_eq!(secret.visibility, None);
    }

    #[test]
    fn test_outline_python_classes_methods_docstrings() {
        let engine = NativeEngine::new();
        let code = r#"
class SearchPipeline(BasePipeline):
    """Executes distributed code searches."""

    def __init__(self, capacity: int):
        self.capacity = capacity

    def run(self, query: str):
        pass

    def _internal_clean(self):
        pass
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("pipeline.py".to_string()),
                content: Some(code.to_string()),
                options: None,
            })
            .expect("python outline should succeed");

        assert_eq!(res.files.len(), 1);
        let file = &res.files[0];
        assert_eq!(file.language, "python");

        let cls = file
            .symbols
            .iter()
            .find(|s| s.name == "SearchPipeline")
            .unwrap();
        assert_eq!(cls.kind, SymbolKind::Class);
        assert_eq!(
            cls.doc_comment.as_deref(),
            Some("Executes distributed code searches.")
        );
        assert!(
            cls.relationships
                .iter()
                .any(|r| r.relation == "extends" && r.target == "BasePipeline")
        );

        let init_m = cls.children.iter().find(|c| c.name == "__init__").unwrap();
        assert_eq!(init_m.kind, SymbolKind::Constructor);

        let run_m = cls.children.iter().find(|c| c.name == "run").unwrap();
        assert_eq!(run_m.kind, SymbolKind::Method);
        assert_eq!(run_m.visibility.as_deref(), Some("public"));

        let helper_m = cls
            .children
            .iter()
            .find(|c| c.name == "_internal_clean")
            .unwrap();
        assert_eq!(helper_m.visibility.as_deref(), Some("private"));
    }

    #[test]
    fn test_outline_python_decorators_and_constants() {
        let engine = NativeEngine::new();
        let code = r#"
TIMEOUT = 30

@app.get("/users")
@auth_required
def get_users():
    """Retrieve all users."""
    return []

@dataclass
class User:
    id: int
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("app.py".to_string()),
                content: Some(code.to_string()),
                options: None,
            })
            .expect("python outline should succeed");

        let file = &res.files[0];
        // 1. Constant TIMEOUT
        let const_sym = file.symbols.iter().find(|s| s.name == "TIMEOUT").unwrap();
        assert_eq!(const_sym.kind, SymbolKind::Constant);

        // 2. Decorated function get_users
        let fn_sym = file.symbols.iter().find(|s| s.name == "get_users").unwrap();
        assert_eq!(fn_sym.kind, SymbolKind::Function);
        assert!(fn_sym.signature.as_ref().unwrap().contains("@app.get"));
        assert_eq!(fn_sym.doc_comment.as_deref(), Some("Retrieve all users."));

        // 3. Surgical read_symbol must include leading decorator
        let read_res = engine
            .read_symbol(&ReadSymbolRequest {
                path: Some("app.py".to_string()),
                content: Some(code.to_string()),
                symbol: "get_users".to_string(),
                ..Default::default()
            })
            .unwrap();

        assert!(read_res.found);
        let src = read_res.source_code.unwrap();
        assert!(
            src.starts_with("@app.get"),
            "read_symbol should include decorators: {}",
            src
        );

        // 4. Decorated class User
        let cls_sym = file.symbols.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(cls_sym.kind, SymbolKind::Class);
        assert!(cls_sym.signature.as_ref().unwrap().contains("@dataclass"));
    }

    #[test]
    fn test_outline_go_functions_methods_receivers() {
        let engine = NativeEngine::new();
        let code = r#"
package main

type Engine struct {
    Workers int
}

func (e *Engine) Execute(task string) error {
    return nil
}

func internalHelper() {}
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("engine.go".to_string()),
                content: Some(code.to_string()),
                options: None,
            })
            .expect("go outline should succeed");

        assert_eq!(res.files.len(), 1);
        let file = &res.files[0];
        assert_eq!(file.language, "go");

        let eng_struct = file.symbols.iter().find(|s| s.name == "Engine").unwrap();
        assert_eq!(eng_struct.kind, SymbolKind::Struct);
        assert_eq!(eng_struct.visibility.as_deref(), Some("exported"));

        let exec_method = file.symbols.iter().find(|s| s.name == "Execute").unwrap();
        assert_eq!(exec_method.kind, SymbolKind::Method);
        assert_eq!(exec_method.visibility.as_deref(), Some("exported"));
        assert!(
            exec_method
                .relationships
                .iter()
                .any(|r| r.relation == "receiver" && r.target == "Engine")
        );

        let helper_fn = file
            .symbols
            .iter()
            .find(|s| s.name == "internalHelper")
            .unwrap();
        assert_eq!(helper_fn.visibility, None);
    }

    #[test]
    fn test_outline_go_grouped_declarations() {
        let engine = NativeEngine::new();
        let code = r#"
package main

const (
    StatusPending = "pending"
    StatusDone = "done"
)

type (
    ID uint64
    HandlerFunc func() error
)

var (
    ErrNotFound = "not found"
)
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("types.go".to_string()),
                content: Some(code.to_string()),
                options: None,
            })
            .expect("go outline should succeed");

        assert_eq!(res.files.len(), 1);
        let file = &res.files[0];
        assert_eq!(file.language, "go");

        let pending = file
            .symbols
            .iter()
            .find(|s| s.name == "StatusPending")
            .unwrap();
        assert_eq!(pending.kind, SymbolKind::Constant);
        assert_eq!(pending.visibility.as_deref(), Some("exported"));

        let done = file
            .symbols
            .iter()
            .find(|s| s.name == "StatusDone")
            .unwrap();
        assert_eq!(done.kind, SymbolKind::Constant);
        assert_eq!(done.visibility.as_deref(), Some("exported"));

        let id = file.symbols.iter().find(|s| s.name == "ID").unwrap();
        assert_eq!(id.kind, SymbolKind::TypeAlias);
        assert_eq!(id.visibility.as_deref(), Some("exported"));

        let handler = file
            .symbols
            .iter()
            .find(|s| s.name == "HandlerFunc")
            .unwrap();
        assert_eq!(handler.kind, SymbolKind::TypeAlias);
        assert_eq!(handler.visibility.as_deref(), Some("exported"));

        let err = file
            .symbols
            .iter()
            .find(|s| s.name == "ErrNotFound")
            .unwrap();
        assert_eq!(err.kind, SymbolKind::Variable);
        assert_eq!(err.visibility.as_deref(), Some("exported"));
    }

    #[test]
    fn test_outline_filters_and_depth_budget() {
        let engine = NativeEngine::new();
        let code = r#"
pub struct Service {
    pub id: u64,
    secret: String,
}

impl Service {
    pub fn public_action(&self) {}
    fn private_action(&self) {}
}

fn private_toplevel() {}
pub fn public_toplevel() {}
"#;

        // 1. Exported only filter
        let res_exported = engine
            .outline(&OutlineRequest {
                path: Some("service.rs".to_string()),
                content: Some(code.to_string()),
                options: Some(OutlineOptions {
                    exported_only: Some(true),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert!(
            !res_exported.files[0]
                .symbols
                .iter()
                .any(|s| s.name == "private_toplevel")
        );
        assert!(
            res_exported.files[0]
                .symbols
                .iter()
                .any(|s| s.name == "public_toplevel")
        );

        // 2. Symbol kinds filter
        let res_kinds = engine
            .outline(&OutlineRequest {
                path: Some("service.rs".to_string()),
                content: Some(code.to_string()),
                options: Some(OutlineOptions {
                    symbol_kinds: Some(vec![SymbolKind::Struct]),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res_kinds.files[0].symbols.len(), 1);
        assert_eq!(res_kinds.files[0].symbols[0].kind, SymbolKind::Struct);

        // 3. Max depth pruning (depth 1 strips fields and methods)
        let res_depth = engine
            .outline(&OutlineRequest {
                path: Some("service.rs".to_string()),
                content: Some(code.to_string()),
                options: Some(OutlineOptions {
                    max_depth: Some(1),
                    ..Default::default()
                }),
            })
            .unwrap();

        let s = res_depth.files[0]
            .symbols
            .iter()
            .find(|sym| sym.name == "Service")
            .unwrap();
        assert!(
            s.children.is_empty(),
            "Children should be pruned at depth 1"
        );
    }

    #[test]
    fn test_outline_resilient_to_syntax_errors() {
        let engine = NativeEngine::new();
        let broken_code = r#"
pub struct ValidStruct {
    pub field: u32,
}

pub fn broken_function( {
    let incomplete = 
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("broken.rs".to_string()),
                content: Some(broken_code.to_string()),
                options: None,
            })
            .expect("should not fail even with syntax errors");

        let file = &res.files[0];
        assert_eq!(file.parse_status, ParseStatus::Partial);
        // ValidStruct should still be extracted!
        assert!(file.symbols.iter().any(|s| s.name == "ValidStruct"));
    }

    #[test]
    fn test_outline_directory_macro_census_and_budget() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Write multiple multi-language files into sandbox
        fs::write(
            sandbox.dir.join("logic.py"),
            "class PythonModel:\n    def execute(self):\n        pass\n",
        )
        .unwrap();

        fs::write(
            sandbox.dir.join("types.ts"),
            "export interface TypeScriptContract {\n    id: string;\n}\n",
        )
        .unwrap();

        let res = engine
            .outline(&OutlineRequest {
                path: Some(sandbox.path_str()),
                content: None,
                options: Some(OutlineOptions {
                    max_symbols: Some(3),
                    ..Default::default()
                }),
            })
            .expect("directory outline should succeed");

        assert!(res.summary.total_files >= 2);
        assert!(res.summary.total_symbols > 0);
        assert!(!res.summary.language_breakdown.is_empty());
        assert!(!res.summary.kind_breakdown.is_empty());
        assert!(res.truncated);
    }

    #[test]
    fn test_outline_max_output_bytes_budget() {
        let engine = NativeEngine::new();
        let code = r#"
pub struct Alpha { pub a: u32, pub b: u32 }
pub struct Beta { pub c: u32, pub d: u32 }
pub struct Gamma { pub e: u32, pub f: u32 }
pub struct Delta { pub g: u32, pub h: u32 }
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("budget_test.rs".to_string()),
                content: Some(code.to_string()),
                options: Some(OutlineOptions {
                    max_output_bytes: Some(450),
                    ..Default::default()
                }),
            })
            .expect("outline should succeed");

        assert!(res.truncated, "Should be truncated by byte budget");
        let json_size = serde_json::to_vec(&res.files).unwrap().len();
        assert!(
            json_size <= 600,
            "Output size {} should be capped",
            json_size
        );
    }

    #[test]
    fn test_outline_include_relationships_toggle_and_single_line_docs() {
        let engine = NativeEngine::new();
        let code = r#"
/// First line summary of contract.
///
/// Long extensive paragraph that should never be dumped into agent context.
/// Multiple lines of prose here.
pub struct Contract;

impl Contract {
    pub fn execute(&self) {}
}
"#;

        // 1. Single line doc summary check
        let res_docs = engine
            .outline(&OutlineRequest {
                path: Some("doc_test.rs".to_string()),
                content: Some(code.to_string()),
                options: None,
            })
            .unwrap();

        let contract_sym = res_docs.files[0]
            .symbols
            .iter()
            .find(|s| s.name == "Contract")
            .unwrap();
        assert_eq!(
            contract_sym.doc_comment.as_deref(),
            Some("First line summary of contract.")
        );

        // 2. Relationships toggle check
        let res_no_rels = engine
            .outline(&OutlineRequest {
                path: Some("doc_test.rs".to_string()),
                content: Some(code.to_string()),
                options: Some(OutlineOptions {
                    include_relationships: Some(false),
                    ..Default::default()
                }),
            })
            .unwrap();

        let impl_sym = res_no_rels.files[0]
            .symbols
            .iter()
            .find(|s| s.name == "impl Contract")
            .unwrap();
        assert!(
            impl_sym.relationships.is_empty(),
            "Relationships should be empty when include_relationships is false"
        );
    }

    #[test]
    fn test_outline_c_and_cpp() {
        let engine = NativeEngine::new();

        // 1. Test C
        let c_code = r#"
/// Adds two numbers
int add(int a, int b) {
    return a + b;
}

struct Point {
    int x;
    int y;
};
"#;
        let c_res = engine
            .outline(&OutlineRequest {
                path: Some("math.c".to_string()),
                content: Some(c_code.to_string()),
                options: None,
            })
            .expect("C outline should succeed");

        assert_eq!(c_res.files[0].language, "c");
        assert!(
            c_res.files[0]
                .symbols
                .iter()
                .any(|s| s.name == "add" && s.kind == SymbolKind::Function)
        );
        assert!(
            c_res.files[0]
                .symbols
                .iter()
                .any(|s| s.name == "Point" && s.kind == SymbolKind::Struct)
        );

        let header_code = r#"
#define MAX_BUFFER 1024
typedef struct _ENTRY {
    int id;
} ENTRY, *PENTRY;

int process_data(ENTRY *e);
"#;
        let h_res = engine
            .outline(&OutlineRequest {
                path: Some("header.h".to_string()),
                content: Some(header_code.to_string()),
                options: None,
            })
            .expect("C header outline should succeed");
        assert_eq!(h_res.files[0].language, "c");
        assert!(
            h_res.files[0]
                .symbols
                .iter()
                .any(|s| s.name == "MAX_BUFFER" && s.kind == SymbolKind::Macro)
        );
        assert!(
            h_res.files[0]
                .symbols
                .iter()
                .any(|s| s.name == "ENTRY" && s.kind == SymbolKind::Struct)
        );
        assert!(
            h_res.files[0]
                .symbols
                .iter()
                .any(|s| s.name == "process_data" && s.kind == SymbolKind::Function)
        );

        // 2. Test C++
        let cpp_code = r#"
class Animal {
public:
    Animal();
    virtual void speak();
private:
    int age;
};
"#;
        let cpp_res = engine
            .outline(&OutlineRequest {
                path: Some("animal.cpp".to_string()),
                content: Some(cpp_code.to_string()),
                options: None,
            })
            .expect("C++ outline should succeed");

        assert_eq!(cpp_res.files[0].language, "cpp");
        let class_sym = cpp_res.files[0]
            .symbols
            .iter()
            .find(|s| s.name == "Animal")
            .unwrap();
        assert_eq!(class_sym.kind, SymbolKind::Class);
        assert!(
            class_sym
                .children
                .iter()
                .any(|c| c.name == "speak" && c.visibility.as_deref() == Some("public"))
        );
        assert!(
            class_sym
                .children
                .iter()
                .any(|c| c.name == "age" && c.visibility.as_deref() == Some("private"))
        );
    }

    #[test]
    fn test_outline_csharp() {
        let engine = NativeEngine::new();
        let cs_code = r#"
namespace Services {
    /// <summary>
    /// Worker contract interface
    /// </summary>
    public interface IWorker {
        void Execute();
    }

    public class BackgroundWorker : IWorker {
        public string Name { get; set; }
        public void Execute() {}
    }
}
"#;
        let res = engine
            .outline(&OutlineRequest {
                path: Some("Worker.cs".to_string()),
                content: Some(cs_code.to_string()),
                options: None,
            })
            .expect("C# outline should succeed");

        assert_eq!(res.files[0].language, "csharp");
        let ns = &res.files[0].symbols[0];
        assert_eq!(ns.kind, SymbolKind::Namespace);
        assert!(
            ns.children
                .iter()
                .any(|s| s.name == "IWorker" && s.kind == SymbolKind::Interface)
        );
        let worker_class = ns
            .children
            .iter()
            .find(|s| s.name == "BackgroundWorker")
            .unwrap();
        assert_eq!(worker_class.kind, SymbolKind::Class);
        assert!(
            worker_class
                .relationships
                .iter()
                .any(|r| r.relation == "implements" && r.target == "IWorker")
        );
        assert!(
            worker_class
                .children
                .iter()
                .any(|c| c.name == "Name" && c.kind == SymbolKind::Property)
        );
        assert!(
            worker_class
                .children
                .iter()
                .any(|c| c.name == "Execute" && c.kind == SymbolKind::Method)
        );
    }

    #[test]
    fn test_outline_java() {
        let engine = NativeEngine::new();
        let java_code = r#"
package com.transcend;

/**
 * Main application service
 */
public class ApplicationService implements Runnable {
    private int counter;

    public ApplicationService() {}

    @Override
    public void run() {
        System.out.println("Running");
    }
}
"#;
        let res = engine
            .outline(&OutlineRequest {
                path: Some("ApplicationService.java".to_string()),
                content: Some(java_code.to_string()),
                options: None,
            })
            .expect("Java outline should succeed");

        assert_eq!(res.files[0].language, "java");
        let app_class = res.files[0]
            .symbols
            .iter()
            .find(|s| s.name == "ApplicationService")
            .unwrap();
        assert_eq!(app_class.kind, SymbolKind::Class);
        assert_eq!(
            app_class.doc_comment.as_deref(),
            Some("Main application service")
        );
        assert!(
            app_class
                .relationships
                .iter()
                .any(|r| r.relation == "implements" && r.target == "Runnable")
        );
        assert!(
            app_class
                .children
                .iter()
                .any(|c| c.name == "run" && c.kind == SymbolKind::Method)
        );
        assert!(
            app_class
                .children
                .iter()
                .any(|c| c.name == "ApplicationService" && c.kind == SymbolKind::Constructor)
        );
    }

    #[test]
    fn test_outline_kotlin() {
        let engine = NativeEngine::new();
        let kt_code = r#"
package com.demo

/**
 * User account model
 */
class User(val id: String) {
    fun getDisplayName(): String {
        return id
    }
}
"#;
        let res = engine
            .outline(&OutlineRequest {
                path: Some("User.kt".to_string()),
                content: Some(kt_code.to_string()),
                options: None,
            })
            .expect("Kotlin outline should succeed");

        assert_eq!(res.files[0].language, "kotlin");
        let user_class = res.files[0]
            .symbols
            .iter()
            .find(|s| s.name == "User")
            .unwrap();
        assert_eq!(user_class.kind, SymbolKind::Class);
        assert_eq!(
            user_class.doc_comment.as_deref(),
            Some("User account model")
        );
        assert!(
            user_class
                .children
                .iter()
                .any(|c| c.name == "getDisplayName" && c.kind == SymbolKind::Method)
        );
    }

    #[test]
    fn test_outline_php() {
        let engine = NativeEngine::new();
        let php_code = r#"<?php
namespace App\Controllers;

/**
 * Controller class
 */
class HomeController {
    public function index() {
        return "hello";
    }
}
"#;
        let res = engine
            .outline(&OutlineRequest {
                path: Some("HomeController.php".to_string()),
                content: Some(php_code.to_string()),
                options: None,
            })
            .expect("PHP outline should succeed");

        assert_eq!(res.files[0].language, "php");
        let ns = &res.files[0].symbols[0];
        assert_eq!(ns.kind, SymbolKind::Namespace);
        let ctrl = ns
            .children
            .iter()
            .find(|s| s.name == "HomeController")
            .unwrap();
        assert_eq!(ctrl.kind, SymbolKind::Class);
        assert!(
            ctrl.children
                .iter()
                .any(|m| m.name == "index" && m.kind == SymbolKind::Method)
        );
    }

    #[test]
    fn test_outline_ruby() {
        let engine = NativeEngine::new();
        let rb_code = r#"
# Core authentication module
module Authentication
  class SessionManager < BaseManager
    def create_session(user)
      # logic
    end
  end
end
"#;
        let res = engine
            .outline(&OutlineRequest {
                path: Some("auth.rb".to_string()),
                content: Some(rb_code.to_string()),
                options: None,
            })
            .expect("Ruby outline should succeed");

        assert_eq!(res.files[0].language, "ruby");
        let m = &res.files[0].symbols[0];
        assert_eq!(m.name, "Authentication");
        assert_eq!(m.kind, SymbolKind::Module);
        let cls = m
            .children
            .iter()
            .find(|s| s.name == "SessionManager")
            .unwrap();
        assert_eq!(cls.kind, SymbolKind::Class);
        assert!(
            cls.relationships
                .iter()
                .any(|r| r.relation == "extends" && r.target == "BaseManager")
        );
        assert!(
            cls.children
                .iter()
                .any(|c| c.name == "create_session" && c.kind == SymbolKind::Method)
        );
    }

    #[test]
    fn test_outline_swift() {
        let engine = NativeEngine::new();
        let swift_code = r#"
/// High performance vehicle
public class SportsCar {
    public init() {}
    public func accelerate() {}
}
"#;
        let res = engine
            .outline(&OutlineRequest {
                path: Some("Car.swift".to_string()),
                content: Some(swift_code.to_string()),
                options: None,
            })
            .expect("Swift outline should succeed");

        assert_eq!(res.files[0].language, "swift");
        let car = res.files[0]
            .symbols
            .iter()
            .find(|s| s.name == "SportsCar")
            .unwrap();
        assert_eq!(car.kind, SymbolKind::Class);
        assert_eq!(car.doc_comment.as_deref(), Some("High performance vehicle"));
        assert!(
            car.children
                .iter()
                .any(|c| c.name == "init" && c.kind == SymbolKind::Constructor)
        );
        assert!(
            car.children
                .iter()
                .any(|c| c.name == "accelerate" && c.kind == SymbolKind::Method)
        );
    }

    #[test]
    fn test_outline_bash() {
        let engine = NativeEngine::new();
        let bash_code = r#"#!/bin/bash
# Deployment helper script
export DEPLOY_ENV="production"

deploy_service() {
    echo "Deploying..."
}
"#;
        let res = engine
            .outline(&OutlineRequest {
                path: Some("deploy.sh".to_string()),
                content: Some(bash_code.to_string()),
                options: None,
            })
            .expect("Bash outline should succeed");

        assert_eq!(res.files[0].language, "bash");
        assert!(
            res.files[0]
                .symbols
                .iter()
                .any(|s| s.name == "deploy_service" && s.kind == SymbolKind::Function)
        );
        assert!(
            res.files[0]
                .symbols
                .iter()
                .any(|s| s.name == "DEPLOY_ENV" && s.kind == SymbolKind::Constant)
        );
    }

    #[test]
    fn test_outline_sql() {
        let engine = NativeEngine::new();
        let sql_code = r#"
-- Customer accounts table
CREATE TABLE customers (
    id INT PRIMARY KEY,
    email VARCHAR(255)
);

CREATE VIEW active_customers AS SELECT * FROM customers;
"#;
        let res = engine
            .outline(&OutlineRequest {
                path: Some("schema.sql".to_string()),
                content: Some(sql_code.to_string()),
                options: None,
            })
            .expect("SQL outline should succeed");

        assert_eq!(res.files[0].language, "sql");
        let tbl = res.files[0]
            .symbols
            .iter()
            .find(|s| s.name == "customers")
            .unwrap();
        assert_eq!(tbl.kind, SymbolKind::Struct);
        assert_eq!(tbl.doc_comment.as_deref(), Some("Customer accounts table"));
        assert!(
            res.files[0]
                .symbols
                .iter()
                .any(|s| s.name == "active_customers" && s.kind == SymbolKind::Interface)
        );
    }

    #[test]
    fn test_outline_dart() {
        let engine = NativeEngine::new();
        let dart_code = r#"
/// User repository
class UserRepository {
    void _internalSync() {}
    void fetchUser() {}
}
"#;
        let res = engine
            .outline(&OutlineRequest {
                path: Some("repo.dart".to_string()),
                content: Some(dart_code.to_string()),
                options: None,
            })
            .expect("Dart outline should succeed");

        assert_eq!(res.files[0].language, "dart");
        let repo = res.files[0]
            .symbols
            .iter()
            .find(|s| s.name == "UserRepository")
            .unwrap();
        assert_eq!(repo.kind, SymbolKind::Class);
        assert_eq!(repo.doc_comment.as_deref(), Some("User repository"));
        assert!(
            repo.children
                .iter()
                .any(|c| c.name == "fetchUser" && c.visibility.as_deref() == Some("public"))
        );
        assert!(
            repo.children
                .iter()
                .any(|c| c.name == "_internalSync" && c.visibility.as_deref() == Some("private"))
        );
    }

    #[test]
    fn test_outline_zig() {
        let engine = NativeEngine::new();
        let zig_code = r#"
/// App configuration
pub const AppConfig = struct {
    port: u16,
};

pub fn startServer() void {
}
"#;
        let res = engine
            .outline(&OutlineRequest {
                path: Some("main.zig".to_string()),
                content: Some(zig_code.to_string()),
                options: None,
            })
            .expect("Zig outline should succeed");

        assert_eq!(res.files[0].language, "zig");
        assert!(
            res.files[0]
                .symbols
                .iter()
                .any(|s| s.name == "AppConfig" && s.kind == SymbolKind::Struct)
        );
        assert!(
            res.files[0]
                .symbols
                .iter()
                .any(|s| s.name == "startServer" && s.kind == SymbolKind::Function)
        );
    }

    #[test]
    fn test_outline_lua() {
        let engine = NativeEngine::new();
        let lua_code = r#"
--- Module documentation
local M = {}

function M:save()
end

function M.load()
end

return M
"#;
        let res = engine
            .outline(&OutlineRequest {
                path: Some("storage.lua".to_string()),
                content: Some(lua_code.to_string()),
                options: None,
            })
            .expect("Lua outline should succeed");

        assert_eq!(res.files[0].language, "lua");
        let save_sym = res.files[0]
            .symbols
            .iter()
            .find(|s| s.name == "M:save")
            .unwrap();
        assert_eq!(save_sym.kind, SymbolKind::Method);
        assert!(
            save_sym
                .relationships
                .iter()
                .any(|r| r.relation == "receiver" && r.target == "M")
        );
        assert!(
            res.files[0]
                .symbols
                .iter()
                .any(|s| s.name == "M.load" && s.kind == SymbolKind::Function)
        );
    }

    #[test]
    fn test_outline_markdown() {
        let engine = NativeEngine::new();
        let md_code = r#"
# Transcend Documentation

Universal agent-native cartographer.

## Core Features

Fast in-process primitives.

### Fast Search
Multi-threaded ripgrep.

### Code Outline
Hierarchical symbol index.

## Verification
Automated test suite.
"#;
        let res = engine
            .outline(&OutlineRequest {
                path: Some("README.md".to_string()),
                content: Some(md_code.to_string()),
                options: None,
            })
            .expect("Markdown outline should succeed");

        assert_eq!(res.files[0].language, "markdown");
        let root_h1 = &res.files[0].symbols[0];
        assert_eq!(root_h1.name, "Transcend Documentation");
        assert_eq!(root_h1.kind, SymbolKind::Module);
        assert_eq!(
            root_h1.doc_comment.as_deref(),
            Some("Universal agent-native cartographer.")
        );

        // H1 should have two H2 children: "Core Features" and "Verification"
        assert_eq!(root_h1.children.len(), 2);
        let core_features = &root_h1.children[0];
        assert_eq!(core_features.name, "Core Features");

        // "Core Features" should have two H3 children: "Fast Search" and "Code Outline"
        assert_eq!(core_features.children.len(), 2);
        assert_eq!(core_features.children[0].name, "Fast Search");
        assert_eq!(core_features.children[1].name, "Code Outline");
    }

    #[test]
    fn test_read_symbol_bare_and_qualified_name() {
        let engine = NativeEngine::new();
        let code = r#"
pub struct Calculator {
    pub scale: f64,
}

impl Calculator {
    /// Adds two numbers with scale
    pub fn add(&self, a: f64, b: f64) -> f64 {
        (a + b) * self.scale
    }
}

pub fn global_helper() -> i32 {
    42
}
"#;

        // 1. Bare name lookup of global function
        let res1 = engine
            .read_symbol(&ReadSymbolRequest {
                path: Some("calc.rs".to_string()),
                content: Some(code.to_string()),
                symbol: "global_helper".to_string(),
                ..Default::default()
            })
            .unwrap();

        assert!(res1.found);
        assert_eq!(res1.symbol.as_ref().unwrap().name, "global_helper");
        assert!(res1.source_code.as_ref().unwrap().contains("42"));
        assert_eq!(res1.total_occurrences, 1);

        // 2. Qualified name lookup of method: Calculator::add
        let res2 = engine
            .read_symbol(&ReadSymbolRequest {
                path: Some("calc.rs".to_string()),
                content: Some(code.to_string()),
                symbol: "Calculator::add".to_string(),
                ..Default::default()
            })
            .unwrap();

        assert!(res2.found);
        assert_eq!(res2.qualified_name.as_deref(), Some("Calculator::add"));
        assert!(
            res2.source_code
                .as_ref()
                .unwrap()
                .contains("(a + b) * self.scale")
        );
        assert_eq!(
            res2.symbol.as_ref().unwrap().doc_comment.as_deref(),
            Some("Adds two numbers with scale")
        );

        // 3. Normalized dot notation lookup: Calculator.add
        let res3 = engine
            .read_symbol(&ReadSymbolRequest {
                path: Some("calc.rs".to_string()),
                content: Some(code.to_string()),
                symbol: "Calculator.add".to_string(),
                ..Default::default()
            })
            .unwrap();

        assert!(res3.found);
        assert_eq!(res3.qualified_name.as_deref(), Some("Calculator::add"));

        // 4. Bare method name lookup
        let res4 = engine
            .read_symbol(&ReadSymbolRequest {
                path: Some("calc.rs".to_string()),
                content: Some(code.to_string()),
                symbol: "add".to_string(),
                ..Default::default()
            })
            .unwrap();

        assert!(res4.found);
        assert_eq!(res4.symbol.as_ref().unwrap().name, "add");
    }

    #[test]
    fn test_read_symbol_occurrences_and_context() {
        let engine = NativeEngine::new();
        let code = r#"// Line 1: header
// Line 2: intro
fn execute() {
    println!("first");
}
// Line 6: middle
fn execute() {
    println!("second");
}
// Line 10: footer"#;

        // Occurrence 0 (first) with 1 context line before and after
        let res0 = engine
            .read_symbol(&ReadSymbolRequest {
                path: Some("exec.rs".to_string()),
                content: Some(code.to_string()),
                symbol: "execute".to_string(),
                occurrence: Some(0),
                context_lines: Some(1),
                ..Default::default()
            })
            .unwrap();

        assert!(res0.found);
        assert_eq!(res0.total_occurrences, 2);
        assert!(res0.source_code.as_ref().unwrap().contains("first"));
        assert!(res0.context_before.as_ref().unwrap().contains("Line 2"));
        assert!(res0.context_after.as_ref().unwrap().contains("Line 6"));

        // Occurrence 1 (second)
        let res1 = engine
            .read_symbol(&ReadSymbolRequest {
                path: Some("exec.rs".to_string()),
                content: Some(code.to_string()),
                symbol: "execute".to_string(),
                occurrence: Some(1),
                ..Default::default()
            })
            .unwrap();

        assert!(res1.found);
        assert!(res1.source_code.as_ref().unwrap().contains("second"));

        // Occurrence out of range
        let res_out = engine
            .read_symbol(&ReadSymbolRequest {
                path: Some("exec.rs".to_string()),
                content: Some(code.to_string()),
                symbol: "execute".to_string(),
                occurrence: Some(99),
                ..Default::default()
            })
            .unwrap();

        assert!(!res_out.found);
        assert!(res_out.message.as_ref().unwrap().contains("out of range"));
    }

    #[test]
    fn test_read_symbol_not_found_suggestions() {
        let engine = NativeEngine::new();
        let code = r#"
pub fn parse_header() {}
pub fn parse_body() {}
"#;
        let res = engine
            .read_symbol(&ReadSymbolRequest {
                path: Some("parser.rs".to_string()),
                content: Some(code.to_string()),
                symbol: "parse_footer".to_string(),
                ..Default::default()
            })
            .unwrap();

        assert!(!res.found);
        assert_eq!(res.total_occurrences, 0);
        let msg = res.message.unwrap();
        assert!(msg.contains("parse_header"));
        assert!(msg.contains("parse_body"));
    }

    #[test]
    fn test_outline_skeleton_format() {
        let engine = NativeEngine::new();

        // 1. Rust skeleton
        let rust_code = r#"
/// Configuration for the server.
pub struct Config {
    pub port: u16,
    pub host: String,
}

impl Config {
    pub fn new(port: u16) -> Self {
        Self { port, host: "localhost".to_string() }
    }
}
"#;
        let rust_res = engine
            .outline(&OutlineRequest {
                path: Some("config.rs".to_string()),
                content: Some(rust_code.to_string()),
                options: Some(OutlineOptions {
                    format: Some(OutlineFormat::Skeleton),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(rust_res.files.len(), 1);
        let skel = rust_res.files[0]
            .skeleton
            .as_ref()
            .expect("skeleton should be present");
        assert!(
            rust_res.files[0].symbols.is_empty(),
            "symbols should be cleared when skeleton requested"
        );
        assert!(skel.contains("pub struct Config {"));
        assert!(skel.contains("pub port: u16;"));
        assert!(skel.contains("impl Config {"));
        assert!(skel.contains("pub fn new(port: u16) -> Self { ... }"));
        assert!(skel.contains("/// Configuration for the server."));

        // 2. Python skeleton
        let py_code = r#"
class Worker:
    """Background worker process."""
    def run(self, job_id: int):
        print(f"Working on {job_id}")
        return True
"#;
        let py_res = engine
            .outline(&OutlineRequest {
                path: Some("worker.py".to_string()),
                content: Some(py_code.to_string()),
                options: Some(OutlineOptions {
                    format: Some(OutlineFormat::Skeleton),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(py_res.files.len(), 1);
        let py_skel = py_res.files[0]
            .skeleton
            .as_ref()
            .expect("python skeleton should be present");
        assert!(py_skel.contains("class Worker:"));
        assert!(py_skel.contains("def run(self, job_id: int): ..."));

        // 3. C skeleton
        let c_code = r#"
struct Point {
    int x;
    int y;
};

int add(int a, int b) {
    return a + b;
}
"#;
        let c_res = engine
            .outline(&OutlineRequest {
                path: Some("point.c".to_string()),
                content: Some(c_code.to_string()),
                options: Some(OutlineOptions {
                    format: Some(OutlineFormat::Skeleton),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(c_res.files.len(), 1);
        let c_skel = c_res.files[0]
            .skeleton
            .as_ref()
            .expect("C skeleton should be present");
        assert!(c_skel.contains("struct Point {"));
        assert!(c_skel.contains("int add(int a, int b) { ... }"));
    }

    #[test]
    fn test_patch_by_target_symbol() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let file_path = sandbox.dir.join("calc.rs");
        let initial_code = r#"
pub struct Calculator;

impl Calculator {
    pub fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
}
"#;
        fs::write(&file_path, initial_code).unwrap();

        let patch_req = PatchRequest {
            path: file_path.to_string_lossy().to_string(),
            target_symbol: Some("Calculator::add".to_string()),
            replacement: "    pub fn add(&self, a: i32, b: i32) -> i32 {\n        // Optimized add\n        a.wrapping_add(b)\n    }".to_string(),
            workspace_root: sandbox.boundary(),
            ..Default::default()
        };

        let res = engine.patch(&patch_req).unwrap();
        assert!(res.success);
        assert!(res.ast_valid);
        assert!(res.diff.is_some());
        assert!(res.diff.as_ref().unwrap().contains("+    pub fn add"));

        let updated_code = fs::read_to_string(&file_path).unwrap();
        assert!(updated_code.contains("wrapping_add"));
        assert!(!updated_code.contains("a + b"));
    }

    #[test]
    fn test_patch_ast_preflight_rejection() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let file_path = sandbox.dir.join("broken.rs");
        let initial_code = r#"
pub fn greet() {
    println!("hello");
}
"#;
        fs::write(&file_path, initial_code).unwrap();

        // Deliberately introduce broken syntax with missing closing brace and invalid token
        let patch_req = PatchRequest {
            path: file_path.to_string_lossy().to_string(),
            target_symbol: Some("greet".to_string()),
            replacement: "pub fn greet( { let = ;".to_string(),
            workspace_root: sandbox.boundary(),
            ..Default::default()
        };

        let res = engine.patch(&patch_req).unwrap();
        assert!(!res.success, "Patch should fail AST preflight check");
        assert!(!res.ast_valid, "AST should be marked invalid");
        assert!(!res.syntax_errors.is_empty(), "Should report syntax errors");
        assert!(res.message.contains("AST preflight verification failed"));

        // Crucial guarantee: disk MUST NOT be modified!
        let untouched_code = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            untouched_code, initial_code,
            "Disk contents must remain untouched on AST failure"
        );
    }

    #[test]
    fn test_patch_target_text_and_dry_run() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let file_path = sandbox.dir.join("text.rs");
        let initial_code = r#"
pub fn run() {
    let flag = false;
    let mode = false;
}
"#;
        fs::write(&file_path, initial_code).unwrap();

        // 1. Dry run targeting text
        let dry_req = PatchRequest {
            path: file_path.to_string_lossy().to_string(),
            target_text: Some("false".to_string()),
            target_occurrence: Some(1), // Second "false"
            replacement: "true".to_string(),
            dry_run: Some(true),
            workspace_root: sandbox.boundary(),
            ..Default::default()
        };

        let dry_res = engine.patch(&dry_req).unwrap();
        assert!(dry_res.success);
        assert!(dry_res.ast_valid);
        assert!(
            dry_res
                .diff
                .as_ref()
                .unwrap()
                .contains("-    let mode = false;")
        );
        assert!(
            dry_res
                .diff
                .as_ref()
                .unwrap()
                .contains("+    let mode = true;")
        );

        // Confirm file on disk is unchanged after dry_run
        let unchanged = fs::read_to_string(&file_path).unwrap();
        assert_eq!(unchanged, initial_code);

        // 2. Real apply
        let mut apply_req = dry_req;
        apply_req.dry_run = Some(false);
        let apply_res = engine.patch(&apply_req).unwrap();
        assert!(apply_res.success);

        let modified = fs::read_to_string(&file_path).unwrap();
        assert!(modified.contains("let flag = false;"));
        assert!(modified.contains("let mode = true;"));
    }

    #[test]
    fn test_find_symbol_exact_and_qualified() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // 1. Rust file with struct and impl method
        let rs_path = sandbox.dir.join("src").join("agent.rs");
        let rs_code = r#"
pub struct AgentRunner {
    pub id: u64,
}

impl AgentRunner {
    pub fn dispatch(&self) -> bool {
        true
    }
}
"#;
        fs::write(&rs_path, rs_code).unwrap();

        // 2. C file with function
        let c_path = sandbox.dir.join("src").join("vmcs.c");
        let c_code = r#"
int SetupVmcsForProcessor(void* ctx) {
    return 0;
}
"#;
        fs::write(&c_path, c_code).unwrap();

        // Test finding C function by exact name
        let c_res = engine
            .find_symbol(&FindSymbolRequest {
                name: "SetupVmcsForProcessor".to_string(),
                path: Some(sandbox.dir.to_string_lossy().to_string()),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(c_res.total_found, 1);
        assert_eq!(c_res.symbols[0].name, "SetupVmcsForProcessor");
        assert_eq!(c_res.symbols[0].language, "c");
        assert_eq!(c_res.symbols[0].kind, SymbolKind::Function);

        // Test finding Rust method by qualified name
        let q_res = engine
            .find_symbol(&FindSymbolRequest {
                name: "AgentRunner::dispatch".to_string(),
                path: Some(sandbox.dir.to_string_lossy().to_string()),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(q_res.total_found, 1);
        assert_eq!(q_res.symbols[0].name, "dispatch");
        assert_eq!(q_res.symbols[0].qualified_name, "AgentRunner::dispatch");
        assert_eq!(q_res.symbols[0].language, "rust");
    }

    #[test]
    fn test_find_symbol_kind_filter_and_case_insensitive() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let rs_path = sandbox.dir.join("src").join("models.rs");
        let rs_code = r#"
pub struct Config {
    pub debug: bool,
}

pub fn config() -> Config {
    Config { debug: true }
}
"#;
        fs::write(&rs_path, rs_code).unwrap();

        // Filter by Struct
        let struct_res = engine
            .find_symbol(&FindSymbolRequest {
                name: "Config".to_string(),
                path: Some(sandbox.dir.to_string_lossy().to_string()),
                kind: Some(SymbolKind::Struct),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(struct_res.total_found, 1);
        assert_eq!(struct_res.symbols[0].kind, SymbolKind::Struct);

        // Case-insensitive search
        let case_res = engine
            .find_symbol(&FindSymbolRequest {
                name: "config".to_string(),
                path: Some(sandbox.dir.to_string_lossy().to_string()),
                case_sensitive: Some(false),
                ..Default::default()
            })
            .unwrap();

        // Should find both Config (struct) and config (fn)
        assert!(case_res.total_found >= 2);
    }

    /// Regression: an intermediate `(limit * 10).max(200)` candidate cap filled in traversal
    /// order (files are sorted alphabetically) *before* the exact-first sort ran, so an exact
    /// match discovered after the window was full was discarded. A caller asking for
    /// `exact: false` got a page of near-misses with no sign the real definition existed
    /// beyond `truncated: true`.
    #[test]
    fn test_find_symbol_exact_match_survives_candidate_saturation() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // 250 partial matches, alphabetically FIRST.
        let mut bulk = String::new();
        for i in 0..250 {
            bulk.push_str(&format!("pub fn handler_alpha_{i:03}() -> u32 {{ {i} }}\n"));
        }
        fs::write(sandbox.dir.join("aaa_handlers.rs"), bulk).unwrap();

        // The one exact match, in a file that sorts last.
        fs::write(
            sandbox.dir.join("zzz_exact.rs"),
            "pub fn handler() -> u32 { 0 }\n",
        )
        .unwrap();

        let res = engine
            .find_symbol(&transcend_protocol::FindSymbolRequest {
                name: "handler".to_string(),
                path: Some(sandbox.path_str()),
                exact: Some(false),
                fuzzy: Some(true),
                limit: Some(5),
                ..Default::default()
            })
            .expect("find_symbol should succeed");

        assert!(
            res.total_found > 200,
            "fixture should saturate the old candidate window, got {}",
            res.total_found
        );
        assert_eq!(res.symbols.len(), 5, "limit must still be honoured");
        assert!(
            res.symbols
                .iter()
                .any(|s| s.name == "handler" && s.is_exact),
            "the exact match must outrank partials and survive the cap; got {:?}",
            res.symbols
                .iter()
                .map(|s| (s.name.clone(), s.is_exact))
                .collect::<Vec<_>>()
        );
        assert!(
            res.symbols[0].is_exact,
            "exact matches must be ranked first"
        );
    }

    #[test]
    fn test_find_symbol_partial_and_limit_budgeting() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let py_path = sandbox.dir.join("src").join("handlers.py");
        let py_code = r#"
def handle_alpha():
    pass

def handle_beta():
    pass

def handle_gamma():
    pass
"#;
        fs::write(&py_path, py_code).unwrap();

        // Partial match with exact: false
        let partial_res = engine
            .find_symbol(&FindSymbolRequest {
                name: "handle".to_string(),
                path: Some(sandbox.dir.to_string_lossy().to_string()),
                exact: Some(false),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(partial_res.total_found, 3);

        // Limit budgeting
        let limit_res = engine
            .find_symbol(&FindSymbolRequest {
                name: "handle".to_string(),
                path: Some(sandbox.dir.to_string_lossy().to_string()),
                exact: Some(false),
                limit: Some(2),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(limit_res.symbols.len(), 2);
        assert_eq!(limit_res.total_found, 3);
        assert!(limit_res.truncated);
    }

    #[test]
    fn test_find_symbol_exact_matches_prioritized_across_files() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // 1. a_adapter.rs contains 5 partial matches for "process"
        let a_path = sandbox.dir.join("src").join("a_adapter.rs");
        let a_code = r#"
pub fn process_event_one() {}
pub fn process_event_two() {}
pub fn process_event_three() {}
pub fn process_event_four() {}
pub fn process_event_five() {}
"#;
        fs::write(&a_path, a_code).unwrap();

        // 2. z_worker.rs contains 1 EXACT match for "process"
        let z_path = sandbox.dir.join("src").join("z_worker.rs");
        let z_code = r#"
pub fn process() {}
"#;
        fs::write(&z_path, z_code).unwrap();

        // Query with exact: false, limit: 3
        let res = engine
            .find_symbol(&FindSymbolRequest {
                name: "process".to_string(),
                path: Some(sandbox.dir.to_string_lossy().to_string()),
                exact: Some(false),
                limit: Some(3),
                ..Default::default()
            })
            .unwrap();

        // Total matches across workspace is 6 (5 partial + 1 exact)
        assert_eq!(res.total_found, 6);
        assert_eq!(res.symbols.len(), 3);
        assert!(res.truncated);

        // FIRST symbol MUST be the exact match from z_worker.rs!
        assert_eq!(res.symbols[0].name, "process");
        assert!(res.symbols[0].is_exact);
        assert!(res.symbols[0].file.contains("z_worker.rs"));
    }

    #[tokio::test]
    async fn test_lsp_definition_symbol_and_coordinates() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let rs_path = sandbox.dir.join("src").join("engine.rs");
        let rs_code = r#"
pub struct Executor {
    pub max_threads: usize,
}

impl Executor {
    pub fn dispatch(&self) -> bool {
        true
    }
}
"#;
        fs::write(&rs_path, rs_code).unwrap();

        let res = engine
            .lsp_definition(&LspDefinitionRequest {
                path: rs_path.to_string_lossy().to_string(),
                symbol: Some("Executor::dispatch".to_string()),
                ..Default::default()
            })
            .await
            .expect("lsp_definition should succeed");

        assert!(!res.targets.is_empty());
        assert_eq!(res.targets[0].span.start_line, 7);
        assert!(res.targets[0].file.contains("engine.rs"));
    }

    #[tokio::test]
    async fn test_lsp_references_symbol() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let rs_path = sandbox.dir.join("src").join("service.rs");
        let rs_code = r#"
pub fn run_task() {}

pub fn caller_one() {
    run_task();
}

pub fn caller_two() {
    run_task();
}
"#;
        fs::write(&rs_path, rs_code).unwrap();

        let res = engine
            .lsp_references(&LspReferencesRequest {
                path: rs_path.to_string_lossy().to_string(),
                symbol: Some("run_task".to_string()),
                ..Default::default()
            })
            .await
            .expect("lsp_references should succeed");

        assert!(res.total_found >= 2);
        assert!(
            res.references
                .iter()
                .any(|r| r.line_text.contains("caller_one") || r.line_text.contains("run_task()"))
        );
    }

    #[tokio::test]
    async fn test_lsp_hover_symbol() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let rs_path = sandbox.dir.join("src").join("worker.rs");
        let rs_code = r#"
/// Computes checksum value.
pub fn compute_checksum(val: u32) -> u32 {
    val * 2
}
"#;
        fs::write(&rs_path, rs_code).unwrap();

        let res = engine
            .lsp_hover(&LspHoverRequest {
                path: rs_path.to_string_lossy().to_string(),
                symbol: Some("compute_checksum".to_string()),
                ..Default::default()
            })
            .await
            .expect("lsp_hover should succeed");

        assert!(res.signature.is_some() || res.documentation.is_some());
    }

    #[tokio::test]
    async fn test_lsp_diagnostics_query() {
        let engine = NativeEngine::new();
        let res = engine
            .lsp_diagnostics(&LspDiagnosticsRequest {
                path: None,
                severity: None,
            })
            .await
            .expect("lsp_diagnostics should succeed");

        assert_eq!(res.total_count, res.diagnostics.len());
    }

    /// Regression: `max_bytes` is documented as a byte budget, but the clip took
    /// `remaining_budget` *characters*, so a multi-byte file could return up to 4x the
    /// documented limit -- defeating the purpose of a token-bounded read.
    #[test]
    fn test_read_file_max_bytes_is_a_byte_budget() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Two-byte characters throughout, so chars and bytes diverge by 2x.
        let path = sandbox.dir.join("multibyte.txt");
        fs::write(&path, "é".repeat(80)).unwrap();

        for budget in [10usize, 11, 12, 13, 40, 41, 100] {
            let res = engine
                .read_file(&ReadFileRequest {
                    path: path.to_string_lossy().to_string(),
                    max_bytes: Some(budget),
                    ..Default::default()
                })
                .expect("read should succeed");

            assert!(
                res.content.len() <= budget,
                "budget {budget} returned {} bytes ({} chars)",
                res.content.len(),
                res.content.chars().count()
            );
            // The content must remain valid UTF-8 on a char boundary.
            assert!(
                res.content.is_char_boundary(res.content.len()),
                "budget {budget} cut mid-character"
            );
        }
    }

    /// ASCII content must still fill the budget exactly.
    #[test]
    fn test_read_file_max_bytes_fills_ascii_budget() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let path = sandbox.dir.join("ascii.txt");
        fs::write(&path, "a".repeat(200)).unwrap();

        let res = engine
            .read_file(&ReadFileRequest {
                path: path.to_string_lossy().to_string(),
                max_bytes: Some(50),
                ..Default::default()
            })
            .expect("read should succeed");

        assert_eq!(res.content.len(), 50);
        assert!(res.truncated);
    }

    #[test]
    fn test_char_boundary_helpers_are_safe_and_clamped() {
        let s = "ab日本語"; // 2 + 3*3 = 11 bytes
        assert_eq!(floor_char_boundary(s, 0), 0);
        assert_eq!(floor_char_boundary(s, 2), 2);
        assert_eq!(floor_char_boundary(s, 3), 2, "inside 日 -> back to 2");
        assert_eq!(floor_char_boundary(s, 5), 5);
        assert_eq!(floor_char_boundary(s, 999), s.len(), "past end clamps");

        assert_eq!(ceil_char_boundary(s, 0), 0);
        assert_eq!(ceil_char_boundary(s, 3), 5, "inside 日 -> forward to 5");
        assert_eq!(ceil_char_boundary(s, 999), s.len());

        // Both must always land on a real boundary within range.
        for i in 0..=s.len() + 3 {
            let f = floor_char_boundary(s, i);
            let c = ceil_char_boundary(s, i);
            assert!(f <= s.len() && s.is_char_boundary(f), "floor({i}) = {f}");
            assert!(c <= s.len() && s.is_char_boundary(c), "ceil({i}) = {c}");
            assert!(f <= c, "floor({i}) > ceil({i})");
        }
    }

    #[test]
    fn test_read_file_slicing_and_line_numbers() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let target = sandbox.dir.join("src").join("main.rs");
        let res = engine
            .read_file(&transcend_protocol::ReadFileRequest {
                path: target.to_string_lossy().to_string(),
                start_line: Some(1),
                end_line: Some(2),
                line_numbers: Some(true),
                max_bytes: None,
            })
            .expect("read_file should succeed");

        assert_eq!(res.start_line, 1);
        assert_eq!(res.end_line, 2);
        assert_eq!(res.total_lines, 3);
        assert!(!res.is_binary);
        assert!(res.content.contains("    1 | fn hello_world() {"));
        assert!(res.content.contains("    2 |     println!(\"Hello\");"));
    }

    #[test]
    fn test_read_file_binary_detection() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let bin_path = sandbox.dir.join("image.bin");
        let res = engine
            .read_file(&transcend_protocol::ReadFileRequest {
                path: bin_path.to_string_lossy().to_string(),
                ..Default::default()
            })
            .expect("read_file binary probe should succeed");

        assert!(res.is_binary);
        assert!(res.content.contains("[Binary file omitted"));
    }

    #[test]
    fn test_write_file_and_collision_guard() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let new_path = sandbox.dir.join("deep").join("nested").join("module.rs");
        let path_str = new_path.to_string_lossy().to_string();

        // 0. Boundary guard: a path outside the sandbox must be refused
        let escape_attempt = engine.write_file(&transcend_protocol::WriteFileRequest {
            path: sandbox
                .dir
                .join("..")
                .join("transcend_escape_probe.rs")
                .to_string_lossy()
                .to_string(),
            content: "malicious".to_string(),
            overwrite: Some(true),
            create_parents: Some(true),
            workspace_root: sandbox.boundary(),
        });
        assert!(
            escape_attempt.is_err(),
            "write_file must reject a path escaping the workspace boundary"
        );

        // 1. Create net-new file with parent directories
        let create_res = engine
            .write_file(&transcend_protocol::WriteFileRequest {
                path: path_str.clone(),
                content: "pub fn add(a: i32, b: i32) -> i32 { a + b }\n".to_string(),
                overwrite: Some(false),
                create_parents: Some(true),
                workspace_root: sandbox.boundary(),
            })
            .expect("write_file create should succeed");

        assert!(create_res.success);
        assert!(create_res.created_new);
        assert!(new_path.exists());

        // 2. Collision guard: attempt overwrite without overwrite flag
        let collision_res = engine
            .write_file(&transcend_protocol::WriteFileRequest {
                path: path_str.clone(),
                content: "corrupt".to_string(),
                overwrite: Some(false),
                create_parents: Some(true),
                workspace_root: sandbox.boundary(),
            })
            .expect("write_file should return result without panic");

        assert!(!collision_res.success);
        assert!(!collision_res.created_new);
        assert!(collision_res.message.contains("already exists"));

        // 3. Overwrite with overwrite: true
        let overwrite_res = engine
            .write_file(&transcend_protocol::WriteFileRequest {
                path: path_str.clone(),
                content: "pub fn updated() {}\n".to_string(),
                overwrite: Some(true),
                create_parents: Some(true),
                workspace_root: sandbox.boundary(),
            })
            .expect("write_file overwrite should succeed");

        assert!(overwrite_res.success);
        assert!(!overwrite_res.created_new);
        let read_back = fs::read_to_string(&new_path).unwrap();
        assert_eq!(read_back, "pub fn updated() {}\n");
    }

    #[test]
    fn test_delete_path_file_and_dir_guards() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // 1. Delete single file
        let file_to_del = sandbox.dir.join("budget.txt");
        let del_file_res = engine
            .delete_path(&transcend_protocol::DeletePathRequest {
                path: file_to_del.to_string_lossy().to_string(),
                recursive: Some(false),
                workspace_root: Some(sandbox.path_str()),
            })
            .expect("delete_path on file should succeed");

        assert!(del_file_res.success);
        assert!(!del_file_res.is_directory);
        assert!(!file_to_del.exists());

        // 2. Delete non-empty dir without recursive (should fail safely)
        let dir_to_del = sandbox.dir.join("src");
        let non_rec_res = engine
            .delete_path(&transcend_protocol::DeletePathRequest {
                path: dir_to_del.to_string_lossy().to_string(),
                recursive: Some(false),
                workspace_root: Some(sandbox.path_str()),
            })
            .expect("delete_path on dir should handle non-recursive safely");

        assert!(!non_rec_res.success);
        assert!(non_rec_res.is_directory);
        assert!(non_rec_res.message.contains("Directory is not empty"));
        assert!(dir_to_del.exists());

        // 3. Delete non-empty dir with recursive: true
        let rec_res = engine
            .delete_path(&transcend_protocol::DeletePathRequest {
                path: dir_to_del.to_string_lossy().to_string(),
                recursive: Some(true),
                workspace_root: Some(sandbox.path_str()),
            })
            .expect("recursive delete_path should succeed");

        assert!(rec_res.success);
        assert!(rec_res.is_directory);
        assert!(!dir_to_del.exists());
    }

    #[test]
    fn test_patch_splicing_modes() {
        let engine = NativeEngine::new();
        let source = "struct Point {\n    x: i32,\n}\n";

        // InsertBefore
        let before_res = engine
            .patch(&transcend_protocol::PatchRequest {
                path: "test.rs".to_string(),
                content: Some(source.to_string()),
                target_symbol: Some("Point".to_string()),
                mode: Some(transcend_protocol::PatchMode::InsertBefore),
                replacement: "#[derive(Debug, Clone)]".to_string(),
                validate_ast: Some(true),
                dry_run: Some(true),
                ..Default::default()
            })
            .expect("patch InsertBefore should succeed");

        assert!(before_res.success);
        assert!(before_res.ast_valid);
        assert!(before_res.diff.unwrap().contains("#[derive(Debug, Clone)]"));

        // PrependToSymbol
        let prepend_res = engine
            .patch(&transcend_protocol::PatchRequest {
                path: "test.rs".to_string(),
                content: Some(source.to_string()),
                target_symbol: Some("Point".to_string()),
                mode: Some(transcend_protocol::PatchMode::PrependToSymbol),
                replacement: "    id: u64,".to_string(),
                validate_ast: Some(true),
                dry_run: Some(true),
                ..Default::default()
            })
            .expect("patch PrependToSymbol should succeed");

        assert!(prepend_res.success);
        assert!(prepend_res.ast_valid);
        assert!(prepend_res.diff.unwrap().contains("id: u64,"));
    }

    /// Anchor discovery must climb to the outermost project marker, not the nearest.
    /// In a Cargo workspace every crate has a `Cargo.toml`; a nearest-match rule would
    /// scope every operation to one crate and silently hide the rest of the repo.
    #[test]
    fn test_outermost_anchor_climbs_past_nested_manifests() {
        let sandbox = TestSandbox::create();

        // repo/  (Cargo.toml + .git)
        //   crates/inner/       (Cargo.toml)
        //     src/              <- discovery starts here
        let repo = sandbox.dir.join("repo");
        let inner_src = repo.join("crates").join("inner").join("src");
        fs::create_dir_all(&inner_src).unwrap();
        fs::write(repo.join("Cargo.toml"), "[workspace]\n").unwrap();
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::write(
            repo.join("crates").join("inner").join("Cargo.toml"),
            "[package]\n",
        )
        .unwrap();

        let found = outermost_anchor(&inner_src).expect("an anchor must be found");
        assert_eq!(
            found.canonicalize().unwrap(),
            repo.canonicalize().unwrap(),
            "discovery must climb past the nested crate manifest to the workspace root"
        );

        // A directory with no marker anywhere above the sandbox still terminates.
        let bare = sandbox.dir.join("bare").join("deep");
        fs::create_dir_all(&bare).unwrap();
        // `sandbox.dir` lives under the OS temp dir, which normally has no anchor, but
        // tolerate an anchor being found further up: the contract is "no panic", plus
        // "never returns something below the start path".
        if let Some(anchor) = outermost_anchor(&bare) {
            assert!(
                bare.starts_with(anchor),
                "anchor {anchor:?} must be an ancestor of the start path"
            );
        }
    }

    /// `get_workspace` must respect an explicit root, and `resolve_path` must resolve
    /// relative paths against it.
    #[test]
    fn test_workspace_resolution_prefers_explicit_root() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        engine
            .set_workspace(&SetWorkspaceRequest {
                path: sandbox.path_str(),
            })
            .expect("set_workspace should succeed");

        assert_eq!(
            engine.get_workspace().canonicalize().unwrap(),
            sandbox.dir.canonicalize().unwrap()
        );

        let resolved = engine.resolve_path(Some("src/main.rs"));
        assert!(resolved.starts_with(&sandbox.dir));
        assert!(resolved.ends_with("main.rs"));

        // An absolute path is passed through untouched.
        let absolute = sandbox.dir.join("absolute.txt");
        assert_eq!(
            engine.resolve_path(Some(&absolute.to_string_lossy())),
            absolute
        );

        // `.` and empty both mean "the workspace root".
        assert_eq!(engine.resolve_path(Some(".")), engine.get_workspace());
        assert_eq!(engine.resolve_path(None), engine.get_workspace());
    }

    /// A bounded search must report an exact count; the capped flag exists so an
    /// agent can tell a truthful `total_matches` from a traversal-dependent lower bound.
    #[test]
    fn test_search_count_capped_flag_is_false_for_bounded_searches() {
        let sandbox = TestSandbox::create();
        fs::write(sandbox.dir.join("a.txt"), "needle\nneedle\nneedle\n").unwrap();

        let engine = NativeEngine::new();
        let res = engine
            .search(&SearchRequest {
                pattern: "needle".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    max_matches: Some(2),
                    ..Default::default()
                }),
            })
            .unwrap();

        // Budget truncation affects `truncated`, never the count.
        assert!(res.truncated, "line budget should be reported as truncated");
        assert!(
            !res.count_capped,
            "a small workspace must never report a capped count"
        );
        assert_eq!(res.total_matches, 3, "count must be exact and truthful");
    }

    /// `count_capped` must survive a JSON round trip, since MCP clients parse it.
    #[test]
    fn test_search_response_json_includes_count_capped() {
        let res = SearchResponse {
            total_matches: 1,
            total_files: 1,
            files: Vec::new(),
            directory_radar: Vec::new(),
            truncated: false,
            count_capped: false,
        };
        let json = serde_json::to_value(&res).expect("serialize");
        assert_eq!(json["count_capped"], serde_json::json!(false));

        // Payloads produced before the field existed must still deserialize.
        let legacy = serde_json::json!({
            "total_matches": 1,
            "total_files": 1,
            "files": [],
            "directory_radar": [],
            "truncated": false
        });
        let parsed: SearchResponse =
            serde_json::from_value(legacy).expect("legacy payload must deserialize");
        assert!(!parsed.count_capped);
    }

    /// A budget-capped cluster must stay distinguishable from a file with no matches.
    ///
    /// The cluster's `match_count` stays exact for the directory radar, but its text is
    /// dropped once the global budget is exhausted. `matches_truncated` is what tells a
    /// consumer to re-query instead of concluding the file was empty.
    #[test]
    fn test_search_budget_marks_clusters_whose_matches_were_dropped() {
        let sandbox = TestSandbox::create();
        // Two files: one dense, one sparse, so the budget must ration between them.
        fs::write(sandbox.dir.join("dense.txt"), "needle\n".repeat(28)).unwrap();
        fs::write(sandbox.dir.join("sparse.txt"), "needle\nneedle\n").unwrap();

        let engine = NativeEngine::new();
        let res = engine
            .search(&SearchRequest {
                pattern: "needle".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    max_matches: Some(10),
                    ..Default::default()
                }),
            })
            .unwrap();

        // The count is never capped by the line budget.
        assert_eq!(res.total_matches, 30, "counts must stay exact");
        assert!(res.truncated, "line budget was exceeded");

        let dense = res.files.iter().find(|f| f.file.contains("dense")).unwrap();
        let sparse = res
            .files
            .iter()
            .find(|f| f.file.contains("sparse"))
            .unwrap();

        // Every cluster reports its true density, yet the text is budgeted.
        assert_eq!(dense.match_count, 28);
        assert_eq!(sparse.match_count, 2);
        let retained: usize = res.files.iter().map(|f| f.matches.len()).sum();
        assert!(retained <= 10, "retained {retained} exceeds the budget");

        // Any cluster whose text falls short of its count must say so.
        for cluster in &res.files {
            assert_eq!(
                cluster.matches_truncated,
                cluster.match_count > cluster.matches.len(),
                "cluster {} has match_count={} but {} retained and matches_truncated={}",
                cluster.file,
                cluster.match_count,
                cluster.matches.len(),
                cluster.matches_truncated
            );
        }

        // Dense-first ordering means the densest file keeps its text.
        assert!(!dense.matches.is_empty(), "densest file should retain text");

        // The radar still reflects true density for both files.
        let total_radar: usize = res.directory_radar.iter().map(|d| d.match_count).sum();
        assert_eq!(total_radar, 30, "radar must not be affected by the budget");
    }

    #[test]
    fn test_batch_patch_transactional_rollback() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let file1 = sandbox.dir.join("f1.rs");
        let file2 = sandbox.dir.join("f2.rs");
        fs::write(&file1, "fn first() -> i32 { 1 }\n").unwrap();
        fs::write(&file2, "fn second() -> i32 { 2 }\n").unwrap();

        // 1. Batch patch where second file has invalid AST syntax
        let batch_fail = engine
            .batch_patch(&transcend_protocol::BatchPatchRequest {
                patches: vec![
                    transcend_protocol::PatchRequest {
                        path: file1.to_string_lossy().to_string(),
                        target_symbol: Some("first".to_string()),
                        replacement: "fn first() -> i32 { 100 }".to_string(),
                        validate_ast: Some(true),
                        ..Default::default()
                    },
                    transcend_protocol::PatchRequest {
                        path: file2.to_string_lossy().to_string(),
                        target_symbol: Some("second".to_string()),
                        replacement: "fn second( { invalid syntax @@".to_string(),
                        validate_ast: Some(true),
                        ..Default::default()
                    },
                ],
                validate_ast: Some(true),
                dry_run: Some(false),
                workspace_root: sandbox.boundary(),
            })
            .expect("batch_patch should return response");

        assert!(!batch_fail.success);
        assert!(!batch_fail.all_ast_valid);
        assert!(!batch_fail.syntax_errors.is_empty());
        // Verify file1 was NOT changed on disk due to transaction abort
        assert_eq!(
            fs::read_to_string(&file1).unwrap(),
            "fn first() -> i32 { 1 }\n"
        );

        // 2. Successful batch patch across multiple files
        let batch_ok = engine
            .batch_patch(&transcend_protocol::BatchPatchRequest {
                patches: vec![
                    transcend_protocol::PatchRequest {
                        path: file1.to_string_lossy().to_string(),
                        target_symbol: Some("first".to_string()),
                        replacement: "fn first() -> i32 { 100 }".to_string(),
                        validate_ast: Some(true),
                        ..Default::default()
                    },
                    transcend_protocol::PatchRequest {
                        path: file2.to_string_lossy().to_string(),
                        target_symbol: Some("second".to_string()),
                        replacement: "fn second() -> i32 { 200 }".to_string(),
                        validate_ast: Some(true),
                        ..Default::default()
                    },
                ],
                validate_ast: Some(true),
                dry_run: Some(false),
                workspace_root: sandbox.boundary(),
            })
            .expect("batch_patch should succeed");

        assert!(batch_ok.success);
        assert_eq!(batch_ok.total_files_patched, 2);
        assert_eq!(
            fs::read_to_string(&file1).unwrap(),
            "fn first() -> i32 { 100 }\n"
        );
        assert_eq!(
            fs::read_to_string(&file2).unwrap(),
            "fn second() -> i32 { 200 }\n"
        );
    }

    #[test]
    fn test_batch_patch_cumulative_same_file() {
        let sandbox = TestSandbox::create();
        let target = sandbox.dir.join("target.rs");
        fs::write(
            &target,
            "fn alpha() -> i32 {\n    1\n}\n\nfn beta() -> i32 {\n    2\n}\n",
        )
        .unwrap();

        let engine = NativeEngine::new();
        let res = engine
            .batch_patch(&transcend_protocol::BatchPatchRequest {
                patches: vec![
                    transcend_protocol::PatchRequest {
                        path: target.to_string_lossy().to_string(),
                        target_symbol: Some("alpha".to_string()),
                        replacement: "fn alpha() -> i32 {\n    10\n}".to_string(),
                        validate_ast: Some(true),
                        ..Default::default()
                    },
                    transcend_protocol::PatchRequest {
                        path: target.to_string_lossy().to_string(),
                        target_symbol: Some("beta".to_string()),
                        replacement: "fn beta() -> i32 {\n    20\n}".to_string(),
                        validate_ast: Some(true),
                        ..Default::default()
                    },
                ],
                validate_ast: Some(true),
                dry_run: Some(false),
                workspace_root: sandbox.boundary(),
            })
            .expect("batch_patch should succeed");

        assert!(res.success);
        assert_eq!(res.results.len(), 2);
        assert_eq!(res.total_files_patched, 1);

        let final_code = fs::read_to_string(&target).unwrap();
        assert!(final_code.contains("10"));
        assert!(final_code.contains("20"));
    }

    #[test]
    fn test_set_workspace_and_relative_resolution() {
        let sandbox = TestSandbox::create();
        let subfile = sandbox.dir.join("sub").join("test.txt");
        fs::create_dir_all(subfile.parent().unwrap()).unwrap();
        fs::write(&subfile, "workspace file content\n").unwrap();

        let engine = NativeEngine::new();
        // Set workspace to sandbox
        let set_res = engine
            .set_workspace(&SetWorkspaceRequest {
                path: sandbox.path_str(),
            })
            .expect("set_workspace should succeed");
        assert!(set_res.success);

        // Read using relative path
        let read_res = engine
            .read_file(&ReadFileRequest {
                path: "sub/test.txt".to_string(),
                ..Default::default()
            })
            .expect("read_file should succeed via workspace resolution");

        assert_eq!(read_res.content.trim(), "workspace file content");

        // Error on non-existent directory
        let err_res = engine.set_workspace(&SetWorkspaceRequest {
            path: sandbox
                .dir
                .join("non_existent_folder_xyz")
                .to_string_lossy()
                .to_string(),
        });
        assert!(err_res.is_err());
    }

    #[test]
    fn test_patch_auto_indentation_modes() {
        let sandbox = TestSandbox::create();
        let target = sandbox.dir.join("indent.rs");
        fs::write(
            &target,
            "fn compute() {\n    let a = 1;\n    let b = 2;\n}\n",
        )
        .unwrap();

        let engine = NativeEngine::new();
        let patch_res = engine
            .patch(&PatchRequest {
                path: target.to_string_lossy().to_string(),
                target_symbol: Some("compute".to_string()),
                mode: Some(transcend_protocol::PatchMode::AppendToSymbol),
                replacement: "let c = 3;".to_string(),
                validate_ast: Some(true),
                dry_run: Some(false),
                workspace_root: sandbox.boundary(),
                ..Default::default()
            })
            .expect("append_to_symbol should succeed");

        assert!(patch_res.success);
        let content = fs::read_to_string(&target).unwrap();
        // The appended line should have 4 spaces indentation
        assert!(content.contains("    let c = 3;"));
    }

    #[test]
    fn test_search_max_empty_clusters_pruning() {
        let sandbox = TestSandbox::create();
        for i in 1..=5 {
            fs::write(sandbox.dir.join(format!("file_{i}.txt")), "needle\n").unwrap();
        }

        let engine = NativeEngine::new();
        let res = engine
            .search(&SearchRequest {
                pattern: "needle".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    max_matches: Some(2),        // File 1 and 2 take the 2 matches
                    max_empty_clusters: Some(1), // Retain only 1 empty cluster
                    ..Default::default()
                }),
            })
            .expect("search should succeed");

        assert_eq!(res.total_matches, 5);
        // 2 matched files + 1 empty cluster = 3 files retained
        assert_eq!(res.files.len(), 3);
    }

    #[test]
    fn test_clean_path_normalization() {
        use std::path::PathBuf;
        let win_verbatim = PathBuf::from(r"\\?\C:\projects\transcend\src\main.rs");
        let cleaned = clean_path(&win_verbatim);
        assert_eq!(cleaned, PathBuf::from(r"C:\projects\transcend\src\main.rs"));

        let slash_verbatim = PathBuf::from("//?/C:/projects/transcend/src/main.rs");
        let cleaned_slash = clean_path(&slash_verbatim);
        assert_eq!(
            cleaned_slash,
            PathBuf::from("C:/projects/transcend/src/main.rs")
        );

        let normal_path = PathBuf::from("C:/projects/transcend/src/main.rs");
        assert_eq!(clean_path(&normal_path), normal_path);
    }

    #[test]
    fn test_search_regex_fallback_on_invalid_syntax() {
        let sandbox = TestSandbox::create();
        fs::write(
            sandbox.dir.join("calc.rs"),
            "fn calculate(x: i32) -> i32 { x * 2 }\n",
        )
        .unwrap();

        let engine = NativeEngine::new();
        // "fn calculate(" has unclosed parenthesis, invalid in strict regex
        let res = engine
            .search(&SearchRequest {
                pattern: "fn calculate(".to_string(),
                path: Some(sandbox.path_str()),
                options: None,
            })
            .expect("search with invalid regex should fall back to literal match");

        assert_eq!(res.total_matches, 1);
        assert!(
            res.files[0].matches[0]
                .line_text
                .contains("fn calculate(x: i32)")
        );
    }

    #[test]
    fn test_find_glob_fallback_on_invalid_syntax() {
        let sandbox = TestSandbox::create();
        fs::write(sandbox.dir.join("test[special].rs"), "content\n").unwrap();

        let engine = NativeEngine::new();
        // "test[" has unclosed bracket, invalid in strict glob
        let res = engine
            .find(&transcend_protocol::FindRequest {
                pattern: Some("test[".to_string()),
                path: Some(sandbox.path_str()),
                options: None,
            })
            .expect("find with invalid glob should fall back to substring match");

        assert_eq!(res.total_count, 1);
        assert!(res.entries[0].path.contains("test[special].rs"));
    }

    #[test]
    fn test_serde_parameter_aliases() {
        use serde_json::json;

        // ReadFileRequest: "file" alias for "path"
        let read_req: transcend_protocol::ReadFileRequest =
            serde_json::from_value(json!({ "file": "src/main.rs" })).unwrap();
        assert_eq!(read_req.path, "src/main.rs");

        // ExecRequest: "cmd" alias for "command", "working_directory" for "cwd"
        let exec_req: transcend_protocol::ExecRequest = serde_json::from_value(
            json!({ "cmd": "cargo check", "working_directory": "crates/core" }),
        )
        .unwrap();
        assert_eq!(exec_req.command, "cargo check");
        assert_eq!(exec_req.cwd, Some("crates/core".to_string()));

        // FindSymbolRequest: "symbol" alias for "name", "dir" for "path"
        let find_sym_req: transcend_protocol::FindSymbolRequest =
            serde_json::from_value(json!({ "symbol": "NativeEngine", "dir": "crates" })).unwrap();
        assert_eq!(find_sym_req.name, "NativeEngine");
        assert_eq!(find_sym_req.path, Some("crates".to_string()));

        // SearchRequest: "query" alias for "pattern", "dir" for "path"
        let search_req: transcend_protocol::SearchRequest =
            serde_json::from_value(json!({ "query": "struct Engine", "dir": "crates" })).unwrap();
        assert_eq!(search_req.pattern, "struct Engine");
        assert_eq!(search_req.path, Some("crates".to_string()));
    }

    #[test]
    fn test_batch_patch_consolidated_diff() {
        let sandbox = TestSandbox::create();
        let target = sandbox.dir.join("multi_patch.rs");
        fs::write(
            &target,
            "fn compute() {\n    let a = 1;\n    let b = 2;\n    let sum = a + b;\n}\n",
        )
        .unwrap();

        let engine = NativeEngine::new();
        let res = engine
            .batch_patch(&BatchPatchRequest {
                patches: vec![
                    PatchRequest {
                        path: target.to_string_lossy().to_string(),
                        target_text: Some("let a = 1;".to_string()),
                        replacement: "let a = 10;".to_string(),
                        validate_ast: Some(true),
                        ..Default::default()
                    },
                    PatchRequest {
                        path: target.to_string_lossy().to_string(),
                        target_text: Some("let b = 2;".to_string()),
                        replacement: "let b = 20;".to_string(),
                        validate_ast: Some(true),
                        ..Default::default()
                    },
                ],
                validate_ast: Some(true),
                dry_run: Some(false),
                workspace_root: sandbox.boundary(),
            })
            .expect("batch_patch should succeed");

        assert!(res.success);
        assert_eq!(res.total_files_patched, 1);
        let diff = res.diff.expect("consolidated diff should be present");
        assert_eq!(diff.matches("--- a/").count(), 1);
        assert_eq!(diff.matches("+++ b/").count(), 1);
        assert!(diff.contains("+    let a = 10;"));
        assert!(diff.contains("+    let b = 20;"));
    }

    #[test]
    fn test_read_file_fast_chunked_counting() {
        let sandbox = TestSandbox::create();
        let target = sandbox.dir.join("massive.txt");
        let mut f = fs::File::create(&target).unwrap();
        use std::io::Write;
        for i in 1..=20_000 {
            writeln!(f, "Line number {i} with payload data").unwrap();
        }
        drop(f);

        let engine = NativeEngine::new();
        let res = engine
            .read_file(&transcend_protocol::ReadFileRequest {
                path: target.to_string_lossy().to_string(),
                start_line: Some(1),
                end_line: Some(5),
                line_numbers: Some(true),
                max_bytes: None,
            })
            .expect("read_file should succeed");

        assert_eq!(res.start_line, 1);
        assert_eq!(res.end_line, 5);
        assert_eq!(res.total_lines, 20_000);
        assert!(res.content.contains("    1 | Line number 1"));
        assert!(res.content.contains("    5 | Line number 5"));
        assert!(!res.content.contains("Line number 6"));
    }

    #[test]
    fn test_find_symbol_smart_casing_and_fuzzy_subsequence() {
        let sandbox = TestSandbox::create();
        let src_file = sandbox.dir.join("processor.rs");
        fs::write(
            &src_file,
            "pub fn SetupVmcsForProcessor() -> bool { true }\npub fn SetupVmcs() -> bool { true }\n",
        )
        .unwrap();

        let engine = NativeEngine::new();

        // 1. Exact match with casing normalization: "setup_vmcs" -> "SetupVmcs"
        let res_exact = engine
            .find_symbol(&transcend_protocol::FindSymbolRequest {
                name: "setup_vmcs".to_string(),
                path: Some(sandbox.path_str()),
                exact: Some(true),
                fuzzy: Some(false),
                ..Default::default()
            })
            .expect("find_symbol should succeed");

        assert!(
            res_exact
                .symbols
                .iter()
                .any(|s| s.name == "SetupVmcs" && s.is_exact)
        );

        // 2. Fuzzy subsequence match: "setup_vmcs" -> "SetupVmcsForProcessor"
        let res_fuzzy = engine
            .find_symbol(&transcend_protocol::FindSymbolRequest {
                name: "setup_vmcs".to_string(),
                path: Some(sandbox.path_str()),
                exact: Some(false),
                fuzzy: Some(true),
                ..Default::default()
            })
            .expect("find_symbol should succeed");

        assert!(
            res_fuzzy
                .symbols
                .iter()
                .any(|s| s.name == "SetupVmcsForProcessor")
        );
    }

    #[test]
    fn test_git_status_porcelain_v2_parser() {
        let mock_output = "\
# branch.oid 1234567890abcdef
# branch.head feature/mcp-hardening
# branch.upstream origin/feature/mcp-hardening
# branch.ab +2 -1
1 .M N... 100644 100644 100644 abc def crates/core/src/lib.rs
1 M. N... 100644 100644 100644 abc def crates/protocol/src/lib.rs
2 R. N... 100644 100644 100644 abc def R100 new_name.rs\told_name.rs
u UU N... 100644 100644 100644 abc def conflict.rs
? untracked_file.txt
";

        let parsed = crate::git_ops::GitEngine::parse_porcelain_v2(mock_output).unwrap();
        assert!(parsed.is_git_repo);
        assert_eq!(parsed.branch, "feature/mcp-hardening");
        assert_eq!(
            parsed.upstream.as_deref(),
            Some("origin/feature/mcp-hardening")
        );
        assert_eq!(parsed.ahead, 2);
        assert_eq!(parsed.behind, 1);
        assert_eq!(parsed.unstaged.len(), 1);
        assert_eq!(parsed.unstaged[0].path, "crates/core/src/lib.rs");
        assert_eq!(
            parsed.unstaged[0].status,
            transcend_protocol::GitFileStatus::Modified
        );
        assert_eq!(parsed.staged.len(), 2);
        assert_eq!(parsed.staged[0].path, "crates/protocol/src/lib.rs");
        assert_eq!(parsed.staged[1].path, "new_name.rs");
        assert_eq!(
            parsed.staged[1].original_path.as_deref(),
            Some("old_name.rs")
        );
        assert_eq!(parsed.conflicted, vec!["conflict.rs".to_string()]);
        assert_eq!(parsed.untracked, vec!["untracked_file.txt".to_string()]);
        assert!(!parsed.is_clean);
    }

    #[tokio::test]
    async fn test_native_engine_git_status() {
        let engine = NativeEngine::new();
        let res = engine
            .git_status(&transcend_protocol::GitStatusRequest { path: None })
            .await
            .expect("git_status on active repository should succeed");

        assert!(res.is_git_repo);
        assert!(!res.branch.is_empty());
    }
}
