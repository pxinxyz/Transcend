//! AST Code Outline Module
//!
//! Provides language-agnostic semantic symbol extraction using Tree-sitter parsers.

pub mod rust;
pub mod typescript;
pub mod python;
pub mod go;
pub mod c_cpp;
pub mod csharp;
pub mod java;
pub mod kotlin;
pub mod php;
pub mod ruby;
pub mod swift;
pub mod bash;
pub mod sql;
pub mod dart;
pub mod zig;
pub mod lua;
pub mod markdown;
pub mod scanner;

use tree_sitter::{Node, Point, Tree};
use transcend_protocol::{OutlineOptions, SourceSpan, Symbol};

/// Trait implemented by language-specific AST adapters.
pub trait LanguageOutline: Send + Sync {
    /// Extract canonical semantic symbols from a parsed Tree-sitter syntax tree.
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol>;
}

/// Convert a Tree-sitter AST node's coordinates into a 1-based `SourceSpan`.
pub fn node_span(node: &Node) -> SourceSpan {
    let start: Point = node.start_position();
    let end: Point = node.end_position();
    SourceSpan {
        start_line: start.row + 1,
        start_col: start.column + 1,
        end_line: end.row + 1,
        end_col: end.column + 1,
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    }
}

/// Extract UTF-8 text slice corresponding to a node.
pub fn node_text<'a>(node: &Node, source: &'a [u8]) -> &'a str {
    let range = node.byte_range();
    if range.end <= source.len() {
        std::str::from_utf8(&source[range]).unwrap_or("")
    } else {
        ""
    }
}

/// Clean up whitespace in a signature string (collapse consecutive spaces, remove newlines).
pub fn clean_signature(sig: &str) -> String {
    sig.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
