//! Transcend Protocol Definitions
//!
//! Strongly-typed request/response data contracts for Transcend MCP tools.
//! All request and response structures derive `schemars::JsonSchema` for
//! automated schema generation within the Model Context Protocol.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Request parameters for code searching.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SearchRequest {
    /// Regular expression or text pattern to search for.
    pub pattern: String,
    /// Optional directory or file path to search within. Defaults to current directory.
    pub path: Option<String>,
    /// Optional search tuning options.
    pub options: Option<SearchOptions>,
}

/// Optional configuration options for code search.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SearchOptions {
    /// Optional glob or file extension filter (e.g. "*.rs", "*.py").
    pub file_pattern: Option<String>,
    /// Whether the search should be case-sensitive. Defaults to false.
    pub case_sensitive: Option<bool>,
    /// Optional maximum number of individual line matches to return across all files. Defaults to 50.
    pub max_matches: Option<usize>,
    /// Optional maximum number of line matches to return from any single file (prevents monster files from monopolizing results).
    pub max_per_file: Option<usize>,
    /// Optional maximum character length of an extracted line before clipping. Defaults to 500.
    pub max_line_length: Option<usize>,
    /// Optional number of surrounding context lines to include before and after matching lines (0, 1, or 2). Defaults to 0.
    pub context_lines: Option<usize>,
}

/// A single matched line within a file.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SearchMatch {
    /// 1-based line number.
    pub line_number: usize,
    /// Text content of the matched line.
    pub line_text: String,
    /// Preceding context lines (if context_lines > 0).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub context_before: Vec<String>,
    /// Following context lines (if context_lines > 0).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub context_after: Vec<String>,
}

/// Cluster of matches belonging to a specific file.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct FileCluster {
    /// File path containing matches (relative to search root).
    pub file: String,
    /// Total number of matches encountered in this file.
    pub match_count: usize,
    /// Extracted line matches for this file.
    pub matches: Vec<SearchMatch>,
}

/// Macro-level directory radar summarizing match distribution across the directory tree.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DirectoryRadar {
    /// Directory path containing matched files.
    pub directory: String,
    /// Number of distinct files with matches in this directory.
    pub file_count: usize,
    /// Total number of matches across all files in this directory.
    pub match_count: usize,
}

/// Response returned by a search operation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchResponse {
    /// Total number of matches encountered across all searched files.
    pub total_matches: usize,
    /// Total number of distinct files containing matches.
    pub total_files: usize,
    /// File clusters containing line matches, grouped per file.
    pub files: Vec<FileCluster>,
    /// Macro-level directory distribution of matches (sorted by density).
    pub directory_radar: Vec<DirectoryRadar>,
    /// Whether individual line matches were capped due to the match budget.
    pub truncated: bool,
}

/// Request parameters for file discovery.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FindRequest {
    /// Optional filename pattern (e.g. "*.rs", "main", "Cargo.*") or glob. If omitted, lists all files.
    pub pattern: Option<String>,
    /// Optional root directory to begin search. Defaults to current directory.
    pub path: Option<String>,
    /// Optional discovery tuning options.
    pub options: Option<FindOptions>,
}

/// Optional configuration options for file discovery.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FindOptions {
    /// Optional maximum depth of directory traversal (e.g. 1 for root items only).
    pub max_depth: Option<usize>,
    /// Optional maximum number of file paths to return. Defaults to 100.
    pub max_results: Option<usize>,
    /// Optional filter by file type: "file", "directory", or "any". Defaults to "file".
    pub file_type: Option<String>,
    /// Optional file extension filter (e.g. "rs", "json").
    pub extension: Option<String>,
    /// Whether pattern matching should be case-sensitive. Defaults to false.
    pub case_sensitive: Option<bool>,
}

/// Response returned by a file discovery operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FindResponse {
    /// Total number of matched files/directories before budget capping.
    pub total_count: usize,
    /// Matched file paths relative to search root (capped by max_results).
    pub paths: Vec<String>,
    /// Macro-level directory radar summarizing match distribution across directories.
    pub directory_radar: Vec<DirectoryRadar>,
    /// Whether the returned paths were capped by max_results.
    pub truncated: bool,
}

/// Request parameters for AST code outlining.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct OutlineRequest {
    /// Path to the source file to outline.
    pub file_path: String,
}

/// A structural code symbol extracted from an AST.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SymbolOutline {
    /// Name of the symbol (e.g., function, struct, class, interface name).
    pub name: String,
    /// Kind of symbol (e.g., "function", "struct", "enum", "method", "class").
    pub kind: String,
    /// 1-based start line.
    pub start_line: usize,
    /// 1-based end line.
    pub end_line: usize,
    /// Signature or declaration snippet.
    pub signature: Option<String>,
}

/// Response returned by an outline operation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OutlineResponse {
    /// Path of the outlined file.
    pub file_path: String,
    /// Detected programming language.
    pub language: String,
    /// Extracted symbols in document order.
    pub symbols: Vec<SymbolOutline>,
}
