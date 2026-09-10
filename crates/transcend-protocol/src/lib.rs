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
    /// Whether the search should be case-sensitive.
    pub case_sensitive: Option<bool>,
    /// Optional maximum number of matches to return before clustering.
    pub max_matches: Option<usize>,
}

/// A single matched line within a file.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct MatchItem {
    /// File path where the match occurred.
    pub file: String,
    /// 1-based line number.
    pub line_number: usize,
    /// Text content of the matched line.
    pub line_text: String,
}

/// Response returned by a search operation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchResponse {
    /// Total number of matches encountered.
    pub total_matches: usize,
    /// List of matched items.
    pub matches: Vec<MatchItem>,
    /// Whether the results were truncated or clustered due to budget limits.
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
