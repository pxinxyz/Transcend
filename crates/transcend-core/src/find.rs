//! In-process file discovery engine powered by ignore traversal.
//!
//! Fast, parallel filesystem traversal respecting `.gitignore`, with glob and name
//! pattern matching, file-type filtering, depth bounding, directory diversity quotas,
//! recency/size sorting, metadata extraction, dynamic exclusions, and extension censuses.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use chrono::{DateTime, SecondsFormat, Utc};
use globset::{GlobBuilder, GlobMatcher, GlobSet, GlobSetBuilder};
use ignore::{WalkBuilder, WalkState};
use transcend_protocol::{DirectoryRadar, FindOptions, FindRequest, FindResponse, PathEntry};

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

        // 1. Prepare positive pattern matcher if provided
        let pattern_matcher: Option<PatternFilter> = match &req.pattern {
            Some(pat) if !pat.is_empty() && pat != "*" => {
                let case_insensitive = !case_sensitive;
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

        // 2. Prepare dynamic exclude matcher if provided
        let exclude_matcher: Option<GlobSet> = if let Some(excludes) = &opts.exclude {
            if !excludes.is_empty() {
                let mut builder = GlobSetBuilder::new();
                for pattern in excludes {
                    let glob = GlobBuilder::new(pattern)
                        .case_insensitive(true)
                        .literal_separator(false)
                        .build()
                        .map_err(|e| CoreError::InvalidPattern(e.to_string()))?;
                    builder.add(glob);
                }
                Some(builder.build().map_err(|e| CoreError::InvalidPattern(e.to_string()))?)
            } else {
                None
            }
        } else {
            None
        };

        // 3. Build file walker
        let include_hidden = opts.include_hidden.unwrap_or(false);
        let mut walk_builder = WalkBuilder::new(root_path);
        walk_builder
            .hidden(!include_hidden)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .parents(true);

        if let Some(depth) = opts.max_depth {
            walk_builder.max_depth(Some(depth));
        }

        let total_count = Arc::new(AtomicUsize::new(0));
        let collected_raw = Arc::new(Mutex::new(Vec::new()));
        let dir_counts = Arc::new(Mutex::new(HashMap::new()));
        let extension_counts = Arc::new(Mutex::new(HashMap::new()));

        let root_path_buf = root_path.to_path_buf();
        let walk_parallel = walk_builder.build_parallel();

        walk_parallel.run(|| {
            let total_count = Arc::clone(&total_count);
            let collected_raw = Arc::clone(&collected_raw);
            let dir_counts = Arc::clone(&dir_counts);
            let extension_counts = Arc::clone(&extension_counts);
            let root_path_buf = root_path_buf.clone();
            let pattern_matcher = pattern_matcher.clone();
            let exclude_matcher = exclude_matcher.clone();
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

                // Skip root directory itself (depth 0)
                if entry.depth() == 0 {
                    return WalkState::Continue;
                }

                let file_type = entry.file_type();
                let is_dir = file_type.map(|ft| ft.is_dir()).unwrap_or(false);
                let is_file = file_type.map(|ft| ft.is_file()).unwrap_or(false);

                // File type filter
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
                let file_name = entry.file_name().to_string_lossy();
                let relative_path = path
                    .strip_prefix(&root_path_buf)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");

                // Dynamic exclusion check
                if let Some(ref matcher) = exclude_matcher {
                    if matcher.is_match(&*relative_path) || matcher.is_match(&*file_name) {
                        return WalkState::Continue;
                    }
                }

                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_lowercase())
                    .unwrap_or_default();

                // Extension filter check
                if let Some(ref ext_filter) = extension_filter {
                    if &ext != ext_filter {
                        return WalkState::Continue;
                    }
                }

                // Positive pattern filter check
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

                // Extract metadata: size and modification time
                let metadata = entry.metadata().ok();
                let size_bytes = if is_file {
                    metadata.as_ref().map(|m| m.len()).unwrap_or(0)
                } else {
                    0
                };
                let modified_system = metadata.and_then(|m| m.modified().ok());
                let modified_iso = modified_system.map(|st| {
                    let dt: DateTime<Utc> = st.into();
                    dt.to_rfc3339_opts(SecondsFormat::Secs, true)
                });

                // Update extension census
                if is_file && !ext.is_empty() {
                    if let Ok(mut e_guard) = extension_counts.lock() {
                        *e_guard.entry(ext).or_insert(0) += 1;
                    }
                }

                // Record parent directory for radar
                let parent_dir = match Path::new(&relative_path).parent() {
                    Some(p) if !p.as_os_str().is_empty() => p.to_string_lossy().replace('\\', "/"),
                    _ => ".".to_string(),
                };

                if let Ok(mut d_guard) = dir_counts.lock() {
                    *d_guard.entry(parent_dir.clone()).or_insert(0) += 1;
                }

                if let Ok(mut p_guard) = collected_raw.lock() {
                    p_guard.push(RawPathEntry {
                        path: relative_path,
                        parent_dir,
                        size_bytes,
                        modified_system,
                        modified_iso,
                    });
                }

                WalkState::Continue
            })
        });

        let total = total_count.load(Ordering::SeqCst);
        let mut raw_entries = Arc::try_unwrap(collected_raw)
            .map(|p| p.into_inner().unwrap_or_default())
            .unwrap_or_else(|p| p.lock().unwrap().clone());
        let dir_map = Arc::try_unwrap(dir_counts)
            .map(|d| d.into_inner().unwrap_or_default())
            .unwrap_or_else(|d| d.lock().unwrap().clone());
        let ext_map = Arc::try_unwrap(extension_counts)
            .map(|e| e.into_inner().unwrap_or_default())
            .unwrap_or_else(|e| e.lock().unwrap().clone());

        // 4. Sorting
        match opts.sort_by.as_deref() {
            Some("modified") => {
                raw_entries.sort_by(|a, b| {
                    b.modified_system
                        .cmp(&a.modified_system)
                        .then_with(|| a.path.cmp(&b.path))
                });
            }
            Some("size") => {
                raw_entries.sort_by(|a, b| {
                    b.size_bytes
                        .cmp(&a.size_bytes)
                        .then_with(|| a.path.cmp(&b.path))
                });
            }
            _ => {
                // Default: "path" (alphabetical)
                raw_entries.sort_by(|a, b| a.path.cmp(&b.path));
            }
        }

        // 5. Apply per-directory diversity quota (max_per_dir) & max_results budget
        let max_per_dir = opts.max_per_dir;
        let mut dir_yield_count: HashMap<String, usize> = HashMap::new();
        let mut entries: Vec<PathEntry> = Vec::new();

        for item in raw_entries {
            if let Some(mpd) = max_per_dir {
                let count = dir_yield_count.entry(item.parent_dir.clone()).or_insert(0);
                if *count >= mpd {
                    continue;
                }
                *count += 1;
            }

            entries.push(PathEntry {
                path: item.path,
                size_bytes: item.size_bytes,
                modified: item.modified_iso,
            });

            if entries.len() >= max_results {
                break;
            }
        }

        let truncated = total > max_results || entries.len() < total;

        // 6. Build directory radar
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

        // 7. Build tech-stack extension breakdown
        let extension_breakdown: BTreeMap<String, usize> = ext_map.into_iter().collect();

        Ok(FindResponse {
            total_count: total,
            entries,
            directory_radar,
            extension_breakdown,
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

/// Raw discovered path entry before sorting, diversity filtering, and serialization.
#[derive(Clone, Debug)]
struct RawPathEntry {
    path: String,
    parent_dir: String,
    size_bytes: u64,
    modified_system: Option<SystemTime>,
    modified_iso: Option<String>,
}
