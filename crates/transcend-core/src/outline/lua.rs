//! Lua Language Outline Adapter
//!
//! Extracts semantic symbols (functions, methods, module tables) from Lua source code.

use tree_sitter::{Node, Tree};
use transcend_protocol::{OutlineOptions, Symbol, SymbolKind, SymbolRelationship};

use super::{clean_signature, node_span, node_text, LanguageOutline};

pub struct LuaOutline;

impl LuaOutline {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageOutline for LuaOutline {
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if let Some(sym) = extract_lua_symbol(&child, source, 0, options) {
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
        if trimmed.starts_with("---") {
            let clean = trimmed.trim_start_matches("---").trim();
            if !clean.is_empty() && !clean.starts_with('@') {
                doc_lines.push(clean);
            }
        } else if trimmed.starts_with("--") {
            let clean = trimmed.trim_start_matches("--").trim();
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
        doc_lines.into_iter().find(|l| !l.is_empty()).map(|l| l.to_string())
    }
}

fn extract_signature(node: &Node, source: &[u8]) -> Option<String> {
    let text = node_text(node, source);
    if let Some(body_node) = node.child_by_field_name("body").or_else(|| {
        let mut cursor = node.walk();
        node.children(&mut cursor).find(|c| c.kind() == "block")
    }) {
        let body_start = body_node.start_byte();
        if body_start >= node.start_byte() {
            let sig_bytes = &source[node.start_byte()..body_start];
            let sig_str = std::str::from_utf8(sig_bytes).unwrap_or("");
            let cleaned = clean_signature(sig_str.trim());
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        }
    }

    let first_line = text.lines().next().unwrap_or("").trim();
    let cleaned = clean_signature(first_line);
    if !cleaned.is_empty() {
        Some(cleaned)
    } else {
        None
    }
}

fn extract_lua_symbol(
    node: &Node,
    source: &[u8],
    depth: usize,
    options: &OutlineOptions,
) -> Option<Symbol> {
    if let Some(max_depth) = options.max_depth {
        if depth > max_depth {
            return None;
        }
    }

    match node.kind() {
        // Global / Module Functions (`function foo()`, `function M.bar()`, `function M:baz()`)
        "function_declaration" | "function_definition" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();

            let is_method = name.contains(':');
            let kind = if is_method {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            };

            if let Some(ref allowed) = options.symbol_kinds {
                if !allowed.contains(&kind) {
                    return None;
                }
            }

            let mut relationships = Vec::new();
            if is_method && options.include_relationships != Some(false) {
                if let Some(receiver) = name.split(':').next() {
                    relationships.push(SymbolRelationship {
                        relation: "receiver".to_string(),
                        target: receiver.to_string(),
                    });
                }
            }

            Some(Symbol {
                name,
                kind,
                span: node_span(node),
                signature: extract_signature(node, source),
                doc_comment: if options.include_doc_comments != Some(false) {
                    extract_doc_comment(node, source)
                } else {
                    None
                },
                visibility: Some("public".to_string()),
                relationships,
                children: Vec::new(),
            })
        }

        // Local Functions (`local function foo()`)
        "local_function_declaration" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();

            if options.exported_only == Some(true) {
                return None;
            }

            let kind = SymbolKind::Function;
            if let Some(ref allowed) = options.symbol_kinds {
                if !allowed.contains(&kind) {
                    return None;
                }
            }

            Some(Symbol {
                name,
                kind,
                span: node_span(node),
                signature: extract_signature(node, source),
                doc_comment: if options.include_doc_comments != Some(false) {
                    extract_doc_comment(node, source)
                } else {
                    None
                },
                visibility: Some("local".to_string()),
                relationships: Vec::new(),
                children: Vec::new(),
            })
        }

        _ => None,
    }
}
