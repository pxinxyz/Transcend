//! Code Skeleton Renderer
//!
//! Generates ultra-compact, indented, syntax-appropriate code skeletons
//! with `{ ... }` or `...` replacing implementation bodies.

use transcend_protocol::{Symbol, SymbolKind};

pub struct SkeletonRenderer;

impl SkeletonRenderer {
    pub fn render(symbols: &[Symbol], language: &str, include_docs: bool) -> String {
        let mut out = String::new();

        for (i, sym) in symbols.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            Self::render_symbol(sym, 0, language, include_docs, &mut out);
        }

        out
    }

    fn render_symbol(
        sym: &Symbol,
        depth: usize,
        language: &str,
        include_docs: bool,
        out: &mut String,
    ) {
        let indent = "    ".repeat(depth);

        // 1. Doc comment
        if include_docs {
            if let Some(ref doc) = sym.doc_comment {
                let doc_line = doc.trim();
                if !doc_line.is_empty() {
                    let doc_prefix = match language {
                        "rust" | "zig" | "dart" | "swift" => "///",
                        "python" | "bash" | "ruby" => "#",
                        "sql" | "lua" => "--",
                        "markdown" => ">",
                        _ => "//",
                    };
                    out.push_str(&format!("{}{} {}\n", indent, doc_prefix, doc_line));
                }
            }
        }

        let sig = sym.signature.as_deref().unwrap_or(&sym.name).trim();

        // 2. Markdown special handling (headings)
        if language == "markdown" {
            out.push_str(&format!("{}{}\n", indent, sig));
            for child in &sym.children {
                Self::render_symbol(child, depth + 1, language, include_docs, out);
            }
            return;
        }

        // 3. Python
        if language == "python" {
            match sym.kind {
                SymbolKind::Class => {
                    out.push_str(&format!("{}{}:\n", indent, sig));
                    if sym.children.is_empty() {
                        out.push_str(&format!("{}    ...\n", indent));
                    } else {
                        for child in &sym.children {
                            Self::render_symbol(child, depth + 1, language, include_docs, out);
                        }
                    }
                }
                SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor => {
                    out.push_str(&format!("{}{}: ...\n", indent, sig));
                }
                _ => {
                    out.push_str(&format!("{}{}\n", indent, sig));
                }
            }
            return;
        }

        // 4. Ruby
        if language == "ruby" {
            match sym.kind {
                SymbolKind::Class | SymbolKind::Module => {
                    out.push_str(&format!("{}{}\n", indent, sig));
                    for child in &sym.children {
                        Self::render_symbol(child, depth + 1, language, include_docs, out);
                    }
                    out.push_str(&format!("{}end\n", indent));
                }
                SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor => {
                    out.push_str(&format!("{}{} ... end\n", indent, sig));
                }
                _ => {
                    out.push_str(&format!("{}{}\n", indent, sig));
                }
            }
            return;
        }

        // 5. Lua
        if language == "lua" {
            match sym.kind {
                SymbolKind::Function | SymbolKind::Method => {
                    out.push_str(&format!("{}{} ... end\n", indent, sig));
                }
                _ => {
                    out.push_str(&format!("{}{}\n", indent, sig));
                }
            }
            return;
        }

        // 6. Curly-brace languages (Rust, C, C++, C#, Java, Go, TS, JS, PHP, Zig, Dart, Swift, Bash, etc.)
        match sym.kind {
            SymbolKind::Struct
            | SymbolKind::Class
            | SymbolKind::Interface
            | SymbolKind::Trait
            | SymbolKind::Implementation
            | SymbolKind::Namespace
            | SymbolKind::Module => {
                if sym.children.is_empty() {
                    let terminator = if language == "c" || language == "cpp" { " {};\n" } else { " {}\n" };
                    out.push_str(&format!("{}{}{}", indent, sig, terminator));
                } else {
                    out.push_str(&format!("{}{} {{\n", indent, sig));
                    for child in &sym.children {
                        Self::render_symbol(child, depth + 1, language, include_docs, out);
                    }
                    let closer = if (language == "c" || language == "cpp") && (sym.kind == SymbolKind::Struct || sym.kind == SymbolKind::Class) {
                        "};\n"
                    } else {
                        "}\n"
                    };
                    out.push_str(&format!("{}{}", indent, closer));
                }
            }
            SymbolKind::Enum => {
                out.push_str(&format!("{}{} {{\n", indent, sig));
                for child in &sym.children {
                    let child_sig = child.signature.as_deref().unwrap_or(&child.name).trim();
                    out.push_str(&format!("{}    {},\n", indent, child_sig));
                }
                out.push_str(&format!("{}}}\n", indent));
            }
            SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor => {
                if sig.ends_with(';') {
                    out.push_str(&format!("{}{}\n", indent, sig));
                } else if language == "go" && !sig.starts_with("func") {
                    out.push_str(&format!("{}{}\n", indent, sig));
                } else {
                    out.push_str(&format!("{}{} {{ ... }}\n", indent, sig));
                }
            }
            SymbolKind::Field | SymbolKind::Property => {
                let term = if sig.ends_with(';') || sig.ends_with(',') { "" } else { ";" };
                out.push_str(&format!("{}{}{}\n", indent, sig, term));
            }
            SymbolKind::Constant | SymbolKind::Static | SymbolKind::TypeAlias => {
                let term = if sig.ends_with(';') { "" } else { ";" };
                out.push_str(&format!("{}{}{}\n", indent, sig, term));
            }
            _ => {
                out.push_str(&format!("{}{}\n", indent, sig));
            }
        }
    }
}
