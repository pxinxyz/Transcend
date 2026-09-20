//! Java Language Outline Adapter
//!
//! Extracts semantic symbols from Java source code using Tree-sitter.

use transcend_protocol::{OutlineOptions, Symbol, SymbolKind, SymbolRelationship};
use tree_sitter::{Node, Tree};

use super::{LanguageOutline, clean_signature, node_span, node_text};

pub struct JavaOutline;

impl Default for JavaOutline {
    fn default() -> Self {
        Self::new()
    }
}

impl JavaOutline {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageOutline for JavaOutline {
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if let Some(sym) = extract_java_symbol(&child, source, 0, options) {
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
            if matches!(text, "public" | "private" | "protected") {
                return Some(text.to_string());
            }
        }
    }
    None
}

fn extract_signature(node: &Node, source: &[u8]) -> Option<String> {
    let text = node_text(node, source);
    if let Some(body_node) = node.child_by_field_name("body") {
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
    let cleaned = clean_signature(
        first_line
            .trim_end_matches('{')
            .trim_end_matches(';')
            .trim(),
    );
    if !cleaned.is_empty() {
        Some(cleaned)
    } else {
        None
    }
}

fn extract_java_relationships(node: &Node, source: &[u8]) -> Vec<SymbolRelationship> {
    let mut rels = Vec::new();

    // Superclass (extends)
    if let Some(super_node) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "superclass")
    {
        let target = node_text(&super_node, source)
            .trim_start_matches("extends")
            .trim()
            .to_string();
        if !target.is_empty() {
            rels.push(SymbolRelationship {
                relation: "extends".to_string(),
                target,
            });
        }
    }

    // Interfaces (implements)
    if let Some(interfaces_node) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "super_interfaces")
    {
        let text = node_text(&interfaces_node, source)
            .trim_start_matches("implements")
            .trim();
        for target in text.split(',') {
            let t = target.trim();
            if !t.is_empty() {
                rels.push(SymbolRelationship {
                    relation: "implements".to_string(),
                    target: t.to_string(),
                });
            }
        }
    }

    rels
}

fn extract_java_symbol(
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
        // Classes, Interfaces, Records, Annotation Types
        "class_declaration"
        | "interface_declaration"
        | "record_declaration"
        | "annotation_type_declaration" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();
            let visibility = extract_visibility(node, source);

            if options.exported_only == Some(true) && visibility.as_deref() == Some("private") {
                return None;
            }

            let kind = match node.kind() {
                "class_declaration" | "record_declaration" => SymbolKind::Class,
                "interface_declaration" | "annotation_type_declaration" => SymbolKind::Interface,
                _ => SymbolKind::Class,
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
                extract_java_relationships(node, source)
            } else {
                Vec::new()
            };

            let mut children = Vec::new();
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if let Some(sym) = extract_java_symbol(&child, source, depth + 1, options) {
                        children.push(sym);
                    }
                }
            }

            Some(Symbol {
                name,
                kind,
                span,
                signature,
                doc_comment,
                visibility,
                relationships,
                children,
            })
        }

        // Enums
        "enum_declaration" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();
            let visibility = extract_visibility(node, source);

            if options.exported_only == Some(true) && visibility.as_deref() == Some("private") {
                return None;
            }

            let mut children = Vec::new();
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if child.kind() == "enum_constant" {
                        if let Some(sym) = extract_java_symbol(&child, source, depth + 1, options) {
                            children.push(sym);
                        }
                    } else if let Some(sym) =
                        extract_java_symbol(&child, source, depth + 1, options)
                    {
                        children.push(sym);
                    }
                }
            }

            Some(Symbol {
                name,
                kind: SymbolKind::Enum,
                span: node_span(node),
                signature: extract_signature(node, source),
                doc_comment: if options.include_doc_comments != Some(false) {
                    extract_doc_comment(node, source)
                } else {
                    None
                },
                visibility,
                relationships: Vec::new(),
                children,
            })
        }

        "enum_constant" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();
            Some(Symbol {
                name,
                kind: SymbolKind::Constant,
                span: node_span(node),
                signature: Some(node_text(node, source).trim().to_string()),
                doc_comment: if options.include_doc_comments != Some(false) {
                    extract_doc_comment(node, source)
                } else {
                    None
                },
                visibility: None,
                relationships: Vec::new(),
                children: Vec::new(),
            })
        }

        // Methods
        "method_declaration" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();
            let visibility = extract_visibility(node, source);

            if options.exported_only == Some(true) && visibility.as_deref() == Some("private") {
                return None;
            }

            let kind = SymbolKind::Method;
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
                visibility,
                relationships: Vec::new(),
                children: Vec::new(),
            })
        }

        // Constructors
        "constructor_declaration" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();
            let visibility = extract_visibility(node, source);

            Some(Symbol {
                name,
                kind: SymbolKind::Constructor,
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

        // Fields
        "field_declaration" => {
            let visibility = extract_visibility(node, source);
            if options.exported_only == Some(true) && visibility.as_deref() == Some("private") {
                return None;
            }

            let mut name = "field";
            if let Some(declarator) = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "variable_declarator")
                && let Some(n) = declarator.child_by_field_name("name")
            {
                name = node_text(&n, source).trim();
            }

            let text = node_text(node, source);
            Some(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Field,
                span: node_span(node),
                signature: Some(text.trim().trim_end_matches(';').trim().to_string()),
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
