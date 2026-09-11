//! Outline Scanner Coordinator
//!
//! Handles traversal, language detection, in-process Tree-sitter parsing,
//! budgeting, and architectural summary aggregation.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use tree_sitter::{Language, Parser};
use transcend_protocol::{
    FileOutline, OutlineOptions, OutlineRequest, OutlineResponse, OutlineSummary, ParseStatus,
    Symbol,
};

use crate::{CoreError, CoreResult};
use super::go::GoOutline;
use super::python::PythonOutline;
use super::rust::RustOutline;
use super::typescript::TypeScriptOutline;
use super::LanguageOutline;

pub struct OutlineScanner;

enum SupportedLang {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Python,
    Go,
}

impl SupportedLang {
    fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        match ext.as_str() {
            "rs" => Some(SupportedLang::Rust),
            "ts" | "mts" | "cts" => Some(SupportedLang::TypeScript),
            "tsx" => Some(SupportedLang::Tsx),
            "js" | "mjs" | "cjs" | "jsx" => Some(SupportedLang::JavaScript),
            "py" | "pyi" => Some(SupportedLang::Python),
            "go" => Some(SupportedLang::Go),
            _ => None,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            SupportedLang::Rust => "rust",
            SupportedLang::TypeScript => "typescript",
            SupportedLang::Tsx => "tsx",
            SupportedLang::JavaScript => "javascript",
            SupportedLang::Python => "python",
            SupportedLang::Go => "go",
        }
    }

    fn language(&self) -> Language {
        match self {
            SupportedLang::Rust => Language::from(tree_sitter_rust::LANGUAGE),
            SupportedLang::TypeScript | SupportedLang::JavaScript => {
                Language::from(tree_sitter_typescript::LANGUAGE_TYPESCRIPT)
            }
            SupportedLang::Tsx => Language::from(tree_sitter_typescript::LANGUAGE_TSX),
            SupportedLang::Python => Language::from(tree_sitter_python::LANGUAGE),
            SupportedLang::Go => Language::from(tree_sitter_go::LANGUAGE),
        }
    }

    fn adapter(&self) -> Box<dyn LanguageOutline> {
        match self {
            SupportedLang::Rust => Box::new(RustOutline::new()),
            SupportedLang::TypeScript | SupportedLang::Tsx | SupportedLang::JavaScript => {
                Box::new(TypeScriptOutline::new())
            }
            SupportedLang::Python => Box::new(PythonOutline::new()),
            SupportedLang::Go => Box::new(GoOutline::new()),
        }
    }
}

impl OutlineScanner {
    pub fn scan(req: &OutlineRequest) -> CoreResult<OutlineResponse> {
        let options = req.options.clone().unwrap_or_default();
        let max_symbols = options.max_symbols.unwrap_or(500);
        let max_files = options.max_files.unwrap_or(20);

        // 1. Direct in-memory content parsing
        if let Some(ref content) = req.content {
            let path_hint = req.path.as_deref().unwrap_or("snippet.rs");
            let lang = SupportedLang::from_path(Path::new(path_hint)).unwrap_or(SupportedLang::Rust);
            let mut file_outline = Self::parse_bytes(
                path_hint,
                content.as_bytes(),
                &lang,
                &options,
            )?;

            // Depth pruning
            if let Some(max_d) = options.max_depth {
                Self::prune_depth(&mut file_outline.symbols, 1, max_d);
            }

            let total_syms = Self::count_symbols(&file_outline.symbols);
            let mut summary = OutlineSummary {
                total_files: 1,
                total_symbols: total_syms,
                ..Default::default()
            };
            Self::accumulate_summary(&file_outline, &mut summary);

            let (files, truncated) = Self::apply_budget(vec![file_outline], max_symbols);

            return Ok(OutlineResponse {
                summary,
                files,
                truncated,
            });
        }

        // 2. Filesystem path target
        let target_path_str = req.path.as_deref().unwrap_or(".");
        let target_path = Path::new(target_path_str);

        if !target_path.exists() {
            return Err(CoreError::General(format!(
                "Path does not exist: {}",
                target_path.display()
            )));
        }

        let mut candidate_files: Vec<PathBuf> = Vec::new();

        if target_path.is_file() {
            candidate_files.push(target_path.to_path_buf());
        } else {
            // Traverse directory respecting .gitignore
            let walker = WalkBuilder::new(target_path)
                .standard_filters(true)
                .hidden(true)
                .build();

            for entry in walker.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_file() {
                    // Check if supported language
                    if SupportedLang::from_path(path).is_some() {
                        // Skip minified bundles or oversized files (> 1MB)
                        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                        if file_name.ends_with(".min.js") || file_name.ends_with(".bundle.js") {
                            continue;
                        }
                        if let Ok(meta) = entry.metadata() {
                            if meta.len() > 1_000_000 {
                                continue;
                            }
                        }
                        candidate_files.push(path.to_path_buf());
                        if candidate_files.len() >= max_files * 2 {
                            break;
                        }
                    }
                }
            }
        }

        let mut file_outlines = Vec::new();
        let mut summary = OutlineSummary::default();
        let mut total_discovered_symbols = 0;

        for file_path in candidate_files.into_iter().take(max_files) {
            let lang = match SupportedLang::from_path(&file_path) {
                Some(l) => l,
                None => continue,
            };

            let bytes = match fs::read(&file_path) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(file = %file_path.display(), error = %e, "Failed to read file for outline");
                    continue;
                }
            };

            // Quick binary check
            if bytes.iter().take(1024).any(|&b| b == 0) {
                continue;
            }

            let rel_path = file_path
                .strip_prefix(target_path)
                .unwrap_or(&file_path)
                .to_string_lossy()
                .replace('\\', "/");
            let display_path = if rel_path.is_empty() {
                file_path.to_string_lossy().replace('\\', "/")
            } else {
                rel_path
            };

            let mut outline = Self::parse_bytes(&display_path, &bytes, &lang, &options)?;

            // Depth pruning
            if let Some(max_d) = options.max_depth {
                Self::prune_depth(&mut outline.symbols, 1, max_d);
            }

            total_discovered_symbols += Self::count_symbols(&outline.symbols);
            Self::accumulate_summary(&outline, &mut summary);
            summary.total_files += 1;

            file_outlines.push(outline);
        }

        summary.total_symbols = total_discovered_symbols;

        let (files, truncated) = Self::apply_budget(file_outlines, max_symbols);

        Ok(OutlineResponse {
            summary,
            files,
            truncated,
        })
    }

    fn parse_bytes(
        display_path: &str,
        source: &[u8],
        lang: &SupportedLang,
        options: &OutlineOptions,
    ) -> CoreResult<FileOutline> {
        let mut parser = Parser::new();
        let ts_lang = lang.language();
        parser
            .set_language(&ts_lang)
            .map_err(|e| CoreError::General(format!("Failed to set parser language: {e}")))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| CoreError::ParseError {
                file: display_path.to_string(),
                message: "Tree-sitter parser returned None".to_string(),
            })?;

        let root = tree.root_node();
        let parse_status = if root.is_error() {
            ParseStatus::SyntaxErrors
        } else if root.has_error() {
            ParseStatus::Partial
        } else {
            ParseStatus::Complete
        };

        let adapter = lang.adapter();
        let symbols = adapter.extract(&tree, source, options);

        Ok(FileOutline {
            file: display_path.to_string(),
            language: lang.name().to_string(),
            parse_status,
            symbols,
        })
    }

    fn prune_depth(symbols: &mut Vec<Symbol>, current_depth: usize, max_depth: usize) {
        if current_depth >= max_depth {
            for sym in symbols.iter_mut() {
                sym.children.clear();
            }
        } else {
            for sym in symbols.iter_mut() {
                Self::prune_depth(&mut sym.children, current_depth + 1, max_depth);
            }
        }
    }

    fn count_symbols(symbols: &[Symbol]) -> usize {
        let mut count = symbols.len();
        for s in symbols {
            count += Self::count_symbols(&s.children);
        }
        count
    }

    fn accumulate_summary(outline: &FileOutline, summary: &mut OutlineSummary) {
        *summary
            .language_breakdown
            .entry(outline.language.clone())
            .or_insert(0) += 1;

        fn record_kinds(symbols: &[Symbol], map: &mut BTreeMap<String, usize>) {
            for s in symbols {
                let kind_str = format!("{:?}", s.kind).to_lowercase();
                *map.entry(kind_str).or_insert(0) += 1;
                record_kinds(&s.children, map);
            }
        }

        record_kinds(&outline.symbols, &mut summary.kind_breakdown);
    }

    fn apply_budget(
        outlines: Vec<FileOutline>,
        max_symbols: usize,
    ) -> (Vec<FileOutline>, bool) {
        let mut budget_remaining = max_symbols;
        let mut budgeted_outlines = Vec::new();
        let mut truncated = false;

        for mut outline in outlines {
            if budget_remaining == 0 {
                truncated = true;
                break;
            }

            let sym_count = Self::count_symbols(&outline.symbols);
            if sym_count <= budget_remaining {
                budget_remaining -= sym_count;
                budgeted_outlines.push(outline);
            } else {
                // Truncate symbols within this file
                truncated = true;
                let mut kept_symbols = Vec::new();
                for sym in outline.symbols {
                    let cost = 1 + Self::count_symbols(&sym.children);
                    if cost <= budget_remaining {
                        budget_remaining -= cost;
                        kept_symbols.push(sym);
                    } else if budget_remaining > 0 {
                        // Keep parent symbol without children
                        let mut trimmed_sym = sym;
                        trimmed_sym.children.clear();
                        kept_symbols.push(trimmed_sym);
                        break;
                    } else {
                        break;
                    }
                }
                outline.symbols = kept_symbols;
                budgeted_outlines.push(outline);
                break;
            }
        }

        (budgeted_outlines, truncated)
    }
}
