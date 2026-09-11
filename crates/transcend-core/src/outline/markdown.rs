//! Markdown Document Outline Adapter
//!
//! Extracts hierarchical document structure (ATX headings #, ##, ###) with nested sections.

use tree_sitter::Tree;
use transcend_protocol::{OutlineOptions, SourceSpan, Symbol, SymbolKind};

use super::LanguageOutline;

pub struct MarkdownOutline;

impl MarkdownOutline {
    pub fn new() -> Self {
        Self
    }
}

struct HeadingEntry {
    level: usize,
    name: String,
    span: SourceSpan,
    signature: String,
    doc_comment: Option<String>,
}

impl LanguageOutline for MarkdownOutline {
    fn extract(&self, _tree: &Tree, source: &[u8], options: &OutlineOptions) -> Vec<Symbol> {
        let text = match std::str::from_utf8(source) {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };

        let mut headings = Vec::new();
        let lines: Vec<&str> = text.lines().collect();
        let mut byte_offset = 0;

        for (idx, line) in lines.iter().enumerate() {
            let line_len = line.len();
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                let hashes = trimmed.chars().take_while(|c| *c == '#').count();
                if hashes <= 6 && trimmed.chars().nth(hashes).map_or(false, |c| c == ' ') {
                    let title = trimmed[hashes..].trim();
                    let start_col = line.len() - trimmed.len() + 1;
                    let end_col = line.len() + 1;
                    let line_num = idx + 1;

                    // Extract first non-empty text line following heading as doc summary
                    let mut doc_comment = None;
                    for follow_line in lines.iter().skip(idx + 1) {
                        let ft = follow_line.trim();
                        if ft.is_empty() {
                            continue;
                        }
                        if ft.starts_with('#') {
                            break;
                        }
                        doc_comment = Some(ft.to_string());
                        break;
                    }

                    headings.push(HeadingEntry {
                        level: hashes,
                        name: title.to_string(),
                        span: SourceSpan {
                            start_line: line_num,
                            start_col,
                            end_line: line_num,
                            end_col,
                            start_byte: byte_offset + (start_col - 1),
                            end_byte: byte_offset + line_len,
                        },
                        signature: format!("{} {}", "#".repeat(hashes), title),
                        doc_comment: if options.include_doc_comments != Some(false) {
                            doc_comment
                        } else {
                            None
                        },
                    });
                }
            }
            // Advance byte offset (+ 1 for \n or + 2 for \r\n)
            byte_offset += line_len;
            if text.as_bytes().get(byte_offset) == Some(&b'\r') {
                byte_offset += 1;
            }
            if text.as_bytes().get(byte_offset) == Some(&b'\n') {
                byte_offset += 1;
            }
        }

        // Build hierarchical tree: H2 nested in H1, H3 nested in H2, etc.
        build_heading_hierarchy(headings, options)
    }
}

fn build_heading_hierarchy(headings: Vec<HeadingEntry>, options: &OutlineOptions) -> Vec<Symbol> {
    let mut root_symbols: Vec<Symbol> = Vec::new();
    let mut stack: Vec<(usize, Symbol)> = Vec::new(); // (level, symbol)

    for entry in headings {
        let sym = Symbol {
            name: entry.name,
            kind: SymbolKind::Module,
            span: entry.span,
            signature: Some(entry.signature),
            doc_comment: entry.doc_comment,
            visibility: Some("public".to_string()),
            relationships: Vec::new(),
            children: Vec::new(),
        };

        // Pop until we find a parent with a strictly smaller heading level
        while let Some((parent_level, _)) = stack.last() {
            if *parent_level >= entry.level {
                let (_, popped) = stack.pop().unwrap();
                if let Some((_, parent)) = stack.last_mut() {
                    parent.children.push(popped);
                } else {
                    root_symbols.push(popped);
                }
            } else {
                break;
            }
        }

        stack.push((entry.level, sym));
    }

    // Unwind remainder of stack
    while let Some((_, popped)) = stack.pop() {
        if let Some((_, parent)) = stack.last_mut() {
            parent.children.push(popped);
        } else {
            root_symbols.push(popped);
        }
    }

    // Apply max_depth if specified
    if let Some(max_depth) = options.max_depth {
        prune_depth(&mut root_symbols, 0, max_depth);
    }

    root_symbols
}

fn prune_depth(symbols: &mut Vec<Symbol>, current_depth: usize, max_depth: usize) {
    if current_depth >= max_depth {
        for sym in symbols.iter_mut() {
            sym.children.clear();
        }
    } else {
        for sym in symbols.iter_mut() {
            prune_depth(&mut sym.children, current_depth + 1, max_depth);
        }
    }
}
