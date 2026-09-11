//! SQL Language Outline Adapter
//!
//! Extracts semantic symbols (Tables, Views, Procedures, Functions, Indexes) from SQL.

use tree_sitter::{Node, Tree};
use transcend_protocol::{OutlineOptions, Symbol, SymbolKind};

use super::{node_span, node_text, LanguageOutline};

pub struct SqlOutline;

impl SqlOutline {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageOutline for SqlOutline {
    fn extract(&self, tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if let Some(sym) = extract_sql_symbol(&child, source, 0, options) {
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
        if trimmed.starts_with("--") {
            let clean = trimmed.trim_start_matches("--").trim();
            if !clean.is_empty() {
                doc_lines.push(clean);
            }
        } else if trimmed.starts_with("/*") || trimmed.starts_with("*/") || trimmed.starts_with('*') {
            let clean = trimmed
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
        doc_lines.into_iter().find(|l| !l.is_empty()).map(|l| l.to_string())
    }
}

fn extract_sql_symbol(
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

    // Unwrap statement wrapper
    if node.kind() == "statement" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_named() {
                if let Some(sym) = extract_sql_symbol(&child, source, depth, options) {
                    return Some(sym);
                }
            }
        }
        return None;
    }

    let text = node_text(node, source).trim();
    let text_upper = text.to_uppercase();

    // Check DDL statements
    if node.kind() == "create_table" || text_upper.starts_with("CREATE TABLE") {
        let mut name = "unnamed_table";
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "object_reference" || child.kind() == "identifier" {
                name = node_text(&child, source).trim();
                break;
            }
        }

        let mut children = Vec::new();
        // Look for column definitions (inside column_definitions or direct)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "column_definitions" {
                let mut col_cursor = child.walk();
                for col_child in child.children(&mut col_cursor) {
                    if col_child.kind() == "column_definition" {
                        let col_name = col_child.children(&mut col_child.walk())
                            .find(|c| c.kind() == "identifier")
                            .map(|c| node_text(&c, source).trim().to_string())
                            .unwrap_or_else(|| "column".to_string());
                        children.push(Symbol {
                            name: col_name,
                            kind: SymbolKind::Field,
                            span: node_span(&col_child),
                            signature: Some(node_text(&col_child, source).trim().to_string()),
                            doc_comment: None,
                            visibility: None,
                            relationships: Vec::new(),
                            children: Vec::new(),
                        });
                    }
                }
            } else if child.kind() == "column_definition" {
                let col_name = child.children(&mut child.walk())
                    .find(|c| c.kind() == "identifier")
                    .map(|c| node_text(&c, source).trim().to_string())
                    .unwrap_or_else(|| "column".to_string());
                children.push(Symbol {
                    name: col_name,
                    kind: SymbolKind::Field,
                    span: node_span(&child),
                    signature: Some(node_text(&child, source).trim().to_string()),
                    doc_comment: None,
                    visibility: None,
                    relationships: Vec::new(),
                    children: Vec::new(),
                });
            }
        }

        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Struct,
            span: node_span(node),
            signature: Some(format!("CREATE TABLE {}", name)),
            doc_comment: if options.include_doc_comments != Some(false) {
                extract_doc_comment(node, source)
            } else {
                None
            },
            visibility: Some("public".to_string()),
            relationships: Vec::new(),
            children,
        });
    }

    if node.kind() == "create_view" || text_upper.starts_with("CREATE VIEW") {
        let mut name = "unnamed_view";
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "object_reference" || child.kind() == "identifier" {
                name = node_text(&child, source).trim();
                break;
            }
        }

        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Interface,
            span: node_span(node),
            signature: Some(format!("CREATE VIEW {}", name)),
            doc_comment: if options.include_doc_comments != Some(false) {
                extract_doc_comment(node, source)
            } else {
                None
            },
            visibility: Some("public".to_string()),
            relationships: Vec::new(),
            children: Vec::new(),
        });
    }

    if node.kind() == "create_procedure" || text_upper.starts_with("CREATE PROCEDURE") || text_upper.starts_with("CREATE OR REPLACE PROCEDURE") {
        let mut name = "unnamed_procedure";
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "object_reference" || child.kind() == "identifier" {
                name = node_text(&child, source).trim();
                break;
            }
        }

        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            span: node_span(node),
            signature: Some(format!("CREATE PROCEDURE {}", name)),
            doc_comment: if options.include_doc_comments != Some(false) {
                extract_doc_comment(node, source)
            } else {
                None
            },
            visibility: Some("public".to_string()),
            relationships: Vec::new(),
            children: Vec::new(),
        });
    }

    if node.kind() == "create_function" || text_upper.starts_with("CREATE FUNCTION") || text_upper.starts_with("CREATE OR REPLACE FUNCTION") {
        let mut name = "unnamed_function";
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "object_reference" || child.kind() == "identifier" {
                name = node_text(&child, source).trim();
                break;
            }
        }

        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            span: node_span(node),
            signature: Some(format!("CREATE FUNCTION {}", name)),
            doc_comment: if options.include_doc_comments != Some(false) {
                extract_doc_comment(node, source)
            } else {
                None
            },
            visibility: Some("public".to_string()),
            relationships: Vec::new(),
            children: Vec::new(),
        });
    }

    if node.kind() == "create_index" || text_upper.starts_with("CREATE INDEX") || text_upper.starts_with("CREATE UNIQUE INDEX") {
        let mut name = "unnamed_index";
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "object_reference" || child.kind() == "identifier" {
                name = node_text(&child, source).trim();
                break;
            }
        }

        return Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Constant,
            span: node_span(node),
            signature: Some(format!("CREATE INDEX {}", name)),
            doc_comment: if options.include_doc_comments != Some(false) {
                extract_doc_comment(node, source)
            } else {
                None
            },
            visibility: Some("public".to_string()),
            relationships: Vec::new(),
            children: Vec::new(),
        });
    }

    None
}
