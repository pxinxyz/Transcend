//! PHP Language Outline Adapter
//!
//! Extracts semantic symbols from PHP source code using Tree-sitter.

use tree_sitter::{Node, Tree};
use transcend_protocol::{OutlineOptions, Symbol, SymbolKind, SymbolRelationship};

use super::{clean_signature, node_span, node_text, LanguageOutline};

pub struct PhpOutline;

impl PhpOutline {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageOutline for PhpOutline {
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        let mut active_namespace: Option<Symbol> = None;

        for child in root.children(&mut cursor) {
            if child.kind() == "namespace_definition" {
                if child.child_by_field_name("body").is_some() {
                    if let Some(ns) = active_namespace.take() {
                        symbols.push(ns);
                    }
                    if let Some(sym) = extract_php_symbol(&child, source, 0, options) {
                        symbols.push(sym);
                    }
                } else {
                    if let Some(ns) = active_namespace.take() {
                        symbols.push(ns);
                    }
                    if let Some(sym) = extract_php_symbol(&child, source, 0, options) {
                        active_namespace = Some(sym);
                    }
                }
            } else if let Some(sym) = extract_php_symbol(&child, source, 0, options) {
                if let Some(ref mut ns) = active_namespace {
                    ns.children.push(sym);
                } else {
                    symbols.push(sym);
                }
            }
        }

        if let Some(ns) = active_namespace {
            symbols.push(ns);
        }

        symbols
    }
}

fn extract_doc_comment(node: &Node, source: &[u8]) -> Option<String> {
    let prefix = std::str::from_utf8(&source[..node.start_byte()]).ok()?;
    let mut doc_lines = Vec::new();

    for line in prefix.lines().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with("/**") || trimmed.starts_with("/*") || trimmed.starts_with("*/") || trimmed.starts_with('*') {
            let clean = trimmed
                .trim_start_matches("/**")
                .trim_start_matches("/*")
                .trim_end_matches("*/")
                .trim_start_matches('*')
                .trim();
            if !clean.is_empty() && !clean.starts_with('@') {
                doc_lines.push(clean);
            }
        } else if trimmed.starts_with("//") || trimmed.starts_with('#') {
            let clean = trimmed.trim_start_matches("//").trim_start_matches('#').trim();
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

fn extract_visibility(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier" {
            return Some(node_text(&child, source).trim().to_string());
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
    let cleaned = clean_signature(first_line.trim_end_matches('{').trim());
    if !cleaned.is_empty() {
        Some(cleaned)
    } else {
        None
    }
}

fn extract_php_relationships(node: &Node, source: &[u8]) -> Vec<SymbolRelationship> {
    let mut rels = Vec::new();

    if let Some(base_clause) = node.children(&mut node.walk()).find(|c| c.kind() == "base_clause") {
        let target = node_text(&base_clause, source).trim_start_matches("extends").trim().to_string();
        if !target.is_empty() {
            rels.push(SymbolRelationship {
                relation: "extends".to_string(),
                target,
            });
        }
    }

    if let Some(iface_clause) = node.children(&mut node.walk()).find(|c| c.kind() == "class_interface_clause") {
        let text = node_text(&iface_clause, source).trim_start_matches("implements").trim();
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

fn extract_php_symbol(
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
        // Namespaces
        "namespace_definition" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();
            let span = node_span(node);

            let mut children = Vec::new();
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if let Some(sym) = extract_php_symbol(&child, source, depth + 1, options) {
                        children.push(sym);
                    }
                }
            }

            Some(Symbol {
                name: name.clone(),
                kind: SymbolKind::Namespace,
                span,
                signature: Some(format!("namespace {}", name)),
                doc_comment: if options.include_doc_comments != Some(false) {
                    extract_doc_comment(node, source)
                } else {
                    None
                },
                visibility: None,
                relationships: Vec::new(),
                children,
            })
        }

        // Classes, Interfaces, Traits, Enums
        "class_declaration" | "interface_declaration" | "trait_declaration" | "enum_declaration" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();
            let visibility = extract_visibility(node, source);

            if options.exported_only == Some(true) && visibility.as_deref() == Some("private") {
                return None;
            }

            let kind = match node.kind() {
                "class_declaration" => SymbolKind::Class,
                "interface_declaration" => SymbolKind::Interface,
                "trait_declaration" => SymbolKind::Trait,
                "enum_declaration" => SymbolKind::Enum,
                _ => SymbolKind::Class,
            };

            if let Some(ref allowed) = options.symbol_kinds {
                if !allowed.contains(&kind) {
                    return None;
                }
            }

            let span = node_span(node);
            let signature = extract_signature(node, source);
            let doc_comment = if options.include_doc_comments != Some(false) {
                extract_doc_comment(node, source)
            } else {
                None
            };
            let relationships = if options.include_relationships != Some(false) {
                extract_php_relationships(node, source)
            } else {
                Vec::new()
            };

            let mut children = Vec::new();
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if let Some(sym) = extract_php_symbol(&child, source, depth + 1, options) {
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

        // Functions
        "function_definition" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();

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
                visibility: Some("public".to_string()),
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

            let kind = if name == "__construct" {
                SymbolKind::Constructor
            } else {
                SymbolKind::Method
            };

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
                visibility,
                relationships: Vec::new(),
                children: Vec::new(),
            })
        }

        // Properties
        "property_declaration" => {
            let visibility = extract_visibility(node, source);
            if options.exported_only == Some(true) && visibility.as_deref() == Some("private") {
                return None;
            }

            let text = node_text(node, source);
            let mut name = "property";
            if let Some(elem) = node.children(&mut node.walk()).find(|c| c.kind() == "property_element") {
                if let Some(var) = elem.children(&mut elem.walk()).find(|c| c.kind() == "variable_name") {
                    name = node_text(&var, source).trim();
                }
            }

            Some(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Property,
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
