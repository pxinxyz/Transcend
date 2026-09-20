//! Zig Language Outline Adapter
//!
//! Extracts semantic symbols (functions, structs, enums, unions, constants) from Zig.

use transcend_protocol::{OutlineOptions, Symbol, SymbolKind};
use tree_sitter::{Node, Tree};

use super::{LanguageOutline, clean_signature, node_span, node_text};

pub struct ZigOutline;

impl Default for ZigOutline {
    fn default() -> Self {
        Self::new()
    }
}

impl ZigOutline {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageOutline for ZigOutline {
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if let Some(sym) = extract_zig_symbol(&child, source, 0, options) {
                symbols.push(sym);
            }
        }

        symbols
    }
}

fn extract_doc_comment(node: &Node, source: &[u8]) -> Option<String> {
    let prefix = std::str::from_utf8(&source[..node.start_byte()]).ok()?;
    let mut doc_lines = Vec::new();

    for line in prefix.lines().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with("///") {
            let clean = trimmed.trim_start_matches("///").trim();
            if !clean.is_empty() {
                doc_lines.push(clean);
            }
        } else if trimmed.starts_with("//!") {
            let clean = trimmed.trim_start_matches("//!").trim();
            if !clean.is_empty() {
                doc_lines.push(clean);
            }
        } else if trimmed.is_empty() && doc_lines.is_empty() {
            continue;
        } else {
            break;
        }
    }

    if doc_lines.is_empty() {
        None
    } else {
        doc_lines.reverse();
        doc_lines
            .into_iter()
            .find(|l| !l.is_empty())
            .map(|l| l.to_string())
    }
}

fn extract_signature(node: &Node, source: &[u8]) -> Option<String> {
    let text = node_text(node, source);
    if let Some(body_node) = node.child_by_field_name("body").or_else(|| {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .find(|c| c.kind() == "Block" || c.kind() == "block")
    }) {
        let body_start = body_node.start_byte();
        if body_start >= node.start_byte() {
            let sig_bytes = &source[node.start_byte()..body_start];
            let sig_str = std::str::from_utf8(sig_bytes).unwrap_or("");
            let cleaned = clean_signature(sig_str.trim().trim_end_matches('{').trim());
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        }
    }

    let first_line = text.lines().next().unwrap_or("").trim();
    let cleaned = clean_signature(first_line.trim_end_matches('{').trim());
    if !cleaned.is_empty() {
        Some(cleaned)
    } else {
        None
    }
}

fn extract_zig_symbol(
    node: &Node,
    source: &[u8],
    depth: usize,
    options: &OutlineOptions,
) -> Option<Symbol> {
    if let Some(max_depth) = options.max_depth
        && depth > max_depth
    {
        return None;
    }

    let text = node_text(node, source).trim();
    let is_pub = text.starts_with("pub ");

    if options.exported_only == Some(true) && !is_pub {
        return None;
    }

    // Zig FnProto / function declaration
    if node.kind() == "FnProto" || node.kind() == "fn_proto" || text.contains("fn ") {
        // Find identifier
        let mut name = None;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "IDENTIFIER" || child.kind() == "identifier" {
                name = Some(node_text(&child, source).trim());
                break;
            }
        }

        if let Some(name) = name {
            let kind = if depth > 0 {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            };
            if let Some(ref allowed) = options.symbol_kinds
                && !allowed.contains(&kind)
            {
                return None;
            }

            return Some(Symbol {
                name: name.to_string(),
                kind,
                span: node_span(node),
                signature: extract_signature(node, source),
                doc_comment: if options.include_doc_comments != Some(false) {
                    extract_doc_comment(node, source)
                } else {
                    None
                },
                visibility: Some(if is_pub {
                    "pub".to_string()
                } else {
                    "internal".to_string()
                }),
                relationships: Vec::new(),
                children: Vec::new(),
            });
        }
    }

    // Zig VarDecl / Container (struct, enum, union, error)
    if node.kind() == "VarDecl"
        || node.kind() == "var_decl"
        || text.starts_with("const ")
        || text.starts_with("pub const ")
    {
        let mut name = None;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "IDENTIFIER" || child.kind() == "identifier" {
                name = Some(node_text(&child, source).trim());
                break;
            }
        }

        if let Some(name) = name {
            let kind = if text.contains("struct") {
                SymbolKind::Struct
            } else if text.contains("enum") || text.contains("union") {
                // Zig unions are tagged variants, which map onto the enum kind.
                SymbolKind::Enum
            } else {
                SymbolKind::Constant
            };

            if let Some(ref allowed) = options.symbol_kinds
                && !allowed.contains(&kind)
            {
                return None;
            }

            // Extract inner members if struct/enum
            let mut children = Vec::new();
            if matches!(kind, SymbolKind::Struct | SymbolKind::Enum) {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if let Some(sym) = extract_zig_symbol(&child, source, depth + 1, options) {
                        children.push(sym);
                    }
                }
            }

            return Some(Symbol {
                name: name.to_string(),
                kind,
                span: node_span(node),
                signature: extract_signature(node, source),
                doc_comment: if options.include_doc_comments != Some(false) {
                    extract_doc_comment(node, source)
                } else {
                    None
                },
                visibility: Some(if is_pub {
                    "pub".to_string()
                } else {
                    "internal".to_string()
                }),
                relationships: Vec::new(),
                children,
            });
        }
    }

    None
}
