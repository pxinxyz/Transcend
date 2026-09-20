//! Language Server Protocol (LSP) Engine
//!
//! Exposes compiler-backed semantic intelligence (definitions, references,
//! type hover, compiler diagnostics) with instant Tree-sitter fallback.

pub mod bridge;
pub mod distiller;
pub mod fallback;
pub mod installer;
pub mod pool;
pub mod protocol;
pub mod registry;
pub mod session;

use std::fs;
use std::path::Path;
use transcend_protocol::{
    LspDefinitionRequest, LspDefinitionResponse, LspDiagnosticsRequest, LspDiagnosticsResponse,
    LspHoverRequest, LspHoverResponse, LspInstallRequest, LspInstallResponse, LspReferencesRequest,
    LspReferencesResponse, LspStatusRequest, LspStatusResponse,
};

use crate::{CoreError, NativeEngine};
use bridge::SymbolCoordinateBridge;
use fallback::HeuristicFallback;
use pool::LspSessionPool;

/// How long to keep retrying a definition query while the server is still indexing.
///
/// rust-analyzer can need tens of seconds on a cold clone. Waiting is strictly better
/// than answering from a textual heuristic, because the caller asked for a
/// compiler-resolved result and the response says which engine produced it.
const COLD_START_GRACE: std::time::Duration = std::time::Duration::from_secs(90);

/// Delay between retries inside the cold-start window.
const COLD_START_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3);

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
        if let Ok(Some((session, profile))) = self.pool.get_or_spawn(file_path).await
            && let Ok((line_0, col_0)) = pos
        {
            let engine_label = format!("lsp:{}", profile.binary_candidates[0]);

            // First attempt: on a warm server this is the only one.
            if let Ok(targets) = session.goto_definition(file_path, line_0, col_0).await
                && !targets.is_empty()
            {
                return Ok(LspDefinitionResponse {
                    targets,
                    engine: engine_label,
                });
            }

            // A cold server returns nothing until it has indexed the project, and an
            // empty answer is indistinguishable from "no definition". Retry for a
            // bounded window so the first query after a fresh clone still gets compiler
            // precision instead of silently degrading to the textual heuristic.
            let deadline = tokio::time::Instant::now() + COLD_START_GRACE;
            while !session.is_warmed() && tokio::time::Instant::now() < deadline {
                tokio::time::sleep(COLD_START_POLL_INTERVAL).await;
                if let Ok(targets) = session.goto_definition(file_path, line_0, col_0).await
                    && !targets.is_empty()
                {
                    return Ok(LspDefinitionResponse {
                        targets,
                        engine: engine_label,
                    });
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

        if let Ok(Some((session, profile))) = self.pool.get_or_spawn(file_path).await
            && let Ok((line_0, col_0)) = pos
            && let Ok((references, total_found, truncated)) = session
                .find_references(file_path, line_0, col_0, include_decl, limit)
                .await
            && !references.is_empty()
        {
            return Ok(LspReferencesResponse {
                total_found,
                references,
                truncated,
                engine: format!("lsp:{}", profile.binary_candidates[0]),
            });
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

        if let Ok(Some((session, profile))) = self.pool.get_or_spawn(file_path).await
            && let Ok((line_0, col_0)) = pos
            && let Ok((signature, documentation, span)) =
                session.hover(file_path, line_0, col_0).await
            && (signature.is_some() || documentation.is_some())
        {
            return Ok(LspHoverResponse {
                signature,
                documentation,
                span,
                engine: format!("lsp:{}", profile.binary_candidates[0]),
            });
        }

        let sym_name = req.symbol.as_deref().unwrap_or("");
        let fallback = HeuristicFallback::hover(engine, file_path, sym_name);
        Ok(fallback)
    }

    /// Retrieve active compiler diagnostics for file or workspace.
    pub async fn diagnostics(
        &self,
        engine: &NativeEngine,
        req: &LspDiagnosticsRequest,
    ) -> Result<LspDiagnosticsResponse, CoreError> {
        let workspace_root = engine.get_workspace();

        // 1. If path is provided, attempt to auto-warm / spawn the language server for that file
        if let Some(ref path_str) = req.path {
            let file_path = Path::new(path_str);
            if file_path.exists()
                && let Ok(Some((session, _profile))) = self.pool.get_or_spawn(file_path).await
            {
                let _ = session.ensure_document_open(file_path).await;
                // Bounded debounce to allow server to publish diagnostics
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            }
        }

        let active_sessions = self.pool.active_sessions().await;
        // Whether any language server actually answered. An empty result from a live server is
        // a verdict ("this file is clean"), which is a completely different thing from "no
        // server was available" -- conflating the two let the `cargo check` fallback replace a
        // correct clean result with errors from unrelated files in the same workspace.
        let lsp_consulted = !active_sessions.is_empty();
        let mut all_diags = Vec::new();

        for session in active_sessions {
            let res = session
                .get_diagnostics(req.path.as_deref(), req.severity)
                .await;
            all_diags.extend(res.diagnostics);
        }

        // 2. Only fall back to native compiler JSON when no language server was available.
        //    An empty-but-consulted LSP result must be returned as-is.
        if !lsp_consulted {
            let path_filter = req.path.clone();
            let sev_filter = req.severity;
            let root = workspace_root.clone();
            let fallback_diags = tokio::task::spawn_blocking(move || {
                HeuristicFallback::compiler_diagnostics(&root, path_filter.as_deref(), sev_filter)
            })
            .await
            .unwrap_or_default();

            all_diags.extend(fallback_diags);
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

    /// Check current installation status and recipes for language servers.
    pub async fn status(&self, req: &LspStatusRequest) -> Result<LspStatusResponse, CoreError> {
        Ok(installer::LspInstaller::check_status(req).await)
    }

    /// Automatically install a language server using host package managers.
    pub async fn install(&self, req: &LspInstallRequest) -> Result<LspInstallResponse, CoreError> {
        installer::LspInstaller::install(req).await
    }
}
