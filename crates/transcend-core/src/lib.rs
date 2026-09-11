//! Transcend Core Engine
//!
//! High-performance, in-process computational primitives for code search,
//! file discovery, AST outlining, and surgical transformations.

pub mod find;
pub mod outline;
pub mod search;

use thiserror::Error;
use transcend_protocol::{
    FindRequest, FindResponse, OutlineRequest, OutlineResponse, ReadSymbolRequest,
    ReadSymbolResponse, SearchRequest, SearchResponse,
};

use crate::find::FindScanner;
use crate::outline::scanner::OutlineScanner;
use crate::outline::symbol_reader::SymbolReader;
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

    /// Surgically extract a specific symbol by name or qualified locator.
    fn read_symbol(&self, req: &ReadSymbolRequest) -> CoreResult<ReadSymbolResponse>;
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
        OutlineScanner::scan(req)
    }

    fn read_symbol(&self, req: &ReadSymbolRequest) -> CoreResult<ReadSymbolResponse> {
        SymbolReader::read(req)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use super::*;
    use transcend_protocol::{
        FindOptions, OutlineOptions, OutlineRequest, ParseStatus, SearchOptions, SymbolKind,
    };

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

    #[test]
    fn test_outline_rust_symbols_and_hierarchy() {
        let engine = NativeEngine::new();
        let code = r#"
/// Primary user entity.
pub struct User {
    pub id: u64,
    secret: String,
}

pub enum Role {
    Admin,
    Member,
}

pub trait Authenticator {
    fn authenticate(&self) -> bool;
}

impl Authenticator for User {
    fn authenticate(&self) -> bool {
        true
    }
}

impl User {
    pub fn new(id: u64) -> Self {
        Self { id, secret: "".into() }
    }
}
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("user.rs".to_string()),
                content: Some(code.to_string()),
                options: None,
            })
            .expect("outline should succeed");

        assert_eq!(res.files.len(), 1);
        let file = &res.files[0];
        assert_eq!(file.language, "rust");
        assert_eq!(file.parse_status, ParseStatus::Complete);

        // 1. Struct User
        let user_struct = file.symbols.iter().find(|s| s.name == "User" && s.kind == SymbolKind::Struct).unwrap();
        assert_eq!(user_struct.visibility.as_deref(), Some("pub"));
        assert_eq!(user_struct.doc_comment.as_deref(), Some("Primary user entity."));
        assert_eq!(user_struct.children.len(), 2);
        assert_eq!(user_struct.children[0].name, "id");
        assert_eq!(user_struct.children[0].kind, SymbolKind::Field);
        assert_eq!(user_struct.children[0].visibility.as_deref(), Some("pub"));
        assert_eq!(user_struct.children[1].name, "secret");
        assert_eq!(user_struct.children[1].visibility, None);

        // Check SourceSpan coordinate preservation
        assert!(user_struct.span.start_line >= 2);
        assert!(user_struct.span.end_line > user_struct.span.start_line);
        assert!(user_struct.span.end_byte > user_struct.span.start_byte);

        // 2. Enum Role
        let role_enum = file.symbols.iter().find(|s| s.name == "Role" && s.kind == SymbolKind::Enum).unwrap();
        assert_eq!(role_enum.children.len(), 2);
        assert_eq!(role_enum.children[0].name, "Admin");

        // 3. Trait Authenticator
        let auth_trait = file.symbols.iter().find(|s| s.name == "Authenticator" && s.kind == SymbolKind::Trait).unwrap();
        assert_eq!(auth_trait.children.len(), 1);
        assert_eq!(auth_trait.children[0].name, "authenticate");

        // 4. Impl Authenticator for User
        let impl_auth = file.symbols.iter().find(|s| s.name.contains("Authenticator for User")).unwrap();
        assert_eq!(impl_auth.kind, SymbolKind::Implementation);
        assert_eq!(impl_auth.relationships.len(), 2);
        assert!(impl_auth.relationships.iter().any(|r| r.relation == "implements" && r.target == "Authenticator"));
        assert!(impl_auth.relationships.iter().any(|r| r.relation == "targets" && r.target == "User"));
        assert_eq!(impl_auth.children.len(), 1);
        assert_eq!(impl_auth.children[0].name, "authenticate");

        // 5. Impl User
        let impl_user = file.symbols.iter().find(|s| s.name == "impl User").unwrap();
        assert_eq!(impl_user.children.len(), 1);
        assert_eq!(impl_user.children[0].name, "new");
        assert_eq!(impl_user.children[0].visibility.as_deref(), Some("pub"));
    }

    #[test]
    fn test_outline_typescript_classes_and_interfaces() {
        let engine = NativeEngine::new();
        let code = r#"
/** Service contract interface */
export interface IService {
    port: number;
    start(): Promise<void>;
}

export class WebServer implements IService {
    port: number;
    private running: boolean;

    constructor(port: number) {
        this.port = port;
        this.running = false;
    }

    async start(): Promise<void> {
        console.log("Started");
    }
}
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("server.ts".to_string()),
                content: Some(code.to_string()),
                options: None,
            })
            .expect("typescript outline should succeed");

        assert_eq!(res.files.len(), 1);
        let file = &res.files[0];
        assert_eq!(file.language, "typescript");

        // Interface IService
        let iface = file.symbols.iter().find(|s| s.name == "IService").unwrap();
        assert_eq!(iface.kind, SymbolKind::Interface);
        assert_eq!(iface.visibility.as_deref(), Some("exported"));
        assert!(iface.doc_comment.as_deref().unwrap().contains("Service contract"));
        assert_eq!(iface.children.len(), 2);

        // Class WebServer
        let cls = file.symbols.iter().find(|s| s.name == "WebServer").unwrap();
        assert_eq!(cls.kind, SymbolKind::Class);
        assert_eq!(cls.visibility.as_deref(), Some("exported"));
        assert!(cls.relationships.iter().any(|r| r.relation == "implements" && r.target == "IService"));
        assert!(cls.children.iter().any(|c| c.name == "constructor"));
        assert!(cls.children.iter().any(|c| c.name == "start" && c.kind == SymbolKind::Method));
    }

    #[test]
    fn test_outline_python_classes_methods_docstrings() {
        let engine = NativeEngine::new();
        let code = r#"
class SearchPipeline(BasePipeline):
    """Executes distributed code searches."""

    def __init__(self, capacity: int):
        self.capacity = capacity

    def run(self, query: str):
        pass

    def _internal_clean(self):
        pass
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("pipeline.py".to_string()),
                content: Some(code.to_string()),
                options: None,
            })
            .expect("python outline should succeed");

        assert_eq!(res.files.len(), 1);
        let file = &res.files[0];
        assert_eq!(file.language, "python");

        let cls = file.symbols.iter().find(|s| s.name == "SearchPipeline").unwrap();
        assert_eq!(cls.kind, SymbolKind::Class);
        assert_eq!(cls.doc_comment.as_deref(), Some("Executes distributed code searches."));
        assert!(cls.relationships.iter().any(|r| r.relation == "extends" && r.target == "BasePipeline"));

        let init_m = cls.children.iter().find(|c| c.name == "__init__").unwrap();
        assert_eq!(init_m.kind, SymbolKind::Constructor);

        let run_m = cls.children.iter().find(|c| c.name == "run").unwrap();
        assert_eq!(run_m.kind, SymbolKind::Method);
        assert_eq!(run_m.visibility.as_deref(), Some("public"));

        let helper_m = cls.children.iter().find(|c| c.name == "_internal_clean").unwrap();
        assert_eq!(helper_m.visibility.as_deref(), Some("private"));
    }

    #[test]
    fn test_outline_go_functions_methods_receivers() {
        let engine = NativeEngine::new();
        let code = r#"
package main

type Engine struct {
    Workers int
}

func (e *Engine) Execute(task string) error {
    return nil
}

func internalHelper() {}
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("engine.go".to_string()),
                content: Some(code.to_string()),
                options: None,
            })
            .expect("go outline should succeed");

        assert_eq!(res.files.len(), 1);
        let file = &res.files[0];
        assert_eq!(file.language, "go");

        let eng_struct = file.symbols.iter().find(|s| s.name == "Engine").unwrap();
        assert_eq!(eng_struct.kind, SymbolKind::Struct);
        assert_eq!(eng_struct.visibility.as_deref(), Some("exported"));

        let exec_method = file.symbols.iter().find(|s| s.name == "Execute").unwrap();
        assert_eq!(exec_method.kind, SymbolKind::Method);
        assert_eq!(exec_method.visibility.as_deref(), Some("exported"));
        assert!(exec_method.relationships.iter().any(|r| r.relation == "receiver" && r.target == "Engine"));

        let helper_fn = file.symbols.iter().find(|s| s.name == "internalHelper").unwrap();
        assert_eq!(helper_fn.visibility, None);
    }

    #[test]
    fn test_outline_filters_and_depth_budget() {
        let engine = NativeEngine::new();
        let code = r#"
pub struct Service {
    pub id: u64,
    secret: String,
}

impl Service {
    pub fn public_action(&self) {}
    fn private_action(&self) {}
}

fn private_toplevel() {}
pub fn public_toplevel() {}
"#;

        // 1. Exported only filter
        let res_exported = engine
            .outline(&OutlineRequest {
                path: Some("service.rs".to_string()),
                content: Some(code.to_string()),
                options: Some(OutlineOptions {
                    exported_only: Some(true),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert!(!res_exported.files[0].symbols.iter().any(|s| s.name == "private_toplevel"));
        assert!(res_exported.files[0].symbols.iter().any(|s| s.name == "public_toplevel"));

        // 2. Symbol kinds filter
        let res_kinds = engine
            .outline(&OutlineRequest {
                path: Some("service.rs".to_string()),
                content: Some(code.to_string()),
                options: Some(OutlineOptions {
                    symbol_kinds: Some(vec![SymbolKind::Struct]),
                    ..Default::default()
                }),
            })
            .unwrap();

        assert_eq!(res_kinds.files[0].symbols.len(), 1);
        assert_eq!(res_kinds.files[0].symbols[0].kind, SymbolKind::Struct);

        // 3. Max depth pruning (depth 1 strips fields and methods)
        let res_depth = engine
            .outline(&OutlineRequest {
                path: Some("service.rs".to_string()),
                content: Some(code.to_string()),
                options: Some(OutlineOptions {
                    max_depth: Some(1),
                    ..Default::default()
                }),
            })
            .unwrap();

        let s = res_depth.files[0].symbols.iter().find(|sym| sym.name == "Service").unwrap();
        assert!(s.children.is_empty(), "Children should be pruned at depth 1");
    }

    #[test]
    fn test_outline_resilient_to_syntax_errors() {
        let engine = NativeEngine::new();
        let broken_code = r#"
pub struct ValidStruct {
    pub field: u32,
}

pub fn broken_function( {
    let incomplete = 
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("broken.rs".to_string()),
                content: Some(broken_code.to_string()),
                options: None,
            })
            .expect("should not fail even with syntax errors");

        let file = &res.files[0];
        assert_eq!(file.parse_status, ParseStatus::Partial);
        // ValidStruct should still be extracted!
        assert!(file.symbols.iter().any(|s| s.name == "ValidStruct"));
    }

    #[test]
    fn test_outline_directory_macro_census_and_budget() {
        let sandbox = TestSandbox::create();
        let engine = NativeEngine::new();

        // Write multiple multi-language files into sandbox
        fs::write(
            sandbox.dir.join("logic.py"),
            "class PythonModel:\n    def execute(self):\n        pass\n",
        )
        .unwrap();

        fs::write(
            sandbox.dir.join("types.ts"),
            "export interface TypeScriptContract {\n    id: string;\n}\n",
        )
        .unwrap();

        let res = engine
            .outline(&OutlineRequest {
                path: Some(sandbox.path_str()),
                content: None,
                options: Some(OutlineOptions {
                    max_symbols: Some(3),
                    ..Default::default()
                }),
            })
            .expect("directory outline should succeed");

        assert!(res.summary.total_files >= 2);
        assert!(res.summary.total_symbols > 0);
        assert!(!res.summary.language_breakdown.is_empty());
        assert!(!res.summary.kind_breakdown.is_empty());
        assert!(res.truncated);
    }

    #[test]
    fn test_outline_max_output_bytes_budget() {
        let engine = NativeEngine::new();
        let code = r#"
pub struct Alpha { pub a: u32, pub b: u32 }
pub struct Beta { pub c: u32, pub d: u32 }
pub struct Gamma { pub e: u32, pub f: u32 }
pub struct Delta { pub g: u32, pub h: u32 }
"#;

        let res = engine
            .outline(&OutlineRequest {
                path: Some("budget_test.rs".to_string()),
                content: Some(code.to_string()),
                options: Some(OutlineOptions {
                    max_output_bytes: Some(450),
                    ..Default::default()
                }),
            })
            .expect("outline should succeed");

        assert!(res.truncated, "Should be truncated by byte budget");
        let json_size = serde_json::to_vec(&res.files).unwrap().len();
        assert!(json_size <= 600, "Output size {} should be capped", json_size);
    }

    #[test]
    fn test_outline_include_relationships_toggle_and_single_line_docs() {
        let engine = NativeEngine::new();
        let code = r#"
/// First line summary of contract.
///
/// Long extensive paragraph that should never be dumped into agent context.
/// Multiple lines of prose here.
pub struct Contract;

impl Contract {
    pub fn execute(&self) {}
}
"#;

        // 1. Single line doc summary check
        let res_docs = engine
            .outline(&OutlineRequest {
                path: Some("doc_test.rs".to_string()),
                content: Some(code.to_string()),
                options: None,
            })
            .unwrap();

        let contract_sym = res_docs.files[0].symbols.iter().find(|s| s.name == "Contract").unwrap();
        assert_eq!(
            contract_sym.doc_comment.as_deref(),
            Some("First line summary of contract.")
        );

        // 2. Relationships toggle check
        let res_no_rels = engine
            .outline(&OutlineRequest {
                path: Some("doc_test.rs".to_string()),
                content: Some(code.to_string()),
                options: Some(OutlineOptions {
                    include_relationships: Some(false),
                    ..Default::default()
                }),
            })
            .unwrap();

        let impl_sym = res_no_rels.files[0].symbols.iter().find(|s| s.name == "impl Contract").unwrap();
        assert!(impl_sym.relationships.is_empty(), "Relationships should be empty when include_relationships is false");
    }

    #[test]
    fn test_outline_c_and_cpp() {
        let engine = NativeEngine::new();

        // 1. Test C
        let c_code = r#"
/// Adds two numbers
int add(int a, int b) {
    return a + b;
}

struct Point {
    int x;
    int y;
};
"#;
        let c_res = engine.outline(&OutlineRequest {
            path: Some("math.c".to_string()),
            content: Some(c_code.to_string()),
            options: None,
        }).expect("C outline should succeed");

        assert_eq!(c_res.files[0].language, "c");
        assert!(c_res.files[0].symbols.iter().any(|s| s.name == "add" && s.kind == SymbolKind::Function));
        assert!(c_res.files[0].symbols.iter().any(|s| s.name == "Point" && s.kind == SymbolKind::Struct));

        let header_code = r#"
#define MAX_BUFFER 1024
typedef struct _ENTRY {
    int id;
} ENTRY, *PENTRY;

int process_data(ENTRY *e);
"#;
        let h_res = engine.outline(&OutlineRequest {
            path: Some("header.h".to_string()),
            content: Some(header_code.to_string()),
            options: None,
        }).expect("C header outline should succeed");
        assert_eq!(h_res.files[0].language, "c");
        assert!(h_res.files[0].symbols.iter().any(|s| s.name == "MAX_BUFFER" && s.kind == SymbolKind::Macro));
        assert!(h_res.files[0].symbols.iter().any(|s| s.name == "ENTRY" && s.kind == SymbolKind::Struct));
        assert!(h_res.files[0].symbols.iter().any(|s| s.name == "process_data" && s.kind == SymbolKind::Function));

        // 2. Test C++
        let cpp_code = r#"
class Animal {
public:
    Animal();
    virtual void speak();
private:
    int age;
};
"#;
        let cpp_res = engine.outline(&OutlineRequest {
            path: Some("animal.cpp".to_string()),
            content: Some(cpp_code.to_string()),
            options: None,
        }).expect("C++ outline should succeed");

        assert_eq!(cpp_res.files[0].language, "cpp");
        let class_sym = cpp_res.files[0].symbols.iter().find(|s| s.name == "Animal").unwrap();
        assert_eq!(class_sym.kind, SymbolKind::Class);
        assert!(class_sym.children.iter().any(|c| c.name == "speak" && c.visibility.as_deref() == Some("public")));
        assert!(class_sym.children.iter().any(|c| c.name == "age" && c.visibility.as_deref() == Some("private")));
    }

    #[test]
    fn test_outline_csharp() {
        let engine = NativeEngine::new();
        let cs_code = r#"
namespace Services {
    /// <summary>
    /// Worker contract interface
    /// </summary>
    public interface IWorker {
        void Execute();
    }

    public class BackgroundWorker : IWorker {
        public string Name { get; set; }
        public void Execute() {}
    }
}
"#;
        let res = engine.outline(&OutlineRequest {
            path: Some("Worker.cs".to_string()),
            content: Some(cs_code.to_string()),
            options: None,
        }).expect("C# outline should succeed");

        assert_eq!(res.files[0].language, "csharp");
        let ns = &res.files[0].symbols[0];
        assert_eq!(ns.kind, SymbolKind::Namespace);
        assert!(ns.children.iter().any(|s| s.name == "IWorker" && s.kind == SymbolKind::Interface));
        let worker_class = ns.children.iter().find(|s| s.name == "BackgroundWorker").unwrap();
        assert_eq!(worker_class.kind, SymbolKind::Class);
        assert!(worker_class.relationships.iter().any(|r| r.relation == "implements" && r.target == "IWorker"));
        assert!(worker_class.children.iter().any(|c| c.name == "Name" && c.kind == SymbolKind::Property));
        assert!(worker_class.children.iter().any(|c| c.name == "Execute" && c.kind == SymbolKind::Method));
    }

    #[test]
    fn test_outline_java() {
        let engine = NativeEngine::new();
        let java_code = r#"
package com.transcend;

/**
 * Main application service
 */
public class ApplicationService implements Runnable {
    private int counter;

    public ApplicationService() {}

    @Override
    public void run() {
        System.out.println("Running");
    }
}
"#;
        let res = engine.outline(&OutlineRequest {
            path: Some("ApplicationService.java".to_string()),
            content: Some(java_code.to_string()),
            options: None,
        }).expect("Java outline should succeed");

        assert_eq!(res.files[0].language, "java");
        let app_class = res.files[0].symbols.iter().find(|s| s.name == "ApplicationService").unwrap();
        assert_eq!(app_class.kind, SymbolKind::Class);
        assert_eq!(app_class.doc_comment.as_deref(), Some("Main application service"));
        assert!(app_class.relationships.iter().any(|r| r.relation == "implements" && r.target == "Runnable"));
        assert!(app_class.children.iter().any(|c| c.name == "run" && c.kind == SymbolKind::Method));
        assert!(app_class.children.iter().any(|c| c.name == "ApplicationService" && c.kind == SymbolKind::Constructor));
    }

    #[test]
    fn test_outline_kotlin() {
        let engine = NativeEngine::new();
        let kt_code = r#"
package com.demo

/**
 * User account model
 */
class User(val id: String) {
    fun getDisplayName(): String {
        return id
    }
}
"#;
        let res = engine.outline(&OutlineRequest {
            path: Some("User.kt".to_string()),
            content: Some(kt_code.to_string()),
            options: None,
        }).expect("Kotlin outline should succeed");

        assert_eq!(res.files[0].language, "kotlin");
        let user_class = res.files[0].symbols.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(user_class.kind, SymbolKind::Class);
        assert_eq!(user_class.doc_comment.as_deref(), Some("User account model"));
        assert!(user_class.children.iter().any(|c| c.name == "getDisplayName" && c.kind == SymbolKind::Method));
    }

    #[test]
    fn test_outline_php() {
        let engine = NativeEngine::new();
        let php_code = r#"<?php
namespace App\Controllers;

/**
 * Controller class
 */
class HomeController {
    public function index() {
        return "hello";
    }
}
"#;
        let res = engine.outline(&OutlineRequest {
            path: Some("HomeController.php".to_string()),
            content: Some(php_code.to_string()),
            options: None,
        }).expect("PHP outline should succeed");

        assert_eq!(res.files[0].language, "php");
        let ns = &res.files[0].symbols[0];
        assert_eq!(ns.kind, SymbolKind::Namespace);
        let ctrl = ns.children.iter().find(|s| s.name == "HomeController").unwrap();
        assert_eq!(ctrl.kind, SymbolKind::Class);
        assert!(ctrl.children.iter().any(|m| m.name == "index" && m.kind == SymbolKind::Method));
    }

    #[test]
    fn test_outline_ruby() {
        let engine = NativeEngine::new();
        let rb_code = r#"
# Core authentication module
module Authentication
  class SessionManager < BaseManager
    def create_session(user)
      # logic
    end
  end
end
"#;
        let res = engine.outline(&OutlineRequest {
            path: Some("auth.rb".to_string()),
            content: Some(rb_code.to_string()),
            options: None,
        }).expect("Ruby outline should succeed");

        assert_eq!(res.files[0].language, "ruby");
        let m = &res.files[0].symbols[0];
        assert_eq!(m.name, "Authentication");
        assert_eq!(m.kind, SymbolKind::Module);
        let cls = m.children.iter().find(|s| s.name == "SessionManager").unwrap();
        assert_eq!(cls.kind, SymbolKind::Class);
        assert!(cls.relationships.iter().any(|r| r.relation == "extends" && r.target == "BaseManager"));
        assert!(cls.children.iter().any(|c| c.name == "create_session" && c.kind == SymbolKind::Method));
    }

    #[test]
    fn test_outline_swift() {
        let engine = NativeEngine::new();
        let swift_code = r#"
/// High performance vehicle
public class SportsCar {
    public init() {}
    public func accelerate() {}
}
"#;
        let res = engine.outline(&OutlineRequest {
            path: Some("Car.swift".to_string()),
            content: Some(swift_code.to_string()),
            options: None,
        }).expect("Swift outline should succeed");

        assert_eq!(res.files[0].language, "swift");
        let car = res.files[0].symbols.iter().find(|s| s.name == "SportsCar").unwrap();
        assert_eq!(car.kind, SymbolKind::Class);
        assert_eq!(car.doc_comment.as_deref(), Some("High performance vehicle"));
        assert!(car.children.iter().any(|c| c.name == "init" && c.kind == SymbolKind::Constructor));
        assert!(car.children.iter().any(|c| c.name == "accelerate" && c.kind == SymbolKind::Method));
    }

    #[test]
    fn test_outline_bash() {
        let engine = NativeEngine::new();
        let bash_code = r#"#!/bin/bash
# Deployment helper script
export DEPLOY_ENV="production"

deploy_service() {
    echo "Deploying..."
}
"#;
        let res = engine.outline(&OutlineRequest {
            path: Some("deploy.sh".to_string()),
            content: Some(bash_code.to_string()),
            options: None,
        }).expect("Bash outline should succeed");

        assert_eq!(res.files[0].language, "bash");
        assert!(res.files[0].symbols.iter().any(|s| s.name == "deploy_service" && s.kind == SymbolKind::Function));
        assert!(res.files[0].symbols.iter().any(|s| s.name == "DEPLOY_ENV" && s.kind == SymbolKind::Constant));
    }

    #[test]
    fn test_outline_sql() {
        let engine = NativeEngine::new();
        let sql_code = r#"
-- Customer accounts table
CREATE TABLE customers (
    id INT PRIMARY KEY,
    email VARCHAR(255)
);

CREATE VIEW active_customers AS SELECT * FROM customers;
"#;
        let res = engine.outline(&OutlineRequest {
            path: Some("schema.sql".to_string()),
            content: Some(sql_code.to_string()),
            options: None,
        }).expect("SQL outline should succeed");

        assert_eq!(res.files[0].language, "sql");
        let tbl = res.files[0].symbols.iter().find(|s| s.name == "customers").unwrap();
        assert_eq!(tbl.kind, SymbolKind::Struct);
        assert_eq!(tbl.doc_comment.as_deref(), Some("Customer accounts table"));
        assert!(res.files[0].symbols.iter().any(|s| s.name == "active_customers" && s.kind == SymbolKind::Interface));
    }

    #[test]
    fn test_outline_dart() {
        let engine = NativeEngine::new();
        let dart_code = r#"
/// User repository
class UserRepository {
    void _internalSync() {}
    void fetchUser() {}
}
"#;
        let res = engine.outline(&OutlineRequest {
            path: Some("repo.dart".to_string()),
            content: Some(dart_code.to_string()),
            options: None,
        }).expect("Dart outline should succeed");

        assert_eq!(res.files[0].language, "dart");
        let repo = res.files[0].symbols.iter().find(|s| s.name == "UserRepository").unwrap();
        assert_eq!(repo.kind, SymbolKind::Class);
        assert_eq!(repo.doc_comment.as_deref(), Some("User repository"));
        assert!(repo.children.iter().any(|c| c.name == "fetchUser" && c.visibility.as_deref() == Some("public")));
        assert!(repo.children.iter().any(|c| c.name == "_internalSync" && c.visibility.as_deref() == Some("private")));
    }

    #[test]
    fn test_outline_zig() {
        let engine = NativeEngine::new();
        let zig_code = r#"
/// App configuration
pub const AppConfig = struct {
    port: u16,
};

pub fn startServer() void {
}
"#;
        let res = engine.outline(&OutlineRequest {
            path: Some("main.zig".to_string()),
            content: Some(zig_code.to_string()),
            options: None,
        }).expect("Zig outline should succeed");

        assert_eq!(res.files[0].language, "zig");
        assert!(res.files[0].symbols.iter().any(|s| s.name == "AppConfig" && s.kind == SymbolKind::Struct));
        assert!(res.files[0].symbols.iter().any(|s| s.name == "startServer" && s.kind == SymbolKind::Function));
    }

    #[test]
    fn test_outline_lua() {
        let engine = NativeEngine::new();
        let lua_code = r#"
--- Module documentation
local M = {}

function M:save()
end

function M.load()
end

return M
"#;
        let res = engine.outline(&OutlineRequest {
            path: Some("storage.lua".to_string()),
            content: Some(lua_code.to_string()),
            options: None,
        }).expect("Lua outline should succeed");

        assert_eq!(res.files[0].language, "lua");
        let save_sym = res.files[0].symbols.iter().find(|s| s.name == "M:save").unwrap();
        assert_eq!(save_sym.kind, SymbolKind::Method);
        assert!(save_sym.relationships.iter().any(|r| r.relation == "receiver" && r.target == "M"));
        assert!(res.files[0].symbols.iter().any(|s| s.name == "M.load" && s.kind == SymbolKind::Function));
    }

    #[test]
    fn test_outline_markdown() {
        let engine = NativeEngine::new();
        let md_code = r#"
# Transcend Documentation

Universal agent-native cartographer.

## Core Features

Fast in-process primitives.

### Fast Search
Multi-threaded ripgrep.

### Code Outline
Hierarchical symbol index.

## Verification
Automated test suite.
"#;
        let res = engine.outline(&OutlineRequest {
            path: Some("README.md".to_string()),
            content: Some(md_code.to_string()),
            options: None,
        }).expect("Markdown outline should succeed");

        assert_eq!(res.files[0].language, "markdown");
        let root_h1 = &res.files[0].symbols[0];
        assert_eq!(root_h1.name, "Transcend Documentation");
        assert_eq!(root_h1.kind, SymbolKind::Module);
        assert_eq!(root_h1.doc_comment.as_deref(), Some("Universal agent-native cartographer."));

        // H1 should have two H2 children: "Core Features" and "Verification"
        assert_eq!(root_h1.children.len(), 2);
        let core_features = &root_h1.children[0];
        assert_eq!(core_features.name, "Core Features");

        // "Core Features" should have two H3 children: "Fast Search" and "Code Outline"
        assert_eq!(core_features.children.len(), 2);
        assert_eq!(core_features.children[0].name, "Fast Search");
        assert_eq!(core_features.children[1].name, "Code Outline");
    }

    #[test]
    fn test_read_symbol_bare_and_qualified_name() {
        let engine = NativeEngine::new();
        let code = r#"
pub struct Calculator {
    pub scale: f64,
}

impl Calculator {
    /// Adds two numbers with scale
    pub fn add(&self, a: f64, b: f64) -> f64 {
        (a + b) * self.scale
    }
}

pub fn global_helper() -> i32 {
    42
}
"#;

        // 1. Bare name lookup of global function
        let res1 = engine.read_symbol(&ReadSymbolRequest {
            path: Some("calc.rs".to_string()),
            content: Some(code.to_string()),
            symbol: "global_helper".to_string(),
            ..Default::default()
        }).unwrap();

        assert!(res1.found);
        assert_eq!(res1.symbol.as_ref().unwrap().name, "global_helper");
        assert!(res1.source_code.as_ref().unwrap().contains("42"));
        assert_eq!(res1.total_occurrences, 1);

        // 2. Qualified name lookup of method: Calculator::add
        let res2 = engine.read_symbol(&ReadSymbolRequest {
            path: Some("calc.rs".to_string()),
            content: Some(code.to_string()),
            symbol: "Calculator::add".to_string(),
            ..Default::default()
        }).unwrap();

        assert!(res2.found);
        assert_eq!(res2.qualified_name.as_deref(), Some("Calculator::add"));
        assert!(res2.source_code.as_ref().unwrap().contains("(a + b) * self.scale"));
        assert_eq!(res2.symbol.as_ref().unwrap().doc_comment.as_deref(), Some("Adds two numbers with scale"));

        // 3. Normalized dot notation lookup: Calculator.add
        let res3 = engine.read_symbol(&ReadSymbolRequest {
            path: Some("calc.rs".to_string()),
            content: Some(code.to_string()),
            symbol: "Calculator.add".to_string(),
            ..Default::default()
        }).unwrap();

        assert!(res3.found);
        assert_eq!(res3.qualified_name.as_deref(), Some("Calculator::add"));

        // 4. Bare method name lookup
        let res4 = engine.read_symbol(&ReadSymbolRequest {
            path: Some("calc.rs".to_string()),
            content: Some(code.to_string()),
            symbol: "add".to_string(),
            ..Default::default()
        }).unwrap();

        assert!(res4.found);
        assert_eq!(res4.symbol.as_ref().unwrap().name, "add");
    }

    #[test]
    fn test_read_symbol_occurrences_and_context() {
        let engine = NativeEngine::new();
        let code = r#"// Line 1: header
// Line 2: intro
fn execute() {
    println!("first");
}
// Line 6: middle
fn execute() {
    println!("second");
}
// Line 10: footer"#;

        // Occurrence 0 (first) with 1 context line before and after
        let res0 = engine.read_symbol(&ReadSymbolRequest {
            path: Some("exec.rs".to_string()),
            content: Some(code.to_string()),
            symbol: "execute".to_string(),
            occurrence: Some(0),
            context_lines: Some(1),
            ..Default::default()
        }).unwrap();

        assert!(res0.found);
        assert_eq!(res0.total_occurrences, 2);
        assert!(res0.source_code.as_ref().unwrap().contains("first"));
        assert!(res0.context_before.as_ref().unwrap().contains("Line 2"));
        assert!(res0.context_after.as_ref().unwrap().contains("Line 6"));

        // Occurrence 1 (second)
        let res1 = engine.read_symbol(&ReadSymbolRequest {
            path: Some("exec.rs".to_string()),
            content: Some(code.to_string()),
            symbol: "execute".to_string(),
            occurrence: Some(1),
            ..Default::default()
        }).unwrap();

        assert!(res1.found);
        assert!(res1.source_code.as_ref().unwrap().contains("second"));

        // Occurrence out of range
        let res_out = engine.read_symbol(&ReadSymbolRequest {
            path: Some("exec.rs".to_string()),
            content: Some(code.to_string()),
            symbol: "execute".to_string(),
            occurrence: Some(99),
            ..Default::default()
        }).unwrap();

        assert!(!res_out.found);
        assert!(res_out.message.as_ref().unwrap().contains("out of range"));
    }

    #[test]
    fn test_read_symbol_not_found_suggestions() {
        let engine = NativeEngine::new();
        let code = r#"
pub fn parse_header() {}
pub fn parse_body() {}
"#;
        let res = engine.read_symbol(&ReadSymbolRequest {
            path: Some("parser.rs".to_string()),
            content: Some(code.to_string()),
            symbol: "parse_footer".to_string(),
            ..Default::default()
        }).unwrap();

        assert!(!res.found);
        assert_eq!(res.total_occurrences, 0);
        let msg = res.message.unwrap();
        assert!(msg.contains("parse_header"));
        assert!(msg.contains("parse_body"));
    }
}


