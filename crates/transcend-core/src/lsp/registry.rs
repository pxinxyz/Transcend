//! Language Server Registry & Project Root Discovery
//!
//! Maps programming languages to known official language server profiles,
//! detects binary availability on system PATH, and discovers workspace roots.

use std::path::{Path, PathBuf};

/// Definition of a language server binary and its launch configuration.
#[derive(Debug, Clone)]
pub struct LspServerProfile {
    /// Canonical language identifier (e.g. "rust", "go", "python", "typescript").
    pub language_id: &'static str,
    /// Candidate executable names to try on PATH (in priority order).
    pub binary_candidates: &'static [&'static str],
    /// Default command-line arguments.
    pub args: &'static [&'static str],
    /// File extensions associated with this language.
    pub extensions: &'static [&'static str],
    /// Project root indicator files (checked in order before fallback to .git).
    pub root_markers: &'static [&'static str],
}

/// Catalog of supported official language server profiles.
pub static KNOWN_SERVERS: &[LspServerProfile] = &[
    LspServerProfile {
        language_id: "rust",
        binary_candidates: &["rust-analyzer"],
        args: &[],
        extensions: &["rs"],
        root_markers: &["Cargo.toml"],
    },
    LspServerProfile {
        language_id: "go",
        binary_candidates: &["gopls"],
        args: &[],
        extensions: &["go"],
        root_markers: &["go.work", "go.mod"],
    },
    LspServerProfile {
        language_id: "typescript",
        binary_candidates: &["typescript-language-server", "vtsls"],
        args: &["--stdio"],
        extensions: &["ts", "tsx", "js", "jsx", "mjs", "cjs"],
        root_markers: &["tsconfig.json", "jsconfig.json", "package.json"],
    },
    LspServerProfile {
        language_id: "python",
        binary_candidates: &["pyright-langserver", "basedpyright-langserver", "ruff"],
        args: &["--stdio"],
        extensions: &["py", "pyi"],
        root_markers: &["pyproject.toml", "setup.py", "setup.cfg", "requirements.txt", "Pipfile"],
    },
    LspServerProfile {
        language_id: "c",
        binary_candidates: &["clangd"],
        args: &["--background-index"],
        extensions: &["c", "h"],
        root_markers: &["compile_commands.json", "CMakeLists.txt"],
    },
    LspServerProfile {
        language_id: "cpp",
        binary_candidates: &["clangd"],
        args: &["--background-index"],
        extensions: &["cpp", "hpp", "cc", "cxx", "hxx"],
        root_markers: &["compile_commands.json", "CMakeLists.txt"],
    },
    LspServerProfile {
        language_id: "zig",
        binary_candidates: &["zls"],
        args: &[],
        extensions: &["zig"],
        root_markers: &["build.zig"],
    },
];

/// Registry for querying language server availability and workspace configurations.
pub struct LspRegistry;

impl LspRegistry {
    /// Find server profile matching a file's extension.
    pub fn profile_for_path(path: &Path) -> Option<&'static LspServerProfile> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        KNOWN_SERVERS.iter().find(|p| p.extensions.contains(&ext.as_str()))
    }

    /// Find server profile by canonical language ID.
    pub fn profile_for_language(language: &str) -> Option<&'static LspServerProfile> {
        let lower = language.to_lowercase();
        KNOWN_SERVERS.iter().find(|p| p.language_id == lower)
    }

    /// Check if any candidate binary for this profile exists on system PATH.
    /// Returns the resolved binary name/path if found.
    pub fn resolve_binary(profile: &LspServerProfile) -> Option<String> {
        for candidate in profile.binary_candidates {
            if is_on_path(candidate) {
                return Some((*candidate).to_string());
            }
        }
        None
    }

    /// Discover workspace root directory for a target file.
    /// Climbs parent directories searching for language root markers or `.git`.
    pub fn find_workspace_root(file_path: &Path, profile: Option<&LspServerProfile>) -> PathBuf {
        let start_dir = if file_path.is_file() {
            file_path.parent().unwrap_or(file_path)
        } else {
            file_path
        };

        let mut curr = start_dir.to_path_buf();
        let mut git_root: Option<PathBuf> = None;

        loop {
            // Check specific language root markers
            if let Some(prof) = profile {
                for marker in prof.root_markers {
                    if curr.join(marker).exists() {
                        return curr;
                    }
                }
            }

            // Record git root if found, but keep climbing if language root might be higher
            if curr.join(".git").exists() && git_root.is_none() {
                git_root = Some(curr.clone());
            }

            match curr.parent() {
                Some(parent) if parent != curr => {
                    curr = parent.to_path_buf();
                }
                _ => break,
            }
        }

        // Fallback to git root if found, otherwise start directory
        git_root.unwrap_or_else(|| start_dir.to_path_buf())
    }
}

/// Check whether an executable exists on the system PATH.
pub fn is_on_path(executable: &str) -> bool {
    let path_var = match std::env::var_os("PATH") {
        Some(v) => v,
        None => return false,
    };

    #[cfg(windows)]
    let extensions: Vec<String> = {
        let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT".to_string());
        pathext.split(';').map(|s| s.to_lowercase()).collect()
    };

    for dir in std::env::split_paths(&path_var) {
        let direct = dir.join(executable);
        if direct.is_file() {
            return true;
        }

        #[cfg(windows)]
        {
            for ext in &extensions {
                let with_ext = dir.join(format!("{executable}{ext}"));
                if with_ext.is_file() {
                    return true;
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_lookup() {
        let rs_path = Path::new("src/main.rs");
        let prof = LspRegistry::profile_for_path(rs_path).expect("rust profile should be found");
        assert_eq!(prof.language_id, "rust");
        assert_eq!(prof.binary_candidates[0], "rust-analyzer");

        let py_path = Path::new("scripts/test.py");
        let prof = LspRegistry::profile_for_path(py_path).expect("python profile should be found");
        assert_eq!(prof.language_id, "python");
    }

    #[test]
    fn test_find_workspace_root() {
        let curr_file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("lib.rs");
        let prof = LspRegistry::profile_for_path(&curr_file);
        let root = LspRegistry::find_workspace_root(&curr_file, prof);
        // Should find Cargo.toml in crates/transcend-core or repository root
        assert!(root.join("Cargo.toml").exists());
    }
}
