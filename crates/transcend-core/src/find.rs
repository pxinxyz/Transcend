//! In-process file discovery engine powered by ignore traversal.
//!
//! Fast, parallel filesystem traversal respecting `.gitignore`, with glob and name
//! pattern matching, file-type filtering, depth bounding, and macro directory radar.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use globset::{GlobBuilder, GlobMatcher};
use ignore::{WalkBuilder, WalkState};
use transcend_protocol::{DirectoryRadar, FindOptions, FindRequest, FindResponse};

use crate::{CoreError, CoreResult};

/// Default maximum number of paths to collect before truncation.
pub const DEFAULT_MAX_FIND_RESULTS: usize = 100;

/// Safety cutoff for total match aggregation.
pub const MAX_SAFETY_FIND_LIMIT: usize = 500_000;

/// In-process file discovery scanner.
pub struct FindScanner;

impl FindScanner {
    /// Execute file discovery traversal against the filesystem.
    pub fn scan(req: &FindRequest) -> CoreResult<FindResponse> {
        let root_str = req.path.as_deref().unwrap_or(".");
        let root_path = Path::new(root_str);

        if !root_path.exists() {
            return Err(CoreError::General(format!(
                "Search path does not exist: {}",
                root_path.display()
            )));
        }

        let default_options = FindOptions::default();
        let opts = req.options.as_ref().unwrap_or(&default_options);

        let max_results = opts.max_results.unwrap_or(DEFAULT_MAX_FIND_RESULTS);
        let case_sensitive = opts.case_sensitive.unwrap_or(false);
        let file_type_filter = opts.file_type.as_deref().unwrap_or("file");
        let extension_filter = opts
            .extension
            .as_deref()
            .map(|e| e.trim_start_matches('.').to_lowercase());

        // Prepare pattern matcher if provided
        let pattern_matcher: Option<PatternFilter> = match &req.pattern {
            Some(pat) if !pat.is_empty() && pat != "*" => {
                let case_insensitive = !case_sensitive;
                // If pattern contains glob metacharacters, compile as GlobMatcher
                if pat.contains('*') || pat.contains('?') || pat.contains('[') {
                    let glob = GlobBuilder::new(pat)
                        .case_insensitive(case_insensitive)
                        .literal_separator(false)
                        .build()
                        .map_err(|e| CoreError::InvalidPattern(e.to_string()))?;
                    Some(PatternFilter::Glob(glob.compile_matcher()))
                } else if case_sensitive {
                    Some(PatternFilter::ExactSubstring(pat.clone()))
                } else {
                    Some(PatternFilter::CaseInsensitiveSubstring(pat.to_lowercase()))
                }
            }
            _ => None,
        };

        // Build file walker
        let mut walk_builder = WalkBuilder::new(root_path);
        walk_builder
            .hidden(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .parents(true);

        if let Some(depth) = opts.max_depth {
            walk_builder.max_depth(Some(depth));
        }

        let total_count = Arc::new(AtomicUsize::new(0));
        let collected_paths = Arc::new(Mutex::new(Vec::new()));
        let dir_counts = Arc::new(Mutex::new(HashMap::new()));

        let root_path_buf = root_path.to_path_buf();
        let walk_parallel = walk_builder.build_parallel();

        walk_parallel.run(|| {
            let total_count = Arc::clone(&total_count);
            let collected_paths = Arc::clone(&collected_paths);
            let dir_counts = Arc::clone(&dir_counts);
            let root_path_buf = root_path_buf.clone();
            let pattern_matcher = pattern_matcher.clone();
            let extension_filter = extension_filter.clone();

            Box::new(move |entry_result| {
                if total_count.load(Ordering::Relaxed) >= MAX_SAFETY_FIND_LIMIT {
                    return WalkState::Quit;
                }

                let entry = match entry_result {
                    Ok(ent) => ent,
                    Err(err) => {
                        tracing::debug!(error = %err, "Error reading directory entry");
                        return WalkState::Continue;
                    }
                };

                // Skip the root directory itself (depth 0)
                if entry.depth() == 0 {
                    return WalkState::Continue;
                }

                let file_type = entry.file_type();
                let is_dir = file_type.map(|ft| ft.is_dir()).unwrap_or(false);
                let is_file = file_type.map(|ft| ft.is_file()).unwrap_or(false);

                // Apply file type filter
                match file_type_filter {
                    "dir" | "directory" => {
                        if !is_dir {
                            return WalkState::Continue;
                        }
                    }
                    "any" => {
                        if !is_file && !is_dir {
                            return WalkState::Continue;
                        }
                    }
                    _ => {
                        // Default: "file"
                        if !is_file {
                            return WalkState::Continue;
                        }
                    }
                }

                let path = entry.path();

                // Apply extension filter
                if let Some(ref ext_filter) = extension_filter {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase())
                        .unwrap_or_default();
                    if &ext != ext_filter {
                        return WalkState::Continue;
                    }
                }

                let file_name = entry.file_name().to_string_lossy();
                let relative_path = path
                    .strip_prefix(&root_path_buf)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");

                // Apply pattern filter
                if let Some(ref matcher) = pattern_matcher {
                    let matched = match matcher {
                        PatternFilter::Glob(g) => g.is_match(&*file_name) || g.is_match(&*relative_path),
                        PatternFilter::ExactSubstring(sub) => file_name.contains(sub) || relative_path.contains(sub),
                        PatternFilter::CaseInsensitiveSubstring(sub) => {
                            file_name.to_lowercase().contains(sub)
                                || relative_path.to_lowercase().contains(sub)
                        }
                    };
                    if !matched {
                        return WalkState::Continue;
                    }
                }

                total_count.fetch_add(1, Ordering::Relaxed);

                // Record parent directory for radar
                let parent_dir = match Path::new(&relative_path).parent() {
                    Some(p) if !p.as_os_str().is_empty() => p.to_string_lossy().replace('\\', "/"),
                    _ => ".".to_string(),
                };

                if let Ok(mut d_guard) = dir_counts.lock() {
                    *d_guard.entry(parent_dir).or_insert(0) += 1;
                }

                if let Ok(mut p_guard) = collected_paths.lock() {
                    p_guard.push(relative_path);
                }

                WalkState::Continue
            })
        });

        let total = total_count.load(Ordering::SeqCst);
        let mut paths = Arc::try_unwrap(collected_paths)
            .map(|p| p.into_inner().unwrap_or_default())
            .unwrap_or_else(|p| p.lock().unwrap().clone());
        let dir_map = Arc::try_unwrap(dir_counts)
            .map(|d| d.into_inner().unwrap_or_default())
            .unwrap_or_else(|d| d.lock().unwrap().clone());

        // Deterministic alphabetical sorting
        paths.sort();

        let truncated = total > max_results;
        paths.truncate(max_results);

        // Compute directory radar
        let mut directory_radar: Vec<DirectoryRadar> = dir_map
            .into_iter()
            .map(|(directory, count)| DirectoryRadar {
                directory,
                file_count: count,
                match_count: count,
            })
            .collect();

        directory_radar.sort_by(|a, b| {
            b.match_count
                .cmp(&a.match_count)
                .then_with(|| a.directory.cmp(&b.directory))
        });

        Ok(FindResponse {
            total_count: total,
            paths,
            directory_radar,
            truncated,
        })
    }
}

/// Pattern matching strategy for filenames and paths.
#[derive(Clone)]
enum PatternFilter {
    Glob(GlobMatcher),
    ExactSubstring(String),
    CaseInsensitiveSubstring(String),
}
