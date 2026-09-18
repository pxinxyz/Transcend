//! AST-Guarded Surgical Code Patcher
//!
//! Provides atomic, AST-validated code modifications targeting symbols, spans,
//! or literal text, preventing syntax corruption before touching disk.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use tree_sitter::{Node, Parser};
use transcend_protocol::{
    BatchPatchRequest, BatchPatchResponse, PatchMode, PatchRequest, PatchResponse,
    PatchSyntaxError, ReadSymbolRequest, SourceSpan,
};

use crate::outline::scanner::SupportedLang;
use crate::outline::symbol_reader::SymbolReader;
use crate::{CoreError, CoreResult};

static ATOMIC_PATCH_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct Patcher;

impl Patcher {
    pub fn patch(req: &PatchRequest) -> CoreResult<PatchResponse> {
        let (display_path, source) = if let Some(ref content) = req.content {
            (req.path.clone(), content.as_bytes().to_vec())
        } else {
            let p = Path::new(&req.path);
            if !p.exists() {
                return Ok(PatchResponse {
                    success: false,
                    file: req.path.clone(),
                    target_span: None,
                    ast_valid: false,
                    syntax_errors: vec![],
                    diff: None,
                    message: format!("File does not exist: {}", p.display()),
                });
            }
            let bytes = fs::read(p).map_err(|e| {
                CoreError::General(format!("Failed to read file {}: {e}", p.display()))
            })?;
            (req.path.clone(), bytes)
        };

        // 1. Resolve target span
        let target_span = if let Some(ref sym_name) = req.target_symbol {
            let read_req = ReadSymbolRequest {
                path: Some(req.path.clone()),
                content: req.content.clone(),
                symbol: sym_name.clone(),
                occurrence: req.target_occurrence,
                ..Default::default()
            };
            let sym_res = SymbolReader::read(&read_req)?;
            if !sym_res.found {
                return Ok(PatchResponse {
                    success: false,
                    file: display_path,
                    target_span: None,
                    ast_valid: false,
                    syntax_errors: vec![],
                    diff: None,
                    message: sym_res.message.unwrap_or_else(|| {
                        format!("Target symbol '{}' not found in file", sym_name)
                    }),
                });
            }
            sym_res.symbol.unwrap().span
        } else if let Some(ref span) = req.target_span {
            span.clone()
        } else if let Some(ref needle) = req.target_text {
            let needle_bytes = needle.as_bytes();
            let mut matches = Vec::new();
            let mut pos = 0;
            while let Some(idx) = source[pos..]
                .windows(needle_bytes.len())
                .position(|w| w == needle_bytes)
            {
                let match_pos = pos + idx;
                matches.push(match_pos);
                pos = match_pos + 1;
            }

            if matches.is_empty() {
                return Ok(PatchResponse {
                    success: false,
                    file: display_path,
                    target_span: None,
                    ast_valid: false,
                    syntax_errors: vec![],
                    diff: None,
                    message: format!("Target text needle '{}' not found in file", needle),
                });
            }

            if matches.len() > 1 && req.target_occurrence.is_none() {
                return Ok(PatchResponse {
                    success: false,
                    file: display_path,
                    target_span: None,
                    ast_valid: false,
                    syntax_errors: vec![],
                    diff: None,
                    message: format!(
                        "Found {} ambiguous occurrences of target text. Specify target_occurrence (0 to {}) to disambiguate.",
                        matches.len(),
                        matches.len() - 1
                    ),
                });
            }

            let occ = req.target_occurrence.unwrap_or(0);
            if occ >= matches.len() {
                return Ok(PatchResponse {
                    success: false,
                    file: display_path,
                    target_span: None,
                    ast_valid: false,
                    syntax_errors: vec![],
                    diff: None,
                    message: format!(
                        "Target occurrence {} out of range (found {} match{})",
                        occ,
                        matches.len(),
                        if matches.len() == 1 { "" } else { "es" }
                    ),
                });
            }

            let start_byte = matches[occ];
            let end_byte = start_byte + needle_bytes.len();
            Self::compute_source_span(&source, start_byte, end_byte)
        } else {
            return Ok(PatchResponse {
                success: false,
                file: display_path,
                target_span: None,
                ast_valid: false,
                syntax_errors: vec![],
                diff: None,
                message: "Must specify one of 'target_symbol', 'target_span', or 'target_text'".to_string(),
            });
        };

        let mode = req.mode.unwrap_or(PatchMode::Replace);

        let mut start_byte = target_span.start_byte;
        let end_byte = target_span.end_byte;

        if start_byte > end_byte || end_byte > source.len() {
            return Ok(PatchResponse {
                success: false,
                file: display_path,
                target_span: Some(target_span),
                ast_valid: false,
                syntax_errors: vec![],
                diff: None,
                message: format!(
                    "Invalid byte range ({}..{}) for file of size {} bytes",
                    start_byte,
                    end_byte,
                    source.len()
                ),
            });
        }

        // Calculate splice range and text according to PatchMode
        let (splice_start, splice_end, splice_text) = match mode {
            PatchMode::Replace => {
                // Auto-heal double-indentation:
                let prefix_line_start = source[..start_byte]
                    .iter()
                    .rposition(|&b| b == b'\n')
                    .map(|idx| idx + 1)
                    .unwrap_or(0);
                let line_prefix = &source[prefix_line_start..start_byte];
                if line_prefix.iter().all(|&b| b == b' ' || b == b'\t') {
                    if let Ok(indent_str) = std::str::from_utf8(line_prefix) {
                        if !indent_str.is_empty() && req.replacement.starts_with(indent_str) {
                            start_byte = prefix_line_start;
                        }
                    }
                }
                (start_byte, end_byte, req.replacement.clone())
            }
            PatchMode::InsertBefore => {
                let text = if !req.replacement.ends_with('\n') {
                    format!("{}\n", req.replacement)
                } else {
                    req.replacement.clone()
                };
                (start_byte, start_byte, text)
            }
            PatchMode::InsertAfter => {
                let text = if !req.replacement.starts_with('\n') {
                    format!("\n{}", req.replacement)
                } else {
                    req.replacement.clone()
                };
                (end_byte, end_byte, text)
            }
            PatchMode::PrependToSymbol => {
                let sym_slice = &source[start_byte..end_byte];
                let insert_pos = if let Some(open_idx) = sym_slice.iter().position(|&b| b == b'{') {
                    let next_pos = start_byte + open_idx + 1;
                    if next_pos < source.len() && source[next_pos] == b'\n' {
                        next_pos + 1
                    } else {
                        next_pos
                    }
                } else if let Some(colon_idx) = sym_slice.iter().position(|&b| b == b':') {
                    let next_pos = start_byte + colon_idx + 1;
                    if next_pos < source.len() && source[next_pos] == b'\n' {
                        next_pos + 1
                    } else {
                        next_pos
                    }
                } else {
                    start_byte
                };
                let text = if !req.replacement.ends_with('\n') {
                    format!("{}\n", req.replacement)
                } else {
                    req.replacement.clone()
                };
                (insert_pos, insert_pos, text)
            }
            PatchMode::AppendToSymbol => {
                let sym_slice = &source[start_byte..end_byte];
                let insert_pos = if let Some(close_idx) = sym_slice.iter().rposition(|&b| b == b'}') {
                    start_byte + close_idx
                } else {
                    end_byte
                };
                let text = if !req.replacement.ends_with('\n') {
                    format!("{}\n", req.replacement)
                } else {
                    req.replacement.clone()
                };
                (insert_pos, insert_pos, text)
            }
        };

        // 2. In-memory splice
        let mut new_source = Vec::with_capacity(
            source.len() - (splice_end - splice_start) + splice_text.len(),
        );
        new_source.extend_from_slice(&source[..splice_start]);
        new_source.extend_from_slice(splice_text.as_bytes());
        new_source.extend_from_slice(&source[splice_end..]);

        // 3. Generate unified diff
        let orig_str = String::from_utf8_lossy(&source);
        let new_str = String::from_utf8_lossy(&new_source);
        let diff = Self::generate_diff(
            &display_path,
            &orig_str,
            &new_str,
            splice_start,
            splice_end,
            &splice_text,
        );

        // 4. AST Preflight Verification
        let validate_ast = req.validate_ast.unwrap_or(true);
        let lang_opt = SupportedLang::from_path(Path::new(&req.path));

        if validate_ast {
            if let Some(lang) = lang_opt {
                let mut parser = Parser::new();
                let ts_lang = lang.language();
                if let Ok(()) = parser.set_language(&ts_lang) {
                    if let Some(tree) = parser.parse(&new_source, None) {
                        let root = tree.root_node();
                        if root.has_error() || root.is_error() {
                            let mut errors = Vec::new();
                            Self::collect_syntax_errors(&root, &new_source, &mut errors, 10);
                            if !errors.is_empty() {
                                return Ok(PatchResponse {
                                    success: false,
                                    file: display_path,
                                    target_span: Some(target_span),
                                    ast_valid: false,
                                    syntax_errors: errors,
                                    diff: Some(diff),
                                    message: "AST preflight verification failed: syntax errors detected in spliced code. Disk was not modified.".to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }

        // 5. Atomic Disk Write (if not dry_run and not in-memory)
        if req.dry_run == Some(true) {
            return Ok(PatchResponse {
                success: true,
                file: display_path,
                target_span: Some(target_span),
                ast_valid: true,
                syntax_errors: vec![],
                diff: Some(diff),
                message: "Dry run: patch successfully validated. No changes written to disk."
                    .to_string(),
            });
        }

        if req.content.is_some() {
            return Ok(PatchResponse {
                success: true,
                file: display_path,
                target_span: Some(target_span),
                ast_valid: true,
                syntax_errors: vec![],
                diff: Some(diff),
                message: "Patch successfully applied in-memory.".to_string(),
            });
        }

        let target_path = Path::new(&req.path);
        let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
        let file_stem = target_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("patch");
        let pid = std::process::id();
        let counter = ATOMIC_PATCH_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_path = parent.join(format!(".{}.transcend_tmp_{}_{}", file_stem, pid, counter));

        let write_res = (|| -> std::io::Result<()> {
            let mut tmp_file = fs::File::create(&tmp_path)?;
            tmp_file.write_all(&new_source)?;
            tmp_file.sync_all()?;
            drop(tmp_file);
            fs::rename(&tmp_path, target_path)?;
            Ok(())
        })();

        if let Err(e) = write_res {
            let _ = fs::remove_file(&tmp_path);
            return Err(CoreError::General(format!(
                "Failed to atomically write patched file {}: {e}",
                target_path.display()
            )));
        }

        Ok(PatchResponse {
            success: true,
            file: display_path,
            target_span: Some(target_span),
            ast_valid: true,
            syntax_errors: vec![],
            diff: Some(diff),
            message: "Patch applied successfully.".to_string(),
        })
    }

    /// Transactionally apply multiple patches across files with AST preflight and rollback guarantees.
    pub fn batch_patch(req: &BatchPatchRequest) -> CoreResult<BatchPatchResponse> {
        if req.patches.is_empty() {
            return Ok(BatchPatchResponse {
                success: true,
                results: vec![],
                total_files_patched: 0,
                all_ast_valid: true,
                syntax_errors: vec![],
                diff: None,
                message: "No patches provided in batch.".to_string(),
            });
        }

        let validate_ast = req.validate_ast.unwrap_or(true);
        let dry_run = req.dry_run.unwrap_or(false);

        // Phase 1: Dry run / simulation across all patches
        let mut simulated_results = Vec::with_capacity(req.patches.len());
        let mut accumulated_syntax_errors = Vec::new();
        let mut combined_diff = String::new();
        let mut any_failed = false;

        for patch_req in &req.patches {
            let mut dry_req = patch_req.clone();
            dry_req.dry_run = Some(true);
            dry_req.validate_ast = Some(validate_ast);

            let res = Self::patch(&dry_req)?;
            if !res.success || !res.ast_valid {
                any_failed = true;
                accumulated_syntax_errors.extend(res.syntax_errors.clone());
            }
            if let Some(ref d) = res.diff {
                if !d.is_empty() {
                    combined_diff.push_str(d);
                    combined_diff.push('\n');
                }
            }
            simulated_results.push(res);
        }

        let distinct_files: std::collections::HashSet<_> = req.patches.iter().map(|p| &p.path).collect();

        if any_failed {
            return Ok(BatchPatchResponse {
                success: false,
                results: simulated_results,
                total_files_patched: 0,
                all_ast_valid: false,
                syntax_errors: accumulated_syntax_errors,
                diff: if combined_diff.is_empty() { None } else { Some(combined_diff) },
                message: "Batch patch aborted: one or more patches failed AST preflight validation or target resolution. No files modified on disk.".to_string(),
            });
        }

        if dry_run {
            return Ok(BatchPatchResponse {
                success: true,
                results: simulated_results,
                total_files_patched: distinct_files.len(),
                all_ast_valid: true,
                syntax_errors: vec![],
                diff: if combined_diff.is_empty() { None } else { Some(combined_diff) },
                message: "Dry run: all patches in batch successfully validated. No changes written to disk.".to_string(),
            });
        }

        // Phase 2: Atomic Execution with Multi-File Rollback Safety
        // 1. Back up original file contents in memory
        let mut original_contents: std::collections::HashMap<std::path::PathBuf, Vec<u8>> = std::collections::HashMap::new();
        for file_path_str in &distinct_files {
            let p = Path::new(file_path_str);
            if p.exists() {
                if let Ok(bytes) = fs::read(p) {
                    original_contents.insert(p.to_path_buf(), bytes);
                }
            }
        }

        // 2. Apply patches on disk
        let mut final_results = Vec::with_capacity(req.patches.len());
        let mut applied_files: Vec<std::path::PathBuf> = Vec::new();
        let mut apply_error = None;

        for patch_req in &req.patches {
            let mut live_req = patch_req.clone();
            live_req.dry_run = Some(false);
            live_req.validate_ast = Some(validate_ast);

            match Self::patch(&live_req) {
                Ok(res) => {
                    if !res.success {
                        apply_error = Some(res.message.clone());
                        final_results.push(res);
                        break;
                    }
                    applied_files.push(Path::new(&patch_req.path).to_path_buf());
                    final_results.push(res);
                }
                Err(e) => {
                    apply_error = Some(e.to_string());
                    break;
                }
            }
        }

        if let Some(err_msg) = apply_error {
            // Rollback all previously modified files!
            for p in &applied_files {
                if let Some(orig_bytes) = original_contents.get(p) {
                    let _ = fs::write(p, orig_bytes);
                }
            }
            return Ok(BatchPatchResponse {
                success: false,
                results: final_results,
                total_files_patched: 0,
                all_ast_valid: false,
                syntax_errors: vec![],
                diff: None,
                message: format!("Batch patch failed during execution and was rolled back: {err_msg}"),
            });
        }

        Ok(BatchPatchResponse {
            success: true,
            results: final_results,
            total_files_patched: distinct_files.len(),
            all_ast_valid: true,
            syntax_errors: vec![],
            diff: if combined_diff.is_empty() { None } else { Some(combined_diff) },
            message: format!("Successfully applied batch patch across {} files.", distinct_files.len()),
        })
    }

    fn collect_syntax_errors(
        node: &Node,
        source: &[u8],
        errors: &mut Vec<PatchSyntaxError>,
        max_errors: usize,
    ) {
        if errors.len() >= max_errors {
            return;
        }

        if node.is_error() {
            let start = node.start_position();
            let end_byte = node.end_byte().min(source.len());
            let start_byte = node.start_byte().min(end_byte);
            let snippet = String::from_utf8_lossy(&source[start_byte..end_byte]).to_string();
            let clean = snippet.trim();
            let token_desc = if clean.is_empty() {
                None
            } else {
                Some(clean.chars().take(40).collect())
            };

            errors.push(PatchSyntaxError {
                line: start.row + 1,
                column: start.column + 1,
                message: format!(
                    "Syntax error near '{}'",
                    token_desc.as_deref().unwrap_or("unexpected token")
                ),
                unexpected_token: token_desc,
            });
            return;
        }

        if node.is_missing() {
            let start = node.start_position();
            errors.push(PatchSyntaxError {
                line: start.row + 1,
                column: start.column + 1,
                message: format!("Missing syntax token: '{}'", node.kind()),
                unexpected_token: Some(node.kind().to_string()),
            });
            return;
        }

        if node.has_error() {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                Self::collect_syntax_errors(&child, source, errors, max_errors);
                if errors.len() >= max_errors {
                    break;
                }
            }
        }
    }

    fn compute_source_span(source: &[u8], start_byte: usize, end_byte: usize) -> SourceSpan {
        let prefix = String::from_utf8_lossy(&source[..start_byte.min(source.len())]);
        let start_line = prefix.lines().count().max(1);
        let last_line_start = prefix.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let start_col = prefix[last_line_start..].chars().count() + 1;

        let total_prefix = String::from_utf8_lossy(&source[..end_byte.min(source.len())]);
        let end_line = total_prefix.lines().count().max(1);
        let last_line_end = total_prefix.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let end_col = total_prefix[last_line_end..].chars().count() + 1;

        SourceSpan {
            start_line,
            start_col,
            end_line,
            end_col,
            start_byte,
            end_byte,
        }
    }

    fn byte_to_line_index(s: &str, byte_offset: usize) -> usize {
        let clamped = byte_offset.min(s.len());
        s[..clamped].lines().count().saturating_sub(1)
    }

    fn generate_diff(
        path: &str,
        orig_str: &str,
        new_str: &str,
        start_byte: usize,
        end_byte: usize,
        replacement: &str,
    ) -> String {
        let orig_lines: Vec<&str> = orig_str.lines().collect();
        let new_lines: Vec<&str> = new_str.lines().collect();

        if orig_lines.is_empty() && new_lines.is_empty() {
            return String::new();
        }

        let end_idx = if end_byte > start_byte {
            end_byte - 1
        } else {
            start_byte
        };
        let l_orig_start = Self::byte_to_line_index(orig_str, start_byte);
        let l_orig_end = Self::byte_to_line_index(orig_str, end_idx);

        let new_end_byte = start_byte + replacement.len();
        let new_end_idx = if new_end_byte > start_byte {
            new_end_byte - 1
        } else {
            start_byte
        };
        let l_new_start = Self::byte_to_line_index(new_str, start_byte);
        let l_new_end = Self::byte_to_line_index(new_str, new_end_idx);

        let ctx_before_start = l_orig_start.saturating_sub(3).min(orig_lines.len());
        let ctx_before_end = l_orig_start.min(orig_lines.len());

        let ctx_after_start = (l_orig_end + 1).min(orig_lines.len());
        let ctx_after_end = (l_orig_end + 4).min(orig_lines.len());

        let orig_diff_lines = (l_orig_end + 1).saturating_sub(l_orig_start);
        let new_diff_lines = (l_new_end + 1).saturating_sub(l_new_start);

        let orig_count = (ctx_before_end - ctx_before_start)
            + orig_diff_lines
            + (ctx_after_end - ctx_after_start);
        let new_count = (ctx_before_end - ctx_before_start)
            + new_diff_lines
            + (ctx_after_end - ctx_after_start);

        let mut diff = String::new();
        diff.push_str(&format!("--- a/{}\n", path));
        diff.push_str(&format!("+++ b/{}\n", path));
        diff.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            ctx_before_start + 1,
            orig_count,
            ctx_before_start + 1,
            new_count
        ));

        // Context before
        for i in ctx_before_start..ctx_before_end {
            if i < orig_lines.len() {
                diff.push_str(&format!(" {}\n", orig_lines[i]));
            }
        }

        // Old lines (-)
        for i in l_orig_start..=l_orig_end {
            if i < orig_lines.len() {
                diff.push_str(&format!("-{}\n", orig_lines[i]));
            }
        }

        // New lines (+)
        for i in l_new_start..=l_new_end {
            if i < new_lines.len() {
                diff.push_str(&format!("+{}\n", new_lines[i]));
            }
        }

        // Context after
        for i in ctx_after_start..ctx_after_end {
            if i < orig_lines.len() {
                diff.push_str(&format!(" {}\n", orig_lines[i]));
            }
        }

        diff
    }
}
