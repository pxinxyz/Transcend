//! Symbol Reader
//!
//! Provides surgical extraction of specific code symbols by name or qualified locator,
//! returning the exact source code, span coordinates, doc comments, and surrounding context lines.

use std::fs;
use std::path::Path;

use transcend_protocol::{
    OutlineOptions, ReadSymbolRequest, ReadSymbolResponse, Symbol, SymbolKind,
};

use crate::{CoreError, CoreResult};
use super::scanner::{OutlineScanner, SupportedLang};

pub struct SymbolReader;

struct SymbolMatch<'a> {
    symbol: &'a Symbol,
    qualified_name: String,
    aliases: Vec<String>,
}

impl<'a> SymbolMatch<'a> {
    fn matches(&self, query: &str, query_norm: &str, kind_filter: Option<&SymbolKind>) -> bool {
        if let Some(kind) = kind_filter {
            if &self.symbol.kind != kind {
                return false;
            }
        }
        if self.qualified_name == query
            || self.qualified_name == query_norm
            || self.symbol.name == query
            || self.aliases.iter().any(|a| a == query || a == query_norm)
        {
            return true;
        }
        false
    }

    fn matches_case_insensitive(&self, query_lower: &str, query_norm_lower: &str, kind_filter: Option<&SymbolKind>) -> bool {
        if let Some(kind) = kind_filter {
            if &self.symbol.kind != kind {
                return false;
            }
        }
        if self.qualified_name.to_lowercase() == query_lower
            || self.qualified_name.to_lowercase() == query_norm_lower
            || self.symbol.name.to_lowercase() == query_lower
            || self.aliases.iter().any(|a| a.to_lowercase() == query_lower || a.to_lowercase() == query_norm_lower)
        {
            return true;
        }
        false
    }
}

impl SymbolReader {
    pub fn read(req: &ReadSymbolRequest) -> CoreResult<ReadSymbolResponse> {
        let (display_path, source) = if let Some(ref content) = req.content {
            (
                req.path.as_deref().unwrap_or("snippet.rs").to_string(),
                content.as_bytes().to_vec(),
            )
        } else if let Some(ref path_str) = req.path {
            let path = Path::new(path_str);
            if !path.exists() {
                return Err(CoreError::General(format!("File does not exist: {}", path.display())));
            }
            if !path.is_file() {
                return Err(CoreError::General(format!("Path is not a file: {}", path.display())));
            }
            let bytes = fs::read(path)?;
            (path_str.clone(), bytes)
        } else {
            return Err(CoreError::General(
                "Either 'path' or 'content' must be provided to read_symbol".to_string(),
            ));
        };

        let lang = SupportedLang::from_path(Path::new(&display_path)).unwrap_or(SupportedLang::Rust);
        let options = OutlineOptions {
            include_doc_comments: Some(true),
            include_relationships: Some(true),
            ..Default::default()
        };

        let outline = OutlineScanner::parse_bytes(&display_path, &source, &lang, &options)?;

        // Flatten all symbols with qualified paths
        let mut all_symbols = Vec::new();
        Self::collect_symbols(&outline.symbols, &[], &mut all_symbols);

        let query = req.symbol.trim();
        let query_normalized = query.replace('.', "::");

        // 1. Filter by exact symbol name or qualified name
        let mut matches: Vec<&SymbolMatch> = all_symbols
            .iter()
            .filter(|m| m.matches(query, &query_normalized, req.kind.as_ref()))
            .collect();

        // 2. Case-insensitive fallback if no exact matches found
        if matches.is_empty() {
            let query_lower = query.to_lowercase();
            let query_norm_lower = query_normalized.to_lowercase();
            matches = all_symbols
                .iter()
                .filter(|m| m.matches_case_insensitive(&query_lower, &query_norm_lower, req.kind.as_ref()))
                .collect();
        }

        let total_occurrences = matches.len();
        if total_occurrences == 0 {
            let available: Vec<String> = all_symbols
                .iter()
                .map(|m| m.qualified_name.clone())
                .take(15)
                .collect();
            let msg = if available.is_empty() {
                format!("No symbols found in {}", display_path)
            } else {
                format!(
                    "Symbol '{}' not found in {}. Available symbols: {}",
                    query,
                    display_path,
                    available.join(", ")
                )
            };
            return Ok(ReadSymbolResponse {
                found: false,
                file: Some(display_path),
                total_occurrences: 0,
                message: Some(msg),
                ..Default::default()
            });
        }

        let occ_index = req.occurrence.unwrap_or(0);
        let target_match = if occ_index < matches.len() {
            matches[occ_index]
        } else {
            return Ok(ReadSymbolResponse {
                found: false,
                file: Some(display_path),
                total_occurrences,
                message: Some(format!(
                    "Occurrence {} out of range (found {} match{}) for '{}'",
                    occ_index,
                    total_occurrences,
                    if total_occurrences == 1 { "" } else { "es" },
                    query
                )),
                ..Default::default()
            });
        };

        let sym = target_match.symbol;
        let start_b = sym.span.start_byte;
        let end_b = sym.span.end_byte.min(source.len());

        let source_code = if start_b <= end_b && end_b <= source.len() {
            String::from_utf8_lossy(&source[start_b..end_b]).to_string()
        } else {
            String::new()
        };

        // Context lines before and after
        let (context_before, context_after) = if let Some(n) = req.context_lines {
            if n > 0 {
                Self::extract_context_lines(&source, sym.span.start_line, sym.span.end_line, n)
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        Ok(ReadSymbolResponse {
            found: true,
            file: Some(display_path),
            qualified_name: Some(target_match.qualified_name.clone()),
            symbol: Some(sym.clone()),
            source_code: Some(source_code),
            context_before,
            context_after,
            total_occurrences,
            message: None,
        })
    }

    fn collect_symbols<'a>(
        symbols: &'a [Symbol],
        parent_prefixes: &[String],
        results: &mut Vec<SymbolMatch<'a>>,
    ) {
        for sym in symbols {
            let mut current_prefixes = Vec::new();

            if sym.kind == SymbolKind::Implementation {
                if let Some(target) = sym.relationships.iter().find(|r| r.relation == "targets").map(|r| &r.target) {
                    current_prefixes.push(target.clone());
                } else if let Some(stripped) = sym.name.strip_prefix("impl ") {
                    let target = stripped.split_whitespace().last().unwrap_or(stripped);
                    current_prefixes.push(target.to_string());
                }
                current_prefixes.push(sym.name.clone());
            } else {
                current_prefixes.push(sym.name.clone());
            }

            let primary_prefix = parent_prefixes.first();
            let qualified_name = match primary_prefix {
                Some(p) if !p.is_empty() => format!("{}::{}", p, sym.name),
                _ => sym.name.clone(),
            };

            let mut aliases = Vec::new();
            for p in parent_prefixes {
                let alias = format!("{}::{}", p, sym.name);
                if alias != qualified_name && !aliases.contains(&alias) {
                    aliases.push(alias);
                }
            }

            if let Some(trait_target) = sym.relationships.iter().find(|r| r.relation == "implements").map(|r| &r.target) {
                let trait_alias = format!("{}::{}", trait_target, sym.name);
                if !aliases.contains(&trait_alias) {
                    aliases.push(trait_alias);
                }
            }

            results.push(SymbolMatch {
                symbol: sym,
                qualified_name: qualified_name.clone(),
                aliases,
            });

            let child_prefixes: Vec<String> = if parent_prefixes.is_empty() {
                current_prefixes
            } else {
                let mut combined = Vec::new();
                for p in parent_prefixes {
                    for c in &current_prefixes {
                        combined.push(format!("{}::{}", p, c));
                    }
                }
                combined
            };

            Self::collect_symbols(&sym.children, &child_prefixes, results);
        }
    }

    fn extract_context_lines(
        source: &[u8],
        start_line: usize,
        end_line: usize,
        n: usize,
    ) -> (Option<String>, Option<String>) {
        let text = String::from_utf8_lossy(source);
        let lines: Vec<&str> = text.lines().collect();
        let total = lines.len();

        let before = if start_line > 1 && n > 0 {
            let from = start_line.saturating_sub(1 + n);
            let to = start_line.saturating_sub(1);
            if from < to && from < total {
                let slice = &lines[from..to.min(total)];
                Some(slice.join("\n"))
            } else {
                None
            }
        } else {
            None
        };

        let after = if end_line < total && n > 0 {
            let from = end_line;
            let to = (end_line + n).min(total);
            if from < to {
                let slice = &lines[from..to];
                Some(slice.join("\n"))
            } else {
                None
            }
        } else {
            None
        };

        (before, after)
    }
}
