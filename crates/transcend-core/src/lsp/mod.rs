//! Language Server Protocol (LSP) Engine
//!
//! Exposes compiler-backed semantic intelligence (definitions, references,
//! type hover, compiler diagnostics) with instant Tree-sitter fallback.

pub mod bridge;
pub mod distiller;
pub mod fallback;
pub mod pool;
pub mod protocol;
pub mod registry;
pub mod session;

use std::fs;
use std::path::Path;
use transcend_protocol::{
    LspDefinitionRequest, LspDefinitionResponse, LspDiagnosticsRequest, LspDiagnosticsResponse,
    LspHoverRequest, LspHoverResponse, LspReferencesRequest, LspReferencesResponse,
};

use crate::{CoreError, NativeEngine};
use bridge::SymbolCoordinateBridge;
use fallback::HeuristicFallback;
use pool::LspSessionPool;

/// The central LSP Engine coordinating sessions, coordinate translation, and fallback.
#[derive(Clone, Default)]
pub struct LspEngine {
    pool: LspSessionPool,
}

impl LspEngine {
    pub fn new() -> Self {
        Self {
            pool: LspSessionPool::new(),
        }
    }

    /// Go to compiler-resolved definition, falling back to Tree-sitter heuristic.
    pub async fn goto_definition(
        &self,
        engine: &NativeEngine,
        req: &LspDefinitionRequest,
    ) -> Result<LspDefinitionResponse, CoreError> {
        let file_path = Path::new(&req.path);
        let content = fs::read_to_string(file_path).unwrap_or_default();

        // 1. Resolve coordinates via Tree-sitter
        let pos = SymbolCoordinateBridge::resolve_position(
            file_path,
            &content,
            req.symbol.as_deref(),
            req.line,
            req.character,
        );

        // 2. Check if a language server session can be obtained
        if let Ok(Some((session, profile))) = self.pool.get_or_spawn(file_path).await {
            if let Ok((line_0, col_0)) = pos {
                if let Ok(targets) = session.goto_definition(file_path, line_0, col_0).await {
                    if !targets.is_empty() {
                        return Ok(LspDefinitionResponse {
                            targets,
                            engine: format!("lsp:{}", profile.binary_candidates[0]),
                        });
                    }
                }
            }
        }

        // 3. Fallback to Tree-sitter heuristic
        let sym_name = req.symbol.as_deref().unwrap_or("symbol");
        let fallback = HeuristicFallback::goto_definition(engine, file_path, sym_name);
        Ok(fallback)
    }

    /// Find compiler-resolved references and call sites across the workspace.
    pub async fn find_references(
        &self,
        engine: &NativeEngine,
        req: &LspReferencesRequest,
    ) -> Result<LspReferencesResponse, CoreError> {
        let file_path = Path::new(&req.path);
        let content = fs::read_to_string(file_path).unwrap_or_default();
        let limit = req.limit.unwrap_or(50);
        let include_decl = req.include_declaration.unwrap_or(false);

        let pos = SymbolCoordinateBridge::resolve_position(
            file_path,
            &content,
            req.symbol.as_deref(),
            req.line,
            req.character,
        );

        if let Ok(Some((session, profile))) = self.pool.get_or_spawn(file_path).await {
            if let Ok((line_0, col_0)) = pos {
                if let Ok((references, total_found, truncated)) = session.find_references(file_path, line_0, col_0, include_decl, limit).await {
                    if !references.is_empty() {
                        return Ok(LspReferencesResponse {
                            total_found,
                            references,
                            truncated,
                            engine: format!("lsp:{}", profile.binary_candidates[0]),
                        });
                    }
                }
            }
        }

        let sym_name = req.symbol.as_deref().unwrap_or("");
        let fallback = HeuristicFallback::find_references(engine, file_path, sym_name, limit);
        Ok(fallback)
    }

    /// Inspect inferred type signature and documentation.
    pub async fn hover(
        &self,
        engine: &NativeEngine,
        req: &LspHoverRequest,
    ) -> Result<LspHoverResponse, CoreError> {
        let file_path = Path::new(&req.path);
        let content = fs::read_to_string(file_path).unwrap_or_default();

        let pos = SymbolCoordinateBridge::resolve_position(
            file_path,
            &content,
            req.symbol.as_deref(),
            req.line,
            req.character,
        );

        if let Ok(Some((session, profile))) = self.pool.get_or_spawn(file_path).await {
            if let Ok((line_0, col_0)) = pos {
                if let Ok((signature, documentation, span)) = session.hover(file_path, line_0, col_0).await {
                    if signature.is_some() || documentation.is_some() {
                        return Ok(LspHoverResponse {
                            signature,
                            documentation,
                            span,
                            engine: format!("lsp:{}", profile.binary_candidates[0]),
                        });
                    }
                }
            }
        }

        let sym_name = req.symbol.as_deref().unwrap_or("");
        let fallback = HeuristicFallback::hover(engine, file_path, sym_name);
        Ok(fallback)
    }

    /// Retrieve active compiler diagnostics for file or workspace.
    pub async fn diagnostics(
        &self,
        _engine: &NativeEngine,
        req: &LspDiagnosticsRequest,
    ) -> Result<LspDiagnosticsResponse, CoreError> {
        let active_sessions = self.pool.active_sessions().await;
        let mut all_diags = Vec::new();

        for session in active_sessions {
            let res = session.get_diagnostics(req.path.as_deref(), req.severity).await;
            all_diags.extend(res.diagnostics);
        }

        let total_count = all_diags.len();
        let mut severity_breakdown = std::collections::BTreeMap::new();
        for d in &all_diags {
            let s = match d.severity {
                transcend_protocol::DiagnosticSeverity::Error => "error",
                transcend_protocol::DiagnosticSeverity::Warning => "warning",
                transcend_protocol::DiagnosticSeverity::Information => "information",
                transcend_protocol::DiagnosticSeverity::Hint => "hint",
            };
            *severity_breakdown.entry(s.to_string()).or_insert(0) += 1;
        }

        Ok(LspDiagnosticsResponse {
            total_count,
            diagnostics: all_diags,
            severity_breakdown,
        })
    }
}
