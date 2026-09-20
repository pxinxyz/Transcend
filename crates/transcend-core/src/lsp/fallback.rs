//! Tree-sitter Heuristic Fallback Engine
//!
//! Provides best-effort semantic resolution when no official language server
//! is installed on the host system, ensuring Transcend never fails or crashes.

use std::fs;
use std::path::Path;
use transcend_protocol::{
    DiagnosticSeverity, LspDefinitionResponse, LspDiagnosticItem, LspHoverResponse,
    LspReferenceLocation, LspReferencesResponse, LspTargetLocation, OutlineOptions, OutlineRequest,
    SearchOptions, SearchRequest, SourceSpan,
};

use crate::outline::symbol_reader::SymbolReader;
use crate::{Engine, NativeEngine};

pub struct HeuristicFallback;

impl HeuristicFallback {
    /// Heuristic go-to-definition using Tree-sitter symbol resolution.
    pub fn goto_definition(
        engine: &NativeEngine,
        file_path: &Path,
        symbol_name: &str,
    ) -> LspDefinitionResponse {
        let bare_name = symbol_name.split("::").last().unwrap_or(symbol_name).trim();

        // 1. Check current file outline first
        if let Ok(content) = fs::read_to_string(file_path)
            && let Ok(outline_res) = engine.outline(&OutlineRequest {
                path: Some(file_path.to_string_lossy().to_string()),
                content: Some(content),
                options: None,
            })
            && let Some(file_symbols) = outline_res.files.first()
        {
            for sym in &file_symbols.symbols {
                if sym.name == bare_name {
                    return LspDefinitionResponse {
                        targets: vec![LspTargetLocation {
                            file: file_path.to_string_lossy().replace('\\', "/"),
                            span: sym.span.clone(),
                            preview: sym.signature.clone(),
                        }],
                        engine: "tree-sitter:heuristic".to_string(),
                    };
                }
            }
        }

        // 2. Fallback to workspace-wide find_symbol
        let search_dir = file_path.parent().unwrap_or(file_path);
        if let Ok(find_res) = engine.find_symbol(&transcend_protocol::FindSymbolRequest {
            name: bare_name.to_string(),
            path: Some(search_dir.to_string_lossy().to_string()),
            exact: Some(true),
            limit: Some(5),
            ..Default::default()
        }) {
            let targets: Vec<LspTargetLocation> = find_res
                .symbols
                .into_iter()
                .map(|s| LspTargetLocation {
                    file: s.file,
                    span: s.span,
                    preview: s.signature,
                })
                .collect();

            if !targets.is_empty() {
                return LspDefinitionResponse {
                    targets,
                    engine: "tree-sitter:heuristic".to_string(),
                };
            }
        }

        LspDefinitionResponse {
            targets: vec![],
            engine: "tree-sitter:heuristic (no matches found)".to_string(),
        }
    }

    /// Heuristic find-references using ripgrep word-boundary pattern search.
    ///
    /// `include_declaration` defaults to `false`, meaning the declaration itself is excluded
    /// and only call sites are returned. The heuristic locates the declaration via the same
    /// outline pass used by `hover`, then drops the match that spans it. This is
    /// line-granular: a call on the same line as the declaration is removed with it.
    pub fn find_references(
        engine: &NativeEngine,
        file_path: &Path,
        symbol_name: &str,
        include_declaration: bool,
        limit: usize,
    ) -> LspReferencesResponse {
        let bare_name = symbol_name.split("::").last().unwrap_or(symbol_name).trim();
        let search_root = file_path.parent().unwrap_or(file_path);

        // Where the declaration lives, so it can be excluded. `None` means it could not be
        // located, in which case nothing is dropped rather than guessing.
        let declaration = if include_declaration {
            None
        } else {
            Self::locate_declaration(engine, file_path, bare_name)
        };

        // Regex word boundary pattern: \b<symbol>\b
        let pattern = format!(r"\b{}\b", regex::escape(bare_name));

        let res = engine.search(&SearchRequest {
            pattern,
            path: Some(search_root.to_string_lossy().to_string()),
            options: Some(SearchOptions {
                case_sensitive: Some(true),
                max_matches: Some(limit),
                ..Default::default()
            }),
        });

        match res {
            Ok(search_res) => {
                let mut references = Vec::new();
                for file_entry in &search_res.files {
                    for m in &file_entry.matches {
                        if let Some((decl_file, decl_line)) = declaration.as_ref()
                            && Self::same_file(&file_entry.file, decl_file)
                            && m.line_number == *decl_line
                        {
                            continue;
                        }
                        references.push(LspReferenceLocation {
                            file: file_entry.file.clone(),
                            span: SourceSpan {
                                start_line: m.line_number,
                                start_col: 1,
                                end_line: m.line_number,
                                end_col: m.line_text.len() + 1,
                                start_byte: 0,
                                end_byte: 0,
                            },
                            line_text: m.line_text.clone(),
                        });
                    }
                }

                LspReferencesResponse {
                    // Report the post-filter count so total_found cannot claim more
                    // references than `references` actually contains.
                    total_found: references.len(),
                    references,
                    truncated: search_res.truncated,
                    engine: "tree-sitter:heuristic".to_string(),
                }
            }
            Err(_) => LspReferencesResponse {
                total_found: 0,
                references: vec![],
                truncated: false,
                engine: "tree-sitter:heuristic".to_string(),
            },
        }
    }

    /// Find the (file, 1-based line) of a symbol's own declaration, if it can be located.
    fn locate_declaration(
        engine: &NativeEngine,
        file_path: &Path,
        symbol_name: &str,
    ) -> Option<(String, usize)> {
        let content = fs::read_to_string(file_path).ok()?;
        let res = engine
            .outline(&OutlineRequest {
                path: Some(file_path.to_string_lossy().to_string()),
                content: Some(content),
                options: None,
            })
            .ok()?;

        let mut symbols = Vec::new();
        for file_outline in &res.files {
            SymbolReader::collect_symbols(&file_outline.symbols, &[], &mut symbols);
        }
        let found = symbols
            .into_iter()
            .find(|s| s.symbol.name == symbol_name || s.qualified_name.ends_with(symbol_name))?;

        Some((
            file_path.to_string_lossy().replace('\\', "/"),
            found.symbol.span.start_line,
        ))
    }

    /// Compare two display paths that may differ in separator style or absoluteness.
    fn same_file(a: &str, b: &str) -> bool {
        let norm = |s: &str| s.replace('\\', "/");
        let (a, b) = (norm(a), norm(b));
        a == b || a.ends_with(&b) || b.ends_with(&a)
    }

    /// Heuristic hover using Tree-sitter outline signature and doc comments.
    pub fn hover(engine: &NativeEngine, file_path: &Path, symbol_name: &str) -> LspHoverResponse {
        let bare_name = symbol_name.split("::").last().unwrap_or(symbol_name).trim();

        if let Ok(content) = fs::read_to_string(file_path)
            && let Ok(outline_res) = engine.outline(&OutlineRequest {
                path: Some(file_path.to_string_lossy().to_string()),
                content: Some(content),
                options: Some(OutlineOptions {
                    include_doc_comments: Some(true),
                    ..Default::default()
                }),
            })
            && let Some(file_symbols) = outline_res.files.first()
        {
            for sym in &file_symbols.symbols {
                if sym.name == bare_name {
                    return LspHoverResponse {
                        signature: sym.signature.clone(),
                        documentation: sym.doc_comment.clone(),
                        span: Some(sym.span.clone()),
                        engine: "tree-sitter:heuristic".to_string(),
                    };
                }
            }
        }

        LspHoverResponse {
            signature: None,
            documentation: None,
            span: None,
            engine: "tree-sitter:heuristic".to_string(),
        }
    }

    /// Attempt to retrieve compiler diagnostics via CLI JSON mode (e.g. cargo check, tsc, ruff/mypy).
    pub fn compiler_diagnostics(
        workspace_root: &Path,
        file_path_filter: Option<&str>,
        severity_filter: Option<DiagnosticSeverity>,
    ) -> Vec<LspDiagnosticItem> {
        let mut results = Vec::new();

        // 1. Rust Project: Cargo.toml
        if workspace_root.join("Cargo.toml").exists() {
            let output = std::process::Command::new("cargo")
                .args(["check", "--message-format=json", "--quiet"])
                .current_dir(workspace_root)
                .output();

            if let Ok(out) = output {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines() {
                    let trimmed = line.trim();
                    if !trimmed.starts_with('{') {
                        continue;
                    }
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed)
                        && val.get("reason").and_then(|r| r.as_str()) == Some("compiler-message")
                        && let Some(msg) = val.get("message")
                    {
                        let level = msg.get("level").and_then(|l| l.as_str()).unwrap_or("error");
                        let severity = match level {
                            "error" => DiagnosticSeverity::Error,
                            "warning" => DiagnosticSeverity::Warning,
                            "note" => DiagnosticSeverity::Information,
                            "help" => DiagnosticSeverity::Hint,
                            _ => DiagnosticSeverity::Error,
                        };

                        if let Some(req_sev) = severity_filter
                            && severity != req_sev
                        {
                            continue;
                        }

                        let message_text = msg
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("")
                            .to_string();
                        let code = msg
                            .get("code")
                            .and_then(|c| c.get("code"))
                            .and_then(|c| c.as_str())
                            .map(|s| s.to_string());

                        if let Some(spans) = msg.get("spans").and_then(|s| s.as_array()) {
                            let target_span = spans
                                .iter()
                                .find(|s| {
                                    s.get("is_primary").and_then(|p| p.as_bool()) == Some(true)
                                })
                                .or_else(|| spans.first());

                            if let Some(span_val) = target_span {
                                let file_name = span_val
                                    .get("file_name")
                                    .and_then(|f| f.as_str())
                                    .unwrap_or("")
                                    .replace('\\', "/");
                                if let Some(filter) = file_path_filter {
                                    let filter_norm = filter.replace('\\', "/");
                                    if !file_name.ends_with(&filter_norm)
                                        && !filter_norm.ends_with(&file_name)
                                    {
                                        continue;
                                    }
                                }

                                let line_start = span_val
                                    .get("line_start")
                                    .and_then(|l| l.as_u64())
                                    .unwrap_or(1)
                                    as usize;
                                let line_end = span_val
                                    .get("line_end")
                                    .and_then(|l| l.as_u64())
                                    .unwrap_or(line_start as u64)
                                    as usize;
                                let col_start = span_val
                                    .get("column_start")
                                    .and_then(|c| c.as_u64())
                                    .unwrap_or(1)
                                    as usize;
                                let col_end = span_val
                                    .get("column_end")
                                    .and_then(|c| c.as_u64())
                                    .unwrap_or(col_start as u64)
                                    as usize;
                                let byte_start = span_val
                                    .get("byte_start")
                                    .and_then(|b| b.as_u64())
                                    .unwrap_or(0)
                                    as usize;
                                let byte_end = span_val
                                    .get("byte_end")
                                    .and_then(|b| b.as_u64())
                                    .unwrap_or(byte_start as u64)
                                    as usize;

                                results.push(LspDiagnosticItem {
                                    file: file_name,
                                    span: SourceSpan {
                                        start_line: line_start,
                                        start_col: col_start,
                                        end_line: line_end,
                                        end_col: col_end,
                                        start_byte: byte_start,
                                        end_byte: byte_end,
                                    },
                                    severity,
                                    code,
                                    source: Some("rustc".to_string()),
                                    message: message_text,
                                });
                            }
                        }
                    }
                }
            }
        }

        results
    }
}
