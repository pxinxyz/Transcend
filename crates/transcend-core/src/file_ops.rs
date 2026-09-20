//! Native File Lifecycle & Inspection Operations
//!
//! Provides bounded line/byte file reading with binary safety, atomic disk writing
//! with sibling temp files and collision guards, and path-contained workspace deletion.

use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use transcend_protocol::{
    DeletePathRequest, DeletePathResponse, ReadFileRequest, ReadFileResponse, WriteFileRequest,
    WriteFileResponse,
};

use crate::{CoreError, CoreResult};

static ATOMIC_WRITE_COUNTER: AtomicU64 = AtomicU64::new(0);
const DEFAULT_MAX_READ_BYTES: usize = 65_536;

/// Resolve the workspace boundary used by the file operations.
///
/// Prefers an explicit request root, then `TRANSCEND_WORKSPACE` / `WORKSPACE_ROOT`,
/// then the nearest ancestor containing a root anchor.
fn resolve_boundary_root(explicit: Option<&str>) -> std::path::PathBuf {
    if let Some(r) = explicit
        && !r.trim().is_empty()
    {
        return std::path::PathBuf::from(r.trim());
    }
    if let Ok(env_root) =
        std::env::var("TRANSCEND_WORKSPACE").or_else(|_| std::env::var("WORKSPACE_ROOT"))
    {
        return std::path::PathBuf::from(env_root);
    }
    if let Ok(cwd) = std::env::current_dir() {
        let mut curr = Some(cwd.as_path());
        while let Some(dir) = curr {
            if dir.join("Cargo.toml").exists()
                || dir.join(".git").exists()
                || dir.join("package.json").exists()
            {
                return dir.to_path_buf();
            }
            curr = dir.parent();
        }
        return cwd;
    }
    std::path::PathBuf::from(".")
}

/// Reject any target that resolves outside `boundary` when a boundary is known.
///
/// Shared by all mutating operations so the guard cannot be forgotten by one of them.
///
/// A `None` boundary means "no boundary was supplied" and allows the operation. That is
/// safe only because `NativeEngine` wraps every mutating dispatch and fills
/// `workspace_root` with its resolved boundary before calling into this module (see
/// `NativeEngine::write_file` / `delete_path` / `patch`). Calling these functions directly
/// with `workspace_root: None` performs NO boundary check, so they are not safe to use as a
/// public entry point on their own.
pub fn ensure_within(boundary: Option<&str>, target: &Path) -> CoreResult<()> {
    let Some(boundary) = boundary.filter(|b| !b.trim().is_empty()) else {
        return Ok(());
    };
    let root = resolve_boundary_root(Some(boundary));
    let Ok(canonical_root) = root.canonicalize() else {
        return Err(CoreError::General(format!(
            "Cannot resolve workspace boundary '{}'",
            root.display()
        )));
    };
    let canonical_root = crate::clean_path(&canonical_root);

    // Canonicalize the target, or its nearest existing ancestor for not-yet-created files.
    let mut probe = target.to_path_buf();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let resolved = loop {
        if let Ok(c) = probe.canonicalize() {
            let mut resolved = crate::clean_path(&c);
            for part in tail.iter().rev() {
                resolved.push(part);
            }
            break resolved;
        }
        let Some(name) = probe.file_name().map(|n| n.to_os_string()) else {
            return Err(CoreError::General(format!(
                "Cannot resolve path '{}' for boundary check",
                target.display()
            )));
        };
        tail.push(name);
        match probe.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => probe = parent.to_path_buf(),
            _ => {
                return Err(CoreError::General(format!(
                    "Cannot resolve path '{}' for boundary check",
                    target.display()
                )));
            }
        }
    };

    if resolved.starts_with(&canonical_root) {
        Ok(())
    } else {
        Err(CoreError::General(format!(
            "Access denied: path '{}' escapes workspace boundary '{}'",
            target.display(),
            canonical_root.display()
        )))
    }
}

pub struct FileOps;

impl FileOps {
    /// Read file content with line/byte boundaries and binary safety checks.
    pub fn read_file(req: &ReadFileRequest) -> CoreResult<ReadFileResponse> {
        let path = Path::new(&req.path);
        let display_path = crate::clean_path(path).to_string_lossy().to_string();
        if !path.exists() {
            return Ok(ReadFileResponse {
                file: display_path,
                content: String::new(),
                start_line: 0,
                end_line: 0,
                total_lines: 0,
                size_bytes: 0,
                truncated: false,
                is_binary: false,
                message: Some(format!("File does not exist: {}", path.display())),
            });
        }

        let metadata = fs::metadata(path).map_err(|e| {
            CoreError::General(format!(
                "Failed to read file metadata for {}: {e}",
                path.display()
            ))
        })?;

        if metadata.is_dir() {
            return Ok(ReadFileResponse {
                file: display_path,
                content: String::new(),
                start_line: 0,
                end_line: 0,
                total_lines: 0,
                size_bytes: metadata.len(),
                truncated: false,
                is_binary: false,
                message: Some(format!(
                    "Target is a directory, not a file: {}",
                    path.display()
                )),
            });
        }

        let size_bytes = metadata.len();

        // 1. Binary Detection Check (inspect first 1024 bytes)
        let mut file = fs::File::open(path).map_err(|e| {
            CoreError::General(format!("Failed to open file {}: {e}", path.display()))
        })?;

        let mut probe = [0u8; 1024];
        let bytes_read = file.read(&mut probe).map_err(|e| {
            CoreError::General(format!(
                "Failed to probe file header for {}: {e}",
                path.display()
            ))
        })?;

        if probe[..bytes_read].contains(&0x00) {
            return Ok(ReadFileResponse {
                file: display_path,
                content: format!("[Binary file omitted ({} bytes)]", size_bytes),
                start_line: 0,
                end_line: 0,
                total_lines: 0,
                size_bytes,
                truncated: false,
                is_binary: true,
                message: Some(format!(
                    "Binary file detected (size: {} bytes). Content omitted for token safety.",
                    size_bytes
                )),
            });
        }

        file.seek(SeekFrom::Start(0)).map_err(|e| {
            CoreError::General(format!("Failed to rewind file {}: {e}", path.display()))
        })?;

        let mut reader = BufReader::new(file);
        let mut line_buf = Vec::new();
        let start_line = req.start_line.unwrap_or(1).max(1);
        let end_line_req = req.end_line;

        // An inverted range produced a self-contradictory response: `start_line: 8,
        // end_line: 8` with empty content for a 10-line file, and no explanation. The
        // reported range claimed to cover a line it had not returned, which is worse than an
        // empty result because a caller trusting the range would read the wrong lines.
        if let Some(end) = end_line_req
            && end < start_line
        {
            return Err(CoreError::InvalidInput(format!(
                "end_line ({end}) is before start_line ({start_line}); the range is empty. \
                 Swap the bounds or omit end_line to read to the end of the file."
            )));
        }
        let max_bytes = req.max_bytes.unwrap_or(DEFAULT_MAX_READ_BYTES);
        let line_numbers = req.line_numbers.unwrap_or(false);

        let mut content = String::new();
        let mut truncated = false;
        let mut total_lines = 0usize;
        let mut actual_end_line = start_line;
        let mut collected_any = false;

        while reader.read_until(b'\n', &mut line_buf).map_err(|e| {
            CoreError::General(format!("Failed to read line from {}: {e}", path.display()))
        })? > 0
        {
            total_lines += 1;
            let line_no = total_lines;

            if line_no >= start_line && end_line_req.is_none_or(|el| line_no <= el) && !truncated {
                let decoded_line = String::from_utf8_lossy(&line_buf);
                let formatted_line = if line_numbers {
                    format!("{:5} | {}", line_no, decoded_line)
                } else {
                    decoded_line.into_owned()
                };

                if content.len() + formatted_line.len() > max_bytes {
                    let remaining_budget = max_bytes.saturating_sub(content.len());
                    if remaining_budget > 0 {
                        // `max_bytes` is a BYTE budget. Taking `remaining_budget` characters
                        // instead let a multi-byte file return up to 4x the documented limit,
                        // which defeats the point of a token-bounded read. Cut on a real char
                        // boundary so the slice cannot panic.
                        let cut = crate::floor_char_boundary(&formatted_line, remaining_budget);
                        content.push_str(&formatted_line[..cut]);
                    }
                    truncated = true;
                } else {
                    content.push_str(&formatted_line);
                    actual_end_line = line_no;
                    collected_any = true;
                }
            }

            line_buf.clear();

            // Fast-path: once target end_line has been collected, switch to fast chunked newline counting
            if let Some(el) = end_line_req
                && line_no >= el
            {
                let mut chunk = [0u8; 65536];
                let mut has_bytes_after = false;
                let mut last_byte = 0u8;
                loop {
                    let n = reader.read(&mut chunk).map_err(|e| {
                        CoreError::General(format!(
                            "Failed to count remaining lines in {}: {e}",
                            path.display()
                        ))
                    })?;
                    if n == 0 {
                        break;
                    }
                    has_bytes_after = true;
                    last_byte = chunk[n - 1];
                    total_lines += chunk[..n].iter().filter(|&&b| b == b'\n').count();
                }
                if has_bytes_after && last_byte != b'\n' {
                    total_lines += 1;
                }
                break;
            }
        }

        if total_lines == 0 {
            return Ok(ReadFileResponse {
                file: display_path,
                content: String::new(),
                start_line: 1,
                end_line: 0,
                total_lines: 0,
                size_bytes,
                truncated: false,
                is_binary: false,
                message: None,
            });
        }

        if start_line > total_lines {
            return Ok(ReadFileResponse {
                file: display_path,
                content: String::new(),
                start_line,
                end_line: start_line,
                total_lines,
                size_bytes,
                truncated: false,
                is_binary: false,
                message: Some(format!(
                    "Requested start_line ({}) exceeds total lines ({})",
                    start_line, total_lines
                )),
            });
        }

        let target_end_line = end_line_req
            .unwrap_or(total_lines)
            .min(total_lines)
            .max(start_line);
        if actual_end_line < target_end_line {
            truncated = true;
        }

        if !collected_any {
            actual_end_line = start_line.min(total_lines);
        }

        Ok(ReadFileResponse {
            file: display_path,
            content,
            start_line,
            end_line: actual_end_line,
            total_lines,
            size_bytes,
            truncated,
            is_binary: false,
            message: None,
        })
    }

    /// Write text content to file atomically, with collision and parent directory guards.
    pub fn write_file(req: &WriteFileRequest) -> CoreResult<WriteFileResponse> {
        let target_path = Path::new(&req.path);
        let display_path = crate::clean_path(target_path).to_string_lossy().to_string();
        ensure_within(req.workspace_root.as_deref(), target_path)?;
        let exists = target_path.exists();

        if exists && req.overwrite != Some(true) {
            return Ok(WriteFileResponse {
                file: display_path,
                success: false,
                bytes_written: 0,
                created_new: false,
                message: format!(
                    "File already exists: {}. Set 'overwrite: true' to replace.",
                    target_path.display()
                ),
            });
        }

        // Parent directory creation
        let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
        if req.create_parents.unwrap_or(true) && !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                CoreError::General(format!(
                    "Failed to create parent directories for {}: {e}",
                    target_path.display()
                ))
            })?;
        }

        // Atomic write via sibling temporary file
        let file_stem = target_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let pid = std::process::id();
        let counter = ATOMIC_WRITE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_path = parent.join(format!(
            ".{}.transcend_write_tmp_{}_{}",
            file_stem, pid, counter
        ));

        let write_result = (|| -> std::io::Result<()> {
            let mut tmp_file = fs::File::create(&tmp_path)?;
            tmp_file.write_all(req.content.as_bytes())?;
            tmp_file.sync_all()?;
            drop(tmp_file);
            fs::rename(&tmp_path, target_path)?;
            Ok(())
        })();

        if let Err(e) = write_result {
            let _ = fs::remove_file(&tmp_path);
            return Err(CoreError::General(format!(
                "Failed to atomically write file {}: {e}",
                target_path.display()
            )));
        }

        Ok(WriteFileResponse {
            file: display_path,
            success: true,
            bytes_written: req.content.len(),
            created_new: !exists,
            message: if exists {
                "File successfully updated.".to_string()
            } else {
                "File successfully created.".to_string()
            },
        })
    }

    /// Delete file or directory within workspace safety boundaries.
    pub fn delete_path(req: &DeletePathRequest) -> CoreResult<DeletePathResponse> {
        let target_path = Path::new(&req.path);
        let display_path = crate::clean_path(target_path).to_string_lossy().to_string();
        if !target_path.exists() {
            return Ok(DeletePathResponse {
                path: display_path,
                success: false,
                is_directory: false,
                deleted_count: 0,
                message: format!("Path does not exist: {}", target_path.display()),
            });
        }

        // Security boundary check: ensure canonical path does not escape workspace_root
        ensure_within(req.workspace_root.as_deref(), target_path)?;

        let is_dir = target_path.is_dir();
        if is_dir {
            if req.recursive != Some(true) {
                // Check if directory is empty
                let mut read_dir = fs::read_dir(target_path).map_err(|e| {
                    CoreError::General(format!(
                        "Failed to read directory {}: {e}",
                        target_path.display()
                    ))
                })?;
                if read_dir.next().is_some() {
                    return Ok(DeletePathResponse {
                        path: display_path,
                        success: false,
                        is_directory: true,
                        deleted_count: 0,
                        message: format!(
                            "Directory is not empty: {}. Set 'recursive: true' to delete directory and all contents.",
                            target_path.display()
                        ),
                    });
                }
                fs::remove_dir(target_path).map_err(|e| {
                    CoreError::General(format!(
                        "Failed to remove directory {}: {e}",
                        target_path.display()
                    ))
                })?;
                return Ok(DeletePathResponse {
                    path: display_path,
                    success: true,
                    is_directory: true,
                    deleted_count: 1,
                    message: "Directory successfully removed.".to_string(),
                });
            }

            // Recursive deletion
            let count = Self::count_entries_recursive(target_path).unwrap_or(1);
            fs::remove_dir_all(target_path).map_err(|e| {
                CoreError::General(format!(
                    "Failed to remove directory tree {}: {e}",
                    target_path.display()
                ))
            })?;

            Ok(DeletePathResponse {
                path: display_path,
                success: true,
                is_directory: true,
                deleted_count: count,
                message: format!("Directory tree successfully deleted ({} items).", count),
            })
        } else {
            fs::remove_file(target_path).map_err(|e| {
                CoreError::General(format!(
                    "Failed to remove file {}: {e}",
                    target_path.display()
                ))
            })?;

            Ok(DeletePathResponse {
                path: display_path,
                success: true,
                is_directory: false,
                deleted_count: 1,
                message: "File successfully deleted.".to_string(),
            })
        }
    }

    fn count_entries_recursive(dir: &Path) -> std::io::Result<usize> {
        let mut count = 1; // Count dir itself
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    count += Self::count_entries_recursive(&path)?;
                } else {
                    count += 1;
                }
            }
        }
        Ok(count)
    }
}
