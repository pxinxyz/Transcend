//! Rust Language Outline Adapter
//!
//! Extracts semantic symbols from Rust source code using Tree-sitter.

use tree_sitter::{Node, Tree};
use transcend_protocol::{
    OutlineOptions, Symbol, SymbolKind, SymbolRelationship,
};

use super::{clean_signature, node_span, node_text, LanguageOutline};

pub struct RustOutline;

impl RustOutline {
    pub fn new() -> Self {
        Self
    }

    fn extract_doc_comment(node: &Node, source: &[u8]) -> Option<String> {
        let prefix = std::str::from_utf8(&source[..node.start_byte()]).ok()?;
        let mut doc_lines = Vec::new();

        for line in prefix.lines().rev() {
            let trimmed = line.trim();
            if trimmed.starts_with("///") {
                doc_lines.push(trimmed.trim_start_matches("///").trim());
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
        node.child_by_field_name("visibility")
            .or_else(|| {
                let mut cursor = node.walk();
                node.children(&mut cursor)
                    .find(|c| c.kind() == "visibility_modifier")
            })
            .map(|v| node_text(&v, source).to_string())
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

        // For declarations without body or semicolon-terminated (e.g. type Foo = Bar;)
        let first_line = text.lines().next().unwrap_or("").trim();
        let cleaned = clean_signature(first_line.trim_end_matches('{').trim());
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
        is_method: bool,
        options: &OutlineOptions,
    ) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();
        let visibility = Self::extract_visibility(node, source);

        if options.exported_only == Some(true) && visibility.is_none() {
            return None;
        }

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

        let doc_comment = if options.include_doc_comments != Some(false) {
            Self::extract_doc_comment(node, source)
        } else {
            None
        };

        Some(Symbol {
            name,
            kind,
            span: node_span(node),
            signature: Self::extract_signature(node, source),
            doc_comment,
            visibility,
            relationships: vec![],
            children: vec![],
        })
    }

    fn extract_struct(&self, node: &Node, source: &[u8], options: &OutlineOptions) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();
        let visibility = Self::extract_visibility(node, source);

        if options.exported_only == Some(true) && visibility.is_none() {
            return None;
        }

        if let Some(ref allowed) = options.symbol_kinds {
            if !allowed.contains(&SymbolKind::Struct) {
                return None;
            }
        }

        let mut children = Vec::new();
        if let Some(field_list) = node.child_by_field_name("body") {
            let mut cursor = field_list.walk();
            for field in field_list.named_children(&mut cursor) {
                if field.kind() == "field_declaration" {
                    if let Some(f_name_node) = field.child_by_field_name("name") {
                        let f_name = node_text(&f_name_node, source).to_string();
                        let f_vis = Self::extract_visibility(&field, source);
                        let f_sig = clean_signature(node_text(&field, source).trim_end_matches(',').trim());
                        children.push(Symbol {
                            name: f_name,
                            kind: SymbolKind::Field,
                            span: node_span(&field),
                            signature: Some(f_sig),
                            doc_comment: Self::extract_doc_comment(&field, source),
                            visibility: f_vis,
                            relationships: vec![],
                            children: vec![],
                        });
                    }
                }
            }
        }

        Some(Symbol {
            name,
            kind: SymbolKind::Struct,
            span: node_span(node),
            signature: Self::extract_signature(node, source),
            doc_comment: Self::extract_doc_comment(node, source),
            visibility,
            relationships: vec![],
            children,
        })
    }

    fn extract_enum(&self, node: &Node, source: &[u8], options: &OutlineOptions) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();
        let visibility = Self::extract_visibility(node, source);

        if options.exported_only == Some(true) && visibility.is_none() {
            return None;
        }

        if let Some(ref allowed) = options.symbol_kinds {
            if !allowed.contains(&SymbolKind::Enum) {
                return None;
            }
        }

        let mut children = Vec::new();
        if let Some(variant_list) = node.child_by_field_name("body") {
            let mut cursor = variant_list.walk();
            for variant in variant_list.named_children(&mut cursor) {
                if variant.kind() == "enum_variant" {
                    if let Some(v_name_node) = variant.child_by_field_name("name") {
                        let v_name = node_text(&v_name_node, source).to_string();
                        children.push(Symbol {
                            name: v_name,
                            kind: SymbolKind::Constant,
                            span: node_span(&variant),
                            signature: Some(clean_signature(node_text(&variant, source).trim_end_matches(',').trim())),
                            doc_comment: Self::extract_doc_comment(&variant, source),
                            visibility: None,
                            relationships: vec![],
                            children: vec![],
                        });
                    }
                }
            }
        }

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

    fn extract_trait(&self, node: &Node, source: &[u8], options: &OutlineOptions) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();
        let visibility = Self::extract_visibility(node, source);

        if options.exported_only == Some(true) && visibility.is_none() {
            return None;
        }

        if let Some(ref allowed) = options.symbol_kinds {
            if !allowed.contains(&SymbolKind::Trait) {
                return None;
            }
        }

        let mut children = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for item in body.named_children(&mut cursor) {
                if item.kind() == "function_item" || item.kind() == "function_signature_item" {
                    if let Some(sym) = self.extract_function(&item, source, true, options) {
                        children.push(sym);
                    }
                }
            }
        }

        Some(Symbol {
            name,
            kind: SymbolKind::Trait,
            span: node_span(node),
            signature: Self::extract_signature(node, source),
            doc_comment: Self::extract_doc_comment(node, source),
            visibility,
            relationships: vec![],
            children,
        })
    }

    fn extract_impl(&self, node: &Node, source: &[u8], options: &OutlineOptions) -> Option<Symbol> {
        if let Some(ref allowed) = options.symbol_kinds {
            if !allowed.contains(&SymbolKind::Implementation) {
                return None;
            }
        }

        let type_node = node.child_by_field_name("type")?;
        let target_type = node_text(&type_node, source).to_string();

        let trait_name = node
            .child_by_field_name("trait")
            .map(|t| node_text(&t, source).to_string());

        let name = if let Some(ref tr) = trait_name {
            format!("impl {} for {}", tr, target_type)
        } else {
            format!("impl {}", target_type)
        };

        let relationships = if options.include_relationships != Some(false) {
            let mut rels = vec![SymbolRelationship {
                relation: "targets".to_string(),
                target: target_type.clone(),
            }];
            if let Some(ref tr) = trait_name {
                rels.push(SymbolRelationship {
                    relation: "implements".to_string(),
                    target: tr.clone(),
                });
            }
            rels
        } else {
            vec![]
        };

        let mut children = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for item in body.named_children(&mut cursor) {
                if item.kind() == "function_item" {
                    if let Some(sym) = self.extract_function(&item, source, true, options) {
                        children.push(sym);
                    }
                } else if item.kind() == "type_item" {
                    if let Some(name_n) = item.child_by_field_name("name") {
                        let t_name = node_text(&name_n, source).to_string();
                        children.push(Symbol {
                            name: t_name,
                            kind: SymbolKind::TypeAlias,
                            span: node_span(&item),
                            signature: Some(clean_signature(node_text(&item, source))),
                            doc_comment: None,
                            visibility: None,
                            relationships: vec![],
                            children: vec![],
                        });
                    }
                }
            }
        }

        // If exported_only is set, and all children are filtered out, still show impl if it has public methods
        if options.exported_only == Some(true) && children.is_empty() {
            return None;
        }

        Some(Symbol {
            name: name.clone(),
            kind: SymbolKind::Implementation,
            span: node_span(node),
            signature: Some(name),
            doc_comment: Self::extract_doc_comment(node, source),
            visibility: None,
            relationships,
            children,
        })
    }

    fn extract_type_alias(&self, node: &Node, source: &[u8], options: &OutlineOptions) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();
        let visibility = Self::extract_visibility(node, source);

        if options.exported_only == Some(true) && visibility.is_none() {
            return None;
        }

        if let Some(ref allowed) = options.symbol_kinds {
            if !allowed.contains(&SymbolKind::TypeAlias) {
                return None;
            }
        }

        Some(Symbol {
            name,
            kind: SymbolKind::TypeAlias,
            span: node_span(node),
            signature: Some(clean_signature(node_text(node, source))),
            doc_comment: Self::extract_doc_comment(node, source),
            visibility,
            relationships: vec![],
            children: vec![],
        })
    }

    fn extract_macro(&self, node: &Node, source: &[u8], options: &OutlineOptions) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();

        if let Some(ref allowed) = options.symbol_kinds {
            if !allowed.contains(&SymbolKind::Macro) {
                return None;
            }
        }

        Some(Symbol {
            name: format!("{}!", name),
            kind: SymbolKind::Macro,
            span: node_span(node),
            signature: Some(format!("macro_rules! {}", name)),
            doc_comment: Self::extract_doc_comment(node, source),
            visibility: None,
            relationships: vec![],
            children: vec![],
        })
    }

    fn extract_module(&self, node: &Node, source: &[u8], options: &OutlineOptions) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();
        let visibility = Self::extract_visibility(node, source);

        if options.exported_only == Some(true) && visibility.is_none() {
            return None;
        }

        if let Some(ref allowed) = options.symbol_kinds {
            if !allowed.contains(&SymbolKind::Module) {
                return None;
            }
        }

        let mut children = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for item in body.named_children(&mut cursor) {
                if let Some(sym) = self.extract_node(&item, source, options) {
                    children.push(sym);
                }
            }
        }

        Some(Symbol {
            name,
            kind: SymbolKind::Module,
            span: node_span(node),
            signature: Self::extract_signature(node, source),
            doc_comment: Self::extract_doc_comment(node, source),
            visibility,
            relationships: vec![],
            children,
        })
    }

    fn extract_const(&self, node: &Node, source: &[u8], options: &OutlineOptions) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();
        let visibility = Self::extract_visibility(node, source);

        if options.exported_only == Some(true) && visibility.is_none() {
            return None;
        }

        if let Some(ref allowed) = options.symbol_kinds {
            if !allowed.contains(&SymbolKind::Constant) {
                return None;
            }
        }

        Some(Symbol {
            name,
            kind: SymbolKind::Constant,
            span: node_span(node),
            signature: Some(clean_signature(node_text(node, source).trim_end_matches(';'))),
            doc_comment: Self::extract_doc_comment(node, source),
            visibility,
            relationships: vec![],
            children: vec![],
        })
    }

    fn extract_static(&self, node: &Node, source: &[u8], options: &OutlineOptions) -> Option<Symbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text(&name_node, source).to_string();
        let visibility = Self::extract_visibility(node, source);

        if options.exported_only == Some(true) && visibility.is_none() {
            return None;
        }

        if let Some(ref allowed) = options.symbol_kinds {
            if !allowed.contains(&SymbolKind::Static) {
                return None;
            }
        }

        Some(Symbol {
            name,
            kind: SymbolKind::Static,
            span: node_span(node),
            signature: Some(clean_signature(node_text(node, source).trim_end_matches(';'))),
            doc_comment: Self::extract_doc_comment(node, source),
            visibility,
            relationships: vec![],
            children: vec![],
        })
    }

    fn extract_node(&self, node: &Node, source: &[u8], options: &OutlineOptions) -> Option<Symbol> {
        match node.kind() {
            "function_item" => self.extract_function(node, source, false, options),
            "struct_item" => self.extract_struct(node, source, options),
            "enum_item" => self.extract_enum(node, source, options),
            "trait_item" => self.extract_trait(node, source, options),
            "impl_item" => self.extract_impl(node, source, options),
            "type_item" => self.extract_type_alias(node, source, options),
            "const_item" => self.extract_const(node, source, options),
            "static_item" => self.extract_static(node, source, options),
            "macro_definition" => self.extract_macro(node, source, options),
            "mod_item" => self.extract_module(node, source, options),
            _ => None,
        }
    }

    fn extract_symbols_from_node(&self, node: &Node, source: &[u8], options: &OutlineOptions, symbols: &mut Vec<Symbol>) {
        if let Some(sym) = self.extract_node(node, source, options) {
            symbols.push(sym);
        } else if node.kind() == "macro_invocation" || node.kind() == "token_tree" {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                self.extract_symbols_from_node(&child, source, options, symbols);
            }
        }
    }
}

impl LanguageOutline for RustOutline {
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let root = tree.root_node();
        let mut symbols = Vec::new();
        let mut cursor = root.walk();

        for child in root.named_children(&mut cursor) {
            self.extract_symbols_from_node(&child, source, options, &mut symbols);
        }

        symbols
    }
}
