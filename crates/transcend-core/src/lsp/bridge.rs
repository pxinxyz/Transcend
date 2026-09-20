//! Tree-sitter Coordinate Bridge
//!
//! Bridges high-level symbol queries (e.g. `symbol: "Engine::dispatch"`) into
//! exact 0-indexed `(line, character)` positions required by standard LSP servers.

use std::path::Path;
use tree_sitter::{Node, Parser};

/// Resolve an agent's symbol or position request into 0-based (line, character) coordinates.
pub struct SymbolCoordinateBridge;

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

        Self::search_node(&root, source_bytes, symbol_name)
    }

    fn search_node(node: &Node, source: &[u8], target: &str) -> Option<(u32, u32)> {
        let kind = node.kind();
        // Check if node is an identifier
        if (kind == "identifier" || kind == "type_identifier" || kind == "field_identifier")
            && node.start_byte() < node.end_byte()
            && node.end_byte() <= source.len()
            && let Ok(text) = std::str::from_utf8(&source[node.start_byte()..node.end_byte()])
            && text == target
        {
            let point = node.start_position();
            return Some((point.row as u32, point.column as u32));
        }

        // Recurse through children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = Self::search_node(&child, source, target) {
                return Some(found);
            }
        }

        None
    }

    /// Fast text search fallback if Tree-sitter AST misses the token.
    fn fallback_text_scan(content: &str, symbol_name: &str) -> Option<(u32, u32)> {
        for (line_idx, line) in content.lines().enumerate() {
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
                    return Some((line_idx as u32, col_idx as u32));
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
}
