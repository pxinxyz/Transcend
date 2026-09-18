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

pub struct FileOps;

impl FileOps {
    /// Read file content with line/byte boundaries and binary safety checks.
    pub fn read_file(req: &ReadFileRequest) -> CoreResult<ReadFileResponse> {
        let path = Path::new(&req.path);
        if !path.exists() {
            return Ok(ReadFileResponse {
                file: req.path.clone(),
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
            CoreError::General(format!("Failed to read file metadata for {}: {e}", path.display()))
        })?;

        if metadata.is_dir() {
            return Ok(ReadFileResponse {
                file: req.path.clone(),
                content: String::new(),
                start_line: 0,
                end_line: 0,
                total_lines: 0,
                size_bytes: metadata.len(),
                truncated: false,
                is_binary: false,
                message: Some(format!("Target is a directory, not a file: {}", path.display())),
            });
        }

        let size_bytes = metadata.len();

        // 1. Binary Detection Check (inspect first 1024 bytes)
        let mut file = fs::File::open(path).map_err(|e| {
            CoreError::General(format!("Failed to open file {}: {e}", path.display()))
        })?;

        let mut probe_buffer = [0u8; 1024];
        let bytes_read = file.read(&mut probe_buffer).unwrap_or(0);
        if probe_buffer[..bytes_read].contains(&0x00) {
            return Ok(ReadFileResponse {
                file: req.path.clone(),
                content: format!("[Binary file omitted: {} bytes]", size_bytes),
                start_line: 0,
                end_line: 0,
                total_lines: 0,
                size_bytes,
                truncated: false,
                is_binary: true,
                message: Some("Binary file detected containing NUL bytes.".to_string()),
            });
        }

        // 2. Stream file content line-by-line via BufReader with O(1) memory overhead
        file.seek(SeekFrom::Start(0)).map_err(|e| {
            CoreError::General(format!("Failed to rewind file {}: {e}", path.display()))
        })?;

        let mut reader = BufReader::new(file);
        let mut line_buf = Vec::new();
        let start_line = req.start_line.unwrap_or(1).max(1);
        let end_line_req = req.end_line;
        let max_bytes = req.max_bytes.unwrap_or(DEFAULT_MAX_READ_BYTES);
        let line_numbers = req.line_numbers.unwrap_or(false);

        let mut content = String::new();
        let mut truncated = false;
        let mut total_lines = 0usize;
        let mut actual_end_line = start_line;
        let mut collected_any = false;

        while reader.read_until(b'\n', &mut line_buf).map_err(|e| {
            CoreError::General(format!("Failed to read line from {}: {e}", path.display()))
        })? > 0 {
            total_lines += 1;
            let line_no = total_lines;

            let within_window = line_no >= start_line && match end_line_req {
                Some(end) => line_no <= end,
                None => true,
            };

            if within_window && !truncated {
                let s = String::from_utf8_lossy(&line_buf);
                let line_text = s.trim_end_matches(['\r', '\n']);

                let formatted_line = if line_numbers {
                    format!("{:>5} | {}\n", line_no, line_text)
                } else {
                    format!("{}\n", line_text)
                };

                if content.len() + formatted_line.len() > max_bytes && !content.is_empty() {
                    truncated = true;
                } else {
                    content.push_str(&formatted_line);
                    actual_end_line = line_no;
                    collected_any = true;
                }
            }

            line_buf.clear();
        }

        if total_lines == 0 {
            return Ok(ReadFileResponse {
                file: req.path.clone(),
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
                file: req.path.clone(),
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

        let target_end_line = end_line_req.unwrap_or(total_lines).min(total_lines).max(start_line);
        if actual_end_line < target_end_line {
            truncated = true;
        }

        if !collected_any {
            actual_end_line = start_line.min(total_lines);
        }

        Ok(ReadFileResponse {
            file: req.path.clone(),
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
        let exists = target_path.exists();

        if exists && req.overwrite != Some(true) {
            return Ok(WriteFileResponse {
                file: req.path.clone(),
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
        let tmp_path = parent.join(format!(".{}.transcend_write_tmp_{}_{}", file_stem, pid, counter));

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
            file: req.path.clone(),
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
        if !target_path.exists() {
            return Ok(DeletePathResponse {
                path: req.path.clone(),
                success: false,
                is_directory: false,
                deleted_count: 0,
                message: format!("Path does not exist: {}", target_path.display()),
            });
        }

        // Security boundary check: ensure canonical path does not escape workspace_root
        let root_str = req.workspace_root.as_deref().unwrap_or(".");
        let root_path = Path::new(root_str);

        if let (Ok(canonical_target), Ok(canonical_root)) = (
            target_path.canonicalize(),
            root_path.canonicalize(),
        ) {
            if !canonical_target.starts_with(&canonical_root) {
                return Err(CoreError::General(format!(
                    "Access denied: path '{}' escapes workspace boundary '{}'",
                    target_path.display(),
                    canonical_root.display()
                )));
            }
        }

        let is_dir = target_path.is_dir();
        if is_dir {
            if req.recursive != Some(true) {
                // Check if directory is empty
                let mut read_dir = fs::read_dir(target_path).map_err(|e| {
                    CoreError::General(format!("Failed to read directory {}: {e}", target_path.display()))
                })?;
                if read_dir.next().is_some() {
                    return Ok(DeletePathResponse {
                        path: req.path.clone(),
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
                    CoreError::General(format!("Failed to remove directory {}: {e}", target_path.display()))
                })?;
                return Ok(DeletePathResponse {
                    path: req.path.clone(),
                    success: true,
                    is_directory: true,
                    deleted_count: 1,
                    message: "Directory successfully removed.".to_string(),
                });
            }

            // Recursive deletion
            let count = Self::count_entries_recursive(target_path).unwrap_or(1);
            fs::remove_dir_all(target_path).map_err(|e| {
                CoreError::General(format!("Failed to remove directory tree {}: {e}", target_path.display()))
            })?;

            Ok(DeletePathResponse {
                path: req.path.clone(),
                success: true,
                is_directory: true,
                deleted_count: count,
                message: format!("Directory tree successfully deleted ({} items).", count),
            })
        } else {
            fs::remove_file(target_path).map_err(|e| {
                CoreError::General(format!("Failed to remove file {}: {e}", target_path.display()))
            })?;

            Ok(DeletePathResponse {
                path: req.path.clone(),
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
