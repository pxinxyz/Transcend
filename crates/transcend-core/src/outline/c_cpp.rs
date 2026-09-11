//! C and C++ Language Outline Adapter
//!
//! Extracts semantic symbols from C and C++ source code using Tree-sitter.

use tree_sitter::{Node, Tree};
use transcend_protocol::{OutlineOptions, Symbol, SymbolKind, SymbolRelationship};

use super::{clean_signature, node_span, node_text, LanguageOutline};

pub struct COutline;
pub struct CppOutline;

impl COutline {
    pub fn new() -> Self {
        Self
    }
}

impl CppOutline {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageOutline for COutline {
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if let Some(sym) = extract_c_symbol(&child, source, 0, options, false, None) {
                symbols.push(sym);
            }
        }

        symbols
    }
}

impl LanguageOutline for CppOutline {
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if let Some(sym) = extract_c_symbol(&child, source, 0, options, true, None) {
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
        if trimmed.starts_with("/**") || trimmed.starts_with("/*") || trimmed.starts_with("*/") {
            let clean = trimmed
                .trim_start_matches("/**")
                .trim_start_matches("/*")
                .trim_end_matches("*/")
                .trim_start_matches('*')
                .trim();
            if !clean.is_empty() {
                doc_lines.push(clean);
            }
        } else if trimmed.starts_with('*') {
            let clean = trimmed.trim_start_matches('*').trim();
            if !clean.is_empty() {
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
        doc_lines.into_iter().find(|l| !l.is_empty()).map(|l| l.to_string())
    }
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
    let cleaned = clean_signature(first_line.trim_end_matches('{').trim_end_matches(';').trim());
    if !cleaned.is_empty() {
        Some(cleaned)
    } else {
        None
    }
}

fn find_identifier<'a>(node: &Node, source: &'a [u8]) -> Option<&'a str> {
    if let Some(id_node) = node.child_by_field_name("declarator") {
        return find_identifier(&id_node, source);
    }
    if let Some(id_node) = node.child_by_field_name("name") {
        return find_identifier(&id_node, source);
    }
    if node.kind() == "identifier" || node.kind() == "type_identifier" || node.kind() == "field_identifier" {
        let text = node_text(node, source).trim();
        if !text.is_empty() {
            return Some(text);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" || child.kind() == "type_identifier" || child.kind() == "field_identifier" || child.kind() == "destructor_name" {
            let text = node_text(&child, source).trim();
            if !text.is_empty() {
                return Some(text);
            }
        } else if child.kind() == "function_declarator" || child.kind() == "pointer_declarator" || child.kind() == "reference_declarator" || child.kind() == "scoped_identifier" {
            if let Some(name) = find_identifier(&child, source) {
                return Some(name);
            }
        }
    }
    None
}

fn extract_c_symbol(
    node: &Node,
    source: &[u8],
    depth: usize,
    options: &OutlineOptions,
    is_cpp: bool,
    current_visibility: Option<&str>,
) -> Option<Symbol> {
    if let Some(max_depth) = options.max_depth {
        if depth > max_depth {
            return None;
        }
    }

    match node.kind() {
        // C++ Template Declaration: unwrap inner declaration
        "template_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "template_parameter_list" && child.is_named() {
                    return extract_c_symbol(&child, source, depth, options, is_cpp, current_visibility);
                }
            }
            None
        }

        // Functions / Methods
        "function_definition" => {
            let name = find_identifier(node, source)?;
            let is_method = depth > 0;
            let kind = if is_method {
                if name.starts_with('~') {
                    SymbolKind::Method
                } else {
                    SymbolKind::Method
                }
            } else {
                SymbolKind::Function
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

            let visibility = current_visibility.map(|s| s.to_string());
            if options.exported_only == Some(true) && visibility.as_deref() == Some("private") {
                return None;
            }

            Some(Symbol {
                name: name.to_string(),
                kind,
                span,
                signature,
                doc_comment,
                visibility,
                relationships: Vec::new(),
                children: Vec::new(),
            })
        }

        // C++ Class
        "class_specifier" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, source).trim().to_string())
                .unwrap_or_else(|| "AnonymousClass".to_string());

            let span = node_span(node);
            let signature = extract_signature(node, source);
            let doc_comment = if options.include_doc_comments != Some(false) {
                extract_doc_comment(node, source)
            } else {
                None
            };

            let mut relationships = Vec::new();
            if let Some(base_clause) = node.children(&mut node.walk()).find(|c| c.kind() == "base_class_clause") {
                let base_text = node_text(&base_clause, source);
                for part in base_text.trim_start_matches(':').split(',') {
                    let target = part.trim().split_whitespace().last().unwrap_or("").trim();
                    if !target.is_empty() {
                        relationships.push(SymbolRelationship {
                            relation: "extends".to_string(),
                            target: target.to_string(),
                        });
                    }
                }
            }

            let mut children = Vec::new();
            if let Some(body) = node.child_by_field_name("body") {
                let mut active_vis = "private"; // default C++ class visibility
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if child.kind() == "access_specifier" {
                        let text = node_text(&child, source).trim();
                        if text.starts_with("public") {
                            active_vis = "public";
                        } else if text.starts_with("protected") {
                            active_vis = "protected";
                        } else if text.starts_with("private") {
                            active_vis = "private";
                        }
                    } else if let Some(sym) = extract_c_symbol(&child, source, depth + 1, options, is_cpp, Some(active_vis)) {
                        children.push(sym);
                    }
                }
            }

            Some(Symbol {
                name,
                kind: SymbolKind::Class,
                span,
                signature,
                doc_comment,
                visibility: current_visibility.map(|s| s.to_string()),
                relationships,
                children,
            })
        }

        // Struct
        "struct_specifier" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, source).trim().to_string())
                .unwrap_or_else(|| "AnonymousStruct".to_string());

            let span = node_span(node);
            let signature = extract_signature(node, source);
            let doc_comment = if options.include_doc_comments != Some(false) {
                extract_doc_comment(node, source)
            } else {
                None
            };

            let mut children = Vec::new();
            if let Some(body) = node.child_by_field_name("body") {
                let mut active_vis = "public"; // default C/C++ struct visibility
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if child.kind() == "access_specifier" {
                        let text = node_text(&child, source).trim();
                        if text.starts_with("public") {
                            active_vis = "public";
                        } else if text.starts_with("protected") {
                            active_vis = "protected";
                        } else if text.starts_with("private") {
                            active_vis = "private";
                        }
                    } else if let Some(sym) = extract_c_symbol(&child, source, depth + 1, options, is_cpp, Some(active_vis)) {
                        children.push(sym);
                    }
                }
            }

            Some(Symbol {
                name,
                kind: SymbolKind::Struct,
                span,
                signature,
                doc_comment,
                visibility: current_visibility.map(|s| s.to_string()),
                relationships: Vec::new(),
                children,
            })
        }

        // Enum
        "enum_specifier" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, source).trim().to_string())
                .unwrap_or_else(|| "AnonymousEnum".to_string());

            let span = node_span(node);
            let signature = extract_signature(node, source);
            let doc_comment = if options.include_doc_comments != Some(false) {
                extract_doc_comment(node, source)
            } else {
                None
            };

            let mut children = Vec::new();
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if child.kind() == "enumerator" {
                        if let Some(sym) = extract_c_symbol(&child, source, depth + 1, options, is_cpp, current_visibility) {
                            children.push(sym);
                        }
                    }
                }
            }

            Some(Symbol {
                name,
                kind: SymbolKind::Enum,
                span,
                signature,
                doc_comment,
                visibility: current_visibility.map(|s| s.to_string()),
                relationships: Vec::new(),
                children,
            })
        }

        "enumerator" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, source).trim().to_string())
                .or_else(|| find_identifier(node, source).map(|s| s.to_string()))?;

            Some(Symbol {
                name,
                kind: SymbolKind::Constant,
                span: node_span(node),
                signature: Some(node_text(node, source).trim().to_string()),
                doc_comment: None,
                visibility: None,
                relationships: Vec::new(),
                children: Vec::new(),
            })
        }

        // Field declaration
        "field_declaration" => {
            let name = find_identifier(node, source)?;
            Some(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Field,
                span: node_span(node),
                signature: Some(node_text(node, source).trim().trim_end_matches(';').trim().to_string()),
                doc_comment: if options.include_doc_comments != Some(false) {
                    extract_doc_comment(node, source)
                } else {
                    None
                },
                visibility: current_visibility.map(|s| s.to_string()),
                relationships: Vec::new(),
                children: Vec::new(),
            })
        }

        // C++ Namespace
        "namespace_definition" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, source).trim().to_string())
                .unwrap_or_else(|| "anonymous_namespace".to_string());

            let span = node_span(node);
            let mut children = Vec::new();
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if let Some(sym) = extract_c_symbol(&child, source, depth + 1, options, is_cpp, current_visibility) {
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

        // Declaration (could be a top-level typedef, struct, or member method prototype in a class)
        "declaration" => {
            // Check if it's a type definition (typedef)
            let text = node_text(node, source);
            if text.starts_with("typedef") {
                let name = find_identifier(node, source).unwrap_or("TypeDef");
                return Some(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::TypeAlias,
                    span: node_span(node),
                    signature: Some(text.trim().trim_end_matches(';').trim().to_string()),
                    doc_comment: if options.include_doc_comments != Some(false) {
                        extract_doc_comment(node, source)
                    } else {
                        None
                    },
                    visibility: current_visibility.map(|s| s.to_string()),
                    relationships: Vec::new(),
                    children: Vec::new(),
                });
            }

            // Check if it wraps a struct/enum/class specifier
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "struct_specifier" || child.kind() == "class_specifier" || child.kind() == "enum_specifier" {
                    return extract_c_symbol(&child, source, depth, options, is_cpp, current_visibility);
                }
            }

            // If inside a class body and has a function declarator, it's a method declaration!
            if depth > 0 && text.contains('(') && text.contains(')') {
                if let Some(name) = find_identifier(node, source) {
                    return Some(Symbol {
                        name: name.to_string(),
                        kind: SymbolKind::Method,
                        span: node_span(node),
                        signature: Some(text.trim().trim_end_matches(';').trim().to_string()),
                        doc_comment: if options.include_doc_comments != Some(false) {
                            extract_doc_comment(node, source)
                        } else {
                            None
                        },
                        visibility: current_visibility.map(|s| s.to_string()),
                        relationships: Vec::new(),
                        children: Vec::new(),
                    });
                }
            }

            None
        }

        _ => None,
    }
}
