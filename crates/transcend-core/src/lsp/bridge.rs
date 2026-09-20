//! Tree-sitter Coordinate Bridge
//!
//! Bridges high-level symbol queries (e.g. `symbol: "Engine::dispatch"`) into
//! exact 0-indexed `(line, character)` positions required by standard LSP servers.

use std::path::Path;
use tree_sitter::{Node, Parser};

/// Resolve an agent's symbol or position request into 0-based (line, character) coordinates.
pub struct SymbolCoordinateBridge;

/// Convert a byte offset within `line` into an LSP `character` offset, which is measured
/// in **UTF-16 code units** (LSP 3.17, `Position.character`), not bytes and not chars.
///
/// Tree-sitter reports byte columns and `str::find` returns byte indices, so every column
/// this module derives from source text must be translated before it reaches a language
/// server. Without this, any non-ASCII text earlier on the line shifts the position.
///
/// The result is clamped so that `character <= line.len()` holds in UTF-16 units, which is
/// the range bound servers enforce.
fn utf16_column(line: &str, byte_col: usize) -> u32 {
    let clamped = byte_col.min(line.len());
    // A byte offset inside a multi-byte character has no valid UTF-16 equivalent; fall back
    // to the byte offset rather than inventing a position.
    if !line.is_char_boundary(clamped) {
        return clamped as u32;
    }
    line[..clamped].encode_utf16().count() as u32
}

/// A precomputed index of line contents, for O(1) row lookup during recursive AST walks.
///
/// Locating a node's line by rescanning the source from the top on every visit would turn
/// a deep symbol search into O(nodes x lines); splitting once up front keeps the walk linear.
///
/// Lines are split on `\n` alone -- deliberately NOT `str::lines()`, which also strips a
/// trailing `\r`. Tree-sitter counts the `\r` as part of the line, so stripping it would
/// shorten the line and make every byte column on a CRLF file fall out of range.
struct LineIndex<'a> {
    lines: Vec<&'a str>,
}

impl<'a> LineIndex<'a> {
    fn line(&self, row: usize) -> &'a str {
        self.lines.get(row).copied().unwrap_or("")
    }
}

fn line_starts(content: &str) -> LineIndex<'_> {
    LineIndex {
        lines: content.split('\n').collect(),
    }
}

impl SymbolCoordinateBridge {
    /// Resolve location from either explicit (line, col) or symbol identifier lookup.
    pub fn resolve_position(
        file_path: &Path,
        content: &str,
        symbol: Option<&str>,
        line_1_based: Option<usize>,
        col_1_based: Option<usize>,
    ) -> Result<(u32, u32), String> {
        // 1. If line is provided, convert 1-based to 0-based
        if let Some(l) = line_1_based {
            let l_0 = (l.saturating_sub(1)) as u32;
            let c_0 = (col_1_based.unwrap_or(1).saturating_sub(1)) as u32;
            return Ok((l_0, c_0));
        }

        // 2. If symbol is provided, use Tree-sitter to find its exact location
        let Some(sym_name) = symbol else {
            return Err("Either 'symbol' or 'line' must be specified".to_string());
        };

        let bare_name = sym_name.split("::").last().unwrap_or(sym_name).trim();

        if let Some(pos) = Self::find_symbol_position(file_path, content, bare_name) {
            return Ok(pos);
        }

        // 3. Fallback: simple text scanner for symbol identifier token
        if let Some(pos) = Self::fallback_text_scan(content, bare_name) {
            return Ok(pos);
        }

        Err(format!(
            "Symbol '{sym_name}' not found in {}",
            file_path.display()
        ))
    }

    /// Use Tree-sitter AST to find the definition or reference node for the symbol.
    fn find_symbol_position(
        file_path: &Path,
        content: &str,
        symbol_name: &str,
    ) -> Option<(u32, u32)> {
        let ext = file_path.extension()?.to_str()?.to_lowercase();
        let mut parser = Parser::new();

        let lang = match ext.as_str() {
            "rs" => tree_sitter_rust::LANGUAGE.into(),
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => {
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
            }
            "py" | "pyi" => tree_sitter_python::LANGUAGE.into(),
            "go" => tree_sitter_go::LANGUAGE.into(),
            "c" | "h" => tree_sitter_c::LANGUAGE.into(),
            "cpp" | "hpp" | "cc" | "cxx" | "hxx" => tree_sitter_cpp::LANGUAGE.into(),
            "cs" => tree_sitter_c_sharp::LANGUAGE.into(),
            "java" => tree_sitter_java::LANGUAGE.into(),
            "kt" | "kts" => tree_sitter_kotlin_ng::LANGUAGE.into(),
            "php" => tree_sitter_php::LANGUAGE_PHP.into(),
            "rb" | "rake" | "gemspec" => tree_sitter_ruby::LANGUAGE.into(),
            "swift" => tree_sitter_swift::LANGUAGE.into(),
            "sh" | "bash" => tree_sitter_bash::LANGUAGE.into(),
            "sql" => tree_sitter_sequel::LANGUAGE.into(),
            "dart" => tree_sitter_dart::LANGUAGE.into(),
            "zig" => tree_sitter_zig::LANGUAGE.into(),
            "lua" => tree_sitter_lua::LANGUAGE.into(),
            "md" | "markdown" => tree_sitter_md::LANGUAGE.into(),
            _ => return None,
        };

        if parser.set_language(&lang).is_err() {
            return None;
        }

        let tree = parser.parse(content, None)?;
        let root = tree.root_node();
        let source_bytes = content.as_bytes();

        Self::search_node(&root, source_bytes, &line_starts(content), symbol_name)
    }

    fn search_node(
        node: &Node,
        source: &[u8],
        lines: &LineIndex,
        target: &str,
    ) -> Option<(u32, u32)> {
        let kind = node.kind();
        // Check if node is an identifier
        if (kind == "identifier" || kind == "type_identifier" || kind == "field_identifier")
            && node.start_byte() < node.end_byte()
            && node.end_byte() <= source.len()
            && let Ok(text) = std::str::from_utf8(&source[node.start_byte()..node.end_byte()])
            && text == target
        {
            let point = node.start_position();
            let (row, byte_col) = (point.row, point.column);
            return Some((row as u32, utf16_column(lines.line(row), byte_col)));
        }

        // Recurse through children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = Self::search_node(&child, source, lines, target) {
                return Some(found);
            }
        }

        None
    }

    /// Fast text search fallback if Tree-sitter AST misses the token.
    ///
    /// Splits on `\n` alone so a CRLF file keeps its `\r`, matching how line spans and
    /// columns are counted everywhere else in the workspace.
    fn fallback_text_scan(content: &str, symbol_name: &str) -> Option<(u32, u32)> {
        for (line_idx, line) in content.split('\n').enumerate() {
            if let Some(col_idx) = line.find(symbol_name) {
                // Ensure word boundary before and after
                let before_ok = col_idx == 0
                    || !line[..col_idx]
                        .chars()
                        .last()
                        .unwrap_or(' ')
                        .is_alphanumeric();
                let after_idx = col_idx + symbol_name.len();
                let after_ok = after_idx >= line.len()
                    || !line[after_idx..]
                        .chars()
                        .next()
                        .unwrap_or(' ')
                        .is_alphanumeric();

                if before_ok && after_ok {
                    return Some((line_idx as u32, utf16_column(line, col_idx)));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_sitter_coordinate_bridge() {
        let code = r#"
pub struct DataStore {
    capacity: usize,
}

impl DataStore {
    pub fn save_item(&self, id: u64) -> bool {
        true
    }
}
"#;
        let path = Path::new("src/store.rs");
        let pos =
            SymbolCoordinateBridge::resolve_position(path, code, Some("save_item"), None, None)
                .expect("should find save_item coordinate");

        // Line 6 in 0-based is row 6
        assert_eq!(pos.0, 6);
        assert!(pos.1 >= 4);
    }

    #[test]
    fn test_explicit_line_coordinate() {
        let path = Path::new("src/store.rs");
        let pos = SymbolCoordinateBridge::resolve_position(path, "", None, Some(10), Some(5))
            .expect("should convert 1-based to 0-based");
        assert_eq!(pos, (9, 4));
    }

    #[test]
    fn test_bridge_multilingual_coordinate() {
        let py_code = "def compute_sum(a, b):\n    return a + b\n";
        let py_path = Path::new("main.py");
        let pos = SymbolCoordinateBridge::resolve_position(
            py_path,
            py_code,
            Some("compute_sum"),
            None,
            None,
        )
        .expect("should find compute_sum");
        assert_eq!(pos.0, 0);

        let java_code = "class Greeter {\n    public void sayHello() {\n    }\n}\n";
        let java_path = Path::new("Greeter.java");
        let pos = SymbolCoordinateBridge::resolve_position(
            java_path,
            java_code,
            Some("sayHello"),
            None,
            None,
        )
        .expect("should find sayHello");
        assert_eq!(pos.0, 1);
    }

    /// LSP `character` is a UTF-16 code-unit offset, but Tree-sitter and `str::find` both
    /// report byte columns. On a line containing multi-byte text before the symbol the two
    /// disagree, and the naive byte value sends the language server to the wrong column.
    #[test]
    fn test_non_ascii_prefix_uses_utf16_columns_not_bytes() {
        // "🎉" is 4 bytes but 2 UTF-16 code units; "é" is 2 bytes and 1 code unit.
        let prefix = "// 🎉 café ";
        let code = format!("{prefix}fn target() {{}}\n");
        let path = Path::new("src/lib.rs");
        let (row, character) =
            SymbolCoordinateBridge::resolve_position(path, &code, Some("target"), None, None)
                .expect("should find target after a non-ASCII prefix");

        // "target" sits after the prefix AND the `fn ` keyword.
        let before_symbol = format!("{prefix}fn ");
        let expected = before_symbol.encode_utf16().count() as u32;
        let byte_column = before_symbol.len() as u32;

        assert_eq!(row, 0);
        assert_eq!(
            character, expected,
            "column must be the UTF-16 code-unit offset (byte col would be {byte_column})"
        );
        // Guard the test itself: if the prefix were pure ASCII the two would coincide and
        // this test would pass even with the bug present.
        assert_ne!(
            expected, byte_column,
            "fixture is vacuous -- prefix must contain multi-byte characters"
        );
        assert_eq!(character, 14);
    }

    /// The text-scan fallback shares the byte-vs-UTF-16 hazard; cover it directly.
    #[test]
    fn test_fallback_text_scan_uses_utf16_columns() {
        let prefix = "// 日本語 ";
        let line = format!("{prefix}fallback_target\n");
        let (_row, character) =
            SymbolCoordinateBridge::fallback_text_scan(&line, "fallback_target")
                .expect("fallback scan should find the token");
        assert_eq!(character, prefix.encode_utf16().count() as u32);
        assert_ne!(character, prefix.len() as u32, "fixture is vacuous");
    }

    /// A CRLF file is the case that surfaced this bug: `str::lines()` strips the `\r`, which
    /// shortens the line and pushes Tree-sitter's byte column out of range, silently falling
    /// back to a raw byte offset. Columns must match the LF variant exactly.
    #[test]
    fn test_crlf_files_report_the_same_column_as_lf() {
        let prefix = "// 🎉 ";
        let lf = format!("{prefix}fn target() {{}}\n");
        let crlf = format!("{prefix}fn target() {{}}\r\n");
        let path = Path::new("src/lib.rs");

        let lf_pos =
            SymbolCoordinateBridge::resolve_position(path, &lf, Some("target"), None, None)
                .expect("LF variant should resolve");
        let crlf_pos =
            SymbolCoordinateBridge::resolve_position(path, &crlf, Some("target"), None, None)
                .expect("CRLF variant should resolve");

        assert_eq!(
            lf_pos, crlf_pos,
            "line endings must not change the reported position"
        );
        let before_symbol = format!("{prefix}fn ");
        assert_eq!(crlf_pos.1, before_symbol.encode_utf16().count() as u32);
        assert_ne!(
            crlf_pos.1,
            before_symbol.len() as u32,
            "fixture is vacuous -- prefix must contain multi-byte characters"
        );
    }

    #[test]
    fn test_utf16_column_is_clamped_and_boundary_safe() {
        let line = "ab🎉";
        assert_eq!(utf16_column(line, 0), 0);
        assert_eq!(utf16_column(line, 2), 2, "before the multi-byte char");
        assert_eq!(utf16_column(line, 6), 4, "after it: 2 + 2 code units");
        // Past the end clamps rather than overrunning the line.
        assert_eq!(utf16_column(line, 999), 4);
        // A byte offset inside a multi-byte character has no UTF-16 equivalent; the helper
        // must not panic and must not invent a position beyond the line length.
        assert!(utf16_column(line, 3) <= 4);
    }
}
