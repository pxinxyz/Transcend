//! Global Project-Wide Symbol Finder
//!
//! Provides fast, two-stage definition discovery across 18 languages:
//! 1. Fast parallel ripgrep candidate pre-filter to prune non-matching files in milliseconds.
//! 2. In-process Tree-sitter AST parsing on candidate files to extract exact definitions,
//!    signatures, spans, docstrings, and qualified hierarchies.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use grep_regex::RegexMatcherBuilder;
use grep_searcher::{BinaryDetection, SearcherBuilder, Sink, SinkMatch};
use ignore::overrides::OverrideBuilder;
use ignore::{WalkBuilder, WalkState};

use transcend_protocol::{FindSymbolRequest, FindSymbolResponse, FoundSymbol, OutlineOptions};

use crate::outline::scanner::{OutlineScanner, SupportedLang};
use crate::outline::symbol_reader::{SymbolReader, tokenize_identifier};
use crate::{CoreError, CoreResult};

pub struct SymbolFinder;

struct CandidateSink {
    matched: bool,
}

impl Sink for CandidateSink {
    type Error = std::io::Error;

    fn matched(
        &mut self,
        _searcher: &grep_searcher::Searcher,
        _mat: &SinkMatch<'_>,
    ) -> Result<bool, Self::Error> {
        self.matched = true;
        // Stop searching this file as soon as at least 1 match is found
        Ok(false)
    }
}

impl SymbolFinder {
    pub fn find(req: &FindSymbolRequest) -> CoreResult<FindSymbolResponse> {
        let root_str = req.path.as_deref().unwrap_or(".");
        let root_path = Path::new(root_str);

        if !root_path.exists() {
            return Err(CoreError::General(format!(
                "Search path does not exist: {}",
                root_path.display()
            )));
        }

        let query = req.name.trim();
        if query.is_empty() {
            return Ok(FindSymbolResponse {
                query: String::new(),
                total_found: 0,
                symbols: Vec::new(),
                kind_breakdown: BTreeMap::new(),
                language_breakdown: BTreeMap::new(),
                truncated: false,
            });
        }

        let exact = req.exact.unwrap_or(true);
        let fuzzy = req.fuzzy.unwrap_or(!exact);
        let case_sensitive = req.case_sensitive.unwrap_or(false);
        let limit = req.limit.unwrap_or(20);

        // Normalize query for qualified lookups
        let query_normalized = query.replace('.', "::");
        // Leaf name for Stage 1 word-boundary search
        let leaf = query_normalized.rsplit("::").next().unwrap_or(query).trim();
        let query_tokens = tokenize_identifier(leaf);

        // -------------------------------------------------------------
        // STAGE 1: Candidate File Discovery (Fast Ripgrep Pre-Filter)
        // -------------------------------------------------------------
        let pattern = if query_tokens.len() > 1 {
            if fuzzy {
                query_tokens
                    .iter()
                    .map(|t| regex::escape(t))
                    .collect::<Vec<_>>()
                    .join(".*")
            } else {
                let joined_sep = query_tokens
                    .iter()
                    .map(|t| regex::escape(t))
                    .collect::<Vec<_>>()
                    .join("[_-]?");
                format!(r"({}|{})", regex::escape(leaf), joined_sep)
            }
        } else if exact {
            if leaf.chars().all(|c| c.is_alphanumeric() || c == '_') {
                format!(r"\b{}\b", regex::escape(leaf))
            } else {
                regex::escape(leaf)
            }
        } else {
            regex::escape(leaf)
        };

        let matcher = Arc::new(
            RegexMatcherBuilder::new()
                .case_insensitive(!case_sensitive)
                .multi_line(false)
                .build(&pattern)
                .map_err(|e| CoreError::InvalidPattern(e.to_string()))?,
        );

        let include_ignored = req.include_ignored.unwrap_or(false);
        let include_hidden = req.include_hidden.unwrap_or(false);
        let mut walk_builder = WalkBuilder::new(root_path);
        walk_builder
            .hidden(!include_hidden)
            .git_ignore(!include_ignored)
            .git_global(!include_ignored)
            .git_exclude(!include_ignored)
            .parents(!include_ignored);

        if let Some(ref file_pat) = req.file_pattern {
            let mut override_builder = OverrideBuilder::new(root_path);
            override_builder
                .add(file_pat)
                .map_err(|e| CoreError::InvalidPattern(e.to_string()))?;
            let overrides = override_builder
                .build()
                .map_err(|e| CoreError::InvalidPattern(e.to_string()))?;
            walk_builder.overrides(overrides);
        }

        let candidate_files = Arc::new(Mutex::new(Vec::new()));
        let walk_parallel = walk_builder.build_parallel();

        walk_parallel.run(|| {
            let matcher = Arc::clone(&matcher);
            let candidate_files = Arc::clone(&candidate_files);

            let mut searcher = SearcherBuilder::new()
                .binary_detection(BinaryDetection::quit(0x00))
                .bom_sniffing(true)
                .build();

            Box::new(move |result| {
                let entry = match result {
                    Ok(entry) => entry,
                    Err(_) => return WalkState::Continue,
                };

                let path = entry.path();
                if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                    return WalkState::Continue;
                }

                // Check language extension
                if SupportedLang::from_path(path).is_none() {
                    return WalkState::Continue;
                }

                let mut sink = CandidateSink { matched: false };
                let _ = searcher.search_path(&*matcher, path, &mut sink);

                if sink.matched
                    && let Ok(mut c) = candidate_files.lock()
                {
                    c.push(path.to_path_buf());
                }

                WalkState::Continue
            })
        });

        let mut files = Arc::try_unwrap(candidate_files)
            .unwrap_or_else(|m| Mutex::new(m.lock().unwrap().clone()))
            .into_inner()
            .unwrap_or_default();

        files.sort();

        // -------------------------------------------------------------
        // STAGE 2: In-Process Tree-sitter AST Extraction & Matching
        // -------------------------------------------------------------
        let outline_options = OutlineOptions {
            include_doc_comments: Some(true),
            include_relationships: Some(true),
            ..Default::default()
        };

        let mut found_symbols = Vec::new();
        let mut kind_breakdown: BTreeMap<String, usize> = BTreeMap::new();
        let mut language_breakdown: BTreeMap<String, usize> = BTreeMap::new();
        let mut total_matches = 0;

        let query_lower = query.to_lowercase();
        let query_norm_lower = query_normalized.to_lowercase();
        let max_candidates_limit = (limit * 10).max(200);

        for file_path in &files {
            let lang = match SupportedLang::from_path(file_path) {
                Some(l) => l,
                None => continue,
            };

            let bytes = match fs::read(file_path) {
                Ok(b) => b,
                Err(_) => continue,
            };

            let display_path = if let Ok(rel) = file_path.strip_prefix(root_path) {
                rel.to_string_lossy().replace('\\', "/")
            } else {
                file_path.to_string_lossy().replace('\\', "/")
            };

            let outline =
                match OutlineScanner::parse_bytes(&display_path, &bytes, &lang, &outline_options) {
                    Ok(o) => o,
                    Err(_) => continue,
                };

            let mut all_symbols = Vec::new();
            SymbolReader::collect_symbols(&outline.symbols, &[], &mut all_symbols);

            for sym_match in all_symbols {
                let is_exact_match = if case_sensitive {
                    sym_match.matches(query, &query_normalized, req.kind.as_ref())
                } else {
                    sym_match.matches_case_insensitive(
                        &query_lower,
                        &query_norm_lower,
                        req.kind.as_ref(),
                    ) || sym_match.matches_token_casing(&query_tokens, req.kind.as_ref())
                };

                let is_partial_match = (!exact || fuzzy)
                    && (sym_match.matches_partial(&query_lower, req.kind.as_ref())
                        || sym_match.matches_fuzzy_subsequence(&query_tokens, req.kind.as_ref()));

                if is_exact_match || is_partial_match {
                    total_matches += 1;
                    let sym = sym_match.symbol;
                    let kind_str = format!("{:?}", sym.kind).to_lowercase();
                    let lang_str = lang.name().to_string();

                    *kind_breakdown.entry(kind_str).or_insert(0) += 1;
                    *language_breakdown.entry(lang_str.clone()).or_insert(0) += 1;

                    if found_symbols.len() < max_candidates_limit {
                        found_symbols.push(FoundSymbol {
                            name: sym.name.clone(),
                            qualified_name: sym_match.qualified_name.clone(),
                            kind: sym.kind,
                            file: display_path.clone(),
                            language: lang_str,
                            span: sym.span.clone(),
                            signature: sym.signature.clone(),
                            doc_comment: sym.doc_comment.clone(),
                            visibility: sym.visibility.clone(),
                            is_exact: is_exact_match,
                        });
                    }
                }
            }
        }

        // Sort: exact matches first, then shorter names, then alphabetical by file, then line number
        found_symbols.sort_by(|a, b| {
            b.is_exact
                .cmp(&a.is_exact)
                .then_with(|| a.name.len().cmp(&b.name.len()))
                .then_with(|| a.file.cmp(&b.file))
                .then_with(|| a.span.start_line.cmp(&b.span.start_line))
        });

        let truncated = total_matches > limit;
        found_symbols.truncate(limit);

        Ok(FindSymbolResponse {
            query: query.to_string(),
            total_found: total_matches,
            symbols: found_symbols,
            kind_breakdown,
            language_breakdown,
            truncated,
        })
    }
}
