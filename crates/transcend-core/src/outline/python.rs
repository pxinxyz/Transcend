//! Python AST Outline Adapter
//!
//! Extracts semantic symbols from Python source code using Tree-sitter.

use transcend_protocol::{OutlineOptions, Symbol, SymbolKind, SymbolRelationship};
use tree_sitter::{Node, Tree};

use super::{LanguageOutline, clean_signature, node_span, node_text};

pub struct PythonOutline;

impl Default for PythonOutline {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonOutline {
    pub fn new() -> Self {
        Self
    }

    fn extract_docstring(body_node: &Node, source: &[u8]) -> Option<String> {
        let mut cursor = body_node.walk();
        for child in body_node.named_children(&mut cursor) {
            if child.kind() == "expression_statement"
                && let Some(first) = child.named_child(0)
                && first.kind() == "string"
            {
                let raw = node_text(&first, source).trim();
                let clean = raw
                    .trim_start_matches("\"\"\"")
                    .trim_end_matches("\"\"\"")
                    .trim_start_matches("'''")
                    .trim_end_matches("'''")
                    .trim_start_matches('"')
                    .trim_end_matches('"')
                    .trim_start_matches('\'')
                    .trim_end_matches('\'')
                    .trim();
                let first_line = clean.lines().next().unwrap_or("").trim();
                if !first_line.is_empty() {
                    return Some(first_line.to_string());
                }
            }
            // Only inspect the very first non-comment statement
            if child.kind() != "comment" {
                break;
            }
        }
        None
    }

    fn extract_signature(
        effective_node: &Node,
        inner_node: &Node,
        source: &[u8],
    ) -> Option<String> {
        if let Some(body_node) = inner_node.child_by_field_name("body") {
            let body_start = body_node.start_byte();
            if body_start >= effective_node.start_byte() {
                let sig_bytes = &source[effective_node.start_byte()..body_start];
                let sig_str = std::str::from_utf8(sig_bytes).unwrap_or("");
                let cleaned = clean_signature(sig_str.trim().trim_end_matches(':').trim());
                if !cleaned.is_empty() {
                    return Some(cleaned);
                }
            }
        }

        let first_line = node_text(effective_node, source)
            .lines()
            .next()
            .unwrap_or("")
            .trim();
        let cleaned = clean_signature(first_line.trim_end_matches(':').trim());
        if !cleaned.is_empty() {
            Some(cleaned)
        } else {
            None
        }
    }

    fn determine_visibility(name: &str) -> Option<String> {
        if name.starts_with("__") && name.ends_with("__") {
            Some("special".to_string())
        } else if name.starts_with('_') {
            Some("private".to_string())
        } else {
            Some("public".to_string())
        }
    }

    fn extract_function(
        &self,
        node: &Node,
        outer_node: Option<&Node>,
        source: &[u8],
        is_method: bool,
        options: &OutlineOptions,
    ) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();
        let visibility = Self::determine_visibility(&name);

        if options.exported_only == Some(true) && visibility.as_deref() == Some("private") {
            return None;
        }

        let kind = if is_method {
            if name == "__init__" {
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

        let doc_comment = if options.include_doc_comments != Some(false) {
            node.child_by_field_name("body")
                .and_then(|b| Self::extract_docstring(&b, source))
        } else {
            None
        };

        let effective_node = outer_node.unwrap_or(node);

        Some(Symbol {
            name,
            kind,
            span: node_span(effective_node),
            signature: Self::extract_signature(effective_node, node, source),
            doc_comment,
            visibility,
            relationships: vec![],
            children: vec![],
        })
    }

    fn extract_class(
        &self,
        node: &Node,
        outer_node: Option<&Node>,
        source: &[u8],
        options: &OutlineOptions,
    ) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();
        let visibility = Self::determine_visibility(&name);

        if options.exported_only == Some(true) && visibility.as_deref() == Some("private") {
            return None;
        }

        if let Some(ref allowed) = options.symbol_kinds
            && !allowed.contains(&SymbolKind::Class)
        {
            return None;
        }

        let mut relationships = Vec::new();
        if options.include_relationships != Some(false)
            && let Some(superclasses) = node.child_by_field_name("superclasses")
        {
            let mut cursor = superclasses.walk();
            for arg in superclasses.named_children(&mut cursor) {
                let target = node_text(&arg, source).to_string();
                if !target.is_empty() {
                    relationships.push(SymbolRelationship {
                        relation: "extends".to_string(),
                        target,
                    });
                }
            }
        }

        let mut children = Vec::new();
        let mut doc_comment = None;

        if let Some(body) = node.child_by_field_name("body") {
            if options.include_doc_comments != Some(false) {
                doc_comment = Self::extract_docstring(&body, source);
            }

            let mut cursor = body.walk();
            for item in body.named_children(&mut cursor) {
                match item.kind() {
                    "function_definition" | "async_function_definition" => {
                        if let Some(sym) = self.extract_function(&item, None, source, true, options)
                        {
                            children.push(sym);
                        }
                    }
                    "decorated_definition" => {
                        if let Some(inner) = item.child_by_field_name("definition")
                            && (inner.kind() == "function_definition"
                                || inner.kind() == "async_function_definition")
                            && let Some(sym) =
                                self.extract_function(&inner, Some(&item), source, true, options)
                        {
                            children.push(sym);
                        }
                    }
                    _ => {}
                }
            }
        }

        let effective_node = outer_node.unwrap_or(node);

        Some(Symbol {
            name,
            kind: SymbolKind::Class,
            span: node_span(effective_node),
            signature: Self::extract_signature(effective_node, node, source),
            doc_comment,
            visibility,
            relationships,
            children,
        })
    }

    fn extract_node(&self, node: &Node, source: &[u8], options: &OutlineOptions) -> Option<Symbol> {
        match node.kind() {
            "function_definition" | "async_function_definition" => {
                self.extract_function(node, None, source, false, options)
            }
            "class_definition" => self.extract_class(node, None, source, options),
            "decorated_definition" => {
                if let Some(inner) = node.child_by_field_name("definition") {
                    match inner.kind() {
                        "function_definition" | "async_function_definition" => {
                            self.extract_function(&inner, Some(node), source, false, options)
                        }
                        "class_definition" => {
                            self.extract_class(&inner, Some(node), source, options)
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }
            "expression_statement" => {
                if let Some(assign) = node.named_child(0)
                    && assign.kind() == "assignment"
                    && let Some(left) = assign.child_by_field_name("left")
                    && left.kind() == "identifier"
                {
                    let name = node_text(&left, source).to_string();
                    let is_constant =
                        name.chars().all(|c| c.is_ascii_uppercase() || c == '_') && name.len() > 1;
                    let kind = if is_constant {
                        SymbolKind::Constant
                    } else {
                        SymbolKind::Variable
                    };

                    if let Some(ref allowed) = options.symbol_kinds
                        && !allowed.contains(&kind)
                    {
                        return None;
                    }

                    let sig = clean_signature(node_text(node, source).trim());
                    return Some(Symbol {
                        name: name.clone(),
                        kind,
                        span: node_span(node),
                        signature: Some(sig),
                        doc_comment: None,
                        visibility: Self::determine_visibility(&name),
                        relationships: vec![],
                        children: vec![],
                    });
                }
                None
            }
            _ => None,
        }
    }
}

impl LanguageOutline for PythonOutline {
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
