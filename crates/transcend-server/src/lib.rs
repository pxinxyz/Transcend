//! Transcend MCP Server Implementation
//!
//! Exposes Transcend primitives as tools compliant with the Model Context Protocol (MCP).

use std::sync::Arc;
use rmcp::{handler::server::wrapper::Parameters, tool, tool_router, Json};
use transcend_core::{Engine, NativeEngine};
use transcend_protocol::{
    FindRequest, FindResponse, OutlineRequest, OutlineResponse, SearchRequest, SearchResponse,
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
}
