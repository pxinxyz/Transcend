//! Transcend MCP Server Implementation
//!
//! Exposes Transcend primitives as tools compliant with the Model Context Protocol (MCP).

use std::sync::Arc;
use rmcp::{handler::server::wrapper::Parameters, tool, tool_router, Json};
use transcend_core::{Engine, NativeEngine};
use transcend_protocol::{
    BatchPatchRequest, BatchPatchResponse, DeletePathRequest, DeletePathResponse, ExecRequest,
    ExecResponse, FindRequest, FindResponse, FindSymbolRequest, FindSymbolResponse, GitStatusRequest, GitStatusResponse,
    LspDefinitionRequest, LspDefinitionResponse, LspDiagnosticsRequest, LspDiagnosticsResponse,
    LspHoverRequest, LspHoverResponse, LspReferencesRequest, LspReferencesResponse, OutlineRequest,
    OutlineResponse, PatchRequest, PatchResponse, ReadFileRequest, ReadFileResponse,
    ReadSymbolRequest, ReadSymbolResponse, SearchRequest, SearchResponse, SetWorkspaceRequest,
    SetWorkspaceResponse, TerminalKillRequest, TerminalKillResponse, TerminalReadRequest,
    TerminalReadResponse, TerminalResizeRequest, TerminalResizeResponse, TerminalWriteRequest,
    TerminalWriteResponse, WriteFileRequest, WriteFileResponse,
};

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
}

#[tool_router(server_handler)]
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
        self.engine.search(&req).map(Json).map_err(|e| e.to_string())
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
        self.engine.find(&req).map(Json).map_err(|e| e.to_string())
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
        self.engine.outline(&req).map(Json).map_err(|e| e.to_string())
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
        self.engine.read_symbol(&req).map(Json).map_err(|e| e.to_string())
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
        self.engine.patch(&req).map(Json).map_err(|e| e.to_string())
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
        self.engine.find_symbol(&req).map(Json).map_err(|e| e.to_string())
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
        self.engine.lsp_definition(&req).await.map(Json).map_err(|e| e.to_string())
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
        self.engine.lsp_references(&req).await.map(Json).map_err(|e| e.to_string())
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
        self.engine.lsp_hover(&req).await.map(Json).map_err(|e| e.to_string())
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
        self.engine.lsp_diagnostics(&req).await.map(Json).map_err(|e| e.to_string())
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
        self.engine.exec(&req).await.map(Json).map_err(|e| e.to_string())
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
        self.engine.terminal_read(&req).await.map(Json).map_err(|e| e.to_string())
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
        self.engine.terminal_write(&req).await.map(Json).map_err(|e| e.to_string())
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
        self.engine.terminal_resize(&req).await.map(Json).map_err(|e| e.to_string())
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
        self.engine.terminal_kill(&req).await.map(Json).map_err(|e| e.to_string())
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
        self.engine.read_file(&req).map(Json).map_err(|e| e.to_string())
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
        self.engine.write_file(&req).map(Json).map_err(|e| e.to_string())
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
        self.engine.delete_path(&req).map(Json).map_err(|e| e.to_string())
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
        self.engine.batch_patch(&req).map(Json).map_err(|e| e.to_string())
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
        self.engine.set_workspace(&req).map(Json).map_err(|e| e.to_string())
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
        self.engine.git_status(&req).await.map(Json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use transcend_protocol::SearchOptions;

    #[tokio::test]
    async fn test_server_search_tool_execution() {
        let server = TranscendServer::default();
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
        assert!(res.0.files[0].matches[0].line_text.contains("TranscendServer"));
    }

    #[tokio::test]
    async fn test_server_find_tool_execution() {
        let server = TranscendServer::default();
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
        let server = TranscendServer::default();
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
        assert!(file.symbols.iter().any(|s| s.name == "impl TranscendServer" && s.children.iter().any(|c| c.name == "new")));

        // Second impl block (tool_router) has search(), find(), outline()
        let tool_impl = file
            .symbols
            .iter()
            .find(|s| s.name == "impl TranscendServer" && s.children.iter().any(|c| c.name == "search"))
            .expect("tool router impl TranscendServer should be outlined");

        assert!(tool_impl.children.iter().any(|c| c.name == "search"));
        assert!(tool_impl.children.iter().any(|c| c.name == "find"));
        assert!(tool_impl.children.iter().any(|c| c.name == "outline"));
        assert!(tool_impl.children.iter().any(|c| c.name == "read_symbol"));
        assert!(tool_impl.children.iter().any(|c| c.name == "patch"));
    }

    #[tokio::test]
    async fn test_server_read_symbol_tool_execution() {
        let server = TranscendServer::default();
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
        let server = TranscendServer::default();
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
        let server = TranscendServer::default();
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
        let server = TranscendServer::default();
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
        let server = TranscendServer::default();
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
        let server = TranscendServer::default();
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

    #[test]
    fn test_export_mcp_schemas() {
        let mcp_dir = std::path::Path::new(r"C:\Users\pxin\.gemini\antigravity\mcp\transcend");
        if mcp_dir.exists() {
            let tools = vec![
                ("search", "High-performance code search returning structured, token-compact results", serde_json::to_value(schemars::schema_for!(SearchRequest)).unwrap()),
                ("find", "Fast filesystem file discovery respecting ignore rules", serde_json::to_value(schemars::schema_for!(FindRequest)).unwrap()),
                ("outline", "Extract AST code structure and symbol definitions from a source file", serde_json::to_value(schemars::schema_for!(OutlineRequest)).unwrap()),
                ("read_symbol", "Surgically extract a specific symbol by name or qualified locator, returning exact source code and span", serde_json::to_value(schemars::schema_for!(ReadSymbolRequest)).unwrap()),
                ("patch", "Surgically modify code targeting a symbol, span, or text with AST syntax validation before touching disk", serde_json::to_value(schemars::schema_for!(PatchRequest)).unwrap()),
                ("find_symbol", "Globally find code symbol definitions across the workspace by name, qualified path, or kind with exact AST spans", serde_json::to_value(schemars::schema_for!(FindSymbolRequest)).unwrap()),
                ("lsp_definition", "Go to compiler-resolved definition of a symbol or position across the workspace", serde_json::to_value(schemars::schema_for!(LspDefinitionRequest)).unwrap()),
                ("lsp_references", "Find all compiler-resolved references and call sites across the workspace", serde_json::to_value(schemars::schema_for!(LspReferencesRequest)).unwrap()),
                ("lsp_hover", "Inspect inferred type signature and documentation for a symbol or position", serde_json::to_value(schemars::schema_for!(LspHoverRequest)).unwrap()),
                ("lsp_diagnostics", "Retrieve compiler diagnostics (errors, warnings) for a file or workspace", serde_json::to_value(schemars::schema_for!(LspDiagnosticsRequest)).unwrap()),
                ("exec", "Execute a command in an isolated terminal or process with hybrid lifecycle control", serde_json::to_value(schemars::schema_for!(ExecRequest)).unwrap()),
                ("terminal_read", "Read output from a running or exited terminal session using incremental cursors", serde_json::to_value(schemars::schema_for!(TerminalReadRequest)).unwrap()),
                ("terminal_write", "Send input, keystrokes, or signals to a running terminal session", serde_json::to_value(schemars::schema_for!(TerminalWriteRequest)).unwrap()),
                ("terminal_resize", "Resize a PTY terminal session's columns and rows", serde_json::to_value(schemars::schema_for!(TerminalResizeRequest)).unwrap()),
                ("terminal_kill", "Forcibly terminate a running terminal session and its entire process tree", serde_json::to_value(schemars::schema_for!(TerminalKillRequest)).unwrap()),
                ("read_file", "Read file content with line/byte boundaries, line numbers, and binary safety checks", serde_json::to_value(schemars::schema_for!(ReadFileRequest)).unwrap()),
                ("write_file", "Write text content to file atomically with collision guards and parent directory creation", serde_json::to_value(schemars::schema_for!(WriteFileRequest)).unwrap()),
                ("delete_path", "Delete a file or directory safely within workspace boundaries", serde_json::to_value(schemars::schema_for!(DeletePathRequest)).unwrap()),
                ("batch_patch", "Transactionally apply multiple patches across files with AST preflight and rollback guarantees", serde_json::to_value(schemars::schema_for!(BatchPatchRequest)).unwrap()),
                ("set_workspace", "Configure or update the active project workspace root directory for all path-based operations", serde_json::to_value(schemars::schema_for!(SetWorkspaceRequest)).unwrap()),
                ("git_status", "Inspect structured git status for a repository directory without terminal text scraping", serde_json::to_value(schemars::schema_for!(GitStatusRequest)).unwrap()),
            ];

            for (name, desc, schema) in tools {
                let tool_def = serde_json::json!({
                    "name": name,
                    "description": desc,
                    "parameters": schema,
                });
                let json_str = serde_json::to_string_pretty(&tool_def).unwrap();
                let file_path = mcp_dir.join(format!("{}.json", name));
                std::fs::write(&file_path, json_str).unwrap();
            }
        }
    }

    #[tokio::test]
    async fn test_server_file_lifecycle_tools() {
        let server = TranscendServer::default();
        let tmp_file = format!("target/test_server_file_{}.txt", std::process::id());

        // 1. write_file
        let write_res = server
            .write_file(Parameters(WriteFileRequest {
                path: tmp_file.clone(),
                content: "line 1\nline 2\nline 3\n".to_string(),
                overwrite: Some(true),
                create_parents: Some(true),
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
        let server = TranscendServer::default();
        let res = server
            .batch_patch(Parameters(BatchPatchRequest {
                patches: vec![
                    PatchRequest {
                        path: "dummy.rs".to_string(),
                        content: Some("fn main() {}\n".to_string()),
                        target_symbol: Some("main".to_string()),
                        replacement: "fn main() { println!(\"patched\"); }".to_string(),
                        validate_ast: Some(true),
                        dry_run: Some(true),
                        ..Default::default()
                    }
                ],
                validate_ast: Some(true),
                dry_run: Some(true),
            }))
            .await
            .expect("batch_patch should succeed");

        assert!(res.0.success);
        assert_eq!(res.0.total_files_patched, 1);
    }

    #[tokio::test]
    async fn test_server_set_workspace_tool() {
        let server = TranscendServer::default();
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
        let server = TranscendServer::default();
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
}


