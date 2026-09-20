//! Kotlin Language Outline Adapter
//!
//! Extracts semantic symbols from Kotlin source code using Tree-sitter.

use transcend_protocol::{OutlineOptions, Symbol, SymbolKind, SymbolRelationship};
use tree_sitter::{Node, Tree};

use super::{LanguageOutline, clean_signature, node_span, node_text};

pub struct KotlinOutline;

impl Default for KotlinOutline {
    fn default() -> Self {
        Self::new()
    }
}

impl KotlinOutline {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageOutline for KotlinOutline {
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if let Some(sym) = extract_kt_symbol(&child, source, 0, options) {
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
        if trimmed.starts_with("/**")
            || trimmed.starts_with("/*")
            || trimmed.starts_with("*/")
            || trimmed.starts_with('*')
        {
            let clean = trimmed
                .trim_start_matches("/**")
                .trim_start_matches("/*")
                .trim_end_matches("*/")
                .trim_start_matches('*')
                .trim();
            if !clean.is_empty() && !clean.starts_with('@') {
                doc_lines.push(clean);
            }
        } else if trimmed.starts_with("//") {
            let clean = trimmed.trim_start_matches("//").trim();
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

fn extract_visibility(node: &Node, source: &[u8]) -> Option<String> {
    if let Some(modifiers) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "modifiers")
    {
        let mut cursor = modifiers.walk();
        for child in modifiers.children(&mut cursor) {
            let text = node_text(&child, source).trim();
            if matches!(text, "public" | "private" | "protected" | "internal") {
                return Some(text.to_string());
            }
        }
    }
    None
}

fn extract_signature(node: &Node, source: &[u8]) -> Option<String> {
    let text = node_text(node, source);
    if let Some(body_node) = node
        .child_by_field_name("body")
        .or_else(|| node.child_by_field_name("class_body"))
    {
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

fn find_kt_name<'a>(node: &Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "simple_identifier"
            || child.kind() == "type_identifier"
            || child.kind() == "identifier"
        {
            let name = node_text(&child, source).trim();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

fn extract_kt_relationships(node: &Node, source: &[u8]) -> Vec<SymbolRelationship> {
    let mut rels = Vec::new();
    if let Some(delegation) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "delegation_specifier" || c.kind() == "delegation_specifiers")
    {
        let text = node_text(&delegation, source)
            .trim_start_matches(':')
            .trim();
        for item in text.split(',') {
            let target = item.trim().split('(').next().unwrap_or("").trim();
            if !target.is_empty() {
                rels.push(SymbolRelationship {
                    relation: "extends".to_string(),
                    target: target.to_string(),
                });
            }
        }
    }
    rels
}

fn extract_kt_symbol(
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
        // Classes, Interfaces, Objects
        "class_declaration" | "object_declaration" => {
            let text = node_text(node, source);
            let name = find_kt_name(node, source)?;
            let visibility = extract_visibility(node, source);

            if options.exported_only == Some(true) && visibility.as_deref() == Some("private") {
                return None;
            }

            let kind = if text.contains("interface ") {
                SymbolKind::Interface
            } else if text.contains("enum class ") {
                SymbolKind::Enum
            } else {
                SymbolKind::Class
            };

            if let Some(ref allowed) = options.symbol_kinds
                && !allowed.contains(&kind)
            {
                return None;
            }

            let span = node_span(node);
            let signature = extract_signature(node, source);
            let doc_comment = if options.include_doc_comments != Some(false) {
                extract_doc_comment(node, source)
            } else {
                None
            };
            let relationships = if options.include_relationships != Some(false) {
                extract_kt_relationships(node, source)
            } else {
                Vec::new()
            };

            let mut children = Vec::new();
            if let Some(body) = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "class_body")
            {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if let Some(sym) = extract_kt_symbol(&child, source, depth + 1, options) {
                        children.push(sym);
                    }
                }
            }

            Some(Symbol {
                name: name.to_string(),
                kind,
                span,
                signature,
                doc_comment,
                visibility,
                relationships,
                children,
            })
        }

        // Functions / Methods
        "function_declaration" => {
            let name = find_kt_name(node, source)?;
            let visibility = extract_visibility(node, source);

            if options.exported_only == Some(true) && visibility.as_deref() == Some("private") {
                return None;
            }

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

            Some(Symbol {
                name: name.to_string(),
                kind,
                span: node_span(node),
                signature: extract_signature(node, source),
                doc_comment: if options.include_doc_comments != Some(false) {
                    extract_doc_comment(node, source)
                } else {
                    None
                },
                visibility,
                relationships: Vec::new(),
                children: Vec::new(),
            })
        }

        // Properties / Variables
        "property_declaration" => {
            let name = find_kt_name(node, source)?;
            let visibility = extract_visibility(node, source);

            if options.exported_only == Some(true) && visibility.as_deref() == Some("private") {
                return None;
            }

            let kind = if depth > 0 {
                SymbolKind::Property
            } else {
                SymbolKind::Variable
            };

            Some(Symbol {
                name: name.to_string(),
                kind,
                span: node_span(node),
                signature: Some(node_text(node, source).trim().to_string()),
                doc_comment: if options.include_doc_comments != Some(false) {
                    extract_doc_comment(node, source)
                } else {
                    None
                },
                visibility,
                relationships: Vec::new(),
                children: Vec::new(),
            })
        }

        _ => None,
    }
}
