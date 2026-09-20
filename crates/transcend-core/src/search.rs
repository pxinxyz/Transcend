//! In-process code search engine powered by ripgrep primitives.
//!
//! Provides fast, parallel recursive code search respecting `.gitignore`,
//! with NUL-byte binary detection, lossy UTF-8 decoding, atomic match budgeting,
//! cross-file diversity sampling, context-line harvesting, and directory radar clustering.

use grep_regex::RegexMatcherBuilder;
use grep_searcher::{BinaryDetection, SearcherBuilder, Sink, SinkContext, SinkMatch};
use ignore::overrides::OverrideBuilder;
use ignore::{WalkBuilder, WalkState};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use transcend_protocol::{
    DirectoryRadar, FileCluster, SearchMatch, SearchOptions, SearchRequest, SearchResponse,
};

use crate::{CoreError, CoreResult};

/// Default maximum number of line matches to collect before truncation.
pub const DEFAULT_MAX_MATCHES: usize = 50;

/// Default maximum character length of an extracted line.
pub const DEFAULT_MAX_LINE_LENGTH: usize = 500;

/// Safety cutoff for total match aggregation to prevent unbounded memory on pathological inputs.
pub const MAX_SAFETY_MATCH_LIMIT: usize = 500_000;

/// In-process search scanner with parallel traversal and adaptive compaction.
pub struct SearchScanner;

impl SearchScanner {
    /// Execute multi-threaded search against the filesystem with adaptive budgeting.
    pub fn scan(req: &SearchRequest) -> CoreResult<SearchResponse> {
        let root_str = req.path.as_deref().unwrap_or(".");
        let root_path = Path::new(root_str);

        if !root_path.exists() {
            return Err(CoreError::General(format!(
                "Search path does not exist: {}",
                root_path.display()
            )));
        }

        let default_options = SearchOptions::default();
        let opts = req.options.as_ref().unwrap_or(&default_options);

        // Build regex matcher (falls back to literal escaped pattern on syntax error for agent resilience)
        let case_insensitive = !opts.case_sensitive.unwrap_or(false);
        let matcher = match RegexMatcherBuilder::new()
            .case_insensitive(case_insensitive)
            .multi_line(false)
            .build(&req.pattern)
        {
            Ok(m) => Arc::new(m),
            Err(_) => {
                let escaped = regex::escape(&req.pattern);
                Arc::new(
                    RegexMatcherBuilder::new()
                        .case_insensitive(case_insensitive)
                        .multi_line(false)
                        .build(&escaped)
                        .map_err(|e| CoreError::InvalidPattern(e.to_string()))?,
                )
            }
        };

        // Build file walker
        let include_hidden = opts.include_hidden.unwrap_or(false);
        let respect_gitignore = opts.respect_gitignore.unwrap_or(true);
        let mut walk_builder = WalkBuilder::new(root_path);
        walk_builder
            .hidden(!include_hidden)
            .git_ignore(respect_gitignore)
            .git_global(respect_gitignore)
            .git_exclude(respect_gitignore)
            .parents(respect_gitignore);

        // Apply optional file pattern glob
        if let Some(pattern) = &opts.file_pattern {
            let mut override_builder = OverrideBuilder::new(root_path);
            override_builder
                .add(pattern)
                .map_err(|e| CoreError::InvalidPattern(e.to_string()))?;
            let overrides = override_builder
                .build()
                .map_err(|e| CoreError::InvalidPattern(e.to_string()))?;
            walk_builder.overrides(overrides);
        }

        // A count budget of zero has no sensible meaning and behaves inconsistently: it was
        // silently ignored for `max_matches` (all matches returned) while `max_files: 0` on
        // outline produced an empty result indistinguishable from an empty directory. Reject
        // it rather than guess which reading the caller intended.
        if opts.max_matches == Some(0) {
            return Err(CoreError::InvalidInput(
                "max_matches must be at least 1; omit it to use the default of 50".to_string(),
            ));
        }
        if opts.max_per_file == Some(0) {
            return Err(CoreError::InvalidInput(
                "max_per_file must be at least 1; omit it to default to max_matches".to_string(),
            ));
        }

        let max_matches = opts.max_matches.unwrap_or(DEFAULT_MAX_MATCHES);
        let max_per_file = opts.max_per_file.unwrap_or(max_matches);
        let max_line_length = opts.max_line_length.unwrap_or(DEFAULT_MAX_LINE_LENGTH);
        let context_lines = opts.context_lines.unwrap_or(0);

        let total_matches = Arc::new(AtomicUsize::new(0));
        let clusters = Arc::new(Mutex::new(Vec::new()));

        let root_path_buf = root_path.to_path_buf();
        let walk_parallel = walk_builder.build_parallel();

        walk_parallel.run(|| {
            let matcher = Arc::clone(&matcher);
            let total_matches = Arc::clone(&total_matches);
            let clusters = Arc::clone(&clusters);
            let root_path_buf = root_path_buf.clone();

            let mut searcher_builder = SearcherBuilder::new();
            searcher_builder
                .binary_detection(BinaryDetection::quit(0x00))
                .bom_sniffing(true)
                .line_number(true);

            if context_lines > 0 {
                searcher_builder.before_context(context_lines);
                searcher_builder.after_context(context_lines);
            }

            let mut searcher = searcher_builder.build();

            Box::new(move |entry_result| {
                if total_matches.load(Ordering::Relaxed) >= MAX_SAFETY_MATCH_LIMIT {
                    return WalkState::Quit;
                }

                let entry = match entry_result {
                    Ok(ent) => ent,
                    Err(err) => {
                        tracing::debug!(error = %err, "Error reading directory entry");
                        return WalkState::Continue;
                    }
                };

                // Only process regular files
                if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                    return WalkState::Continue;
                }

                let path = entry.path();
                let relative_path = path
                    .strip_prefix(&root_path_buf)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");
                // `FileCluster.file` is documented as "relative to search root". When `path`
                // names a single file the root IS that file, so stripping the prefix yields ""
                // and there is no relative form to report. Fall back to the file's own NAME
                // rather than its absolute path: an absolute path is not relative to anything,
                // and it made `search` disagree with `find` and `outline`, which both report
                // `root.rs` for the same file under a directory root. Reporting the basename
                // keeps the field's meaning consistent, and the name is the part a caller
                // actually uses -- the file it asked about.
                let relative_path = if relative_path.is_empty() {
                    path.file_name()
                        .map(|n| n.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"))
                } else {
                    relative_path
                };

                let mut local_matches: Vec<SearchMatch> = Vec::new();
                let mut file_match_count: usize = 0;

                let mut collector = ThreadMatchCollector {
                    max_matches,
                    max_per_file,
                    max_line_length,
                    context_lines,
                    local_matches: &mut local_matches,
                    file_match_count: &mut file_match_count,
                    total_matches: &total_matches,
                    pending_before: Vec::new(),
                    pending_after_count: 0,
                };

                if let Err(err) = searcher.search_path(&*matcher, path, &mut collector) {
                    tracing::debug!(path = %path.display(), error = %err, "Search failed for path");
                }

                if file_match_count > 0
                    && let Ok(mut c_guard) = clusters.lock()
                {
                    c_guard.push(FileCluster {
                        file: relative_path,
                        match_count: file_match_count,
                        matches: local_matches,
                        // Decided below, once the global budget is known.
                        matches_truncated: false,
                    });
                }

                WalkState::Continue
            })
        });

        let total = total_matches.load(Ordering::SeqCst);
        let mut files = Arc::try_unwrap(clusters)
            .map(|c| c.into_inner().unwrap_or_default())
            .unwrap_or_else(|c| c.lock().unwrap().clone());

        // Sort files by match_count descending, then file path ascending
        files.sort_by(|a, b| {
            b.match_count
                .cmp(&a.match_count)
                .then_with(|| a.file.cmp(&b.file))
        });

        // Ensure each file's matches are sorted by line number ascending
        for file in &mut files {
            file.matches.sort_by_key(|m| m.line_number);
        }

        // Apply global max_matches budget across files.
        //
        // Clusters are ordered by density before this point, so the most concentrated
        // files keep their text. A cluster whose text is dropped still reports its exact
        // `match_count`, and `matches_truncated` records that the omission was the budget
        // rather than an absence of matches — without it a consumer cannot tell an empty
        // cluster from an exhausted one.
        let mut accumulated_matches = 0;
        for file in &mut files {
            if accumulated_matches >= max_matches {
                if !file.matches.is_empty() {
                    file.matches.clear();
                }
                file.matches_truncated = file.match_count > 0;
            } else if accumulated_matches + file.matches.len() > max_matches {
                let allowed = max_matches - accumulated_matches;
                file.matches.truncate(allowed);
                accumulated_matches += allowed;
                file.matches_truncated = file.match_count > file.matches.len();
            } else {
                accumulated_matches += file.matches.len();
                // A file can also be short of its count when `max_per_file` capped it.
                file.matches_truncated = file.match_count > file.matches.len();
            }
        }

        // Compute macro-level directory radar
        let mut dir_map: HashMap<String, (usize, usize)> = HashMap::new();
        for file_cluster in &files {
            let dir = match Path::new(&file_cluster.file).parent() {
                Some(p) if !p.as_os_str().is_empty() => p.to_string_lossy().replace('\\', "/"),
                _ => ".".to_string(),
            };
            let entry = dir_map.entry(dir).or_insert((0, 0));
            entry.0 += 1; // file_count
            entry.1 += file_cluster.match_count; // match_count
        }

        let mut directory_radar: Vec<DirectoryRadar> = dir_map
            .into_iter()
            .map(|(directory, (file_count, match_count))| DirectoryRadar {
                directory,
                file_count,
                match_count,
            })
            .collect();

        directory_radar.sort_by(|a, b| {
            b.match_count
                .cmp(&a.match_count)
                .then_with(|| a.directory.cmp(&b.directory))
        });

        let total_files = files.len();
        // `truncated` is documented as "individual line matches were capped due to the match
        // budget". The global count only reflects `max_matches`, so a `max_per_file` cap --
        // which also drops line text -- was invisible here even though each affected cluster
        // set `matches_truncated`. Capture the per-cluster truth before pruning reorders it.
        let truncated = total > max_matches || files.iter().any(|f| f.matches_truncated);

        // Prune excess empty file clusters when max_empty_clusters is specified
        if let Some(max_empty_clusters) = opts.max_empty_clusters {
            let mut empty_count = 0usize;
            files.retain(|f| {
                if f.matches.is_empty() {
                    if empty_count < max_empty_clusters {
                        empty_count += 1;
                        true
                    } else {
                        false
                    }
                } else {
                    true
                }
            });
        }

        Ok(SearchResponse {
            total_matches: total,
            total_files,
            files,
            directory_radar,
            truncated,
            // The walker quits once the safety ceiling is reached, so the observed
            // total can never exceed it. Reaching it means the count is a lower bound
            // whose exact value depends on traversal scheduling.
            count_capped: total >= MAX_SAFETY_MATCH_LIMIT,
        })
    }
}

/// Lossy line decoder with Unicode-safe character truncation.
fn format_line(raw_bytes: &[u8], max_len: usize) -> String {
    let lossy = String::from_utf8_lossy(raw_bytes);
    let trimmed = lossy.trim_end_matches(['\r', '\n']);
    let char_count = trimmed.chars().count();
    if char_count > max_len {
        let truncated: String = trimmed.chars().take(max_len).collect();
        let omitted = char_count - max_len;
        format!("{}... [truncated {} chars]", truncated, omitted)
    } else {
        trimmed.to_string()
    }
}

/// Thread-local sink that collects matches for a single file into local buffers.
struct ThreadMatchCollector<'a> {
    max_matches: usize,
    max_per_file: usize,
    max_line_length: usize,
    context_lines: usize,
    local_matches: &'a mut Vec<SearchMatch>,
    file_match_count: &'a mut usize,
    total_matches: &'a AtomicUsize,
    pending_before: Vec<String>,
    pending_after_count: usize,
}

impl<'a> Sink for ThreadMatchCollector<'a> {
    type Error = std::io::Error;

    fn matched(
        &mut self,
        _searcher: &grep_searcher::Searcher,
        mat: &SinkMatch<'_>,
    ) -> Result<bool, Self::Error> {
        self.total_matches.fetch_add(1, Ordering::Relaxed);
        *self.file_match_count += 1;

        // Counting is cheap; decoding and formatting a line is not. Once this file has
        // filled its share of the budget the match is counted for the directory radar
        // but its text is deliberately never materialized. The traversal still visits
        // every file (so `total_matches` and `directory_radar` stay truthful), but the
        // per-match UTF-8 decode, truncation and allocation are skipped for the
        // overwhelming majority of matches in a low-budget query.
        let within_budget = self.local_matches.len() < self.max_per_file
            && self.local_matches.len() < self.max_matches;

        if within_budget {
            let line_number = mat.line_number().unwrap_or(0) as usize;
            let line_text = format_line(mat.bytes(), self.max_line_length);

            let context_before = std::mem::take(&mut self.pending_before);
            self.pending_after_count = self.context_lines;

            self.local_matches.push(SearchMatch {
                line_number,
                line_text,
                context_before,
                context_after: Vec::new(),
            });
        } else {
            // Dropping the pending context is what keeps `context_lines` from doing
            // formatting work for matches that will never be returned.
            self.pending_before.clear();
            self.pending_after_count = 0;
        }

        Ok(true)
    }

    fn context(
        &mut self,
        _searcher: &grep_searcher::Searcher,
        ctx: &SinkContext<'_>,
    ) -> Result<bool, Self::Error> {
        // Context is only ever attached to a retained match, so skip the formatting
        // entirely when the budget is exhausted (the common case in a large work tree).
        if self.pending_after_count == 0
            && (self.context_lines == 0
                || (self.local_matches.len() >= self.max_per_file
                    || self.local_matches.len() >= self.max_matches))
        {
            return Ok(true);
        }

        let line_text = format_line(ctx.bytes(), self.max_line_length);
        if self.pending_after_count > 0 {
            if let Some(last) = self.local_matches.last_mut() {
                last.context_after.push(line_text);
                self.pending_after_count -= 1;
            }
        } else if self.context_lines > 0 {
            self.pending_before.push(line_text);
            if self.pending_before.len() > self.context_lines {
                self.pending_before.remove(0);
            }
        }
        Ok(true)
    }
}
