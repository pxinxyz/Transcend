//! Transcend MCP Server Implementation
//!
//! Exposes Transcend primitives as tools compliant with the Model Context Protocol (MCP).

pub mod schema;

use rmcp::{
    ErrorData as McpError, Json, RoleServer, handler::server::wrapper::Parameters, model::Tool,
    tool, tool_handler, tool_router,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use transcend_core::{CoreError, Engine, NativeEngine};
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

/// Canonical MCP server name advertised during `initialize`.
pub const SERVER_NAME: &str = "transcend";

/// Usage guidance advertised to clients in the `initialize` response.
///
/// This is the only prose an agent receives before choosing a tool, so it states the
/// intended search order and the failure modes of the raw-shell alternative.
pub const SERVER_INSTRUCTIONS: &str = r#"Transcend is a native codebase-intelligence engine. Prefer these typed tools over shell commands (grep/rg/cat/find/sed) — they return token-compact structured data with exact byte spans instead of raw text to re-parse.

Recommended order of operations:
1. `find` — locate relevant paths (directory radar, extension census).
2. `outline` — map a file's symbol hierarchy before reading it (use format "skeleton" for maximum token savings).
3. `read_symbol` — pull exactly one symbol's source instead of the whole file.
4. `search` — regex search returning per-file clusters plus a directory density radar.
5. `find_symbol` — locate definitions by name/kind across the workspace.
6. `lsp_definition` / `lsp_references` / `lsp_hover` / `lsp_diagnostics` — compiler-resolved semantics; each falls back to Tree-sitter heuristics when no language server is installed.
7. `patch` / `batch_patch` — edit by symbol, span, or text with AST preflight validation; `batch_patch` is transactional across files.

Execution and workspace:
- `exec` runs commands with hybrid lifecycle control (pipe vs pty, blocking vs detached).
- Relative paths resolve against the active workspace root; call `set_workspace` to change it.
- `git_status` returns structured porcelain-v2 data instead of scraped text."#;

/// An exported MCP tool definition: name, description, and input JSON Schema.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolDefinition {
    /// Tool name as advertised over MCP.
    pub name: String,
    /// Human/agent-facing description.
    pub description: String,
    /// JSON Schema for the tool's `arguments` object.
    #[serde(rename = "parameters")]
    pub parameters: serde_json::Value,
}

impl ToolDefinition {
    fn from_tool(tool: &Tool) -> Self {
        Self {
            name: tool.name.to_string(),
            description: tool.description.clone().unwrap_or_default().to_string(),
            parameters: schema::normalize_schema(&serde_json::Value::Object(
                (*tool.input_schema).clone(),
            )),
        }
    }
}

/// Error returned when exporting tool schemas to disk.
#[derive(Debug, thiserror::Error)]
pub enum SchemaExportError {
    #[error("failed to serialize tool definition for '{tool}': {source}")]
    Serialize {
        tool: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to create output directory {dir}: {source}")]
    CreateDir {
        dir: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// A single exported schema file.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportedSchemaFile {
    /// Tool name the file describes.
    pub name: String,
    /// Absolute path the schema was written to.
    pub path: String,
}

/// The Transcend MCP Server instance.
#[derive(Clone)]
pub struct TranscendServer {
    engine: Arc<dyn Engine>,
}

impl Default for TranscendServer {
    fn default() -> Self {
        Self::new(Arc::new(NativeEngine::new()))
    }
}

impl TranscendServer {
    /// Create a new server instance with a custom engine.
    pub fn new(engine: Arc<dyn Engine>) -> Self {
        Self { engine }
    }

    /// Enumerate the registered tool definitions as exposed by the live tool router.
    ///
    /// Reading the router (rather than restating each schema by hand) guarantees the
    /// exported schemas cannot drift from what the server actually serves.
    pub fn tool_definitions() -> Vec<ToolDefinition> {
        Self::tool_router()
            .list_all()
            .iter()
            .map(ToolDefinition::from_tool)
            .collect()
    }

    /// Write every registered tool schema to `out_dir` as pretty-printed `<tool>.json`.
    ///
    /// Replaces the previous in-test schema dump so that no test mutates state outside
    /// the build directory.
    pub fn write_schemas(out_dir: &Path) -> Result<Vec<ExportedSchemaFile>, SchemaExportError> {
        std::fs::create_dir_all(out_dir).map_err(|source| SchemaExportError::CreateDir {
            dir: out_dir.display().to_string(),
            source,
        })?;

        let mut written = Vec::new();
        for def in Self::tool_definitions() {
            let json = serde_json::to_string_pretty(&def).map_err(|source| {
                SchemaExportError::Serialize {
                    tool: def.name.clone(),
                    source,
                }
            })?;
            let path: PathBuf = out_dir.join(format!("{}.json", def.name));
            std::fs::write(&path, json).map_err(|source| SchemaExportError::Write {
                path: path.display().to_string(),
                source,
            })?;
            written.push(ExportedSchemaFile {
                name: def.name,
                path: path.display().to_string(),
            });
        }
        written.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(written)
    }

    /// Run a CPU/IO-bound engine operation off the async runtime's worker threads.
    ///
    /// Search, traversal and AST parsing are synchronous and can occupy a worker for
    /// hundreds of milliseconds; running them inline would stall concurrent `exec`,
    /// `terminal_*` and `lsp_*` calls.
    async fn offload<T, F>(f: F) -> Result<T, String>
    where
        F: FnOnce() -> Result<T, CoreError> + Send + 'static,
        T: Send + 'static,
    {
        tokio::task::spawn_blocking(f)
            .await
            .map_err(|e| format!("engine task failed: {e}"))?
            .map_err(|e| e.to_string())
    }
}

#[tool_router]
impl TranscendServer {
    /// High-performance code search returning structured, token-compact results.
    #[tool(
        name = "search",
        description = "High-performance code search returning structured, token-compact results"
    )]
    pub async fn search(
        &self,
        Parameters(req): Parameters<SearchRequest>,
    ) -> Result<Json<SearchResponse>, String> {
        let engine = Arc::clone(&self.engine);
        Self::offload(move || engine.search(&req)).await.map(Json)
    }

    /// Fast filesystem file discovery respecting ignore rules.
    #[tool(
        name = "find",
        description = "Fast filesystem file discovery respecting ignore rules"
    )]
    pub async fn find(
        &self,
        Parameters(req): Parameters<FindRequest>,
    ) -> Result<Json<FindResponse>, String> {
        let engine = Arc::clone(&self.engine);
        Self::offload(move || engine.find(&req)).await.map(Json)
    }

    /// Extract AST code structure and symbol definitions from a source file.
    #[tool(
        name = "outline",
        description = "Extract AST code structure and symbol definitions from a source file"
    )]
    pub async fn outline(
        &self,
        Parameters(req): Parameters<OutlineRequest>,
    ) -> Result<Json<OutlineResponse>, String> {
        let engine = Arc::clone(&self.engine);
        Self::offload(move || engine.outline(&req)).await.map(Json)
    }

    /// Surgically extract a specific symbol by name or qualified locator.
    #[tool(
        name = "read_symbol",
        description = "Surgically extract a specific symbol by name or qualified locator, returning exact source code and span"
    )]
    pub async fn read_symbol(
        &self,
        Parameters(req): Parameters<ReadSymbolRequest>,
    ) -> Result<Json<ReadSymbolResponse>, String> {
        let engine = Arc::clone(&self.engine);
        Self::offload(move || engine.read_symbol(&req))
            .await
            .map(Json)
    }

    /// Surgically modify code with AST preflight validation before touching disk.
    #[tool(
        name = "patch",
        description = "Surgically modify code targeting a symbol, span, or text with AST syntax validation before touching disk"
    )]
    pub async fn patch(
        &self,
        Parameters(req): Parameters<PatchRequest>,
    ) -> Result<Json<PatchResponse>, String> {
        let engine = Arc::clone(&self.engine);
        Self::offload(move || engine.patch(&req)).await.map(Json)
    }

    /// Globally find code symbol definitions across the workspace.
    #[tool(
        name = "find_symbol",
        description = "Globally find code symbol definitions across the workspace by name, qualified path, or kind with exact AST spans"
    )]
    pub async fn find_symbol(
        &self,
        Parameters(req): Parameters<FindSymbolRequest>,
    ) -> Result<Json<FindSymbolResponse>, String> {
        let engine = Arc::clone(&self.engine);
        Self::offload(move || engine.find_symbol(&req))
            .await
            .map(Json)
    }

    /// Go to compiler-resolved definition of a symbol or position across the workspace.
    #[tool(
        name = "lsp_definition",
        description = "Go to compiler-resolved definition of a symbol or position across the workspace"
    )]
    pub async fn lsp_definition(
        &self,
        Parameters(req): Parameters<LspDefinitionRequest>,
    ) -> Result<Json<LspDefinitionResponse>, String> {
        self.engine
            .lsp_definition(&req)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Find all compiler-resolved references and call sites across the workspace.
    #[tool(
        name = "lsp_references",
        description = "Find all compiler-resolved references and call sites across the workspace"
    )]
    pub async fn lsp_references(
        &self,
        Parameters(req): Parameters<LspReferencesRequest>,
    ) -> Result<Json<LspReferencesResponse>, String> {
        self.engine
            .lsp_references(&req)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Inspect inferred type signature and documentation for a symbol or position.
    #[tool(
        name = "lsp_hover",
        description = "Inspect inferred type signature and documentation for a symbol or position"
    )]
    pub async fn lsp_hover(
        &self,
        Parameters(req): Parameters<LspHoverRequest>,
    ) -> Result<Json<LspHoverResponse>, String> {
        self.engine
            .lsp_hover(&req)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Retrieve compiler diagnostics (errors, warnings) for a file or workspace.
    #[tool(
        name = "lsp_diagnostics",
        description = "Retrieve compiler diagnostics (errors, warnings) for a file or workspace"
    )]
    pub async fn lsp_diagnostics(
        &self,
        Parameters(req): Parameters<LspDiagnosticsRequest>,
    ) -> Result<Json<LspDiagnosticsResponse>, String> {
        self.engine
            .lsp_diagnostics(&req)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Audit status and available installation recipes for official language servers.
    #[tool(
        name = "lsp_status",
        description = "Audit status and available installation recipes for official language servers"
    )]
    pub async fn lsp_status(
        &self,
        Parameters(req): Parameters<LspStatusRequest>,
    ) -> Result<Json<LspStatusResponse>, String> {
        self.engine
            .lsp_status(&req)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Automatically install an official language server using host package managers.
    #[tool(
        name = "lsp_install",
        description = "Automatically install an official language server using host package managers"
    )]
    pub async fn lsp_install(
        &self,
        Parameters(req): Parameters<LspInstallRequest>,
    ) -> Result<Json<LspInstallResponse>, String> {
        self.engine
            .lsp_install(&req)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Execute a command in an isolated terminal or process with hybrid lifecycle control.
    #[tool(
        name = "exec",
        description = "Execute a command in an isolated terminal or process with hybrid lifecycle control (blocking vs detached, pipe vs pty)"
    )]
    pub async fn exec(
        &self,
        Parameters(req): Parameters<ExecRequest>,
    ) -> Result<Json<ExecResponse>, String> {
        self.engine
            .exec(&req)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Read output from a running or exited terminal session using incremental cursors.
    #[tool(
        name = "terminal_read",
        description = "Read output from a running or exited terminal session using incremental cursors"
    )]
    pub async fn terminal_read(
        &self,
        Parameters(req): Parameters<TerminalReadRequest>,
    ) -> Result<Json<TerminalReadResponse>, String> {
        self.engine
            .terminal_read(&req)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Send input, keystrokes, or signals to a running terminal session.
    #[tool(
        name = "terminal_write",
        description = "Send input, keystrokes, or signals to a running terminal session"
    )]
    pub async fn terminal_write(
        &self,
        Parameters(req): Parameters<TerminalWriteRequest>,
    ) -> Result<Json<TerminalWriteResponse>, String> {
        self.engine
            .terminal_write(&req)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Resize a PTY terminal session's columns and rows.
    #[tool(
        name = "terminal_resize",
        description = "Resize a PTY terminal session's columns and rows"
    )]
    pub async fn terminal_resize(
        &self,
        Parameters(req): Parameters<TerminalResizeRequest>,
    ) -> Result<Json<TerminalResizeResponse>, String> {
        self.engine
            .terminal_resize(&req)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Forcibly terminate a running terminal session and its entire process tree.
    #[tool(
        name = "terminal_kill",
        description = "Forcibly terminate a running terminal session and its entire process tree"
    )]
    pub async fn terminal_kill(
        &self,
        Parameters(req): Parameters<TerminalKillRequest>,
    ) -> Result<Json<TerminalKillResponse>, String> {
        self.engine
            .terminal_kill(&req)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Read file content with line/byte boundaries, line numbers, and binary safety checks.
    #[tool(
        name = "read_file",
        description = "Read file content with line/byte boundaries, line numbers, and binary safety checks"
    )]
    pub async fn read_file(
        &self,
        Parameters(req): Parameters<ReadFileRequest>,
    ) -> Result<Json<ReadFileResponse>, String> {
        let engine = Arc::clone(&self.engine);
        Self::offload(move || engine.read_file(&req))
            .await
            .map(Json)
    }

    /// Write text content to file atomically with collision guards and parent directory creation.
    #[tool(
        name = "write_file",
        description = "Write text content to file atomically with collision guards and parent directory creation"
    )]
    pub async fn write_file(
        &self,
        Parameters(req): Parameters<WriteFileRequest>,
    ) -> Result<Json<WriteFileResponse>, String> {
        let engine = Arc::clone(&self.engine);
        Self::offload(move || engine.write_file(&req))
            .await
            .map(Json)
    }

    /// Delete a file or directory safely within workspace boundaries.
    #[tool(
        name = "delete_path",
        description = "Delete a file or directory safely within workspace boundaries"
    )]
    pub async fn delete_path(
        &self,
        Parameters(req): Parameters<DeletePathRequest>,
    ) -> Result<Json<DeletePathResponse>, String> {
        let engine = Arc::clone(&self.engine);
        Self::offload(move || engine.delete_path(&req))
            .await
            .map(Json)
    }

    /// Transactionally apply multiple patches across files with AST preflight and rollback guarantees.
    #[tool(
        name = "batch_patch",
        description = "Transactionally apply multiple patches across files with AST preflight and rollback guarantees"
    )]
    pub async fn batch_patch(
        &self,
        Parameters(req): Parameters<BatchPatchRequest>,
    ) -> Result<Json<BatchPatchResponse>, String> {
        let engine = Arc::clone(&self.engine);
        Self::offload(move || engine.batch_patch(&req))
            .await
            .map(Json)
    }

    /// Configure or update the active project workspace root directory for all path-based operations.
    #[tool(
        name = "set_workspace",
        description = "Configure or update the active project workspace root directory for all path-based operations"
    )]
    pub async fn set_workspace(
        &self,
        Parameters(req): Parameters<SetWorkspaceRequest>,
    ) -> Result<Json<SetWorkspaceResponse>, String> {
        let engine = Arc::clone(&self.engine);
        Self::offload(move || engine.set_workspace(&req))
            .await
            .map(Json)
    }

    /// Inspect structured git status for a repository directory without terminal text scraping.
    #[tool(
        name = "git_status",
        description = "Inspect structured git status for a repository directory without terminal text scraping"
    )]
    pub async fn git_status(
        &self,
        Parameters(req): Parameters<GitStatusRequest>,
    ) -> Result<Json<GitStatusResponse>, String> {
        self.engine
            .git_status(&req)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }
}

/// MCP handler wiring: dispatches `tools/list` and `tools/call` through the generated
/// router, and advertises the server identity plus usage `instructions` on `initialize`.
///
/// The `name`/`instructions` literals are asserted equal to [`SERVER_NAME`] and
/// [`SERVER_INSTRUCTIONS`] by `test_server_info_advertises_identity_and_instructions`;
/// the attribute requires literal strings, so the constants cannot be inlined here.
#[tool_handler(
    router = Self::tool_router(),
    name = "transcend",
    instructions = r#"Transcend is a native codebase-intelligence engine. Prefer these typed tools over shell commands (grep/rg/cat/find/sed) — they return token-compact structured data with exact byte spans instead of raw text to re-parse.

Recommended order of operations:
1. `find` — locate relevant paths (directory radar, extension census).
2. `outline` — map a file's symbol hierarchy before reading it (use format "skeleton" for maximum token savings).
3. `read_symbol` — pull exactly one symbol's source instead of the whole file.
4. `search` — regex search returning per-file clusters plus a directory density radar.
5. `find_symbol` — locate definitions by name/kind across the workspace.
6. `lsp_definition` / `lsp_references` / `lsp_hover` / `lsp_diagnostics` — compiler-resolved semantics; each falls back to Tree-sitter heuristics when no language server is installed.
7. `patch` / `batch_patch` — edit by symbol, span, or text with AST preflight validation; `batch_patch` is transactional across files.

Execution and workspace:
- `exec` runs commands with hybrid lifecycle control (pipe vs pty, blocking vs detached).
- Relative paths resolve against the active workspace root; call `set_workspace` to change it.
- `git_status` returns structured porcelain-v2 data instead of scraped text."#
)]
impl rmcp::ServerHandler for TranscendServer {
    /// Advertise the tool list with input schemas rewritten into the portable subset.
    ///
    /// This override is load-bearing, not cosmetic. `rmcp`'s generated handler would
    /// serve the raw `schemars` output (`$defs`, `$ref`, `$schema`, `type` arrays,
    /// `anyOf`, `format`), which hosts enforcing a restricted JSON Schema subset
    /// reject outright — DeepSeek Harness refuses the whole tool and registers **zero**
    /// tools for this server. See [`schema::normalize_schema`].
    ///
    /// `#[tool_handler]` skips generating `list_tools` when the annotated impl already
    /// defines it, so this replaces the default rather than colliding with it.
    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, McpError> {
        let mut tools = TranscendServer::tool_router().list_all();
        for tool in &mut tools {
            let normalized =
                schema::normalize_schema(&serde_json::Value::Object((*tool.input_schema).clone()));
            if let serde_json::Value::Object(map) = normalized {
                tool.input_schema = Arc::new(map);
            }
        }
        Ok(rmcp::model::ListToolsResult {
            tools,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use transcend_core::NativeEngine;
    use transcend_protocol::SearchOptions;

    /// The crate directory (`crates/transcend-server`), where this crate's relative
    /// fixture paths such as `src/lib.rs` actually live.
    fn crate_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    /// Workspace root of the enclosing repository (`Cargo.toml` + `.git`).
    fn repo_root() -> std::path::PathBuf {
        crate_dir()
            .parent()
            .and_then(|p| p.parent())
            .expect("crates/<name> must have a workspace parent")
            .to_path_buf()
    }

    /// Build a server whose workspace root is pinned rather than inferred from the
    /// process cwd. Anchor discovery intentionally climbs to the outermost project
    /// root, so tests that use crate-relative fixtures must state their root.
    fn server_pinned_to(workspace: &std::path::Path) -> TranscendServer {
        let engine = NativeEngine::new();
        engine
            .set_workspace(&SetWorkspaceRequest {
                path: workspace.to_string_lossy().to_string(),
            })
            .expect("pinning the workspace root should succeed");
        TranscendServer::new(Arc::new(engine))
    }

    /// Server scoped to this crate's own directory.
    fn pinned_server() -> TranscendServer {
        server_pinned_to(&crate_dir())
    }

    #[tokio::test]
    async fn test_server_search_tool_execution() {
        let server = pinned_server();
        let res = server
            .search(Parameters(SearchRequest {
                pattern: "TranscendServer".to_string(),
                path: Some(".".to_string()),
                options: Some(SearchOptions {
                    file_pattern: Some("*.rs".to_string()),
                    case_sensitive: Some(true),
                    max_matches: Some(10),
                    ..Default::default()
                }),
            }))
            .await
            .expect("tool call should succeed");

        assert!(res.0.total_matches > 0);
        assert!(!res.0.files.is_empty());
        assert!(!res.0.files[0].matches.is_empty());
        assert!(
            res.0.files[0].matches[0]
                .line_text
                .contains("TranscendServer")
        );
    }

    #[tokio::test]
    async fn test_server_find_tool_execution() {
        let server = pinned_server();
        let res = server
            .find(Parameters(FindRequest {
                pattern: Some("lib.rs".to_string()),
                path: Some(".".to_string()),
                options: None,
            }))
            .await
            .expect("find tool call should succeed");

        assert!(res.0.total_count > 0);
        assert!(!res.0.entries.is_empty());
        assert!(res.0.entries.iter().any(|e| e.path.ends_with("lib.rs")));
        assert!(res.0.entries[0].size_bytes > 0);
    }

    #[tokio::test]
    async fn test_server_outline_tool_execution() {
        let server = pinned_server();
        let res = server
            .outline(Parameters(OutlineRequest {
                path: Some("src/lib.rs".to_string()),
                content: None,
                options: None,
            }))
            .await
            .expect("outline tool call should succeed");

        assert_eq!(res.0.files.len(), 1);
        let file = &res.0.files[0];
        assert_eq!(file.language, "rust");

        // Should extract TranscendServer struct
        assert!(file.symbols.iter().any(|s| s.name == "TranscendServer"));

        // Should extract impl TranscendServer with search, find, outline methods
        // First impl block has new()
        assert!(file.symbols.iter().any(
            |s| s.name == "impl TranscendServer" && s.children.iter().any(|c| c.name == "new")
        ));

        // Second impl block (tool_router) has search(), find(), outline()
        let tool_impl = file
            .symbols
            .iter()
            .find(|s| {
                s.name == "impl TranscendServer" && s.children.iter().any(|c| c.name == "search")
            })
            .expect("tool router impl TranscendServer should be outlined");

        assert!(tool_impl.children.iter().any(|c| c.name == "search"));
        assert!(tool_impl.children.iter().any(|c| c.name == "find"));
        assert!(tool_impl.children.iter().any(|c| c.name == "outline"));
        assert!(tool_impl.children.iter().any(|c| c.name == "read_symbol"));
        assert!(tool_impl.children.iter().any(|c| c.name == "patch"));
    }

    #[tokio::test]
    async fn test_server_read_symbol_tool_execution() {
        let server = pinned_server();
        let res = server
            .read_symbol(Parameters(ReadSymbolRequest {
                path: Some("src/lib.rs".to_string()),
                symbol: "TranscendServer::new".to_string(),
                ..Default::default()
            }))
            .await
            .expect("read_symbol tool call should succeed");

        assert!(res.0.found);
        assert_eq!(res.0.symbol.unwrap().name, "new");
        assert!(res.0.source_code.unwrap().contains("pub fn new"));
    }

    #[tokio::test]
    async fn test_server_patch_tool_execution() {
        let server = pinned_server();
        let snippet = r#"
pub fn compute() -> i32 {
    10
}
"#;
        let res = server
            .patch(Parameters(PatchRequest {
                path: "compute.rs".to_string(),
                content: Some(snippet.to_string()),
                target_symbol: Some("compute".to_string()),
                replacement: "pub fn compute() -> i32 {\n    42\n}".to_string(),
                dry_run: Some(true),
                ..Default::default()
            }))
            .await
            .expect("patch tool call should succeed");

        assert!(res.0.success);
        assert!(res.0.ast_valid);
        assert!(res.0.diff.unwrap().contains("+    42"));
    }

    #[tokio::test]
    async fn test_server_find_symbol_tool_execution() {
        let server = pinned_server();
        let res = server
            .find_symbol(Parameters(FindSymbolRequest {
                name: "TranscendServer".to_string(),
                path: Some("src".to_string()),
                ..Default::default()
            }))
            .await
            .expect("find_symbol tool call should succeed");

        assert!(res.0.total_found > 0);
        assert!(res.0.symbols.iter().any(|s| s.name == "TranscendServer"));
    }

    #[tokio::test]
    async fn test_server_lsp_definition_tool() {
        let server = pinned_server();
        let res = server
            .lsp_definition(Parameters(LspDefinitionRequest {
                path: "src/lib.rs".to_string(),
                symbol: Some("TranscendServer".to_string()),
                ..Default::default()
            }))
            .await
            .expect("lsp_definition tool call should succeed");

        assert!(!res.0.targets.is_empty());
    }

    #[tokio::test]
    async fn test_server_lsp_hover_tool() {
        let server = pinned_server();
        let res = server
            .lsp_hover(Parameters(LspHoverRequest {
                path: "src/lib.rs".to_string(),
                symbol: Some("TranscendServer".to_string()),
                ..Default::default()
            }))
            .await
            .expect("lsp_hover tool call should succeed");

        assert!(res.0.signature.is_some() || res.0.documentation.is_some());
    }

    #[tokio::test]
    async fn test_server_exec_and_terminal_lifecycle() {
        let server = pinned_server();
        let exec_res = server
            .exec(Parameters(ExecRequest {
                command: "echo test_exec_echo".to_string(),
                ..Default::default()
            }))
            .await
            .expect("exec should succeed");

        assert_eq!(exec_res.0.status, transcend_protocol::ExecStatus::Exited);
        assert_eq!(exec_res.0.exit_code, Some(0));
        assert!(exec_res.0.output.contains("test_exec_echo"));
    }

    /// Every registered tool must expose a name, a non-empty description, and an
    /// object-rooted input schema. Guards against a `#[tool]` attribute losing its
    /// description or a request type degrading into a non-object schema.
    #[test]
    fn test_tool_definitions_are_complete() {
        let defs = TranscendServer::tool_definitions();
        assert!(
            defs.len() >= 23,
            "expected at least 23 registered tools, found {}",
            defs.len()
        );

        for def in &defs {
            assert!(!def.name.is_empty(), "tool with empty name");
            assert!(
                !def.description.is_empty(),
                "tool '{}' is missing a description",
                def.name
            );
            let schema = def
                .parameters
                .as_object()
                .unwrap_or_else(|| panic!("tool '{}' schema is not a JSON object", def.name));
            assert!(
                schema.contains_key("properties") || schema.contains_key("$ref"),
                "tool '{}' schema has neither properties nor $ref: {}",
                def.name,
                def.parameters
            );
            assert!(
                schema.contains_key("title")
                    || schema.contains_key("type")
                    || schema.contains_key("$ref"),
                "tool '{}' schema is missing a title/type marker",
                def.name
            );

            // Portable-subset invariants. A host enforcing a restricted JSON Schema
            // subset rejects the *whole tool* on any of these, and the model then
            // never sees it. See `crate::schema`.
            assert_eq!(
                schema.get("type").and_then(|t| t.as_str()),
                Some("object"),
                "tool '{}' must be object-rooted",
                def.name
            );
            for banned in ["$defs", "definitions", "$schema", "$ref", "anyOf", "allOf"] {
                assert!(
                    !schema.contains_key(banned),
                    "tool '{}' advertises unsupported keyword '{banned}'",
                    def.name
                );
            }

            // Every name in `required` must be declared in `properties`; a strict
            // host rejects the schema otherwise. This is what caught `SearchRequest`'s
            // `pattern` field being dropped as if it were the `pattern` keyword.
            let required = schema
                .get("required")
                .and_then(|r| r.as_array())
                .cloned()
                .unwrap_or_default();
            let properties = schema.get("properties").and_then(|p| p.as_object());
            for name in &required {
                let Some(name) = name.as_str() else { continue };
                assert!(
                    properties.is_some_and(|p| p.contains_key(name)),
                    "tool '{}' requires '{name}' but does not declare it in properties",
                    def.name
                );
            }

            assert_no_unsupported_keywords(&def.parameters, &def.name, "$");
        }

        // Names must be unique and sorted deterministically for stable diffing.
        let mut names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate tool names registered");
    }

    /// Walk a schema and reject keywords outside the portable subset, at any depth.
    ///
    /// Recursion is what makes this meaningful: a nested request struct (e.g.
    /// `SearchOptions` inside `SearchRequest`) is the common place these leak back in.
    fn assert_no_unsupported_keywords(node: &serde_json::Value, tool: &str, path: &str) {
        match node {
            serde_json::Value::Object(map) => {
                let in_name_map = path.ends_with("/properties") || path.ends_with("/$defs");
                for (key, value) in map {
                    // Inside a properties map, keys are field names, not keywords.
                    if !in_name_map {
                        for banned in ["$defs", "definitions", "$schema", "$ref", "anyOf", "allOf"]
                        {
                            assert!(
                                key != banned,
                                "tool '{tool}' advertises '{banned}' at {path}"
                            );
                        }
                        assert!(
                            key != "type" || value.is_string(),
                            "tool '{tool}' uses a type array at {path}: {value}"
                        );
                    }
                    assert_no_unsupported_keywords(value, tool, &format!("{path}/{key}"));
                }
            }
            serde_json::Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    assert_no_unsupported_keywords(item, tool, &format!("{path}/{index}"));
                }
            }
            _ => {}
        }
    }

    /// The exported schema for a tool must match what the router serves, and must
    /// deserialize back into the protocol request type.
    #[test]
    fn test_exported_schemas_round_trip() {
        let defs = TranscendServer::tool_definitions();
        let search = defs
            .iter()
            .find(|d| d.name == "search")
            .expect("search tool must be registered");

        // The schema describes the arguments object the client sends.
        let props = search.parameters["properties"]
            .as_object()
            .expect("search schema must declare properties");
        assert!(props.contains_key("pattern"), "pattern must be required");
        let args = serde_json::json!({ "pattern": "fn main", "path": "src" });
        let parsed: SearchRequest = serde_json::from_value(args)
            .expect("exported search schema must accept its own example");
        assert_eq!(parsed.pattern, "fn main");

        // Writing to a temp dir keeps the export hermetic: no state outside the build tree.
        let out =
            std::env::temp_dir().join(format!("transcend_schema_export_{}", std::process::id()));
        let written = TranscendServer::write_schemas(&out).expect("schema export should succeed");
        assert_eq!(written.len(), defs.len());

        for entry in &written {
            let raw =
                std::fs::read_to_string(&entry.path).expect("exported schema file must exist");
            let value: serde_json::Value =
                serde_json::from_str(&raw).expect("exported schema must be valid JSON");
            assert_eq!(value["name"], serde_json::json!(entry.name));
            assert!(
                value["description"].as_str().is_some_and(|d| !d.is_empty()),
                "exported schema '{}' lost its description",
                entry.name
            );
        }

        let _ = std::fs::remove_dir_all(&out);
    }

    /// `initialize` must advertise the server identity and the usage instructions,
    /// and must declare the tools capability.
    #[test]
    fn test_server_info_advertises_identity_and_instructions() {
        use rmcp::ServerHandler;

        let info = TranscendServer::default().get_info();
        assert_eq!(info.server_info.name, SERVER_NAME);
        assert!(!info.server_info.version.is_empty());
        assert!(
            info.capabilities.tools.is_some(),
            "tools capability must be declared"
        );
        let instructions = info
            .instructions
            .as_deref()
            .expect("instructions must be advertised to clients");
        assert!(instructions.contains("read_symbol"));
        assert!(instructions.contains("batch_patch"));
    }

    /// Exercises the real MCP JSON-RPC surface over an in-memory duplex transport:
    /// `initialize` -> `tools/list` -> `tools/call`, including schema negotiation.
    #[tokio::test]
    async fn test_mcp_stdio_protocol_round_trip() {
        use rmcp::{model::CallToolRequestParams, service::ServiceExt};

        // `()` implements `ClientHandler`, so no bespoke client type is needed.
        let (server_io, client_io) = tokio::io::duplex(64 * 1024);

        let server_task = tokio::spawn(async move {
            let running = TranscendServer::default()
                .serve(server_io)
                .await
                .expect("server should serve over duplex");
            let _ = running.waiting().await;
        });

        let client =
            ().serve(client_io)
                .await
                .expect("client should complete the initialize handshake");

        // 1. tools/list must reflect the live router.
        let listed = client
            .list_tools(None)
            .await
            .expect("tools/list should succeed");
        let names: Vec<String> = listed.tools.iter().map(|t| t.name.to_string()).collect();
        assert!(names.contains(&"search".to_string()), "got {names:?}");
        assert!(names.contains(&"batch_patch".to_string()), "got {names:?}");
        assert!(
            listed.tools.iter().all(|t| t.description.is_some()),
            "every advertised tool must carry a description"
        );

        // 2. tools/call must round-trip a real request over the wire.
        let called = client
            .call_tool(
                CallToolRequestParams::new("read_file").with_arguments(
                    serde_json::json!({ "path": "Cargo.toml", "start_line": 1, "end_line": 3 })
                        .as_object()
                        .cloned()
                        .expect("object"),
                ),
            )
            .await
            .expect("tools/call should succeed");
        let called_text = format!("{called:?}");
        assert!(
            called_text.contains("[package]") || called_text.contains("Cargo.toml"),
            "read_file should return Cargo.toml content: {called_text}"
        );
        assert_ne!(called.is_error, Some(true), "call should not be an error");

        // 3. Missing required arguments must surface as a tool error, not a hang or crash.
        let bad = client
            .call_tool(CallToolRequestParams::new("read_file"))
            .await
            .expect("missing field should be reported as a tool result");
        assert_eq!(
            bad.is_error,
            Some(true),
            "expected an error result: {bad:?}"
        );
        let bad_text = format!("{bad:?}");
        assert!(
            bad_text.contains("failed to deserialize parameters"),
            "expected a deserialization diagnostic, got: {bad_text}"
        );

        // 4. An unknown tool must be rejected rather than panic.
        let unknown = client
            .call_tool(CallToolRequestParams::new("definitely_not_a_tool"))
            .await;
        assert!(unknown.is_err(), "unknown tool must produce an error");

        client.cancel().await.expect("client should cancel cleanly");
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_server_file_lifecycle_tools() {
        // Scratch files live in the workspace-level `target/` directory.
        let root = repo_root();
        let server = server_pinned_to(&root);
        let tmp_file = format!("target/test_server_file_{}.txt", std::process::id());
        let workspace = root.to_string_lossy().to_string();

        // 0. A path escaping the workspace must be refused by the boundary guard.
        let escape = server
            .write_file(Parameters(WriteFileRequest {
                path: "../transcend_escape_probe.txt".to_string(),
                content: "nope".to_string(),
                overwrite: Some(true),
                create_parents: Some(true),
                workspace_root: Some(workspace.clone()),
            }))
            .await;
        assert!(
            escape.is_err(),
            "write_file must reject paths outside the workspace boundary"
        );

        // 1. write_file
        let write_res = server
            .write_file(Parameters(WriteFileRequest {
                path: tmp_file.clone(),
                content: "line 1\nline 2\nline 3\n".to_string(),
                overwrite: Some(true),
                create_parents: Some(true),
                workspace_root: Some(workspace.clone()),
            }))
            .await
            .expect("write_file should succeed");
        assert!(write_res.0.success);

        // 2. read_file
        let read_res = server
            .read_file(Parameters(ReadFileRequest {
                path: tmp_file.clone(),
                start_line: Some(1),
                end_line: Some(2),
                line_numbers: Some(true),
                max_bytes: None,
            }))
            .await
            .expect("read_file should succeed");
        assert_eq!(read_res.0.start_line, 1);
        assert_eq!(read_res.0.end_line, 2);
        assert!(read_res.0.content.contains("line 1"));

        // 3. delete_path
        let del_res = server
            .delete_path(Parameters(DeletePathRequest {
                path: tmp_file.clone(),
                recursive: Some(false),
                workspace_root: None,
            }))
            .await
            .expect("delete_path should succeed");
        assert!(del_res.0.success);
    }

    #[tokio::test]
    async fn test_server_batch_patch_tool() {
        let server = pinned_server();
        let res = server
            .batch_patch(Parameters(BatchPatchRequest {
                patches: vec![PatchRequest {
                    path: "dummy.rs".to_string(),
                    content: Some("fn main() {}\n".to_string()),
                    target_symbol: Some("main".to_string()),
                    replacement: "fn main() { println!(\"patched\"); }".to_string(),
                    validate_ast: Some(true),
                    dry_run: Some(true),
                    ..Default::default()
                }],
                validate_ast: Some(true),
                dry_run: Some(true),
                workspace_root: Some(
                    std::env::current_dir()
                        .expect("cwd")
                        .to_string_lossy()
                        .to_string(),
                ),
            }))
            .await
            .expect("batch_patch should succeed");

        assert!(res.0.success);
        assert_eq!(res.0.total_files_patched, 1);
    }

    #[tokio::test]
    async fn test_server_set_workspace_tool() {
        let server = pinned_server();
        let cur = std::env::current_dir().unwrap();
        let res = server
            .set_workspace(Parameters(SetWorkspaceRequest {
                path: cur.to_string_lossy().to_string(),
            }))
            .await
            .expect("set_workspace should succeed");

        assert!(res.0.success);
        assert!(!res.0.workspace_root.is_empty());
    }

    #[tokio::test]
    async fn test_server_git_status_tool() {
        let server = pinned_server();
        let cur = std::env::current_dir().unwrap();
        let res = server
            .git_status(Parameters(GitStatusRequest {
                path: Some(cur.to_string_lossy().to_string()),
            }))
            .await
            .expect("git_status should succeed");

        assert!(res.0.is_git_repo);
        assert!(!res.0.branch.is_empty());
    }

    #[tokio::test]
    async fn test_server_lsp_status_tool() {
        let server = pinned_server();
        let res = server
            .lsp_status(Parameters(LspStatusRequest {
                language: Some("rust".to_string()),
            }))
            .await
            .expect("lsp_status should succeed");

        assert_eq!(res.0.total_servers, 1);
        assert_eq!(res.0.servers[0].language, "rust");
        assert!(!res.0.servers[0].install_methods.is_empty());
    }
}
