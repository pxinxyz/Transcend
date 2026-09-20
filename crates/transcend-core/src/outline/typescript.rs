//! TypeScript / JavaScript AST Outline Adapter
//!
//! Extracts semantic symbols from TypeScript, TSX, and JavaScript source code.

use transcend_protocol::{OutlineOptions, Symbol, SymbolKind, SymbolRelationship};
use tree_sitter::{Node, Tree};

use super::{LanguageOutline, clean_signature, node_span, node_text};

pub struct TypeScriptOutline;

impl Default for TypeScriptOutline {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeScriptOutline {
    pub fn new() -> Self {
        Self
    }

    fn extract_doc_comment(node: &Node, source: &[u8]) -> Option<String> {
        let prefix = std::str::from_utf8(&source[..node.start_byte()]).ok()?;
        let mut doc_lines = Vec::new();

        for line in prefix.lines().rev() {
            let trimmed = line.trim();
            if trimmed.starts_with("/**") || trimmed.starts_with("/*") || trimmed.starts_with("*/")
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
            } else if trimmed.starts_with('*') {
                let clean = trimmed.trim_start_matches('*').trim();
                if !clean.is_empty() {
                    doc_lines.push(clean);
                }
            } else if trimmed.starts_with("//") {
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
            doc_lines
                .into_iter()
                .find(|l| !l.is_empty())
                .map(|l| l.to_string())
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

    fn extract_function(
        &self,
        node: &Node,
        source: &[u8],
        is_exported: bool,
        options: &OutlineOptions,
    ) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        if options.exported_only == Some(true) && !is_exported {
            return None;
        }

        if let Some(ref allowed) = options.symbol_kinds
            && !allowed.contains(&SymbolKind::Function)
        {
            return None;
        }

        let doc_comment = if options.include_doc_comments != Some(false) {
            Self::extract_doc_comment(node, source)
        } else {
            None
        };

        let visibility = if is_exported {
            Some("exported".to_string())
        } else {
            None
        };

        Some(Symbol {
            name,
            kind: SymbolKind::Function,
            span: node_span(node),
            signature: Self::extract_signature(node, source),
            doc_comment,
            visibility,
            relationships: vec![],
            children: vec![],
        })
    }

    fn extract_class(
        &self,
        node: &Node,
        source: &[u8],
        is_exported: bool,
        options: &OutlineOptions,
    ) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        if options.exported_only == Some(true) && !is_exported {
            return None;
        }

        if let Some(ref allowed) = options.symbol_kinds
            && !allowed.contains(&SymbolKind::Class)
        {
            return None;
        }

        let mut relationships = Vec::new();
        if options.include_relationships != Some(false) {
            // Check heritage (extends / implements)
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "class_heritage" {
                    let mut h_cursor = child.walk();
                    for h_child in child.named_children(&mut h_cursor) {
                        if h_child.kind() == "extends_clause" {
                            if let Some(val) = h_child.child_by_field_name("value") {
                                relationships.push(SymbolRelationship {
                                    relation: "extends".to_string(),
                                    target: node_text(&val, source).to_string(),
                                });
                            }
                        } else if h_child.kind() == "implements_clause" {
                            let mut t_cursor = h_child.walk();
                            for t in h_child.named_children(&mut t_cursor) {
                                if t.kind() == "type_identifier" {
                                    relationships.push(SymbolRelationship {
                                        relation: "implements".to_string(),
                                        target: node_text(&t, source).to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut children = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            let mut b_cursor = body.walk();
            for item in body.named_children(&mut b_cursor) {
                match item.kind() {
                    "method_definition" => {
                        if let Some(m_name_n) = item.child_by_field_name("name") {
                            let m_name = node_text(&m_name_n, source).to_string();
                            let m_vis = if m_name.starts_with('#') || m_name.starts_with('_') {
                                Some("private".to_string())
                            } else {
                                Some("public".to_string())
                            };
                            children.push(Symbol {
                                name: m_name,
                                kind: SymbolKind::Method,
                                span: node_span(&item),
                                signature: Self::extract_signature(&item, source),
                                doc_comment: Self::extract_doc_comment(&item, source),
                                visibility: m_vis,
                                relationships: vec![],
                                children: vec![],
                            });
                        }
                    }
                    "public_field_definition" | "field_definition" => {
                        if let Some(f_name_n) = item.child_by_field_name("name") {
                            let f_name = node_text(&f_name_n, source).to_string();
                            children.push(Symbol {
                                name: f_name,
                                kind: SymbolKind::Property,
                                span: node_span(&item),
                                signature: Some(clean_signature(
                                    node_text(&item, source).trim_end_matches(';').trim(),
                                )),
                                doc_comment: Self::extract_doc_comment(&item, source),
                                visibility: None,
                                relationships: vec![],
                                children: vec![],
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        let visibility = if is_exported {
            Some("exported".to_string())
        } else {
            None
        };

        Some(Symbol {
            name,
            kind: SymbolKind::Class,
            span: node_span(node),
            signature: Self::extract_signature(node, source),
            doc_comment: Self::extract_doc_comment(node, source),
            visibility,
            relationships,
            children,
        })
    }

    fn extract_interface(
        &self,
        node: &Node,
        source: &[u8],
        is_exported: bool,
        options: &OutlineOptions,
    ) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        if options.exported_only == Some(true) && !is_exported {
            return None;
        }

        if let Some(ref allowed) = options.symbol_kinds
            && !allowed.contains(&SymbolKind::Interface)
        {
            return None;
        }

        let mut relationships = Vec::new();
        if options.include_relationships != Some(false) {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "extends_type_clause" {
                    let mut h_cursor = child.walk();
                    for t in child.named_children(&mut h_cursor) {
                        if t.kind() == "type_identifier" {
                            relationships.push(SymbolRelationship {
                                relation: "extends".to_string(),
                                target: node_text(&t, source).to_string(),
                            });
                        }
                    }
                }
            }
        }

        let mut children = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            let mut b_cursor = body.walk();
            for item in body.named_children(&mut b_cursor) {
                match item.kind() {
                    "property_signature" => {
                        if let Some(name_n) = item.child_by_field_name("name") {
                            children.push(Symbol {
                                name: node_text(&name_n, source).to_string(),
                                kind: SymbolKind::Property,
                                span: node_span(&item),
                                signature: Some(clean_signature(
                                    node_text(&item, source).trim_end_matches(';').trim(),
                                )),
                                doc_comment: Self::extract_doc_comment(&item, source),
                                visibility: None,
                                relationships: vec![],
                                children: vec![],
                            });
                        }
                    }
                    "method_signature" => {
                        if let Some(name_n) = item.child_by_field_name("name") {
                            children.push(Symbol {
                                name: node_text(&name_n, source).to_string(),
                                kind: SymbolKind::Method,
                                span: node_span(&item),
                                signature: Some(clean_signature(
                                    node_text(&item, source).trim_end_matches(';').trim(),
                                )),
                                doc_comment: Self::extract_doc_comment(&item, source),
                                visibility: None,
                                relationships: vec![],
                                children: vec![],
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        let visibility = if is_exported {
            Some("exported".to_string())
        } else {
            None
        };

        Some(Symbol {
            name,
            kind: SymbolKind::Interface,
            span: node_span(node),
            signature: Self::extract_signature(node, source),
            doc_comment: Self::extract_doc_comment(node, source),
            visibility,
            relationships,
            children,
        })
    }

    fn extract_type_alias(
        &self,
        node: &Node,
        source: &[u8],
        is_exported: bool,
        options: &OutlineOptions,
    ) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        if options.exported_only == Some(true) && !is_exported {
            return None;
        }

        if let Some(ref allowed) = options.symbol_kinds
            && !allowed.contains(&SymbolKind::TypeAlias)
        {
            return None;
        }

        let visibility = if is_exported {
            Some("exported".to_string())
        } else {
            None
        };

        Some(Symbol {
            name,
            kind: SymbolKind::TypeAlias,
            span: node_span(node),
            signature: Some(clean_signature(
                node_text(node, source).trim_end_matches(';').trim(),
            )),
            doc_comment: Self::extract_doc_comment(node, source),
            visibility,
            relationships: vec![],
            children: vec![],
        })
    }

    fn extract_enum(
        &self,
        node: &Node,
        source: &[u8],
        is_exported: bool,
        options: &OutlineOptions,
    ) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        if options.exported_only == Some(true) && !is_exported {
            return None;
        }

        if let Some(ref allowed) = options.symbol_kinds
            && !allowed.contains(&SymbolKind::Enum)
        {
            return None;
        }

        let mut children = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            let mut b_cursor = body.walk();
            for item in body.named_children(&mut b_cursor) {
                if item.kind() == "enum_assignment" || item.kind() == "property_identifier" {
                    let v_name = if let Some(n) = item.child_by_field_name("name") {
                        node_text(&n, source).to_string()
                    } else {
                        node_text(&item, source).to_string()
                    };
                    children.push(Symbol {
                        name: v_name,
                        kind: SymbolKind::Constant,
                        span: node_span(&item),
                        signature: Some(clean_signature(
                            node_text(&item, source).trim_end_matches(',').trim(),
                        )),
                        doc_comment: Self::extract_doc_comment(&item, source),
                        visibility: None,
                        relationships: vec![],
                        children: vec![],
                    });
                }
            }
        }

        let visibility = if is_exported {
            Some("exported".to_string())
        } else {
            None
        };

        Some(Symbol {
            name,
            kind: SymbolKind::Enum,
            span: node_span(node),
            signature: Self::extract_signature(node, source),
            doc_comment: Self::extract_doc_comment(node, source),
            visibility,
            relationships: vec![],
            children,
        })
    }

    fn extract_nodes(
        &self,
        node: &Node,
        source: &[u8],
        is_exported: bool,
        options: &OutlineOptions,
        out: &mut Vec<Symbol>,
    ) {
        match node.kind() {
            "export_statement" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    let before_len = out.len();
                    self.extract_nodes(&child, source, true, options, out);
                    let doc = Self::extract_doc_comment(node, source);
                    if doc.is_some() {
                        for sym in &mut out[before_len..] {
                            if sym.doc_comment.is_none() {
                                sym.doc_comment = doc.clone();
                            }
                        }
                    }
                }
            }
            "function_declaration" => {
                if let Some(sym) = self.extract_function(node, source, is_exported, options) {
                    out.push(sym);
                }
            }
            "class_declaration" => {
                if let Some(sym) = self.extract_class(node, source, is_exported, options) {
                    out.push(sym);
                }
            }
            "interface_declaration" => {
                if let Some(sym) = self.extract_interface(node, source, is_exported, options) {
                    out.push(sym);
                }
            }
            "type_alias_declaration" => {
                if let Some(sym) = self.extract_type_alias(node, source, is_exported, options) {
                    out.push(sym);
                }
            }
            "enum_declaration" => {
                if let Some(sym) = self.extract_enum(node, source, is_exported, options) {
                    out.push(sym);
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                let mut cursor = node.walk();
                for decl in node.named_children(&mut cursor) {
                    if decl.kind() == "variable_declarator"
                        && let Some(name_n) = decl.child_by_field_name("name")
                    {
                        let name = node_text(&name_n, source).to_string();
                        if options.exported_only == Some(true) && !is_exported {
                            continue;
                        }

                        let is_fn = if let Some(val) = decl.child_by_field_name("value") {
                            val.kind() == "arrow_function" || val.kind() == "function"
                        } else {
                            false
                        };

                        let kind = if is_fn {
                            SymbolKind::Function
                        } else if name.chars().all(|c| c.is_ascii_uppercase() || c == '_')
                            && name.len() > 1
                        {
                            SymbolKind::Constant
                        } else {
                            SymbolKind::Variable
                        };

                        if let Some(ref allowed) = options.symbol_kinds
                            && !allowed.contains(&kind)
                        {
                            continue;
                        }

                        let sig =
                            clean_signature(node_text(&decl, source).trim_end_matches(';').trim());
                        out.push(Symbol {
                            name,
                            kind,
                            span: node_span(&decl),
                            signature: Some(sig),
                            doc_comment: Self::extract_doc_comment(node, source),
                            visibility: if is_exported {
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
            _ => {}
        }
    }
}

impl LanguageOutline for TypeScriptOutline {
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let root = tree.root_node();
        let mut symbols = Vec::new();
        let mut cursor = root.walk();

        for child in root.named_children(&mut cursor) {
            self.extract_nodes(&child, source, false, options, &mut symbols);
        }

        symbols
    }
}
