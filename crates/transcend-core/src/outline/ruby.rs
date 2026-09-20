//! Ruby Language Outline Adapter
//!
//! Extracts semantic symbols from Ruby source code using Tree-sitter.

use transcend_protocol::{OutlineOptions, Symbol, SymbolKind, SymbolRelationship};
use tree_sitter::{Node, Tree};

use super::{LanguageOutline, clean_signature, node_span, node_text};

pub struct RubyOutline;

impl Default for RubyOutline {
    fn default() -> Self {
        Self::new()
    }
}

impl RubyOutline {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageOutline for RubyOutline {
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if let Some(sym) = extract_ruby_symbol(&child, source, 0, options) {
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
        if trimmed.starts_with('#') {
            let clean = trimmed.trim_start_matches('#').trim();
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
    let first_line = text.lines().next().unwrap_or("").trim();
    let cleaned = clean_signature(first_line);
    if !cleaned.is_empty() {
        Some(cleaned)
    } else {
        None
    }
}

fn extract_ruby_symbol(
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

    match node.kind() {
        // Modules
        "module" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();
            let span = node_span(node);

            let mut children = Vec::new();
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if let Some(sym) = extract_ruby_symbol(&child, source, depth + 1, options) {
                        children.push(sym);
                    }
                }
            }

            Some(Symbol {
                name: name.clone(),
                kind: SymbolKind::Module,
                span,
                signature: Some(format!("module {}", name)),
                doc_comment: if options.include_doc_comments != Some(false) {
                    extract_doc_comment(node, source)
                } else {
                    None
                },
                visibility: Some("public".to_string()),
                relationships: Vec::new(),
                children,
            })
        }

        // Classes
        "class" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();

            let mut relationships = Vec::new();
            if let Some(super_node) = node.child_by_field_name("superclass") {
                let target = node_text(&super_node, source)
                    .trim_start_matches('<')
                    .trim()
                    .to_string();
                if !target.is_empty() {
                    relationships.push(SymbolRelationship {
                        relation: "extends".to_string(),
                        target,
                    });
                }
            }

            let span = node_span(node);
            let mut children = Vec::new();
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if let Some(sym) = extract_ruby_symbol(&child, source, depth + 1, options) {
                        children.push(sym);
                    }
                }
            }

            Some(Symbol {
                name,
                kind: SymbolKind::Class,
                span,
                signature: extract_signature(node, source),
                doc_comment: if options.include_doc_comments != Some(false) {
                    extract_doc_comment(node, source)
                } else {
                    None
                },
                visibility: Some("public".to_string()),
                relationships,
                children,
            })
        }

        // Methods
        "method" | "singleton_method" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();

            let kind = if depth > 0 {
                if name == "initialize" {
                    SymbolKind::Constructor
                } else {
                    SymbolKind::Method
                }
            } else {
                SymbolKind::Function
            };

            if let Some(ref allowed) = options.symbol_kinds
                && !allowed.contains(&kind)
            {
                return None;
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
                relationships: Vec::new(),
                children: Vec::new(),
            })
        }

        _ => None,
    }
}
