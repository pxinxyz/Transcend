//! Transcend Protocol Definitions
//!
//! Strongly-typed request/response data contracts for Transcend MCP tools.
//! All request and response structures derive `schemars::JsonSchema` for
//! automated schema generation within the Model Context Protocol.

use std::collections::BTreeMap;
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
    /// Optional maximum number of results to return from any single directory (prevents fixture folders from monopolizing results).
    pub max_per_dir: Option<usize>,
    /// Optional filter by file type: "file", "directory", or "any". Defaults to "file".
    pub file_type: Option<String>,
    /// Optional file extension filter (e.g. "rs", "json").
    pub extension: Option<String>,
    /// Optional glob patterns to exclude from discovery (e.g. ["fixtures/**", "*.min.js"]).
    pub exclude: Option<Vec<String>>,
    /// Optional sorting criteria: "path" (alphabetical), "modified" (most recently updated first), or "size" (largest first). Defaults to "path".
    pub sort_by: Option<String>,
    /// Whether pattern matching should be case-sensitive. Defaults to false.
    pub case_sensitive: Option<bool>,
}

/// A discovered filesystem entry with compact metadata.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PathEntry {
    /// Relative path from search root.
    pub path: String,
    /// Size of the file in bytes (0 for directories).
    pub size_bytes: u64,
    /// ISO 8601 timestamp of last modification time (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

/// Response returned by a file discovery operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FindResponse {
    /// Total number of matched files/directories before budget capping.
    pub total_count: usize,
    /// Matched file entries relative to search root (capped by max_results and max_per_dir).
    pub entries: Vec<PathEntry>,
    /// Macro-level directory radar summarizing match distribution across directories.
    pub directory_radar: Vec<DirectoryRadar>,
    /// Tech-stack extension census mapping extension to count of matched files.
    pub extension_breakdown: BTreeMap<String, usize>,
    /// Whether the returned entries were capped by max_results or max_per_dir.
    pub truncated: bool,
}

/// Strongly-typed canonical symbol kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Module,
    Namespace,
    Struct,
    Enum,
    Trait,
    Interface,
    Class,
    Function,
    Method,
    Constructor,
    Constant,
    Static,
    TypeAlias,
    Field,
    Property,
    Variable,
    Macro,
    Import,
    Implementation,
}

/// Exact source code coordinates for surgical follow-up.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SourceSpan {
    /// 1-based start line.
    pub start_line: usize,
    /// 1-based start column (character offset in line).
    pub start_col: usize,
    /// 1-based end line.
    pub end_line: usize,
    /// 1-based end column (character offset in line).
    pub end_col: usize,
    /// 0-based byte offset where symbol begins.
    pub start_byte: usize,
    /// 0-based byte offset where symbol ends.
    pub end_byte: usize,
}

/// Structural relationship between symbols.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SymbolRelationship {
    /// Relationship type (e.g. "implements", "extends", "receiver", "targets").
    pub relation: String,
    /// Target type, interface, or trait name.
    pub target: String,
}

/// A semantic code symbol with hierarchy and relationship links.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Symbol {
    /// Identifier or declaration name.
    pub name: String,
    /// Canonical symbol kind.
    pub kind: SymbolKind,
    /// Exact source location span.
    pub span: SourceSpan,
    /// Declaration signature (excluding implementation body).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// First line / summary of doc comments or docstring.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
    /// Symbol visibility (e.g. "public", "private", "crate").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    /// Structural relationships (implements, extends, receiver).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub relationships: Vec<SymbolRelationship>,
    /// Child symbols representing structural ownership.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<Symbol>,
}

/// Parse fidelity status of a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParseStatus {
    Complete,
    Partial,
    SyntaxErrors,
}

/// Output format for outline results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutlineFormat {
    /// Full structured JSON metadata (default).
    #[default]
    Json,
    /// Ultra-compact, syntax-valid code skeleton stubs.
    Skeleton,
}

/// Outlines for an individual source file.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct FileOutline {
    /// Relative path to file.
    pub file: String,
    /// Detected language.
    pub language: String,
    /// Parse fidelity status.
    pub parse_status: ParseStatus,
    /// Root symbols in document order.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub symbols: Vec<Symbol>,
    /// Formatted code skeleton (when format == OutlineFormat::Skeleton).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skeleton: Option<String>,
}

/// High-level architectural census across all outlined files.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OutlineSummary {
    pub total_files: usize,
    pub total_symbols: usize,
    pub kind_breakdown: BTreeMap<String, usize>,
    pub language_breakdown: BTreeMap<String, usize>,
}

/// Request parameters for code outlining.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct OutlineRequest {
    /// File path or directory to outline. If a directory, traverses respecting ignore rules.
    #[serde(alias = "file_path")]
    pub path: Option<String>,
    /// Optional direct code content (for in-memory buffer / unsaved code inspection).
    pub content: Option<String>,
    /// Optional tuning and budgeting options.
    pub options: Option<OutlineOptions>,
}

/// Optional configuration and budget options for outlining.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct OutlineOptions {
    /// Output representation format: "json" (default) or "skeleton".
    pub format: Option<OutlineFormat>,
    /// Only include public / exported symbols. Defaults to false.
    pub exported_only: Option<bool>,
    /// Filter to specific symbol kinds (e.g. ["struct", "function", "trait"]).
    pub symbol_kinds: Option<Vec<SymbolKind>>,
    /// Maximum depth of symbol hierarchy to return.
    pub max_depth: Option<usize>,
    /// Maximum number of symbols to return across the response. Defaults to 500.
    pub max_symbols: Option<usize>,
    /// Maximum number of files to process if path is a directory. Defaults to 20.
    pub max_files: Option<usize>,
    /// Maximum number of bytes in the serialized symbol payload before truncation.
    pub max_output_bytes: Option<usize>,
    /// Whether to extract doc comment summaries (first line only). Defaults to true.
    pub include_doc_comments: Option<bool>,
    /// Whether to extract structural relationships (e.g. implements, extends, receiver). Defaults to true.
    pub include_relationships: Option<bool>,
}

/// Response returned by an outline operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OutlineResponse {
    /// High-level architectural census.
    pub summary: OutlineSummary,
    /// Outlines per file.
    pub files: Vec<FileOutline>,
    /// Whether results were capped by symbol or file budget.
    pub truncated: bool,
}

/// Request parameters for reading a specific symbol.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ReadSymbolRequest {
    /// File path on disk.
    pub path: Option<String>,
    /// Optional direct source content (for in-memory buffers / unsaved code inspection).
    pub content: Option<String>,
    /// Symbol locator: bare name (e.g. "poll") or qualified path (e.g. "Heartbeat::poll", "Uart.write_byte").
    pub symbol: String,
    /// Optional symbol kind filter to disambiguate.
    pub kind: Option<SymbolKind>,
    /// 0-based occurrence index if multiple symbols match (defaults to 0 / first match).
    pub occurrence: Option<usize>,
    /// Optional surrounding context lines before/after symbol span. Defaults to 0.
    pub context_lines: Option<usize>,
}

/// Response returned by a read_symbol operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ReadSymbolResponse {
    /// Whether the symbol was found.
    pub found: bool,
    /// Relative or absolute path to file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Full qualified name of the symbol (e.g. "Heartbeat::poll").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<String>,
    /// Complete symbol metadata and span.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<Symbol>,
    /// Exact source code of the symbol (from start_byte to end_byte).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_code: Option<String>,
    /// Surrounding context lines before the symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_before: Option<String>,
    /// Surrounding context lines after the symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_after: Option<String>,
    /// Total number of matching symbols in the file.
    pub total_occurrences: usize,
    /// Diagnostic feedback or suggestions if symbol was not found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Detailed syntax error detected during AST preflight verification.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PatchSyntaxError {
    /// 1-based line where syntax error occurred.
    pub line: usize,
    /// 1-based column where syntax error occurred.
    pub column: usize,
    /// Error description.
    pub message: String,
    /// Snippet of the erroneous code or token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unexpected_token: Option<String>,
}

/// Request parameters for AST-guarded surgical patching.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PatchRequest {
    /// File path to patch.
    #[serde(alias = "file_path")]
    pub path: String,
    /// Target locator: symbol name (e.g. "Heartbeat::poll" or "SetupVmcsForProcessor").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_symbol: Option<String>,
    /// Occurrence index if multiple symbols share the name (0-based, default: 0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_occurrence: Option<usize>,
    /// Explicit source coordinate span to replace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_span: Option<SourceSpan>,
    /// Literal needle string to find and replace (if not targeting symbol or span).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_text: Option<String>,
    /// Optional in-memory code buffer (for unsaved buffer testing).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// New replacement code for the target.
    pub replacement: String,
    /// Validate AST with Tree-sitter before modifying disk (defaults to true).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate_ast: Option<bool>,
    /// If true, performs validation and diff calculation without writing to disk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
}

/// Response returned by a patch operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PatchResponse {
    /// Whether the patch was successfully applied (or successfully validated if dry_run).
    pub success: bool,
    /// Target file path.
    pub file: String,
    /// Exact coordinates replaced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_span: Option<SourceSpan>,
    /// Whether the syntax tree is valid after splicing.
    pub ast_valid: bool,
    /// Syntax errors detected during AST preflight.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub syntax_errors: Vec<PatchSyntaxError>,
    /// Unified diff showing the applied change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    /// Human-readable explanation / diagnostic message.
    pub message: String,
}

/// Request parameters for finding code symbol definitions across the workspace.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FindSymbolRequest {
    /// Symbol name or identifier pattern (e.g. "SetupVmcsForProcessor", "VmmContext", "poll").
    pub name: String,
    /// Optional directory or file path to search within. Defaults to current directory.
    pub path: Option<String>,
    /// Optional symbol kind filter (e.g. "function", "struct", "macro", "interface").
    pub kind: Option<SymbolKind>,
    /// Whether to require exact symbol name match. Defaults to true. If false, matches prefix/substring.
    pub exact: Option<bool>,
    /// Whether matching should be case-sensitive. Defaults to true for exact matches.
    pub case_sensitive: Option<bool>,
    /// Maximum number of matching symbols to return across the workspace. Defaults to 20.
    pub limit: Option<usize>,
    /// Optional file glob or extension filter (e.g. "*.c", "*.rs").
    pub file_pattern: Option<String>,
    /// Whether to search gitignored files. Defaults to false.
    pub include_ignored: Option<bool>,
}

/// A code symbol definition located across the workspace.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct FoundSymbol {
    /// Identifier name of the symbol.
    pub name: String,
    /// Full qualified name (e.g. "VmmContext::init", "SetupVmcsForProcessor").
    pub qualified_name: String,
    /// Symbol kind (function, struct, method, class, macro, etc.).
    pub kind: SymbolKind,
    /// Relative path to the file defining this symbol.
    pub file: String,
    /// Detected programming language.
    pub language: String,
    /// Exact source code location span.
    pub span: SourceSpan,
    /// Declaration signature (excluding implementation body).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// First line / summary of doc comments or docstring.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
    /// Symbol visibility (e.g. "public", "private", "pub(crate)").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    /// Whether this is an exact name match.
    pub is_exact: bool,
}

/// Response returned by a find_symbol operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct FindSymbolResponse {
    /// The query string searched for.
    pub query: String,
    /// Total matching definitions found.
    pub total_found: usize,
    /// List of matching symbols found across the workspace.
    pub symbols: Vec<FoundSymbol>,
    /// Breakdown of matched symbols by kind.
    pub kind_breakdown: BTreeMap<String, usize>,
    /// Breakdown of matched symbols by language.
    pub language_breakdown: BTreeMap<String, usize>,
    /// Whether results were capped due to the limit budget.
    pub truncated: bool,
}


