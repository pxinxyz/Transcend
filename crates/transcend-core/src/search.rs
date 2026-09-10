//! In-process code search engine powered by ripgrep primitives.
//!
//! Provides fast, single-threaded (V1) recursive code search respecting `.gitignore`,
//! with NUL-byte binary detection, lossy UTF-8 decoding, and token-budget bounding.

use std::path::Path;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{BinaryDetection, SearcherBuilder, Sink, SinkMatch};
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use transcend_protocol::{FileCluster, MatchItem, SearchRequest, SearchResponse};

use crate::{CoreError, CoreResult};

/// Default maximum number of line matches to collect before truncation.
pub const DEFAULT_MAX_MATCHES: usize = 50;

/// In-process search scanner.
pub struct SearchScanner;

impl SearchScanner {
    /// Execute search against the filesystem.
    pub fn scan(req: &SearchRequest) -> CoreResult<SearchResponse> {
        let root_str = req.path.as_deref().unwrap_or(".");
        let root_path = Path::new(root_str);

        if !root_path.exists() {
            return Err(CoreError::General(format!(
                "Search path does not exist: {}",
                root_path.display()
            )));
        }

        // Build regex matcher
        let case_insensitive = !req.case_sensitive.unwrap_or(false);
        let matcher = RegexMatcherBuilder::new()
            .case_insensitive(case_insensitive)
            .multi_line(false)
            .build(&req.pattern)
            .map_err(|e| CoreError::InvalidPattern(e.to_string()))?;

        // Build file walker
        let mut walk_builder = WalkBuilder::new(root_path);
        walk_builder
            .hidden(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .parents(true);

        // Apply optional file pattern glob
        if let Some(pattern) = &req.file_pattern {
            let mut override_builder = OverrideBuilder::new(root_path);
            override_builder
                .add(pattern)
                .map_err(|e| CoreError::InvalidPattern(e.to_string()))?;
            let overrides = override_builder
                .build()
                .map_err(|e| CoreError::InvalidPattern(e.to_string()))?;
            walk_builder.overrides(overrides);
        }

        let max_matches = req.max_matches.unwrap_or(DEFAULT_MAX_MATCHES);
        let mut collected_matches: Vec<MatchItem> = Vec::new();
        let mut clusters: Vec<FileCluster> = Vec::new();
        let mut total_matches = 0;

        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(0x00))
            .bom_sniffing(true)
            .line_number(true)
            .build();

        for entry_result in walk_builder.build() {
            let entry = match entry_result {
                Ok(ent) => ent,
                Err(err) => {
                    tracing::debug!(error = %err, "Error reading directory entry");
                    continue;
                }
            };

            // Only process regular files
            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                continue;
            }

            let path = entry.path();
            let relative_path = path
                .strip_prefix(root_path)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");

            let mut file_match_count = 0;
            let mut collector = MatchCollector {
                file_path: &relative_path,
                max_matches,
                matches: &mut collected_matches,
                file_match_count: &mut file_match_count,
                total_matches: &mut total_matches,
            };

            if let Err(err) = searcher.search_path(&matcher, path, &mut collector) {
                tracing::debug!(path = %path.display(), error = %err, "Search failed for path");
            }

            if file_match_count > 0 {
                clusters.push(FileCluster {
                    file: relative_path,
                    match_count: file_match_count,
                });
            }
        }

        let truncated = total_matches > max_matches;

        Ok(SearchResponse {
            total_matches,
            matches: collected_matches,
            clusters,
            truncated,
        })
    }
}

/// Custom sink that collects matches into strongly-typed items.
struct MatchCollector<'a> {
    file_path: &'a str,
    max_matches: usize,
    matches: &'a mut Vec<MatchItem>,
    file_match_count: &'a mut usize,
    total_matches: &'a mut usize,
}

impl<'a> Sink for MatchCollector<'a> {
    type Error = std::io::Error;

    fn matched(
        &mut self,
        _searcher: &grep_searcher::Searcher,
        mat: &SinkMatch<'_>,
    ) -> Result<bool, Self::Error> {
        *self.total_matches += 1;
        *self.file_match_count += 1;

        if self.matches.len() < self.max_matches {
            let line_number = mat.line_number().unwrap_or(0) as usize;
            // Lossy UTF-8 decoding to guarantee no panics on non-UTF-8 / Latin-1 text files
            let raw_bytes = mat.bytes();
            let line_str = String::from_utf8_lossy(raw_bytes);
            let line_trimmed = line_str.trim_end_matches(['\r', '\n']).to_string();

            self.matches.push(MatchItem {
                file: self.file_path.to_string(),
                line_number,
                line_text: line_trimmed,
            });
        }

        Ok(true)
    }
}
