//! Bash / Shell Language Outline Adapter
//!
//! Extracts semantic symbols from Bash scripts using Tree-sitter.

use transcend_protocol::{OutlineOptions, Symbol, SymbolKind};
use tree_sitter::{Node, Tree};

use super::{LanguageOutline, clean_signature, node_span, node_text};

pub struct BashOutline;

impl Default for BashOutline {
    fn default() -> Self {
        Self::new()
    }
}

impl BashOutline {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageOutline for BashOutline {
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if let Some(sym) = extract_bash_symbol(&child, source, 0, options) {
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
        if trimmed.starts_with('#') && !trimmed.starts_with("#!") {
            let clean = trimmed.trim_start_matches('#').trim();
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

fn extract_bash_symbol(
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
        // Functions: `foo() { ... }` or `function foo { ... }`
        "function_definition" => {
            let mut name = None;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "word" {
                    name = Some(node_text(&child, source).trim());
                    break;
                }
            }
            let name = name.or_else(|| {
                node.child_by_field_name("name")
                    .map(|n| node_text(&n, source).trim())
            })?;

            let kind = SymbolKind::Function;
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
                visibility: Some("public".to_string()),
                relationships: Vec::new(),
                children: Vec::new(),
            })
        }

        // Declaration command (e.g. export FOO=bar, readonly BAZ=qux)
        "declaration_command" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "variable_assignment" {
                    return extract_bash_symbol(&child, source, depth, options);
                }
            }
            None
        }

        // Top-level exported variables / constants (e.g. readonly FOO=bar or export BAR=baz)
        "variable_assignment" => {
            let mut name = None;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "variable_name" {
                    name = Some(node_text(&child, source).trim());
                    break;
                }
            }
            let name = name.or_else(|| {
                node.child_by_field_name("name")
                    .map(|n| node_text(&n, source).trim())
            })?;
            // Only capture UPPERCASE constants or exports to avoid noise
            if !name
                .chars()
                .all(|c| c.is_uppercase() || c == '_' || c.is_ascii_digit())
            {
                return None;
            }

            let kind = SymbolKind::Constant;
            if let Some(ref allowed) = options.symbol_kinds
                && !allowed.contains(&kind)
            {
                return None;
            }

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
                visibility: Some("public".to_string()),
                relationships: Vec::new(),
                children: Vec::new(),
            })
        }

        _ => None,
    }
}
