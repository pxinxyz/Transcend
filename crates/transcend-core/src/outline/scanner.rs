//! Outline Scanner Coordinator
//!
//! Handles traversal, language detection, in-process Tree-sitter parsing,
//! budgeting, and architectural summary aggregation.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use transcend_protocol::{
    FileOutline, OutlineFormat, OutlineOptions, OutlineRequest, OutlineResponse, OutlineSummary,
    ParseStatus, Symbol,
};
use tree_sitter::{Language, Parser};

use super::LanguageOutline;
use super::bash::BashOutline;
use super::c_cpp::{COutline, CppOutline};
use super::csharp::CSharpOutline;
use super::dart::DartOutline;
use super::go::GoOutline;
use super::java::JavaOutline;
use super::kotlin::KotlinOutline;
use super::lua::LuaOutline;
use super::markdown::MarkdownOutline;
use super::php::PhpOutline;
use super::python::PythonOutline;
use super::ruby::RubyOutline;
use super::rust::RustOutline;
use super::sql::SqlOutline;
use super::swift::SwiftOutline;
use super::typescript::TypeScriptOutline;
use super::zig::ZigOutline;
use crate::{CoreError, CoreResult};

pub struct OutlineScanner;

pub(crate) enum SupportedLang {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Python,
    Go,
    C,
    Cpp,
    CSharp,
    Java,
    Kotlin,
    Php,
    Ruby,
    Swift,
    Bash,
    Sql,
    Dart,
    Zig,
    Lua,
    Markdown,
}

impl SupportedLang {
    pub(crate) fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        match ext.as_str() {
            "rs" => Some(SupportedLang::Rust),
            "ts" | "mts" | "cts" => Some(SupportedLang::TypeScript),
            "tsx" => Some(SupportedLang::Tsx),
            "js" | "mjs" | "cjs" | "jsx" => Some(SupportedLang::JavaScript),
            "py" | "pyi" => Some(SupportedLang::Python),
            "go" => Some(SupportedLang::Go),
            "c" | "h" => Some(SupportedLang::C),
            "cpp" | "hpp" | "cc" | "cxx" | "hh" | "hxx" | "c++" | "h++" => Some(SupportedLang::Cpp),
            "cs" => Some(SupportedLang::CSharp),
            "java" => Some(SupportedLang::Java),
            "kt" | "kts" => Some(SupportedLang::Kotlin),
            "php" | "phtml" => Some(SupportedLang::Php),
            "rb" => Some(SupportedLang::Ruby),
            "swift" => Some(SupportedLang::Swift),
            "sh" | "bash" | "zsh" => Some(SupportedLang::Bash),
            "sql" => Some(SupportedLang::Sql),
            "dart" => Some(SupportedLang::Dart),
            "zig" => Some(SupportedLang::Zig),
            "lua" => Some(SupportedLang::Lua),
            "md" | "markdown" => Some(SupportedLang::Markdown),
            _ => None,
        }
    }

    pub(crate) fn name(&self) -> &'static str {
        match self {
            SupportedLang::Rust => "rust",
            SupportedLang::TypeScript => "typescript",
            SupportedLang::Tsx => "tsx",
            SupportedLang::JavaScript => "javascript",
            SupportedLang::Python => "python",
            SupportedLang::Go => "go",
            SupportedLang::C => "c",
            SupportedLang::Cpp => "cpp",
            SupportedLang::CSharp => "csharp",
            SupportedLang::Java => "java",
            SupportedLang::Kotlin => "kotlin",
            SupportedLang::Php => "php",
            SupportedLang::Ruby => "ruby",
            SupportedLang::Swift => "swift",
            SupportedLang::Bash => "bash",
            SupportedLang::Sql => "sql",
            SupportedLang::Dart => "dart",
            SupportedLang::Zig => "zig",
            SupportedLang::Lua => "lua",
            SupportedLang::Markdown => "markdown",
        }
    }

    pub(crate) fn language(&self) -> Language {
        match self {
            SupportedLang::Rust => Language::from(tree_sitter_rust::LANGUAGE),
            SupportedLang::TypeScript | SupportedLang::JavaScript => {
                Language::from(tree_sitter_typescript::LANGUAGE_TYPESCRIPT)
            }
            SupportedLang::Tsx => Language::from(tree_sitter_typescript::LANGUAGE_TSX),
            SupportedLang::Python => Language::from(tree_sitter_python::LANGUAGE),
            SupportedLang::Go => Language::from(tree_sitter_go::LANGUAGE),
            SupportedLang::C => Language::from(tree_sitter_c::LANGUAGE),
            SupportedLang::Cpp => Language::from(tree_sitter_cpp::LANGUAGE),
            SupportedLang::CSharp => Language::from(tree_sitter_c_sharp::LANGUAGE),
            SupportedLang::Java => Language::from(tree_sitter_java::LANGUAGE),
            SupportedLang::Kotlin => Language::from(tree_sitter_kotlin_ng::LANGUAGE),
            SupportedLang::Php => Language::from(tree_sitter_php::LANGUAGE_PHP),
            SupportedLang::Ruby => Language::from(tree_sitter_ruby::LANGUAGE),
            SupportedLang::Swift => Language::from(tree_sitter_swift::LANGUAGE),
            SupportedLang::Bash => Language::from(tree_sitter_bash::LANGUAGE),
            SupportedLang::Sql => Language::from(tree_sitter_sequel::LANGUAGE),
            SupportedLang::Dart => Language::from(tree_sitter_dart::LANGUAGE),
            SupportedLang::Zig => Language::from(tree_sitter_zig::LANGUAGE),
            SupportedLang::Lua => Language::from(tree_sitter_lua::LANGUAGE),
            SupportedLang::Markdown => Language::from(tree_sitter_md::LANGUAGE),
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
            SupportedLang::C => Box::new(COutline::new()),
            SupportedLang::Cpp => Box::new(CppOutline::new()),
            SupportedLang::CSharp => Box::new(CSharpOutline::new()),
            SupportedLang::Java => Box::new(JavaOutline::new()),
            SupportedLang::Kotlin => Box::new(KotlinOutline::new()),
            SupportedLang::Php => Box::new(PhpOutline::new()),
            SupportedLang::Ruby => Box::new(RubyOutline::new()),
            SupportedLang::Swift => Box::new(SwiftOutline::new()),
            SupportedLang::Bash => Box::new(BashOutline::new()),
            SupportedLang::Sql => Box::new(SqlOutline::new()),
            SupportedLang::Dart => Box::new(DartOutline::new()),
            SupportedLang::Zig => Box::new(ZigOutline::new()),
            SupportedLang::Lua => Box::new(LuaOutline::new()),
            SupportedLang::Markdown => Box::new(MarkdownOutline::new()),
        }
    }
}

/// Largest char boundary in `s` that is `<= index` (clamped to `s.len()`).
///
/// `String::truncate` panics when its index is not a char boundary, so any byte budget used
/// as a truncation point must be walked back first.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    crate::floor_char_boundary(s, index)
}

impl OutlineScanner {
    pub fn scan(req: &OutlineRequest) -> CoreResult<OutlineResponse> {
        let options = req.options.clone().unwrap_or_default();
        let max_symbols = options.max_symbols.unwrap_or(500);
        let max_files = options.max_files.unwrap_or(20);

        // 1. Direct in-memory content parsing
        if let Some(ref content) = req.content {
            let path_hint = req.path.as_deref().unwrap_or("snippet.rs");
            let lang =
                SupportedLang::from_path(Path::new(path_hint)).unwrap_or(SupportedLang::Rust);
            let mut file_outline =
                Self::parse_bytes(path_hint, content.as_bytes(), &lang, &options)?;

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

            if options.format == Some(OutlineFormat::Skeleton) {
                let skel = super::skeleton::SkeletonRenderer::render(
                    &file_outline.symbols,
                    &file_outline.language,
                    options.include_doc_comments != Some(false),
                );
                file_outline.skeleton = Some(skel);
                file_outline.symbols.clear();
            }

            let (files, truncated) =
                Self::apply_budget(vec![file_outline], max_symbols, options.max_output_bytes);

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
        // Set when the traversal stops before exhausting the tree, or found more files than
        // `max_files` will be parsed. `apply_budget` only sees the files that were selected,
        // so it cannot detect either case on its own.
        let mut file_budget_hit = false;

        if target_path.is_file() {
            candidate_files.push(target_path.to_path_buf());
        } else {
            // Traverse directory respecting .gitignore and include_hidden option
            let include_hidden = options.include_hidden.unwrap_or(false);
            let respect_gitignore = options.respect_gitignore.unwrap_or(true);
            let walker = WalkBuilder::new(target_path)
                .standard_filters(respect_gitignore)
                .git_ignore(respect_gitignore)
                .git_global(respect_gitignore)
                .git_exclude(respect_gitignore)
                .parents(respect_gitignore)
                .hidden(!include_hidden)
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
                        if let Ok(meta) = entry.metadata()
                            && meta.len() > 1_000_000
                        {
                            continue;
                        }
                        candidate_files.push(path.to_path_buf());
                        if candidate_files.len() >= max_files * 2 {
                            file_budget_hit = true;
                            break;
                        }
                    }
                }
            }
        }

        let candidate_count = candidate_files.len();

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

            if options.format == Some(OutlineFormat::Skeleton) {
                let skel = super::skeleton::SkeletonRenderer::render(
                    &outline.symbols,
                    &outline.language,
                    options.include_doc_comments != Some(false),
                );
                outline.skeleton = Some(skel);
                outline.symbols.clear();
            }

            file_outlines.push(outline);
        }

        summary.total_symbols = total_discovered_symbols;

        // More candidates were collected than will be parsed, so files were dropped.
        if candidate_count > max_files {
            file_budget_hit = true;
        }

        let (files, truncated) =
            Self::apply_budget(file_outlines, max_symbols, options.max_output_bytes);

        Ok(OutlineResponse {
            summary,
            files,
            // `truncated` is documented as "capped by symbol or file budget", so the
            // file-budget signal has to be folded in here: apply_budget only inspects the
            // selected files and returns false when their symbols happen to fit.
            truncated: truncated || file_budget_hit,
        })
    }

    pub(crate) fn parse_bytes(
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
            skeleton: None,
        })
    }

    fn prune_depth(symbols: &mut [Symbol], current_depth: usize, max_depth: usize) {
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
        max_output_bytes: Option<usize>,
    ) -> (Vec<FileOutline>, bool) {
        let mut symbols_remaining = max_symbols;
        let mut bytes_remaining = max_output_bytes.unwrap_or(usize::MAX);
        let mut budgeted_outlines = Vec::new();
        let mut truncated = false;

        for outline in outlines {
            if bytes_remaining == 0 || (outline.skeleton.is_none() && symbols_remaining == 0) {
                truncated = true;
                break;
            }

            if let Some(mut skel) = outline.skeleton {
                let skel_bytes = skel.len();
                if skel_bytes <= bytes_remaining {
                    bytes_remaining = bytes_remaining.saturating_sub(skel_bytes);
                    budgeted_outlines.push(FileOutline {
                        file: outline.file,
                        language: outline.language,
                        parse_status: outline.parse_status,
                        symbols: Vec::new(),
                        skeleton: Some(skel),
                    });
                } else {
                    truncated = true;
                    if bytes_remaining > 50 {
                        // Reserve room for the marker, then cut on a char boundary. Both
                        // halves matter: `String::truncate` panics off a char boundary, and
                        // counting the marker *after* truncating would overflow the caller's
                        // byte budget by the marker's length.
                        const MARKER: &str = "\n// ... [truncated]";
                        let content_budget = bytes_remaining.saturating_sub(MARKER.len());
                        let cut = floor_char_boundary(&skel, content_budget);
                        skel.truncate(cut);
                        skel.push_str(MARKER);
                        budgeted_outlines.push(FileOutline {
                            file: outline.file,
                            language: outline.language,
                            parse_status: outline.parse_status,
                            symbols: Vec::new(),
                            skeleton: Some(skel),
                        });
                    }
                    break;
                }
                continue;
            }

            let mut kept_symbols = Vec::new();
            for sym in outline.symbols {
                if symbols_remaining == 0 || bytes_remaining == 0 {
                    truncated = true;
                    break;
                }

                let cost = 1 + Self::count_symbols(&sym.children);
                let sym_bytes = serde_json::to_vec(&sym).map(|v| v.len()).unwrap_or(150);

                if cost <= symbols_remaining && sym_bytes <= bytes_remaining {
                    symbols_remaining -= cost;
                    bytes_remaining = bytes_remaining.saturating_sub(sym_bytes);
                    kept_symbols.push(sym);
                } else {
                    truncated = true;
                    if symbols_remaining > 0 {
                        let mut trimmed = sym;
                        trimmed.children.clear();
                        let trimmed_bytes =
                            serde_json::to_vec(&trimmed).map(|v| v.len()).unwrap_or(80);
                        if trimmed_bytes <= bytes_remaining {
                            symbols_remaining -= 1;
                            bytes_remaining = bytes_remaining.saturating_sub(trimmed_bytes);
                            kept_symbols.push(trimmed);
                        }
                    }
                    break;
                }
            }

            let was_empty = kept_symbols.is_empty();
            budgeted_outlines.push(FileOutline {
                file: outline.file,
                language: outline.language,
                parse_status: outline.parse_status,
                symbols: kept_symbols,
                skeleton: None,
            });

            if was_empty && truncated {
                break;
            }
        }

        (budgeted_outlines, truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use transcend_protocol::{OutlineFormat, OutlineOptions, OutlineRequest};

    /// A multi-byte source whose skeleton is long enough that most byte budgets land in
    /// the middle of a character. `content` is passed in memory, so no fixture file and no
    /// assumption about the process cwd is needed.
    fn non_ascii_skeleton_request(budget: usize) -> OutlineRequest {
        let jp = "日本語".repeat(120);
        let src = format!(
            "/// {jp}\npub fn alpha_one() -> u32 {{ 1 }}\n\
             /// {jp}\npub fn alpha_two() -> u32 {{ 2 }}\n\
             /// {jp}\npub fn alpha_three() -> u32 {{ 3 }}\n"
        );
        OutlineRequest {
            path: Some("src/lib.rs".to_string()),
            content: Some(src),
            options: Some(OutlineOptions {
                format: Some(OutlineFormat::Skeleton),
                max_output_bytes: Some(budget),
                ..Default::default()
            }),
        }
    }

    /// Regression: `skel.truncate(bytes_remaining)` used a byte budget as a `String` index
    /// and panicked off a char boundary, so on any file with multi-byte text the skeleton
    /// truncation path failed for the large majority of budgets. It must never panic and
    /// must always honour the budget.
    #[test]
    fn skeleton_truncation_never_panics_on_multibyte_source() {
        for budget in 51usize..2800 {
            let req = non_ascii_skeleton_request(budget);
            let response = OutlineScanner::scan(&req)
                .unwrap_or_else(|e| panic!("scan failed at budget {budget}: {e}"));

            for file in &response.files {
                let skel = file.skeleton.as_deref().unwrap_or("");
                assert!(
                    skel.len() <= budget,
                    "budget {budget} exceeded: skeleton was {} bytes",
                    skel.len()
                );
                assert!(
                    skel.starts_with("///"),
                    "budget {budget} produced a skeleton cut inside the leading doc comment"
                );
            }
        }
    }

    /// Regression: `truncated` is documented as "capped by symbol or file budget", but the
    /// walker collected `max_files * 2` candidates, parsed only `max_files`, and reported
    /// `truncated: false` whenever the selected files happened to fit the symbol/byte budget.
    /// An agent taking an architecture census would conclude the directory was smaller than
    /// it is.
    #[test]
    fn file_budget_truncation_is_reported() {
        let dir = std::env::temp_dir().join(format!(
            "transcend_outline_files_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // More files than max_files, each with a single symbol so the symbol budget never
        // bites -- only the file budget can be responsible for any truncation.
        let total = 25usize;
        for i in 0..total {
            std::fs::write(
                dir.join(format!("mod_{i:02}.rs")),
                format!("pub fn function_{i}() -> u32 {{ {i} }}\n"),
            )
            .unwrap();
        }

        let req = OutlineRequest {
            path: Some(dir.to_string_lossy().to_string()),
            content: None,
            options: Some(OutlineOptions {
                max_files: Some(20),
                max_symbols: Some(10_000),
                ..Default::default()
            }),
        };

        let res = OutlineScanner::scan(&req).expect("scan should succeed");
        assert_eq!(res.files.len(), 20, "only max_files are parsed");
        assert!(
            res.truncated,
            "5 files were dropped by the file budget, so truncated must be true"
        );

        // A directory that fits the budget must NOT be flagged.
        let small = OutlineRequest {
            path: Some(dir.to_string_lossy().to_string()),
            content: None,
            options: Some(OutlineOptions {
                max_files: Some(100),
                max_symbols: Some(10_000),
                ..Default::default()
            }),
        };
        let small_res = OutlineScanner::scan(&small).expect("scan should succeed");
        assert_eq!(small_res.files.len(), total);
        assert!(
            !small_res.truncated,
            "a complete census must not be flagged as truncated"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The truncation point must be walked back to a real char boundary, never beyond the
    /// budget, and never panic for indices past the end.
    #[test]
    fn floor_char_boundary_walks_back_and_clamps() {
        let s = "ab日本語"; // 2 + 3*3 = 11 bytes
        assert_eq!(floor_char_boundary(s, 0), 0);
        assert_eq!(floor_char_boundary(s, 2), 2, "ascii boundary is exact");
        assert_eq!(floor_char_boundary(s, 3), 2, "inside 日 -> back to 2");
        assert_eq!(floor_char_boundary(s, 4), 2, "inside 日 -> back to 2");
        assert_eq!(floor_char_boundary(s, 5), 5, "start of 本 is a boundary");
        assert_eq!(floor_char_boundary(s, 11), 11);
        assert_eq!(floor_char_boundary(s, 999), s.len(), "past end clamps");
        for i in 0..=s.len() {
            let cut = floor_char_boundary(s, i);
            assert!(cut <= i, "cut {cut} exceeded index {i}");
            assert!(s.is_char_boundary(cut), "cut {cut} is not a boundary");
        }
    }
}
