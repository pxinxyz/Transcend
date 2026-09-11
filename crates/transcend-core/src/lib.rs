//! Transcend Core Engine
//!
//! High-performance, in-process computational primitives for code search,
//! file discovery, AST outlining, and surgical transformations.

pub mod find;
pub mod search;

use thiserror::Error;
use transcend_protocol::{
    FindRequest, FindResponse, OutlineRequest, OutlineResponse, SearchRequest, SearchResponse,
};

use crate::find::FindScanner;
use crate::search::SearchScanner;

/// Core engine errors.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Pattern error: {0}")]
    InvalidPattern(String),

    #[error("Parsing error in {file}: {message}")]
    ParseError { file: String, message: String },

    #[error("Operation failed: {0}")]
    General(String),
}

/// Result alias for Core operations.
pub type CoreResult<T> = Result<T, CoreError>;

/// Core execution engine trait.
pub trait Engine: Send + Sync {
    /// Execute a code search.
    fn search(&self, req: &SearchRequest) -> CoreResult<SearchResponse>;

    /// Execute a file discovery traversal.
    fn find(&self, req: &FindRequest) -> CoreResult<FindResponse>;

    /// Extract an AST outline of a source file.
    fn outline(&self, req: &OutlineRequest) -> CoreResult<OutlineResponse>;
}

/// Default in-process engine implementation.
#[derive(Debug, Default, Clone)]
pub struct NativeEngine;

impl NativeEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Engine for NativeEngine {
    fn search(&self, req: &SearchRequest) -> CoreResult<SearchResponse> {
        SearchScanner::scan(req)
    }

    fn find(&self, req: &FindRequest) -> CoreResult<FindResponse> {
        FindScanner::scan(req)
    }

    fn outline(&self, req: &OutlineRequest) -> CoreResult<OutlineResponse> {
        // Skeleton placeholder implementation
        tracing::debug!(path = ?req.path, "Executing skeleton outline");
        Ok(OutlineResponse {
            summary: Default::default(),
            files: vec![],
            truncated: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use super::*;
    use transcend_protocol::{FindOptions, SearchOptions};

    struct TestSandbox {
        dir: PathBuf,
    }

    impl TestSandbox {
        fn create() -> Self {
            let id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!("transcend_test_{}", id));
            fs::create_dir_all(&dir).unwrap();

            // 1. Regular UTF-8 file
            let src_dir = dir.join("src");
            fs::create_dir_all(&src_dir).unwrap();
            fs::write(
                src_dir.join("main.rs"),
                "fn hello_world() {\n    println!(\"Hello\");\n}\n",
            )
            .unwrap();

            // 2. Binary file with NUL byte
            fs::write(
                dir.join("image.bin"),
                &[b'f', b'n', 0x00, b'b', b'i', b'n', 0x00],
            )
            .unwrap();

            // 3. Non-UTF-8 Latin-1 text file with 0xE9 (é in Latin-1, invalid standalone in UTF-8)
            fs::write(
                dir.join("latin1.txt"),
                &[b'f', b'n', b' ', b'c', 0xE9, b'l', b'e', b'b', b'r', b'e', b'\n'],
            )
            .unwrap();

            // 4. Repeated match file for budget testing
            let mut budget_content = String::new();
            for i in 0..10 {
                budget_content.push_str(&format!("item_{}: target_hit\n", i));
            }
            fs::write(dir.join("budget.txt"), budget_content).unwrap();

            Self { dir }
        }

        fn path_str(&self) -> String {
            self.dir.to_string_lossy().to_string()
        }
    }

    impl Drop for TestSandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn test_search_basic_and_line_mapping() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .search(&SearchRequest {
                pattern: "hello_world".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    case_sensitive: Some(true),
                    max_matches: Some(50),
                    ..Default::default()
                }),
            })
            .expect("search should succeed");

        assert_eq!(res.total_matches, 1);
        assert_eq!(res.files.len(), 1);
        assert_eq!(res.files[0].matches.len(), 1);
        assert_eq!(res.files[0].matches[0].line_number, 1);
        assert!(res.files[0].matches[0].line_text.contains("fn hello_world()"));
        assert!(!res.truncated);
    }

    #[test]
    fn test_search_case_sensitivity() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Case-insensitive (should find)
        let res_ci = engine
            .search(&SearchRequest {
                pattern: "HELLO_WORLD".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    case_sensitive: Some(false),
                    ..Default::default()
                }),
            })
            .unwrap();
        assert_eq!(res_ci.total_matches, 1);

        // Case-sensitive (should not find)
        let res_cs = engine
            .search(&SearchRequest {
                pattern: "HELLO_WORLD".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    case_sensitive: Some(true),
                    ..Default::default()
                }),
            })
            .unwrap();
        assert_eq!(res_cs.total_matches, 0);
    }

    #[test]
    fn test_search_skips_binary_with_nul_bytes() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Search for 'bin' which exists only in image.bin
        let res = engine
            .search(&SearchRequest {
                pattern: "bin".to_string(),
                path: Some(sandbox.path_str()),
                options: None,
            })
            .unwrap();

        // Binary file must have been skipped by quit(0x00)
        assert_eq!(res.total_matches, 0);
    }

    #[test]
    fn test_search_handles_invalid_utf8_without_panic() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Search for 'fn' in latin1.txt which contains 0xE9
        let res = engine
            .search(&SearchRequest {
                pattern: "fn".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    file_pattern: Some("latin1.txt".to_string()),
                    ..Default::default()
                }),
            })
            .expect("should not panic on invalid UTF-8 bytes");

        assert_eq!(res.total_matches, 1);
        assert_eq!(res.files.len(), 1);
        assert_eq!(res.files[0].matches.len(), 1);
        // Lossy UTF-8 turns 0xE9 into replacement char 
        assert!(res.files[0].matches[0].line_text.contains("c\u{FFFD}lebre"));
    }

    #[test]
    fn test_search_budget_clipping_and_clustering() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .search(&SearchRequest {
                pattern: "target_hit".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    max_matches: Some(3),
                    ..Default::default()
                }),
            })
            .unwrap();

        // Total matches is 10, but returned items is capped at 3
        assert_eq!(res.total_matches, 10);
        assert_eq!(res.files.len(), 1);
        assert_eq!(res.files[0].match_count, 10);
        assert_eq!(res.files[0].matches.len(), 3);
        assert!(res.truncated);
    }

    #[test]
    fn test_search_concurrency_and_deterministic_ordering() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Create 20 files across subdirectories with variable hit counts
        let mut expected_total = 0;
        for i in 1..=20 {
            let sub = sandbox.dir.join(format!("sub_{}", i % 4));
            fs::create_dir_all(&sub).unwrap();
            let mut file_content = String::new();
            for line_idx in 0..i {
                file_content.push_str(&format!("noise line {}\n", line_idx));
                file_content.push_str("concurrent_needle_marker\n");
                expected_total += 1;
            }
            fs::write(sub.join(format!("file_{:02}.txt", i)), file_content).unwrap();
        }

        let res = engine
            .search(&SearchRequest {
                pattern: "concurrent_needle_marker".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    case_sensitive: Some(true),
                    max_matches: Some(25),
                    ..Default::default()
                }),
            })
            .expect("concurrent search should succeed");

        assert_eq!(res.total_matches, expected_total);
        assert!(res.truncated);

        // Verify total matches across files is capped at 25
        let total_inlined_matches: usize = res.files.iter().map(|f| f.matches.len()).sum();
        assert_eq!(total_inlined_matches, 25);

        // Verify files are sorted by match_count descending
        assert_eq!(res.files.len(), 20);
        for window in res.files.windows(2) {
            let a = &window[0];
            let b = &window[1];
            assert!(
                a.match_count >= b.match_count,
                "Files must be sorted by match_count descending: {} vs {}",
                a.match_count,
                b.match_count
            );
        }
        // Top file cluster must have 20 matches (from file_20)
        assert_eq!(res.files[0].match_count, 20);

        // Verify line matches in each file are sorted by line_number ascending
        for file in &res.files {
            for window in file.matches.windows(2) {
                assert!(window[0].line_number <= window[1].line_number);
            }
        }
    }

    #[test]
    fn test_search_max_per_file_diversity() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // monster file with 20 hits
        let mut monster_content = String::new();
        for i in 0..20 {
            monster_content.push_str(&format!("monster_line_{}: diversity_needle\n", i));
        }
        fs::write(sandbox.dir.join("monster.txt"), monster_content).unwrap();

        // regular file with 5 hits
        let mut regular_content = String::new();
        for i in 0..5 {
            regular_content.push_str(&format!("regular_line_{}: diversity_needle\n", i));
        }
        fs::write(sandbox.dir.join("regular.txt"), regular_content).unwrap();

        let res = engine
            .search(&SearchRequest {
                pattern: "diversity_needle".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    max_matches: Some(10),
                    max_per_file: Some(3),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.total_matches, 25);
        assert_eq!(res.total_files, 2);

        let monster = res.files.iter().find(|f| f.file.contains("monster.txt")).unwrap();
        let regular = res.files.iter().find(|f| f.file.contains("regular.txt")).unwrap();
        assert_eq!(monster.match_count, 20);
        assert_eq!(monster.matches.len(), 3);
        assert_eq!(regular.match_count, 5);
        assert_eq!(regular.matches.len(), 3);
    }

    #[test]
    fn test_search_line_length_truncation() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Generate 1000 char line with needle
        let long_line = format!("long_prefix_{}_long_suffix\n", "x".repeat(1000));
        fs::write(sandbox.dir.join("long.txt"), long_line).unwrap();

        let res = engine
            .search(&SearchRequest {
                pattern: "long_prefix".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    file_pattern: Some("long.txt".to_string()),
                    max_line_length: Some(40),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.files.len(), 1);
        assert_eq!(res.files[0].matches.len(), 1);
        assert!(res.files[0].matches[0].line_text.contains("[truncated"));
    }

    #[test]
    fn test_search_directory_clusters() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let billing = sandbox.dir.join("billing");
        let auth = sandbox.dir.join("auth");
        fs::create_dir_all(&billing).unwrap();
        fs::create_dir_all(&auth).unwrap();

        fs::write(billing.join("invoice.rs"), "cluster_radar\ncluster_radar\n").unwrap();
        fs::write(billing.join("payment.rs"), "cluster_radar\n").unwrap();
        fs::write(auth.join("token.rs"), "cluster_radar\n").unwrap();

        let res = engine
            .search(&SearchRequest {
                pattern: "cluster_radar".to_string(),
                path: Some(sandbox.path_str()),
                options: None,
            })
            .unwrap();

        assert_eq!(res.total_matches, 4);
        assert_eq!(res.total_files, 3);

        // billing has 2 files and 3 matches; auth has 1 file and 1 match
        assert!(res.directory_radar.len() >= 2);
        let billing_radar = res
            .directory_radar
            .iter()
            .find(|d| d.directory.contains("billing"))
            .unwrap();
        assert_eq!(billing_radar.file_count, 2);
        assert_eq!(billing_radar.match_count, 3);

        let auth_radar = res
            .directory_radar
            .iter()
            .find(|d| d.directory.contains("auth"))
            .unwrap();
        assert_eq!(auth_radar.file_count, 1);
        assert_eq!(auth_radar.match_count, 1);
    }

    #[test]
    fn test_search_context_lines() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let content = "line 1 before\nline 2 before\nmatched_needle_target\nline 4 after\nline 5 after\n";
        fs::write(sandbox.dir.join("context.txt"), content).unwrap();

        let res = engine
            .search(&SearchRequest {
                pattern: "matched_needle_target".to_string(),
                path: Some(sandbox.path_str()),
                options: Some(SearchOptions {
                    file_pattern: Some("context.txt".to_string()),
                    context_lines: Some(2),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.files.len(), 1);
        assert_eq!(res.files[0].matches.len(), 1);
        let m = &res.files[0].matches[0];
        assert_eq!(m.context_before, vec!["line 1 before", "line 2 before"]);
        assert_eq!(m.context_after, vec!["line 4 after", "line 5 after"]);
    }

    #[test]
    fn test_find_all_files() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: None,
            })
            .expect("find all files should succeed");

        assert_eq!(res.total_count, 4);
        assert_eq!(res.entries.len(), 4);
        assert!(!res.truncated);
        assert!(res.entries.iter().any(|e| e.path.ends_with("main.rs")));
        assert!(res.entries.iter().any(|e| e.path.ends_with("budget.txt")));

        // Metadata check
        for entry in &res.entries {
            assert!(entry.size_bytes > 0);
            assert!(entry.modified.is_some());
        }

        // Extension breakdown check
        assert_eq!(res.extension_breakdown.get("rs"), Some(&1));
        assert_eq!(res.extension_breakdown.get("txt"), Some(&2));
        assert_eq!(res.extension_breakdown.get("bin"), Some(&1));
    }

    #[test]
    fn test_find_glob_pattern() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .find(&FindRequest {
                pattern: Some("*.rs".to_string()),
                path: Some(sandbox.path_str()),
                options: None,
            })
            .unwrap();

        assert_eq!(res.total_count, 1);
        assert_eq!(res.entries.len(), 1);
        assert_eq!(res.entries[0].path, "src/main.rs");
    }

    #[test]
    fn test_find_extension_filter() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: Some(FindOptions {
                    extension: Some("txt".to_string()),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.total_count, 2);
        assert!(res.entries.iter().all(|e| e.path.ends_with(".txt")));
    }

    #[test]
    fn test_find_max_depth() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // max_depth 1 should only return files in root sandbox, not in src/main.rs
        let res = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: Some(FindOptions {
                    max_depth: Some(1),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.total_count, 3);
        assert!(!res.entries.iter().any(|e| e.path.contains("main.rs")));
    }

    #[test]
    fn test_find_directory_type_filter() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: Some(FindOptions {
                    file_type: Some("directory".to_string()),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.total_count, 1);
        assert_eq!(res.entries[0].path, "src");
    }

    #[test]
    fn test_find_dynamic_excludes() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: Some(FindOptions {
                    exclude: Some(vec!["*.txt".to_string(), "*.bin".to_string()]),
                    ..Default::default()
                }),
            })
            .unwrap();

        // Only src/main.rs remains
        assert_eq!(res.total_count, 1);
        assert_eq!(res.entries[0].path, "src/main.rs");
    }

    #[test]
    fn test_find_max_per_dir_diversity() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Create heavy folder with 5 files
        let heavy = sandbox.dir.join("heavy");
        fs::create_dir_all(&heavy).unwrap();
        for i in 0..5 {
            fs::write(heavy.join(format!("item_{}.rs", i)), "fn x() {}\n").unwrap();
        }

        let res = engine
            .find(&FindRequest {
                pattern: Some("*.rs".to_string()),
                path: Some(sandbox.path_str()),
                options: Some(FindOptions {
                    max_per_dir: Some(2),
                    ..Default::default()
                }),
            })
            .unwrap();

        // Total .rs files is 6 (1 in src + 5 in heavy)
        assert_eq!(res.total_count, 6);
        // But heavy contributed at most 2, src contributed 1 -> 3 entries returned
        assert_eq!(res.entries.len(), 3);
        let heavy_count = res.entries.iter().filter(|e| e.path.starts_with("heavy/")).count();
        assert_eq!(heavy_count, 2);
    }

    #[test]
    fn test_find_sort_by_size() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: Some(FindOptions {
                    sort_by: Some("size".to_string()),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.entries.len(), 4);
        for window in res.entries.windows(2) {
            assert!(
                window[0].size_bytes >= window[1].size_bytes,
                "Entries must be sorted by size descending"
            );
        }
    }

    #[test]
    fn test_find_budget_capping_and_radar() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .find(&FindRequest {
                pattern: None,
                path: Some(sandbox.path_str()),
                options: Some(FindOptions {
                    max_results: Some(2),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res.total_count, 4);
        assert_eq!(res.entries.len(), 2);
        assert!(res.truncated);
        assert!(!res.directory_radar.is_empty());
    }
}


