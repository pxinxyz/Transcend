//! C# Language Outline Adapter
//!
//! Extracts semantic symbols from C# source code using Tree-sitter.

use transcend_protocol::{OutlineOptions, Symbol, SymbolKind, SymbolRelationship};
use tree_sitter::{Node, Tree};

use super::{LanguageOutline, clean_signature, node_span, node_text};

pub struct CSharpOutline;

impl Default for CSharpOutline {
    fn default() -> Self {
        Self::new()
    }
}

impl CSharpOutline {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageOutline for CSharpOutline {
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if let Some(sym) = extract_cs_symbol(&child, source, 0, options) {
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
            let mut clean = trimmed.trim_start_matches("///").trim();
            // Strip XML doc tags like <summary>, </summary>, <param...>
            if clean.starts_with("<summary>") {
                clean = clean.trim_start_matches("<summary>").trim();
            }
            if clean.ends_with("</summary>") {
                clean = clean.trim_end_matches("</summary>").trim();
            }
            if !clean.is_empty() && !clean.starts_with('<') {
                doc_lines.push(clean);
            }
        } else if trimmed.starts_with("/**")
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
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifier" {
            let text = node_text(&child, source).trim();
            if matches!(
                text,
                "public" | "private" | "protected" | "internal" | "file"
            ) {
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

    // Check for expression-bodied members (=> ...)
    if let Some(arrow) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "arrow_expression_clause")
    {
        let arrow_start = arrow.start_byte();
        if arrow_start >= node.start_byte() {
            let sig_bytes = &source[node.start_byte()..arrow_start];
            let sig_str = std::str::from_utf8(sig_bytes).unwrap_or("");
            let cleaned = clean_signature(sig_str.trim());
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

fn extract_base_relationships(node: &Node, source: &[u8]) -> Vec<SymbolRelationship> {
    let mut rels = Vec::new();
    if let Some(base_list) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "base_list")
    {
        let mut cursor = base_list.walk();
        for child in base_list.children(&mut cursor) {
            if child.kind() != ":" && child.kind() != "," && child.is_named() {
                let target = node_text(&child, source).trim().to_string();
                if !target.is_empty() {
                    let relation = if target.starts_with('I')
                        && target.chars().nth(1).is_some_and(|c| c.is_uppercase())
                    {
                        "implements"
                    } else {
                        "extends"
                    };
                    rels.push(SymbolRelationship {
                        relation: relation.to_string(),
                        target,
                    });
                }
            }
        }
    }
    rels
}

fn extract_cs_symbol(
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
        // Namespaces
        "namespace_declaration" | "file_scoped_namespace_declaration" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();
            let span = node_span(node);

            let mut children = Vec::new();
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if let Some(sym) = extract_cs_symbol(&child, source, depth + 1, options) {
                        children.push(sym);
                    }
                }
            } else {
                // file_scoped_namespace_declaration: children are siblings or in declaration list
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "name"
                        && child.kind() != "modifier"
                        && child.is_named()
                        && let Some(sym) = extract_cs_symbol(&child, source, depth + 1, options)
                    {
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

        // Classes, Structs, Interfaces, Records
        "class_declaration"
        | "struct_declaration"
        | "interface_declaration"
        | "record_declaration"
        | "record_struct_declaration" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();
            let visibility = extract_visibility(node, source);

            if options.exported_only == Some(true) && visibility.as_deref() == Some("private") {
                return None;
            }

            let kind = match node.kind() {
                "class_declaration" => SymbolKind::Class,
                "interface_declaration" => SymbolKind::Interface,
                "struct_declaration" | "record_struct_declaration" => SymbolKind::Struct,
                "record_declaration" => SymbolKind::Class,
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
                extract_base_relationships(node, source)
            } else {
                Vec::new()
            };

            let mut children = Vec::new();
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if let Some(sym) = extract_cs_symbol(&child, source, depth + 1, options) {
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
                    if child.kind() == "enum_member_declaration"
                        && let Some(sym) = extract_cs_symbol(&child, source, depth + 1, options)
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

        "enum_member_declaration" => {
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

        // Properties
        "property_declaration" => {
            let name_node = node.child_by_field_name("name")?;
            let name = node_text(&name_node, source).trim().to_string();
            let visibility = extract_visibility(node, source);

            if options.exported_only == Some(true) && visibility.as_deref() == Some("private") {
                return None;
            }

            Some(Symbol {
                name,
                kind: SymbolKind::Property,
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
            let text = node_text(node, source);
            let visibility = extract_visibility(node, source);
            if options.exported_only == Some(true) && visibility.as_deref() == Some("private") {
                return None;
            }

            let mut name = "field";
            if let Some(var_decl) = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "variable_declaration")
                && let Some(declarator) = var_decl
                    .children(&mut var_decl.walk())
                    .find(|c| c.kind() == "variable_declarator")
                && let Some(n) = declarator.child_by_field_name("name")
            {
                name = node_text(&n, source).trim();
            }

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
