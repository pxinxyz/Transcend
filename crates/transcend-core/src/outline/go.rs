//! Go AST Outline Adapter
//!
//! Extracts semantic symbols from Go source code using Tree-sitter.

use tree_sitter::{Node, Tree};
use transcend_protocol::{OutlineOptions, Symbol, SymbolKind, SymbolRelationship};

use super::{clean_signature, node_span, node_text, LanguageOutline};

pub struct GoOutline;

impl GoOutline {
    pub fn new() -> Self {
        Self
    }

    fn extract_doc_comment(node: &Node, source: &[u8]) -> Option<String> {
        let prefix = std::str::from_utf8(&source[..node.start_byte()]).ok()?;
        let mut doc_lines = Vec::new();

        for line in prefix.lines().rev() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                doc_lines.push(trimmed.trim_start_matches("//").trim());
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
            Some(doc_lines.join(" "))
        }
    }

    fn is_exported(name: &str) -> bool {
        name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
    }

    fn extract_signature(node: &Node, source: &[u8]) -> Option<String> {
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

        let first_line = node_text(node, source).lines().next().unwrap_or("").trim();
        let cleaned = clean_signature(first_line.trim_end_matches('{').trim());
        if !cleaned.is_empty() {
            Some(cleaned)
        } else {
            None
        }
    }

    fn extract_function(&self, node: &Node, source: &[u8], options: &OutlineOptions) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();
        let exported = Self::is_exported(&name);

        if options.exported_only == Some(true) && !exported {
            return None;
        }

        if let Some(ref allowed) = options.symbol_kinds {
            if !allowed.contains(&SymbolKind::Function) {
                return None;
            }
        }

        Some(Symbol {
            name,
            kind: SymbolKind::Function,
            span: node_span(node),
            signature: Self::extract_signature(node, source),
            doc_comment: Self::extract_doc_comment(node, source),
            visibility: if exported { Some("exported".to_string()) } else { None },
            relationships: vec![],
            children: vec![],
        })
    }

    fn extract_method(&self, node: &Node, source: &[u8], options: &OutlineOptions) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();
        let exported = Self::is_exported(&name);

        if options.exported_only == Some(true) && !exported {
            return None;
        }

        if let Some(ref allowed) = options.symbol_kinds {
            if !allowed.contains(&SymbolKind::Method) {
                return None;
            }
        }

        let mut relationships = Vec::new();
        if let Some(recv) = node.child_by_field_name("receiver") {
            let recv_text = clean_signature(node_text(&recv, source));
            let clean_target = recv_text
                .trim_start_matches('(')
                .trim_end_matches(')')
                .split_whitespace()
                .last()
                .unwrap_or("")
                .trim_start_matches('*')
                .to_string();

            if !clean_target.is_empty() {
                relationships.push(SymbolRelationship {
                    relation: "receiver".to_string(),
                    target: clean_target,
                });
            }
        }

        Some(Symbol {
            name,
            kind: SymbolKind::Method,
            span: node_span(node),
            signature: Self::extract_signature(node, source),
            doc_comment: Self::extract_doc_comment(node, source),
            visibility: if exported { Some("exported".to_string()) } else { None },
            relationships,
            children: vec![],
        })
    }

    fn extract_type(&self, node: &Node, source: &[u8], options: &OutlineOptions) -> Option<Symbol> {
        // node is type_declaration, children can be type_spec
        let mut cursor = node.walk();
        for spec in node.named_children(&mut cursor) {
            if spec.kind() == "type_spec" {
                if let Some(name_n) = spec.child_by_field_name("name") {
                    let name = node_text(&name_n, source).to_string();
                    let exported = Self::is_exported(&name);

                    if options.exported_only == Some(true) && !exported {
                        continue;
                    }

                    let type_n = spec.child_by_field_name("type")?;
                    let mut kind = SymbolKind::TypeAlias;
                    let mut children = Vec::new();

                    match type_n.kind() {
                        "struct_type" => {
                            kind = SymbolKind::Struct;
                            if let Some(field_list) = type_n.child_by_field_name("fields") {
                                let mut f_cursor = field_list.walk();
                                for f in field_list.named_children(&mut f_cursor) {
                                    if f.kind() == "field_declaration" {
                                        if let Some(f_name_n) = f.child_by_field_name("name") {
                                            let f_name = node_text(&f_name_n, source).to_string();
                                            children.push(Symbol {
                                                name: f_name,
                                                kind: SymbolKind::Field,
                                                span: node_span(&f),
                                                signature: Some(clean_signature(node_text(&f, source))),
                                                doc_comment: Self::extract_doc_comment(&f, source),
                                                visibility: if Self::is_exported(&node_text(&f_name_n, source)) {
                                                    Some("exported".to_string())
                                                } else {
                                                    None
                                                },
                                                relationships: vec![],
                                                children: vec![],
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        "interface_type" => {
                            kind = SymbolKind::Interface;
                            if let Some(method_list) = type_n.child_by_field_name("methods") {
                                let mut m_cursor = method_list.walk();
                                for m in method_list.named_children(&mut m_cursor) {
                                    if m.kind() == "method_spec" {
                                        if let Some(m_name_n) = m.child_by_field_name("name") {
                                            let m_name = node_text(&m_name_n, source).to_string();
                                            let is_exp = Self::is_exported(&m_name);
                                            children.push(Symbol {
                                                name: m_name,
                                                kind: SymbolKind::Method,
                                                span: node_span(&m),
                                                signature: Some(clean_signature(node_text(&m, source))),
                                                doc_comment: Self::extract_doc_comment(&m, source),
                                                visibility: if is_exp {
                                                    Some("exported".to_string())
                                                } else {
                                                    None
                                                },
                                                relationships: vec![],
                                                children: vec![],
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }

                    if let Some(ref allowed) = options.symbol_kinds {
                        if !allowed.contains(&kind) {
                            continue;
                        }
                    }

                    return Some(Symbol {
                        name,
                        kind,
                        span: node_span(node),
                        signature: Some(clean_signature(node_text(node, source).trim_end_matches('{').trim())),
                        doc_comment: Self::extract_doc_comment(node, source),
                        visibility: if exported { Some("exported".to_string()) } else { None },
                        relationships: vec![],
                        children,
                    });
                }
            }
        }
        None
    }

    fn extract_node(&self, node: &Node, source: &[u8], options: &OutlineOptions) -> Option<Symbol> {
        match node.kind() {
            "function_declaration" => self.extract_function(node, source, options),
            "method_declaration" => self.extract_method(node, source, options),
            "type_declaration" => self.extract_type(node, source, options),
            _ => None,
        }
    }
}

impl LanguageOutline for GoOutline {
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let root = tree.root_node();
        let mut symbols = Vec::new();
        let mut cursor = root.walk();

        for child in root.named_children(&mut cursor) {
            if let Some(sym) = self.extract_node(&child, source, options) {
                symbols.push(sym);
            }
        }

        symbols
    }
}
