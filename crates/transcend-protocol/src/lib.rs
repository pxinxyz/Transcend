//! Transcend Protocol Definitions
//!
//! Strongly-typed request/response data contracts for Transcend MCP tools.
//! All request and response structures derive `schemars::JsonSchema` for
//! automated schema generation within the Model Context Protocol.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Request parameters for code searching.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SearchRequest {
    /// Regular expression or text pattern to search for.
    #[serde(alias = "query", alias = "regex")]
    pub pattern: String,
    /// Optional directory or file path to search within. Defaults to current directory.
    #[serde(
        alias = "dir",
        alias = "directory",
        alias = "file_path",
        alias = "search_path"
    )]
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
    /// Whether to include hidden files and directories (e.g. .github, .env). Defaults to false.
    pub include_hidden: Option<bool>,
    /// Whether to honour `.gitignore`. Defaults to true.
    ///
    /// Set to false to search ignored paths (vendored SDKs, generated code, build output).
    /// Independent of `include_hidden`: disabling it does not pull in dotfiles.
    pub respect_gitignore: Option<bool>,
    /// Optional maximum number of file clusters with empty matches to return before pruning (default: 10).
    pub max_empty_clusters: Option<usize>,
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
    ///
    /// Counting stops at a safety ceiling on pathologically broad patterns. When
    /// `count_capped` is true this value is a **lower bound**, not an exact count.
    pub total_matches: usize,
    /// Total number of distinct files containing matches.
    pub total_files: usize,
    /// File clusters containing line matches, grouped per file.
    pub files: Vec<FileCluster>,
    /// Macro-level directory distribution of matches (sorted by density).
    pub directory_radar: Vec<DirectoryRadar>,
    /// Whether individual line matches were capped due to the match budget.
    pub truncated: bool,
    /// Whether `total_matches` hit the internal safety ceiling and was cut short.
    ///
    /// A capped count depends on traversal scheduling, so it is not reproducible
    /// between runs. Narrow the pattern or scope the path when this is true.
    #[serde(default)]
    pub count_capped: bool,
}

/// Request parameters for file discovery.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FindRequest {
    /// Optional filename pattern (e.g. "*.rs", "main", "Cargo.*") or glob. If omitted, lists all files.
    #[serde(alias = "query", alias = "name", alias = "glob")]
    pub pattern: Option<String>,
    /// Optional root directory to begin search. Defaults to current directory.
    #[serde(alias = "dir", alias = "directory", alias = "search_path")]
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
    /// Whether to include hidden files and directories (e.g. .github, .env). Defaults to false.
    pub include_hidden: Option<bool>,
    /// Whether to honour `.gitignore`. Defaults to true.
    ///
    /// Set to false to discover ignored paths (vendored SDKs, generated code, build output).
    /// Independent of `include_hidden`: disabling it does not pull in dotfiles.
    pub respect_gitignore: Option<bool>,
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
    #[serde(
        alias = "file_path",
        alias = "file",
        alias = "dir",
        alias = "directory"
    )]
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
    /// Whether to include hidden files and directories (e.g. .github, .env). Defaults to false.
    pub include_hidden: Option<bool>,
    /// Whether to honour `.gitignore`. Defaults to true.
    ///
    /// Set to false to outline ignored paths (vendored SDKs, generated code, build output).
    /// Independent of `include_hidden`: disabling it does not pull in dotfiles.
    pub respect_gitignore: Option<bool>,
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
    #[serde(alias = "file", alias = "file_path")]
    pub path: Option<String>,
    /// Optional direct source content (for in-memory buffers / unsaved code inspection).
    pub content: Option<String>,
    /// Symbol locator: bare name (e.g. "poll") or qualified path (e.g. "Heartbeat::poll", "Uart.write_byte").
    #[serde(alias = "name", alias = "symbol_name", alias = "query")]
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

/// Splicing mode for applying patch changes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PatchMode {
    /// Replace the target symbol, span, or needle text (default).
    #[default]
    Replace,
    /// Insert replacement immediately before target symbol, span, or needle text.
    InsertBefore,
    /// Insert replacement immediately after target symbol, span, or needle text.
    InsertAfter,
    /// Insert replacement at the beginning of the target symbol's body.
    PrependToSymbol,
    /// Insert replacement at the end of the target symbol's body.
    AppendToSymbol,
}

/// Request parameters for AST-guarded surgical patching.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PatchRequest {
    /// File path to patch.
    #[serde(alias = "file_path", alias = "file", alias = "target_file")]
    pub path: String,
    /// Splicing mode: "replace" (default), "insert_before", "insert_after", "prepend_to_symbol", or "append_to_symbol".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<PatchMode>,
    /// Target locator: symbol name (e.g. "Heartbeat::poll" or "SetupVmcsForProcessor").
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "symbol",
        alias = "name"
    )]
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
    /// Optional workspace root boundary to guard against path traversal escape.
    /// Defaults to the engine's active workspace root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
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

/// Request parameters for multi-file atomic batch patching.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct BatchPatchRequest {
    /// Ordered list of patch requests across files.
    pub patches: Vec<PatchRequest>,
    /// Validate all modified ASTs with Tree-sitter before modifying any file on disk. Defaults to true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate_ast: Option<bool>,
    /// If true, performs validation and diff calculation without writing to disk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
    /// Optional workspace root boundary to guard against path traversal escape.
    /// Defaults to the engine's active workspace root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
}

/// Response returned by a batch patch operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct BatchPatchResponse {
    /// Whether all patches in the batch succeeded and passed AST validation.
    pub success: bool,
    /// Individual patch responses corresponding to each patch request.
    pub results: Vec<PatchResponse>,
    /// Number of distinct files patched.
    pub total_files_patched: usize,
    /// Whether all resulting files have valid ASTs.
    pub all_ast_valid: bool,
    /// Accumulated syntax errors across any files that failed AST preflight.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub syntax_errors: Vec<PatchSyntaxError>,
    /// Combined unified diff across all touched files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    /// Summary status message.
    pub message: String,
}

/// Request parameters for finding code symbol definitions across the workspace.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FindSymbolRequest {
    /// Symbol name or identifier pattern (e.g. "SetupVmcsForProcessor", "VmmContext", "poll").
    #[serde(alias = "symbol", alias = "query", alias = "pattern")]
    pub name: String,
    /// Optional directory or file path to search within. Defaults to current directory.
    #[serde(
        alias = "file",
        alias = "file_path",
        alias = "dir",
        alias = "directory"
    )]
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
    /// Whether to search hidden files and directories. Defaults to false.
    pub include_hidden: Option<bool>,
    /// Whether to enable smart casing and snake_case <-> camelCase fuzzy token matching. Defaults to true when exact is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fuzzy: Option<bool>,
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

/// Request to locate the compiler-resolved definition of a symbol or position.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspDefinitionRequest {
    /// File path where the symbol or position is referenced.
    #[serde(alias = "file", alias = "file_path")]
    pub path: String,
    /// Identifier name or symbol to find definition for (e.g. "poll", "Config::new").
    /// If provided, Tree-sitter resolves its coordinate in the file before querying LSP.
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "name",
        alias = "query"
    )]
    pub symbol: Option<String>,
    /// 1-based line number (optional if symbol is provided).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// 1-based column number (optional if symbol is provided).
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "col",
        alias = "column"
    )]
    pub character: Option<usize>,
}

/// A target location returned by an LSP definition query.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspTargetLocation {
    /// Relative or absolute path to the target file.
    pub file: String,
    /// Exact target location span.
    pub span: SourceSpan,
    /// Preview snippet of the definition line(s).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

/// Response returned by an LSP definition query.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspDefinitionResponse {
    /// Target definitions found (typically 1, or multiple for overloads/traits).
    pub targets: Vec<LspTargetLocation>,
    /// Underlying resolution engine: "lsp:<server>" or "tree-sitter:heuristic".
    pub engine: String,
}

/// Request to find all compiler-resolved references and call sites across the workspace.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspReferencesRequest {
    /// File path where the symbol or position is referenced.
    #[serde(alias = "file", alias = "file_path")]
    pub path: String,
    /// Identifier name or symbol to find references for.
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "name",
        alias = "query"
    )]
    pub symbol: Option<String>,
    /// 1-based line number (optional if symbol is provided).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// 1-based column number (optional if symbol is provided).
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "col",
        alias = "column"
    )]
    pub character: Option<usize>,
    /// Whether to include the declaration/definition itself in the results. Defaults to false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_declaration: Option<bool>,
    /// Maximum number of references to return. Defaults to 50.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// A single reference occurrence.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspReferenceLocation {
    /// File path where the reference occurs.
    pub file: String,
    /// Exact location span.
    pub span: SourceSpan,
    /// Preview snippet of the referencing line.
    pub line_text: String,
}

/// Response returned by an LSP references query.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspReferencesResponse {
    /// Total count of references found.
    pub total_found: usize,
    /// References list.
    pub references: Vec<LspReferenceLocation>,
    /// Whether references were capped by limit.
    pub truncated: bool,
    /// Underlying resolution engine: "lsp:<server>" or "tree-sitter:heuristic".
    pub engine: String,
}

/// Request to inspect inferred type signature and documentation for a symbol or position.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspHoverRequest {
    /// File path to inspect.
    #[serde(alias = "file", alias = "file_path")]
    pub path: String,
    /// Identifier name or symbol to hover over.
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "name",
        alias = "query"
    )]
    pub symbol: Option<String>,
    /// 1-based line number (optional if symbol is provided).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// 1-based column number (optional if symbol is provided).
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "col",
        alias = "column"
    )]
    pub character: Option<usize>,
}

/// Response returned by an LSP hover query.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspHoverResponse {
    /// Resolved signature or type description (e.g. "fn poll(&mut self) -> Poll<Result<()>>").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Documentation text or doc comments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    /// Exact span of the hovered token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<SourceSpan>,
    /// Underlying resolution engine: "lsp:<server>" or "tree-sitter:heuristic".
    pub engine: String,
}

/// Severity level of an LSP diagnostic.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

/// A structured compiler diagnostic message.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspDiagnosticItem {
    /// File path where the diagnostic was issued.
    pub file: String,
    /// Diagnostic severity (error, warning, information, hint).
    pub severity: DiagnosticSeverity,
    /// Exact source code location span.
    pub span: SourceSpan,
    /// Human-readable compiler/linter message.
    pub message: String,
    /// Optional error code (e.g. "E0308", "unused_variables").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Source compiler/tool (e.g. "rustc", "pyright", "gopls").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Request to retrieve compiler diagnostics.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspDiagnosticsRequest {
    /// File or directory path to retrieve diagnostics for. If omitted, returns all workspace diagnostics.
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "file",
        alias = "file_path",
        alias = "dir"
    )]
    pub path: Option<String>,
    /// Optional severity filter (e.g. only return errors).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<DiagnosticSeverity>,
}

/// Response returned by an LSP diagnostics query.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspDiagnosticsResponse {
    /// Total count of diagnostics matching filter.
    pub total_count: usize,
    /// Structured diagnostic items.
    pub diagnostics: Vec<LspDiagnosticItem>,
    /// Breakdown of diagnostics by severity.
    pub severity_breakdown: BTreeMap<String, usize>,
}

/// Request to audit status and installation recipes of official language servers.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspStatusRequest {
    /// Optional language filter (e.g. "rust", "typescript", "python", "go").
    /// If omitted, returns status for all 18 supported languages.
    #[serde(skip_serializing_if = "Option::is_none", alias = "lang")]
    pub language: Option<String>,
}

/// An installation recipe for a language server.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspInstallRecipe {
    /// Package manager or installation tool (e.g. "npm", "rustup", "cargo", "pip", "go", "winget", "brew").
    pub manager: String,
    /// Exact shell command for installation.
    pub command: String,
    /// Whether the required package manager is available on the host system.
    pub available: bool,
    /// Human-readable explanation or package notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Current status and installation profile for a language server.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspServerStatus {
    /// Canonical language identifier (e.g. "rust", "typescript", "python").
    pub language: String,
    /// Primary candidate binary name (e.g. "rust-analyzer", "pyright-langserver").
    pub primary_binary: String,
    /// Whether any candidate binary is currently installed and discoverable.
    pub installed: bool,
    /// Resolved executable binary name if installed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary: Option<String>,
    /// Full path to the executable if installed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Reported version string (e.g. "rust-analyzer 1.85.0").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Available installation recipes for this language server.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub install_methods: Vec<LspInstallRecipe>,
}

/// Response returned by an LSP status query.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspStatusResponse {
    /// List of language server profiles and their current host status.
    pub servers: Vec<LspServerStatus>,
    /// Total count of servers inspected.
    pub total_servers: usize,
    /// Count of servers currently installed.
    pub total_installed: usize,
}

/// Request to install a language server.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspInstallRequest {
    /// Canonical language ID to install (e.g. "rust", "typescript", "python", "go").
    #[serde(alias = "lang")]
    pub language: String,
    /// Specific package manager or recipe to use (e.g. "npm", "rustup", "cargo", "pip", "go", "auto").
    /// Defaults to "auto", selecting the highest-priority available package manager.
    #[serde(skip_serializing_if = "Option::is_none", alias = "manager")]
    pub method: Option<String>,
}

/// Response returned by an LSP installation operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspInstallResponse {
    /// Whether installation succeeded and the binary is now discoverable.
    pub success: bool,
    /// Canonical language ID.
    pub language: String,
    /// Resolved binary name after installation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary: Option<String>,
    /// Absolute path to the newly installed binary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Version reported by the newly installed binary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Stdout and stderr logs captured during execution.
    pub output: String,
    /// Human-readable summary message.
    pub message: String,
}

// =========================================================================
// Terminal & Execution Subsystem Contracts
// =========================================================================

/// Transport mechanism for terminal execution.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExecTransport {
    /// Automatically select: pipes for standard non-interactive commands, PTY for commands requiring terminal emulation.
    #[default]
    Auto,
    /// Standard OS pipes (stdout/stderr captured cleanly without terminal control codes).
    Pipe,
    /// Pseudoterminal (ConPTY on Windows, openpty on Unix) for interactive sessions, REPLs, and TTY-aware tools.
    Pty,
}

/// Action to take when command execution reaches the timeout threshold.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TimeoutAction {
    /// Detach into an active background session and return session_id for subsequent streaming.
    #[default]
    Detach,
    /// Forcibly terminate the process tree.
    Kill,
    /// Abort and return an execution error.
    Error,
}

/// Execution status of a command.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExecStatus {
    /// Command finished and process exited cleanly.
    #[default]
    Exited,
    /// Execution timed out and process was detached into an active background session.
    Detached,
    /// Process failed to spawn or encountered an OS runtime failure.
    Failed,
}

/// Request to execute a command via the hybrid terminal engine.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ExecRequest {
    /// Shell command string to execute.
    #[serde(alias = "cmd")]
    pub command: String,
    /// Working directory for execution. If omitted, defaults to the current workspace root.
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "working_directory",
        alias = "dir"
    )]
    pub cwd: Option<String>,
    /// Transport mode: "auto" (default), "pipe", or "pty".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<ExecTransport>,
    /// Milliseconds to wait synchronously before applying timeout_action (default: 10,000ms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// Action upon reaching timeout: "detach" (default), "kill", or "error".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_action: Option<TimeoutAction>,
    /// Optional shell executable override (e.g. "powershell", "pwsh", "cmd", "bash", "sh").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    /// Maximum bytes of output to return in the response (default: 32,768 bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_bytes: Option<usize>,
    /// If true, executes the command binary directly without shell wrapping.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<bool>,
}

/// Response returned from an execution request.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ExecResponse {
    /// Execution outcome: "exited", "detached", or "failed".
    pub status: ExecStatus,
    /// Exit code if process completed, or null if detached/running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Ephemeral session identifier assigned if the command was detached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Normalized, token-compact terminal output.
    pub output: String,
    /// Current read cursor position in the session output stream.
    pub cursor: usize,
    /// Whether the output was truncated by max_output_bytes.
    pub truncated: bool,
    /// Total wall-clock execution time elapsed in milliseconds.
    pub elapsed_ms: u64,
}

/// Status of an active background terminal session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TerminalSessionStatus {
    /// Process is actively running.
    #[default]
    Running,
    /// Process has exited.
    Exited,
    /// Process failed or crashed.
    Failed,
}

/// Request to read incremental output from an active background session.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TerminalReadRequest {
    /// Identifier of the session to read from.
    #[serde(alias = "id")]
    pub session_id: String,
    /// Read cursor offset. Only output appended after this cursor will be returned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<usize>,
    /// Maximum bytes of output to return in this chunk (default: 16,384 bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
    /// Optional timeout in milliseconds to wait for new output if buffer has no new data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// Optional pattern to await in the terminal output stream before returning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_for_pattern: Option<String>,
}

/// Response containing incremental terminal output.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TerminalReadResponse {
    /// Session identifier.
    pub session_id: String,
    /// Current lifecycle status of the session.
    pub status: TerminalSessionStatus,
    /// Incremental output since provided cursor.
    pub output: String,
    /// Updated cursor position for subsequent reads.
    pub next_cursor: usize,
    /// Process exit code if completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Whether output chunk was truncated by max_bytes.
    pub truncated: bool,
}

/// Request to send interactive input to a running terminal session.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TerminalWriteRequest {
    /// Identifier of the active session.
    #[serde(alias = "id")]
    pub session_id: String,
    /// Raw text or control characters to send to the terminal stdin (e.g. "y\n", "\x03" for Ctrl+C).
    pub input: String,
}

/// Response returned from an interactive write operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TerminalWriteResponse {
    /// Session identifier.
    pub session_id: String,
    /// Number of bytes successfully dispatched to terminal stdin.
    pub bytes_written: usize,
}

/// Request to resize terminal dimensions.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TerminalResizeRequest {
    /// Identifier of the active session.
    #[serde(alias = "id")]
    pub session_id: String,
    /// Terminal column width (e.g. 120).
    pub cols: u16,
    /// Terminal row height (e.g. 30).
    pub rows: u16,
}

/// Response returned from a resize operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TerminalResizeResponse {
    /// Session identifier.
    pub session_id: String,
    /// Whether the resize operation succeeded.
    pub success: bool,
}

/// Request to terminate a background terminal session.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TerminalKillRequest {
    /// Identifier of the session to terminate.
    #[serde(alias = "id")]
    pub session_id: String,
}

/// Response returned from a session kill operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TerminalKillResponse {
    /// Session identifier.
    pub session_id: String,
    /// Whether the session process tree was successfully terminated.
    pub success: bool,
    /// Final exit code if captured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Remaining unread output before process termination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_output: Option<String>,
}

// =========================================================================
// File Lifecycle & Content Operation Contracts
// =========================================================================

/// Request parameters for structured, token-bounded file reading.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ReadFileRequest {
    /// Path to file on disk.
    #[serde(alias = "file", alias = "file_path")]
    pub path: String,
    /// Optional 1-based start line (inclusive). Defaults to 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    /// Optional 1-based end line (inclusive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    /// Maximum bytes of content to return before clipping (default: 65,536 bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
    /// Whether to prefix returned lines with 1-based line numbers (e.g. "   1 | fn main() {"). Defaults to false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_numbers: Option<bool>,
}

/// Response returned by a read_file operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ReadFileResponse {
    /// Target file path.
    pub file: String,
    /// Extracted text content.
    pub content: String,
    /// 1-based start line actually returned.
    pub start_line: usize,
    /// 1-based end line actually returned.
    pub end_line: usize,
    /// Total number of lines in the file.
    pub total_lines: usize,
    /// Total size of the file on disk in bytes.
    pub size_bytes: u64,
    /// Whether content was truncated by line range or max_bytes budget.
    pub truncated: bool,
    /// Whether file was identified as binary (containing NUL bytes).
    pub is_binary: bool,
    /// Optional status or diagnostic message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Request parameters for atomic file writing.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct WriteFileRequest {
    /// Destination file path.
    #[serde(alias = "file", alias = "file_path", alias = "target_file")]
    pub path: String,
    /// Text content to write.
    pub content: String,
    /// Whether to overwrite if the file already exists. Defaults to false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overwrite: Option<bool>,
    /// Whether to automatically create missing parent directories. Defaults to true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_parents: Option<bool>,
    /// Optional workspace root boundary to guard against path traversal escape.
    /// Defaults to the engine's active workspace root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
}

/// Response returned by a write_file operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct WriteFileResponse {
    /// Destination file path.
    pub file: String,
    /// Whether the write succeeded.
    pub success: bool,
    /// Number of bytes written to disk.
    pub bytes_written: usize,
    /// Whether this write created a new file (true) or updated an existing file (false).
    pub created_new: bool,
    /// Status or diagnostic message.
    pub message: String,
}

/// Request parameters for safe workspace path deletion.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DeletePathRequest {
    /// File or directory path to delete.
    #[serde(alias = "file", alias = "file_path", alias = "target_path")]
    pub path: String,
    /// Whether to recursively delete non-empty directories. Defaults to false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recursive: Option<bool>,
    /// Optional workspace root boundary to guard against path traversal escape. Defaults to current directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
}

/// Response returned by a delete_path operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DeletePathResponse {
    /// Deleted path.
    pub path: String,
    /// Whether the deletion succeeded.
    pub success: bool,
    /// Whether the deleted target was a directory.
    pub is_directory: bool,
    /// Number of items deleted (1 for single file, or count of deleted entries for recursive dir).
    pub deleted_count: usize,
    /// Status or diagnostic message.
    pub message: String,
}

/// Request parameters to configure the active project workspace root directory.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SetWorkspaceRequest {
    /// Workspace root directory path.
    #[serde(alias = "workspace_root", alias = "dir", alias = "directory")]
    pub path: String,
}

/// Response returned by a set_workspace operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SetWorkspaceResponse {
    /// Whether the workspace root was successfully set.
    pub success: bool,
    /// Canonical path of the active workspace root directory.
    pub workspace_root: String,
    /// Status or diagnostic message.
    pub message: String,
}

// =========================================================================
// Version Control & Git Operation Contracts
// =========================================================================

/// Status of a version-controlled file in git.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GitFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    TypeChanged,
    Untracked,
    Conflicted,
}

/// A version-controlled file entry with staging state.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct GitFileEntry {
    /// Relative path from repository root.
    pub path: String,
    /// Git working tree / index status.
    pub status: GitFileStatus,
    /// Original path if renamed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_path: Option<String>,
}

/// Request parameters to inspect git status.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct GitStatusRequest {
    /// Optional directory path within the git repository. Defaults to active workspace root.
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "dir",
        alias = "directory",
        alias = "workspace_root"
    )]
    pub path: Option<String>,
}

/// Response containing structured, token-compact git repository status.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct GitStatusResponse {
    /// Whether the directory is inside a git repository.
    pub is_git_repo: bool,
    /// Current checked-out branch name or commit SHA (e.g. "main", "HEAD (detached)").
    pub branch: String,
    /// Upstream tracking branch if configured (e.g. "origin/main").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    /// Number of commits ahead of upstream.
    pub ahead: usize,
    /// Number of commits behind upstream.
    pub behind: usize,
    /// Staged file changes ready to commit.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub staged: Vec<GitFileEntry>,
    /// Unstaged modifications in working tree.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub unstaged: Vec<GitFileEntry>,
    /// Untracked files not yet added to version control.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub untracked: Vec<String>,
    /// Files with unresolved merge conflicts.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub conflicted: Vec<String>,
    /// Clean status indicator (true if working tree has no changes).
    pub is_clean: bool,
}
