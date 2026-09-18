//! Tree-sitter Heuristic Fallback Engine
//!
//! Provides best-effort semantic resolution when no official language server
//! is installed on the host system, ensuring Transcend never fails or crashes.

use std::fs;
use std::path::Path;
use transcend_protocol::{
    LspDefinitionResponse, LspHoverResponse, LspReferenceLocation, LspReferencesResponse,
    LspTargetLocation, OutlineOptions, OutlineRequest, SearchOptions, SearchRequest, SourceSpan,
};

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
        if let Ok(content) = fs::read_to_string(file_path) {
            if let Ok(outline_res) = engine.outline(&OutlineRequest {
                path: Some(file_path.to_string_lossy().to_string()),
                content: Some(content),
                options: None,
            }) {
                if let Some(file_symbols) = outline_res.files.first() {
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
    pub fn find_references(
        engine: &NativeEngine,
        file_path: &Path,
        symbol_name: &str,
        limit: usize,
    ) -> LspReferencesResponse {
        let bare_name = symbol_name.split("::").last().unwrap_or(symbol_name).trim();
        let search_root = file_path.parent().unwrap_or(file_path);

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
                    total_found: search_res.total_matches,
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

    /// Heuristic hover using Tree-sitter outline signature and doc comments.
    pub fn hover(
        engine: &NativeEngine,
        file_path: &Path,
        symbol_name: &str,
    ) -> LspHoverResponse {
        let bare_name = symbol_name.split("::").last().unwrap_or(symbol_name).trim();

        if let Ok(content) = fs::read_to_string(file_path) {
            if let Ok(outline_res) = engine.outline(&OutlineRequest {
                path: Some(file_path.to_string_lossy().to_string()),
                content: Some(content),
                options: Some(OutlineOptions {
                    include_doc_comments: Some(true),
                    ..Default::default()
                }),
            }) {
                if let Some(file_symbols) = outline_res.files.first() {
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
            }
        }

        LspHoverResponse {
            signature: None,
            documentation: None,
            span: None,
            engine: "tree-sitter:heuristic".to_string(),
        }
    }
}
