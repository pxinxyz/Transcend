//! Token Distiller & Compactor for LSP Responses
//!
//! Transforms verbose, unformatted LSP JSON responses into token-compact,
//! structured data models tailored for LLM agents.

use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use transcend_protocol::{
    DiagnosticSeverity, LspDiagnosticItem, LspDiagnosticsResponse, LspReferenceLocation,
    LspTargetLocation, SourceSpan,
};

use super::protocol::uri_to_path;

pub struct LspDistiller;

impl LspDistiller {
    /// Distill an LSP `textDocument/definition` result into compact target locations.
    pub fn distill_definition(raw: &Value, workspace_root: &Path) -> Vec<LspTargetLocation> {
        let mut targets = Vec::new();

        if raw.is_null() {
            return targets;
        }

        if let Some(arr) = raw.as_array() {
            for item in arr {
                if let Some(target) = Self::parse_single_location(item, workspace_root) {
                    targets.push(target);
                }
            }
        } else if let Some(target) = Self::parse_single_location(raw, workspace_root) {
            targets.push(target);
        }

        targets
    }

    fn parse_single_location(item: &Value, workspace_root: &Path) -> Option<LspTargetLocation> {
        // Can be Location { uri, range } or LocationLink { targetUri, targetRange, targetSelectionRange }
        let uri_str = item
            .get("targetUri")
            .or_else(|| item.get("uri"))?
            .as_str()?;
        let range = item
            .get("targetSelectionRange")
            .or_else(|| item.get("targetRange"))
            .or_else(|| item.get("range"))?;

        let file_path = uri_to_path(uri_str);
        let rel_path = file_path
            .strip_prefix(workspace_root)
            .unwrap_or(&file_path)
            .to_string_lossy()
            .replace('\\', "/");

        let start = range.get("start")?;
        let end = range.get("end")?;

        let start_line = start.get("line")?.as_u64()? as usize + 1;
        let start_col = start.get("character")?.as_u64()? as usize + 1;
        let end_line = end.get("line")?.as_u64()? as usize + 1;
        let end_col = end.get("character")?.as_u64()? as usize + 1;

        // Try reading preview snippet from disk if file exists
        let preview = if file_path.exists() {
            fs::read_to_string(&file_path).ok().and_then(|content| {
                content
                    .lines()
                    .nth(start_line.saturating_sub(1))
                    .map(|l| l.trim().to_string())
            })
        } else {
            None
        };

        Some(LspTargetLocation {
            file: rel_path,
            span: SourceSpan {
                start_line,
                start_col,
                end_line,
                end_col,
                start_byte: 0,
                end_byte: 0,
            },
            preview,
        })
    }

    /// Distill an LSP `textDocument/references` result into compact reference locations.
    pub fn distill_references(
        raw: &Value,
        workspace_root: &Path,
        limit: usize,
    ) -> (Vec<LspReferenceLocation>, usize, bool) {
        let mut references = Vec::new();
        let Some(arr) = raw.as_array() else {
            return (references, 0, false);
        };

        let total_found = arr.len();

        for item in arr.iter().take(limit) {
            let Some(uri_str) = item.get("uri").and_then(|u| u.as_str()) else {
                continue;
            };
            let Some(range) = item.get("range") else {
                continue;
            };

            let file_path = uri_to_path(uri_str);
            let rel_path = file_path
                .strip_prefix(workspace_root)
                .unwrap_or(&file_path)
                .to_string_lossy()
                .replace('\\', "/");

            let start_line = range
                .get("start")
                .and_then(|s| s.get("line"))
                .and_then(|l| l.as_u64())
                .unwrap_or(0) as usize
                + 1;
            let start_col = range
                .get("start")
                .and_then(|s| s.get("character"))
                .and_then(|c| c.as_u64())
                .unwrap_or(0) as usize
                + 1;
            let end_line = range
                .get("end")
                .and_then(|e| e.get("line"))
                .and_then(|l| l.as_u64())
                .unwrap_or(0) as usize
                + 1;
            let end_col = range
                .get("end")
                .and_then(|e| e.get("character"))
                .and_then(|c| c.as_u64())
                .unwrap_or(0) as usize
                + 1;

            let line_text = if file_path.exists() {
                fs::read_to_string(&file_path)
                    .ok()
                    .and_then(|content| {
                        content
                            .lines()
                            .nth(start_line.saturating_sub(1))
                            .map(|l| l.trim().to_string())
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };

            references.push(LspReferenceLocation {
                file: rel_path,
                span: SourceSpan {
                    start_line,
                    start_col,
                    end_line,
                    end_col,
                    start_byte: 0,
                    end_byte: 0,
                },
                line_text,
            });
        }

        let truncated = total_found > limit;
        (references, total_found, truncated)
    }

    /// Distill an LSP `textDocument/hover` result into (signature, documentation, span).
    pub fn distill_hover(raw: &Value) -> (Option<String>, Option<String>, Option<SourceSpan>) {
        if raw.is_null() {
            return (None, None, None);
        }

        let contents = raw.get("contents");
        let (signature, doc) = match contents {
            Some(Value::Object(obj)) => {
                let text = obj.get("value").and_then(|v| v.as_str()).unwrap_or("");
                Self::extract_signature_and_doc(text)
            }
            Some(Value::String(s)) => Self::extract_signature_and_doc(s),
            Some(Value::Array(arr)) => {
                let mut combined_sig: Option<String> = None;
                let mut docs = Vec::new();
                for item in arr {
                    if let Some(s) = item.as_str() {
                        docs.push(s.to_string());
                    } else if let Some(v) = item.get("value").and_then(|v| v.as_str()) {
                        if combined_sig.is_none()
                            && (v.contains("fn ")
                                || v.contains("struct ")
                                || v.contains("class ")
                                || v.contains("type "))
                        {
                            combined_sig = Some(v.trim().to_string());
                        } else {
                            docs.push(v.to_string());
                        }
                    }
                }
                (
                    combined_sig,
                    if docs.is_empty() {
                        None
                    } else {
                        Some(docs.join("\n\n"))
                    },
                )
            }
            _ => (None, None),
        };

        let span = raw.get("range").and_then(|range| {
            let start = range.get("start")?;
            let end = range.get("end")?;
            Some(SourceSpan {
                start_line: start.get("line")?.as_u64()? as usize + 1,
                start_col: start.get("character")?.as_u64()? as usize + 1,
                end_line: end.get("line")?.as_u64()? as usize + 1,
                end_col: end.get("character")?.as_u64()? as usize + 1,
                start_byte: 0,
                end_byte: 0,
            })
        });

        (signature, doc, span)
    }

    fn extract_signature_and_doc(markdown: &str) -> (Option<String>, Option<String>) {
        let trimmed = markdown.trim();
        if trimmed.is_empty() {
            return (None, None);
        }

        // Check if starts with code block
        if let Some(code_start) = trimmed.find("```") {
            let after_code = &trimmed[code_start + 3..];
            // Find end of first line (language tag)
            let lang_end = after_code.find('\n').unwrap_or(0);
            let code_body = &after_code[lang_end..];
            if let Some(code_end) = code_body.find("```") {
                let sig = code_body[..code_end].trim().to_string();
                let remainder = code_body[code_end + 3..].trim();
                let doc = if remainder.is_empty() {
                    None
                } else {
                    Some(remainder.to_string())
                };
                return (Some(sig), doc);
            }
        }

        (
            Some(trimmed.lines().next().unwrap_or("").to_string()),
            Some(trimmed.to_string()),
        )
    }

    /// Format and filter compiler diagnostics from active cache.
    pub fn distill_diagnostics(
        all_diags: &[LspDiagnosticItem],
        path_filter: Option<&str>,
        severity_filter: Option<DiagnosticSeverity>,
    ) -> LspDiagnosticsResponse {
        let mut filtered = Vec::new();
        let mut severity_breakdown = BTreeMap::new();

        for diag in all_diags {
            if let Some(pf) = path_filter
                && !Self::path_matches(&diag.file, pf)
            {
                continue;
            }

            if let Some(sf) = severity_filter
                && diag.severity != sf
            {
                continue;
            }

            let sev_str = match diag.severity {
                DiagnosticSeverity::Error => "error",
                DiagnosticSeverity::Warning => "warning",
                DiagnosticSeverity::Information => "information",
                DiagnosticSeverity::Hint => "hint",
            };
            *severity_breakdown.entry(sev_str.to_string()).or_insert(0) += 1;

            filtered.push(diag.clone());
        }

        LspDiagnosticsResponse {
            total_count: filtered.len(),
            diagnostics: filtered,
            severity_breakdown,
        }
    }

    /// Whether a cached diagnostic path satisfies a caller's path filter.
    ///
    /// Both sides arrive in one of two shapes -- absolute, as the engine resolves them, or
    /// relative to the session root, as a language server reports them -- and either side may
    /// be either shape. `LspDiagnosticsRequest.path` is documented as "file or directory", so
    /// the filter may also name a directory that a cached file sits under.
    ///
    /// A substring test (`cached.contains(filter)`) handles none of that: `"src/lib.rs"`
    /// contains neither `"/abs/src"` nor `"/abs/proj/src/lib.rs"`, which is why a filtered
    /// call returned zero diagnostics while the file really had errors in it.
    ///
    /// The rule is therefore: match on a path-SEGMENT basis, accepting the filter as an
    /// ancestor of the cached file, or the two as the same file written in different shapes.
    fn path_matches(diag_file: &str, filter: &str) -> bool {
        fn segments(p: &str) -> Vec<String> {
            p.replace('\\', "/")
                .split('/')
                .filter(|s| !s.is_empty() && *s != ".")
                .map(str::to_string)
                .collect()
        }
        let cached = segments(diag_file);
        let wanted = segments(filter);
        if cached.is_empty() || wanted.is_empty() {
            return false;
        }

        // Same depth: identical paths, or the same file in two shapes.
        if cached.len() == wanted.len() {
            return cached == wanted;
        }
        // Deeper filter than cache: the filter names the file (or something under it) and
        // the cache holds a relative prefix, so the cache must lead the filter.
        if wanted.len() > cached.len() {
            return wanted[..cached.len()] == cached[..]
                || wanted[wanted.len() - cached.len()..] == cached[..];
        }
        // Deeper cache than filter: the filter names a directory the cached file sits under.
        cached[..wanted.len()] == wanted[..] || cached[cached.len() - wanted.len()..] == wanted[..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_signature_and_doc() {
        let hover_text = "```rust\npub fn dispatch(&self, task: Task) -> Result<()>\n```\nDispatches an asynchronous task to workers.";
        let (sig, doc) = LspDistiller::extract_signature_and_doc(hover_text);
        assert_eq!(
            sig.as_deref(),
            Some("pub fn dispatch(&self, task: Task) -> Result<()>")
        );
        assert_eq!(
            doc.as_deref(),
            Some("Dispatches an asynchronous task to workers.")
        );
    }

    fn diag(file: &str, sev: DiagnosticSeverity) -> LspDiagnosticItem {
        LspDiagnosticItem {
            file: file.to_string(),
            severity: sev,
            span: SourceSpan {
                start_line: 1,
                start_col: 1,
                end_line: 1,
                end_col: 2,
                start_byte: 0,
                end_byte: 1,
            },
            message: "boom".to_string(),
            code: None,
            source: None,
        }
    }

    /// Regression: the diagnostics cache used to store file paths *relative* to the session
    /// root while the caller filtered with the absolute engine-resolved path. A relative key
    /// can never `contains()` an absolute path, so every filtered `lsp_diagnostics` call
    /// returned zero results and a broken file was reported as clean.
    #[test]
    fn test_diagnostics_path_filter_matches_absolute_paths() {
        let abs_file = "/home/dev/project/src/lib.rs";
        let other = "/home/dev/project/src/main.rs";
        let all = vec![
            diag(abs_file, DiagnosticSeverity::Error),
            diag(other, DiagnosticSeverity::Warning),
        ];

        // Filtering by the exact absolute file must keep only that file's diagnostics.
        let by_file = LspDistiller::distill_diagnostics(&all, Some(abs_file), None);
        assert_eq!(
            by_file.diagnostics.len(),
            1,
            "absolute file filter returned {:?}",
            by_file
                .diagnostics
                .iter()
                .map(|d| d.file.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(by_file.diagnostics[0].file, abs_file);

        // Filtering by an absolute directory must keep everything beneath it.
        let by_dir = LspDistiller::distill_diagnostics(&all, Some("/home/dev/project"), None);
        assert_eq!(by_dir.diagnostics.len(), 2);

        // An unrelated path that shares no tail must still filter everything out.
        let by_other =
            LspDistiller::distill_diagnostics(&all, Some("/elsewhere/src/other.rs"), None);
        assert_eq!(by_other.diagnostics.len(), 0);

        // A filter naming an unrelated project with the same tail must NOT match: the paths
        // differ in their middle segments, so neither is a prefix of the other.
        let by_same_tail =
            LspDistiller::distill_diagnostics(&all, Some("/elsewhere/src/lib.rs"), None);
        assert_eq!(
            by_same_tail.diagnostics.len(),
            0,
            "an unrelated absolute root must not match on a shared tail"
        );
    }

    /// Regression: filtering by the DIRECTORY containing a file returned zero diagnostics,
    /// while filtering by the file itself returned the real errors. Verified against a live
    /// rust-analyzer: a file with two compiler errors answered `total_count: 2` when named
    /// Both sides absolute -- the shape the compiler fallback produces, and what a server
    /// reporting absolute URIs produces.
    ///
    /// What actually occurs with rust-analyzer is different and is pinned in the
    /// relative-cache test below: the cache holds `src/lib.rs` while the filter is absolute.
    /// This test previously claimed to cover the observed directory defect while using
    /// absolute paths on both sides, which nothing produces -- so it passed while the real
    /// directory case still returned zero. Keep the two shapes separate.
    #[test]
    fn test_diagnostics_directory_filter_finds_nested_files() {
        let root = "/abs/project";
        let all = vec![
            diag("/abs/project/src/lib.rs", DiagnosticSeverity::Error),
            diag(
                "/abs/project/src/nested/deep.rs",
                DiagnosticSeverity::Warning,
            ),
            diag("/abs/project/tests/it.rs", DiagnosticSeverity::Hint),
        ];

        let by_dir = LspDistiller::distill_diagnostics(&all, Some("/abs/project/src"), None);
        assert_eq!(
            by_dir.diagnostics.len(),
            2,
            "directory filter must match files beneath it, got {:?}",
            by_dir
                .diagnostics
                .iter()
                .map(|d| d.file.clone())
                .collect::<Vec<_>>()
        );

        // The project root matches everything.
        let by_root = LspDistiller::distill_diagnostics(&all, Some(root), None);
        assert_eq!(by_root.diagnostics.len(), 3);

        // A directory that contains nothing must still match nothing.
        let by_empty = LspDistiller::distill_diagnostics(&all, Some("/abs/project/vendor"), None);
        assert_eq!(by_empty.diagnostics.len(), 0);

        // Filtering by one file must not drag in its siblings.
        let by_file =
            LspDistiller::distill_diagnostics(&all, Some("/abs/project/src/lib.rs"), None);
        assert_eq!(by_file.diagnostics.len(), 1);
    }

    /// The shapes that actually occur, verified against a live rust-analyzer on a crate with
    /// two real compiler errors: the cache holds the server's RELATIVE path (`src/lib.rs`)
    /// while the engine resolves the filter ABSOLUTELY. A file filter works (the cache is the
    /// filter's tail); a directory filter cannot be resolved, because placing `src/lib.rs`
    /// under an absolute directory requires the session root, which is not threaded here.
    ///
    /// This test exists because an earlier version of it used absolute paths on both sides --
    /// a shape nothing produces -- and so passed while the real directory case still returned
    /// zero. Assert the observed reality, not a convenient fiction.
    #[test]
    fn test_diagnostics_relative_cache_matches_absolute_filter_by_tail() {
        let rel = vec![diag("src/lib.rs", DiagnosticSeverity::Error)];

        // Same file, two shapes: the documented and common case, and it works.
        let same_file =
            LspDistiller::distill_diagnostics(&rel, Some("/abs/project/src/lib.rs"), None);
        assert_eq!(same_file.diagnostics.len(), 1);

        // Ancestor at a different depth: NOT resolvable without the session root.
        let dir_of_rel = LspDistiller::distill_diagnostics(&rel, Some("/abs/project/src"), None);
        assert_eq!(
            dir_of_rel.diagnostics.len(),
            0,
            "a relative cache cannot be placed under an absolute directory without the root"
        );

        // The absolute-cache + absolute-directory-filter case does work, and is what the
        // segmentation fixes for callers whose server reports absolute paths.
        let abs = vec![diag("/abs/project/src/lib.rs", DiagnosticSeverity::Error)];
        let abs_dir = LspDistiller::distill_diagnostics(&abs, Some("/abs/project/src"), None);
        assert_eq!(abs_dir.diagnostics.len(), 1);
    }

    /// The filter normalises separators on both sides, so a Windows-style path matches the
    /// forward-slash form the cache stores.
    #[test]
    fn test_diagnostics_path_filter_normalises_separators() {
        let all = vec![
            diag("C:/proj/src/lib.rs", DiagnosticSeverity::Error),
            diag("C:/proj/src/other.rs", DiagnosticSeverity::Hint),
        ];
        let res = LspDistiller::distill_diagnostics(&all, Some("C:\\proj\\src\\lib.rs"), None);
        assert_eq!(res.diagnostics.len(), 1);
        assert_eq!(res.diagnostics[0].file, "C:/proj/src/lib.rs");
    }

    /// The cache records the file path derived from the server's `publishDiagnostics` URI.
    /// That value is both the cache key and the `file` field, and the caller filters it
    /// against an absolute engine-resolved path, so it MUST be absolute. Deriving it
    /// relative to the session root is what made every filtered call return nothing.
    #[test]
    fn test_diagnostics_cache_path_is_absolute_so_filter_can_match() {
        // Build a genuinely absolute path for whatever platform the test runs on. A literal
        // POSIX path is not absolute on Windows and would make this test vacuous.
        let root_dir = std::env::temp_dir().join("transcend-diag-root");
        let file = root_dir.join("src").join("lib.rs");
        let root = root_dir.to_string_lossy().replace('\\', "/");

        // Round-trip through the same URI conversion the LSP reader uses.
        let uri = format!(
            "file:///{}",
            file.to_string_lossy()
                .replace('\\', "/")
                .trim_start_matches('/')
        );
        let derived = crate::lsp::protocol::uri_to_path(&uri);
        let stored = derived.to_string_lossy().replace('\\', "/");

        assert!(
            !stored.is_empty(),
            "uri_to_path should recover a path from {uri:?}, got {stored:?}"
        );
        assert!(
            !stored.starts_with('/') || cfg!(unix),
            "unexpected leading separator on this platform: {stored:?}"
        );

        // The value the cache would store must be matchable by the absolute filter.
        let all = vec![diag(&stored, DiagnosticSeverity::Error)];
        let res = LspDistiller::distill_diagnostics(&all, Some(&stored), None);
        assert_eq!(
            res.diagnostics.len(),
            1,
            "absolute cache path {stored:?} must match an identical filter"
        );

        // The bug being guarded: a root-relative key can never contain an absolute filter.
        // Assert the invariant directly rather than relying on string shape.
        let relative_form = stored
            .split_once(&root)
            .map(|(_, tail)| tail.trim_start_matches('/').to_string());
        if let Some(rel) = relative_form.filter(|r| !r.is_empty()) {
            let abs_filter = format!("{root}/{rel}");
            assert!(
                !rel.contains(&abs_filter),
                "a relative key cannot contain an absolute path -- this is exactly why the \
                 cache must store absolute paths (rel={rel:?}, abs={abs_filter:?})"
            );
        }
    }

    /// Severity filtering must compose with the path filter rather than replace it.
    #[test]
    fn test_diagnostics_path_and_severity_filters_compose() {
        let all = vec![
            diag("/p/src/lib.rs", DiagnosticSeverity::Error),
            diag("/p/src/lib.rs", DiagnosticSeverity::Hint),
            diag("/p/src/main.rs", DiagnosticSeverity::Error),
        ];
        let res = LspDistiller::distill_diagnostics(
            &all,
            Some("/p/src/lib.rs"),
            Some(DiagnosticSeverity::Error),
        );
        assert_eq!(res.diagnostics.len(), 1);
        assert_eq!(res.diagnostics[0].file, "/p/src/lib.rs");
        assert_eq!(res.severity_breakdown.get("error"), Some(&1));
        assert_eq!(res.total_count, 1);
    }
}
