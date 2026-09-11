//! In-process code search engine powered by ripgrep primitives.
//!
//! Provides fast, parallel recursive code search respecting `.gitignore`,
//! with NUL-byte binary detection, lossy UTF-8 decoding, atomic match budgeting,
//! cross-file diversity sampling, context-line harvesting, and directory radar clustering.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{BinaryDetection, SearcherBuilder, Sink, SinkContext, SinkMatch};
use ignore::overrides::OverrideBuilder;
use ignore::{WalkBuilder, WalkState};
use transcend_protocol::{
    DirectoryCluster, FileCluster, MatchItem, SearchRequest, SearchResponse,
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

        // Build regex matcher
        let case_insensitive = !req.case_sensitive.unwrap_or(false);
        let matcher = Arc::new(
            RegexMatcherBuilder::new()
                .case_insensitive(case_insensitive)
                .multi_line(false)
                .build(&req.pattern)
                .map_err(|e| CoreError::InvalidPattern(e.to_string()))?,
        );

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
        let max_per_file = req.max_per_file.unwrap_or(max_matches);
        let max_line_length = req.max_line_length.unwrap_or(DEFAULT_MAX_LINE_LENGTH);
        let context_lines = req.context_lines.unwrap_or(0);

        let total_matches = Arc::new(AtomicUsize::new(0));
        let collected_matches = Arc::new(Mutex::new(Vec::new()));
        let clusters = Arc::new(Mutex::new(Vec::new()));

        let root_path_buf = root_path.to_path_buf();
        let walk_parallel = walk_builder.build_parallel();

        walk_parallel.run(|| {
            let matcher = Arc::clone(&matcher);
            let total_matches = Arc::clone(&total_matches);
            let collected_matches = Arc::clone(&collected_matches);
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

                let mut local_matches: Vec<MatchItem> = Vec::new();
                let mut file_match_count: usize = 0;

                let mut collector = ThreadMatchCollector {
                    file_path: &relative_path,
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

                if file_match_count > 0 {
                    // Record file cluster summary
                    if let Ok(mut c_guard) = clusters.lock() {
                        c_guard.push(FileCluster {
                            file: relative_path,
                            match_count: file_match_count,
                        });
                    }

                    // Flush local matches to shared collector up to budget cap
                    if !local_matches.is_empty() {
                        if let Ok(mut m_guard) = collected_matches.lock() {
                            if m_guard.len() < max_matches {
                                let remaining = max_matches - m_guard.len();
                                m_guard.extend(local_matches.into_iter().take(remaining));
                            }
                        }
                    }
                }

                WalkState::Continue
            })
        });

        let total = total_matches.load(Ordering::SeqCst);
        let mut matches = Arc::try_unwrap(collected_matches)
            .map(|m| m.into_inner().unwrap_or_default())
            .unwrap_or_else(|m| m.lock().unwrap().clone());
        let mut clusters = Arc::try_unwrap(clusters)
            .map(|c| c.into_inner().unwrap_or_default())
            .unwrap_or_else(|c| c.lock().unwrap().clone());

        // Deterministic sorting across concurrent worker completions:
        // 1. Line matches ordered by file path then line number
        matches.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then_with(|| a.line_number.cmp(&b.line_number))
        });
        matches.truncate(max_matches);

        // 2. Clusters ordered by match_count descending (dense files first)
        clusters.sort_by(|a, b| {
            b.match_count
                .cmp(&a.match_count)
                .then_with(|| a.file.cmp(&b.file))
        });

        // 3. Compute macro-level directory clusters (Directory Radar)
        let mut dir_map: HashMap<String, (usize, usize)> = HashMap::new();
        for cluster in &clusters {
            let dir = match Path::new(&cluster.file).parent() {
                Some(p) if !p.as_os_str().is_empty() => p.to_string_lossy().replace('\\', "/"),
                _ => ".".to_string(),
            };
            let entry = dir_map.entry(dir).or_insert((0, 0));
            entry.0 += 1; // file_count
            entry.1 += cluster.match_count; // match_count
        }

        let mut directory_clusters: Vec<DirectoryCluster> = dir_map
            .into_iter()
            .map(|(directory, (file_count, match_count))| DirectoryCluster {
                directory,
                file_count,
                match_count,
            })
            .collect();

        directory_clusters.sort_by(|a, b| {
            b.match_count
                .cmp(&a.match_count)
                .then_with(|| a.directory.cmp(&b.directory))
        });

        let total_files = clusters.len();
        let truncated = total > max_matches;

        Ok(SearchResponse {
            total_matches: total,
            total_files,
            matches,
            clusters,
            directory_clusters,
            truncated,
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
    file_path: &'a str,
    max_matches: usize,
    max_per_file: usize,
    max_line_length: usize,
    context_lines: usize,
    local_matches: &'a mut Vec<MatchItem>,
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

        if self.local_matches.len() < self.max_per_file && self.local_matches.len() < self.max_matches {
            let line_number = mat.line_number().unwrap_or(0) as usize;
            let line_text = format_line(mat.bytes(), self.max_line_length);

            let context_before = std::mem::take(&mut self.pending_before);
            self.pending_after_count = self.context_lines;

            self.local_matches.push(MatchItem {
                file: self.file_path.to_string(),
                line_number,
                line_text,
                context_before,
                context_after: Vec::new(),
            });
        } else {
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
