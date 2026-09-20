//! Dart Language Outline Adapter
//!
//! Extracts semantic symbols from Dart source code using Tree-sitter.

use transcend_protocol::{OutlineOptions, Symbol, SymbolKind};
use tree_sitter::{Node, Tree};

use super::{LanguageOutline, clean_signature, node_span, node_text};

pub struct DartOutline;

impl Default for DartOutline {
    fn default() -> Self {
        Self::new()
    }
}

impl DartOutline {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageOutline for DartOutline {
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if let Some(sym) = extract_dart_symbol(&child, source, 0, options) {
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
            let clean = trimmed.trim_start_matches("///").trim();
            if !clean.is_empty() {
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

fn find_dart_name<'a>(node: &Node, source: &'a [u8]) -> Option<&'a str> {
    if let Some(name_node) = node.child_by_field_name("name") {
        return Some(node_text(&name_node, source).trim());
    }
    if node.kind() == "identifier" || node.kind() == "type_identifier" {
        let text = node_text(node, source).trim();
        if !text.is_empty()
            && text != "class"
            && text != "mixin"
            && text != "enum"
            && text != "void"
        {
            return Some(text);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "function_body"
            && child.kind() != "block"
            && child.kind() != "formal_parameter_list"
            && let Some(name) = find_dart_name(&child, source)
        {
            return Some(name);
        }
    }
    None
}

fn extract_dart_symbol(
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
        // Classes, Mixins, Enums, Extensions
        "class_definition"
        | "class_declaration"
        | "mixin_declaration"
        | "enum_declaration"
        | "extension_declaration" => {
            let name = find_dart_name(node, source)?;
            let is_private = name.starts_with('_');

            if options.exported_only == Some(true) && is_private {
                return None;
            }

            let kind = match node.kind() {
                "class_definition" | "class_declaration" => SymbolKind::Class,
                "mixin_declaration" => SymbolKind::Trait,
                "enum_declaration" => SymbolKind::Enum,
                "extension_declaration" => SymbolKind::Implementation,
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

            let mut children = Vec::new();
            if let Some(body) = node.child_by_field_name("body").or_else(|| {
                let mut cursor = node.walk();
                node.children(&mut cursor)
                    .find(|c| c.kind().ends_with("_body") || c.kind() == "body")
            }) {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if let Some(sym) = extract_dart_symbol(&child, source, depth + 1, options) {
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
                visibility: Some(if is_private {
                    "private".to_string()
                } else {
                    "public".to_string()
                }),
                relationships: Vec::new(),
                children,
            })
        }

        // Class member wrapper
        "class_member" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named()
                    && let Some(sym) = extract_dart_symbol(&child, source, depth, options)
                {
                    return Some(sym);
                }
            }
            None
        }

        // Functions / Methods
        "method_declaration"
        | "function_signature"
        | "method_signature"
        | "function_definition" => {
            let name = find_dart_name(node, source)?;
            let is_private = name.starts_with('_');

            if options.exported_only == Some(true) && is_private {
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
                visibility: Some(if is_private {
                    "private".to_string()
                } else {
                    "public".to_string()
                }),
                relationships: Vec::new(),
                children: Vec::new(),
            })
        }

        _ => None,
    }
}
