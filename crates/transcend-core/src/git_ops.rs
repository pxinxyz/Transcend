//! In-Process Native Git Status Inspector
//!
//! Parses `git status --porcelain=v2 --branch` into strongly typed, token-compact JSON data contracts.

use std::path::Path;
use std::process::Command;

use transcend_protocol::{
    GitFileEntry, GitFileStatus, GitStatusResponse,
};

use crate::{CoreError, CoreResult};

pub struct GitEngine;

impl GitEngine {
    /// Inspect git status for a repository at the specified directory.
    pub fn status(target_dir: &Path) -> CoreResult<GitStatusResponse> {
        let output = match Command::new("git")
            .arg("status")
            .arg("--porcelain=v2")
            .arg("--branch")
            .current_dir(target_dir)
            .output()
        {
            Ok(out) => out,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(GitStatusResponse {
                    is_git_repo: false,
                    branch: String::new(),
                    upstream: None,
                    ahead: 0,
                    behind: 0,
                    staged: Vec::new(),
                    unstaged: Vec::new(),
                    untracked: Vec::new(),
                    conflicted: Vec::new(),
                    is_clean: true,
                });
            }
            Err(e) => {
                return Err(CoreError::General(format!(
                    "Failed to execute git command in {}: {e}",
                    target_dir.display()
                )));
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not a git repository") {
                return Ok(GitStatusResponse {
                    is_git_repo: false,
                    branch: String::new(),
                    upstream: None,
                    ahead: 0,
                    behind: 0,
                    staged: Vec::new(),
                    unstaged: Vec::new(),
                    untracked: Vec::new(),
                    conflicted: Vec::new(),
                    is_clean: true,
                });
            }
            return Err(CoreError::General(format!(
                "git status failed with exit code {}: {}",
                output.status.code().unwrap_or(-1),
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_porcelain_v2(&stdout)
    }

    /// Parse raw `git status --porcelain=v2 --branch` output.
    pub fn parse_porcelain_v2(output: &str) -> CoreResult<GitStatusResponse> {
        let mut branch = String::new();
        let mut upstream = None;
        let mut ahead = 0;
        let mut behind = 0;
        let mut staged = Vec::new();
        let mut unstaged = Vec::new();
        let mut untracked = Vec::new();
        let mut conflicted = Vec::new();

        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("# branch.") {
                if let Some(b) = rest.strip_prefix("head ") {
                    branch = b.to_string();
                } else if let Some(u) = rest.strip_prefix("upstream ") {
                    upstream = Some(u.to_string());
                } else if let Some(ab) = rest.strip_prefix("ab ") {
                    // format: +<ahead> -<behind>
                    for part in ab.split_whitespace() {
                        if let Some(a) = part.strip_prefix('+') {
                            ahead = a.parse().unwrap_or(0);
                        } else if let Some(b) = part.strip_prefix('-') {
                            behind = b.parse().unwrap_or(0);
                        }
                    }
                }
                continue;
            }

            let mut parts = trimmed.split_whitespace();
            let entry_type = match parts.next() {
                Some(t) => t,
                None => continue,
            };

            match entry_type {
                "1" => {
                    // Ordinary change:
                    // 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
                    let xy = parts.next().unwrap_or("..");
                    // Path is after the first 8 whitespace-separated tokens
                    let path = trimmed.split_whitespace().skip(8).collect::<Vec<_>>().join(" ");
                    if path.is_empty() {
                        continue;
                    }

                    let x = xy.chars().next().unwrap_or('.');
                    let y = xy.chars().nth(1).unwrap_or('.');

                    if let Some(status) = Self::char_to_status(x) {
                        staged.push(GitFileEntry {
                            path: path.clone(),
                            status,
                            original_path: None,
                        });
                    }

                    if let Some(status) = Self::char_to_status(y) {
                        unstaged.push(GitFileEntry {
                            path,
                            status,
                            original_path: None,
                        });
                    }
                }
                "2" => {
                    // Renamed or copied change:
                    // 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path><sep><origPath>
                    let xy = parts.next().unwrap_or("..");
                    let x = xy.chars().next().unwrap_or('.');
                    let y = xy.chars().nth(1).unwrap_or('.');

                    let path_part = line.split('\t').collect::<Vec<_>>();
                    let (path, orig_path) = if path_part.len() >= 2 {
                        let p = path_part[0].split_whitespace().last().unwrap_or("").to_string();
                        let orig = path_part[1].to_string();
                        (p, Some(orig))
                    } else {
                        (trimmed.split_whitespace().last().unwrap_or("").to_string(), None)
                    };

                    if let Some(status) = Self::char_to_status(x) {
                        staged.push(GitFileEntry {
                            path: path.clone(),
                            status,
                            original_path: orig_path.clone(),
                        });
                    }

                    if let Some(status) = Self::char_to_status(y) {
                        unstaged.push(GitFileEntry {
                            path,
                            status,
                            original_path: orig_path,
                        });
                    }
                }
                "u" => {
                    // Unmerged / conflict:
                    let path = trimmed.split_whitespace().last().unwrap_or("").to_string();
                    if !path.is_empty() {
                        conflicted.push(path);
                    }
                }
                "?" => {
                    // Untracked:
                    let path = trimmed.strip_prefix("? ").unwrap_or("").trim().to_string();
                    if !path.is_empty() {
                        untracked.push(path);
                    }
                }
                _ => {}
            }
        }

        let is_clean = staged.is_empty() && unstaged.is_empty() && untracked.is_empty() && conflicted.is_empty();

        Ok(GitStatusResponse {
            is_git_repo: true,
            branch,
            upstream,
            ahead,
            behind,
            staged,
            unstaged,
            untracked,
            conflicted,
            is_clean,
        })
    }

    fn char_to_status(c: char) -> Option<GitFileStatus> {
        match c {
            'M' => Some(GitFileStatus::Modified),
            'A' => Some(GitFileStatus::Added),
            'D' => Some(GitFileStatus::Deleted),
            'R' => Some(GitFileStatus::Renamed),
            'T' => Some(GitFileStatus::TypeChanged),
            'C' => Some(GitFileStatus::Renamed),
            _ => None,
        }
    }
}
