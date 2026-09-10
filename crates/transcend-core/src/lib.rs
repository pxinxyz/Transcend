//! Transcend Core Engine
//!
//! High-performance, in-process computational primitives for code search,
//! file discovery, AST outlining, and surgical transformations.

pub mod search;

use thiserror::Error;
use transcend_protocol::{
    FindRequest, FindResponse, OutlineRequest, OutlineResponse, SearchRequest, SearchResponse,
};

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
        // Skeleton placeholder implementation
        tracing::debug!(pattern = %req.pattern, "Executing skeleton find");
        Ok(FindResponse {
            total_count: 0,
            paths: vec![],
        })
    }

    fn outline(&self, req: &OutlineRequest) -> CoreResult<OutlineResponse> {
        // Skeleton placeholder implementation
        tracing::debug!(file = %req.file_path, "Executing skeleton outline");
        Ok(OutlineResponse {
            file_path: req.file_path.clone(),
            language: "unknown".to_string(),
            symbols: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use super::*;

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
                file_pattern: None,
                case_sensitive: Some(true),
                max_matches: Some(50),
            })
            .expect("search should succeed");

        assert_eq!(res.total_matches, 1);
        assert_eq!(res.matches.len(), 1);
        assert_eq!(res.matches[0].line_number, 1);
        assert!(res.matches[0].line_text.contains("fn hello_world()"));
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
                file_pattern: None,
                case_sensitive: Some(false),
                max_matches: None,
            })
            .unwrap();
        assert_eq!(res_ci.total_matches, 1);

        // Case-sensitive (should not find)
        let res_cs = engine
            .search(&SearchRequest {
                pattern: "HELLO_WORLD".to_string(),
                path: Some(sandbox.path_str()),
                file_pattern: None,
                case_sensitive: Some(true),
                max_matches: None,
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
                file_pattern: None,
                case_sensitive: None,
                max_matches: None,
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
                file_pattern: Some("latin1.txt".to_string()),
                case_sensitive: None,
                max_matches: None,
            })
            .expect("should not panic on invalid UTF-8 bytes");

        assert_eq!(res.total_matches, 1);
        assert_eq!(res.matches.len(), 1);
        // Lossy UTF-8 turns 0xE9 into replacement char 
        assert!(res.matches[0].line_text.contains("c\u{FFFD}lebre"));
    }

    #[test]
    fn test_search_budget_clipping_and_clustering() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        let res = engine
            .search(&SearchRequest {
                pattern: "target_hit".to_string(),
                path: Some(sandbox.path_str()),
                file_pattern: None,
                case_sensitive: None,
                max_matches: Some(3),
            })
            .unwrap();

        // Total matches is 10, but returned items is capped at 3
        assert_eq!(res.total_matches, 10);
        assert_eq!(res.matches.len(), 3);
        assert!(res.truncated);

        // Cluster accurately records all 10 occurrences in budget.txt
        assert_eq!(res.clusters.len(), 1);
        assert_eq!(res.clusters[0].match_count, 10);
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
                file_pattern: None,
                case_sensitive: Some(true),
                max_matches: Some(25),
            })
            .expect("concurrent search should succeed");

        assert_eq!(res.total_matches, expected_total);
        assert_eq!(res.matches.len(), 25);
        assert!(res.truncated);

        // Verify deterministic match sorting: (file, line_number)
        for window in res.matches.windows(2) {
            let a = &window[0];
            let b = &window[1];
            assert!(
                a.file < b.file || (a.file == b.file && a.line_number <= b.line_number),
                "Matches must be strictly sorted by file and line: {:?} vs {:?}",
                a,
                b
            );
        }

        // Verify clusters are sorted by match_count descending
        assert_eq!(res.clusters.len(), 20);
        for window in res.clusters.windows(2) {
            let a = &window[0];
            let b = &window[1];
            assert!(
                a.match_count >= b.match_count,
                "Clusters must be sorted by match_count descending: {} vs {}",
                a.match_count,
                b.match_count
            );
        }
        // Top cluster must have 20 matches (from file_20)
        assert_eq!(res.clusters[0].match_count, 20);
    }
}

