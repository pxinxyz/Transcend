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

        // A zero budget returned an empty census that looked like a complete one: with
        // `max_files: 0` the summary reported `total_files: 0` for a directory that plainly
        // contained files, and the response was indistinguishable from an empty directory.
        // Reject it instead of guessing between "none" and "unlimited".
        if options.max_files == Some(0) {
            return Err(CoreError::InvalidInput(
                "max_files must be at least 1; omit it to use the default of 20".to_string(),
            ));
        }
        if options.max_symbols == Some(0) {
            return Err(CoreError::InvalidInput(
                "max_symbols must be at least 1; omit it to use the default of 500".to_string(),
            ));
        }

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
        let mut symbols = adapter.extract(&tree, source, options);

        // Apply the symbol_kinds / exported_only filters centrally.
        //
        // Adapters are responsible for honouring these, and several did not: sql.rs and
        // markdown.rs read neither, while ruby.rs and bash.rs applied only symbol_kinds. A
        // caller asking for `symbol_kinds: ["function"]` against a SQL file received structs
        // too, with nothing in the response indicating the filter was skipped. Filtering here
        // makes it impossible for an adapter to forget.
        //
        // The predicates match what the adapters already use, so an adapter that does filter
        // yields the same result through this pass rather than being filtered twice
        // differently.
        Self::apply_symbol_filters(&mut symbols, options);

        Ok(FileOutline {
            file: display_path.to_string(),
            language: lang.name().to_string(),
            parse_status,
            symbols,
            skeleton: None,
        })
    }

    /// Recursively drop symbols excluded by `symbol_kinds` or `exported_only`.
    ///
    /// A container that no longer holds any matching descendant is dropped with it, so a
    /// filtered outline never advertises empty scaffolding.
    fn apply_symbol_filters(symbols: &mut Vec<Symbol>, options: &OutlineOptions) {
        for sym in symbols.iter_mut() {
            Self::apply_symbol_filters(&mut sym.children, options);
        }
        symbols.retain(|sym| Self::symbol_passes_filters(sym, options));
    }

    fn symbol_passes_filters(sym: &Symbol, options: &OutlineOptions) -> bool {
        if let Some(ref allowed) = options.symbol_kinds
            && !allowed.contains(&sym.kind)
        {
            return false;
        }
        if options.exported_only == Some(true) && sym.visibility.is_none() {
            return false;
        }
        true
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
        // Escapes rather than literal non-ASCII, so the fixture cannot be corrupted by an
        // editor or tool rewriting the file in a different encoding.
        let jp = "\u{65E5}\u{672C}\u{8A9E}".repeat(120);
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

    /// Regression: `symbol_kinds` and `exported_only` were applied by individual adapters,
    /// and several ignored them entirely (sql.rs and markdown.rs read neither, ruby.rs and
    /// bash.rs only `symbol_kinds`). A caller filtering for functions in those languages
    /// received every kind, with nothing indicating the filter was skipped.
    #[test]
    fn symbol_kinds_filter_applies_to_adapters_that_ignored_it() {
        let sql = b"CREATE TABLE users (id INT);\nCREATE FUNCTION add_one(x INT) RETURNS INT AS $$ SELECT x + 1; $$ LANGUAGE SQL;\n";
        let lang = SupportedLang::from_path(std::path::Path::new("schema.sql"))
            .expect("sql should be supported");

        // Unfiltered: both kinds present.
        let all = OutlineScanner::parse_bytes("schema.sql", sql, &lang, &OutlineOptions::default())
            .expect("parse should succeed");
        let all_kinds: Vec<String> = all
            .symbols
            .iter()
            .map(|s| format!("{:?}", s.kind).to_lowercase())
            .collect();
        assert!(
            all_kinds.len() >= 2,
            "fixture should yield several symbols, got {all_kinds:?}"
        );

        // Filtered to functions only.
        let filtered = OutlineScanner::parse_bytes(
            "schema.sql",
            sql,
            &lang,
            &OutlineOptions {
                symbol_kinds: Some(vec![transcend_protocol::SymbolKind::Function]),
                ..Default::default()
            },
        )
        .expect("parse should succeed");

        assert!(
            !filtered.symbols.is_empty(),
            "the fixture contains a function, so filtering must not remove everything"
        );
        for sym in &filtered.symbols {
            assert_eq!(
                sym.kind,
                transcend_protocol::SymbolKind::Function,
                "symbol_kinds filter leaked a {:?} symbol ({})",
                sym.kind,
                sym.name
            );
        }
        assert!(
            filtered.symbols.len() < all_kinds.len(),
            "the filter should have removed at least one symbol"
        );
    }

    /// `exported_only` must likewise be enforced centrally rather than per adapter. No
    /// adapter makes it easy to construct a clean end-to-end case (the bash adapter marks
    /// every function public, and the languages that ignore the flag populate no visibility
    /// at all), so this exercises the filter itself, which is what the central pass applies.
    #[test]
    fn exported_only_filter_drops_symbols_without_visibility() {
        fn sym(name: &str, kind: transcend_protocol::SymbolKind, vis: Option<&str>) -> Symbol {
            Symbol {
                name: name.to_string(),
                kind,
                span: transcend_protocol::SourceSpan {
                    start_line: 1,
                    start_col: 1,
                    end_line: 1,
                    end_col: 1,
                    start_byte: 0,
                    end_byte: 0,
                },
                signature: None,
                doc_comment: None,
                visibility: vis.map(|v| v.to_string()),
                relationships: vec![],
                children: vec![],
            }
        }

        let mut symbols = vec![
            sym(
                "public_fn",
                transcend_protocol::SymbolKind::Function,
                Some("public"),
            ),
            sym("private_fn", transcend_protocol::SymbolKind::Function, None),
            sym("a_struct", transcend_protocol::SymbolKind::Struct, None),
        ];
        // A public container holding a private child: the child must still be dropped.
        symbols[0].children.push(sym(
            "hidden_child",
            transcend_protocol::SymbolKind::Function,
            None,
        ));

        OutlineScanner::apply_symbol_filters(
            &mut symbols,
            &OutlineOptions {
                exported_only: Some(true),
                ..Default::default()
            },
        );

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["public_fn"], "got {names:?}");
        assert!(
            symbols[0].children.is_empty(),
            "a private child of a public symbol must also be dropped"
        );

        // Off by default: nothing is removed.
        let mut untouched = vec![
            sym("a", transcend_protocol::SymbolKind::Function, None),
            sym("b", transcend_protocol::SymbolKind::Struct, None),
        ];
        OutlineScanner::apply_symbol_filters(&mut untouched, &OutlineOptions::default());
        assert_eq!(untouched.len(), 2, "exported_only must default to off");
    }

    /// A container whose children are all filtered out is dropped with them, so a filtered
    /// outline never advertises empty scaffolding.
    #[test]
    fn symbol_kinds_filter_prunes_empty_containers() {
        fn fn_sym(name: &str, children: Vec<Symbol>) -> Symbol {
            Symbol {
                name: name.to_string(),
                kind: transcend_protocol::SymbolKind::Function,
                span: transcend_protocol::SourceSpan {
                    start_line: 1,
                    start_col: 1,
                    end_line: 1,
                    end_col: 1,
                    start_byte: 0,
                    end_byte: 0,
                },
                signature: None,
                doc_comment: None,
                visibility: None,
                relationships: vec![],
                children,
            }
        }
        fn struct_sym(name: &str, children: Vec<Symbol>) -> Symbol {
            Symbol {
                kind: transcend_protocol::SymbolKind::Struct,
                ..fn_sym(name, children)
            }
        }

        // A struct whose only child is a function.
        let mut symbols = vec![
            struct_sym("container", vec![fn_sym("inner", vec![])]),
            fn_sym("top_level", vec![]),
        ];

        OutlineScanner::apply_symbol_filters(
            &mut symbols,
            &OutlineOptions {
                symbol_kinds: Some(vec![transcend_protocol::SymbolKind::Function]),
                ..Default::default()
            },
        );

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["top_level"],
            "the struct does not match the kind filter and must be dropped with its child"
        );
    }
}
