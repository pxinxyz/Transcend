//! Transcend MCP Server Implementation
//!
//! Exposes Transcend primitives as tools compliant with the Model Context Protocol (MCP).

use std::sync::Arc;
use rmcp::{handler::server::wrapper::Parameters, tool, tool_router, Json};
use transcend_core::{Engine, NativeEngine};
use transcend_protocol::{
    FindRequest, FindResponse, FindSymbolRequest, FindSymbolResponse, OutlineRequest,
    OutlineResponse, PatchRequest, PatchResponse, ReadSymbolRequest, ReadSymbolResponse,
    SearchRequest, SearchResponse,
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
            ];

            for (name, desc, schema) in tools {
                let tool_def = serde_json::json!({
                    "name": name,
                    "description": desc,
                    "parameters": schema,
                });
                let json_str = serde_json::to_string(&tool_def).unwrap();
                let file_path = mcp_dir.join(format!("{}.json", name));
                std::fs::write(&file_path, json_str).unwrap();
            }
        }
    }
}


