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
            if let Some(pf) = path_filter {
                let clean_pf = pf.replace('\\', "/");
                let clean_diag_file = diag.file.replace('\\', "/");
                if !clean_diag_file.contains(&clean_pf) {
                    continue;
                }
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
}
