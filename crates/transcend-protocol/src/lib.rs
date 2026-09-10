//! Transcend Protocol Definitions
//!
//! Strongly-typed request/response data contracts for Transcend MCP tools.
//! All request and response structures derive `schemars::JsonSchema` for
//! automated schema generation within the Model Context Protocol.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Request parameters for code searching.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchRequest {
    /// Regular expression or text pattern to search for.
    pub pattern: String,
    /// Optional directory or file path to search within. Defaults to current directory.
    pub path: Option<String>,
    /// Optional glob or file extension filter (e.g. "*.rs", "*.py").
    pub file_pattern: Option<String>,
    /// Whether the search should be case-sensitive. Defaults to false.
    pub case_sensitive: Option<bool>,
    /// Optional maximum number of individual line matches to return before truncation. Defaults to 50.
    pub max_matches: Option<usize>,
}

/// A single matched line within a file.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct MatchItem {
    /// File path where the match occurred (relative to search root).
    pub file: String,
    /// 1-based line number.
    pub line_number: usize,
    /// Text content of the matched line.
    pub line_text: String,
}

/// Summary cluster of matches within a specific file.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct FileCluster {
    /// File path containing matches.
    pub file: String,
    /// Total number of matches in this file.
    pub match_count: usize,
}

/// Response returned by a search operation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchResponse {
    /// Total number of matches encountered across all searched files.
    pub total_matches: usize,
    /// List of matched items (capped at max_matches).
    pub matches: Vec<MatchItem>,
    /// Macro-level distribution of matches across files.
    pub clusters: Vec<FileCluster>,
    /// Whether individual line matches were capped due to the match budget.
    pub truncated: bool,
}

/// Request parameters for file discovery.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FindRequest {
    /// Glob or name pattern to match filenames against.
    pub pattern: String,
    /// Optional root directory to begin search. Defaults to current directory.
    pub path: Option<String>,
    /// Optional maximum depth of directory traversal.
    pub max_depth: Option<usize>,
}

/// Response returned by a file discovery operation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FindResponse {
    /// Total number of matched files/directories.
    pub total_count: usize,
    /// Matched file paths relative to search root.
    pub paths: Vec<String>,
}

/// Request parameters for AST code outlining.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
