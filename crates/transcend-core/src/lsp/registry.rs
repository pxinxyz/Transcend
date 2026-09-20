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
        binary_candidates: &[
            "pyright-langserver",
            "pyright",
            "basedpyright-langserver",
            "ruff",
        ],
        args: &["--stdio"],
        extensions: &["py", "pyi"],
        root_markers: &[
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "requirements.txt",
            "Pipfile",
        ],
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
        language_id: "csharp",
        binary_candidates: &["csharp-ls", "OmniSharp"],
        args: &[],
        extensions: &["cs"],
        root_markers: &["*.sln", "*.csproj"],
    },
    LspServerProfile {
        language_id: "java",
        binary_candidates: &["jdtls"],
        args: &[],
        extensions: &["java"],
        root_markers: &["pom.xml", "build.gradle", "build.gradle.kts"],
    },
    LspServerProfile {
        language_id: "kotlin",
        binary_candidates: &["kotlin-language-server"],
        args: &[],
        extensions: &["kt", "kts"],
        root_markers: &["build.gradle.kts", "build.gradle", "pom.xml"],
    },
    LspServerProfile {
        language_id: "php",
        binary_candidates: &["intelephense", "phpactor"],
        args: &["--stdio"],
        extensions: &["php"],
        root_markers: &["composer.json"],
    },
    LspServerProfile {
        language_id: "ruby",
        binary_candidates: &["solargraph", "ruby-lsp"],
        args: &["stdio"],
        extensions: &["rb", "rake", "gemspec"],
        root_markers: &["Gemfile", ".rubocop.yml"],
    },
    LspServerProfile {
        language_id: "swift",
        binary_candidates: &["sourcekit-lsp"],
        args: &[],
        extensions: &["swift"],
        root_markers: &["Package.swift"],
    },
    LspServerProfile {
        language_id: "bash",
        binary_candidates: &["bash-language-server"],
        args: &["start"],
        extensions: &["sh", "bash"],
        root_markers: &[".git"],
    },
    LspServerProfile {
        language_id: "sql",
        binary_candidates: &["sql-language-server", "sqls"],
        args: &["up", "--method", "stdio"],
        extensions: &["sql"],
        root_markers: &[".git"],
    },
    LspServerProfile {
        language_id: "dart",
        binary_candidates: &["dart"],
        args: &["language-server", "--protocol=lsp"],
        extensions: &["dart"],
        root_markers: &["pubspec.yaml"],
    },
    LspServerProfile {
        language_id: "zig",
        binary_candidates: &["zls"],
        args: &[],
        extensions: &["zig"],
        root_markers: &["build.zig"],
    },
    LspServerProfile {
        language_id: "lua",
        binary_candidates: &["lua-language-server"],
        args: &[],
        extensions: &["lua"],
        root_markers: &[".luarc.json", ".git"],
    },
    LspServerProfile {
        language_id: "markdown",
        binary_candidates: &["marksman"],
        args: &["server"],
        extensions: &["md", "markdown"],
        root_markers: &[".marksman.toml", ".git"],
    },
];

/// Registry for querying language server availability and workspace configurations.
pub struct LspRegistry;

impl LspRegistry {
    /// Find server profile matching a file's extension.
    pub fn profile_for_path(path: &Path) -> Option<&'static LspServerProfile> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        KNOWN_SERVERS
            .iter()
            .find(|p| p.extensions.contains(&ext.as_str()))
    }

    /// Find server profile by canonical language ID.
    pub fn profile_for_language(language: &str) -> Option<&'static LspServerProfile> {
        let lower = language.to_lowercase();
        KNOWN_SERVERS.iter().find(|p| p.language_id == lower)
    }

    /// Check if any candidate binary for this profile exists on system PATH or toolchain directories.
    /// Returns the resolved candidate binary name and its absolute path.
    pub fn resolve_binary_path(profile: &LspServerProfile) -> Option<(String, PathBuf)> {
        for candidate in profile.binary_candidates {
            if let Some(path) = find_executable(candidate) {
                return Some(((*candidate).to_string(), path));
            }
        }
        None
    }

    /// Check if any candidate binary for this profile exists on system PATH.
    /// Returns the resolved binary name if found.
    pub fn resolve_binary(profile: &LspServerProfile) -> Option<String> {
        Self::resolve_binary_path(profile).map(|(name, _)| name)
    }

    /// Discover workspace root directory for a target file.
    /// Climbs parent directories searching for language root markers or `.git`.
    ///
    /// Markers may be literal filenames (`Cargo.toml`) or simple globs
    /// (`*.sln`, `*.csproj`). Globs are matched against the directory entries, so a
    /// C# project rooted at a `.sln` is detected instead of falling back to `.git`.
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
                    if Self::marker_present(&curr, marker) {
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

    /// Whether `marker` exists inside `dir`.
    ///
    /// A marker containing glob metacharacters (`*`, `?`, `[`) is matched against the
    /// directory's entry names; otherwise it is a plain `join(...).exists()` probe,
    /// which is the common and cheapest case.
    fn marker_present(dir: &Path, marker: &str) -> bool {
        if !marker.contains(['*', '?', '[']) {
            return dir.join(marker).exists();
        }

        let mut builder = globset::GlobBuilder::new(marker);
        builder.literal_separator(false);
        // Case-insensitive matching keeps `*.SLN` working on case-insensitive
        // filesystems (Windows, default macOS) without special-casing per-OS.
        builder.case_insensitive(true);
        let Ok(glob) = builder.build() else {
            // A malformed marker must not abort root discovery.
            return dir.join(marker).exists();
        };
        let matcher = glob.compile_matcher();

        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        entries.flatten().any(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| matcher.is_match(name))
        })
    }
}

/// Helper to collect candidate directories to search for executables.
/// Includes system PATH plus common user toolchain directories.
pub fn search_directories() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(path_var) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path_var));
    }

    // Include common user-local toolchain directories if present
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);

    if let Some(home_dir) = home {
        let cargo_bin = home_dir.join(".cargo").join("bin");
        if cargo_bin.is_dir() && !dirs.contains(&cargo_bin) {
            dirs.push(cargo_bin);
        }
        let transcend_bin = home_dir.join(".transcend").join("bin");
        if transcend_bin.is_dir() && !dirs.contains(&transcend_bin) {
            dirs.push(transcend_bin);
        }
        let go_bin = home_dir.join("go").join("bin");
        if go_bin.is_dir() && !dirs.contains(&go_bin) {
            dirs.push(go_bin);
        }
        #[cfg(windows)]
        {
            if let Some(appdata) = std::env::var_os("APPDATA") {
                let npm_dir = PathBuf::from(appdata).join("npm");
                if npm_dir.is_dir() && !dirs.contains(&npm_dir) {
                    dirs.push(npm_dir);
                }
            }
        }
        #[cfg(not(windows))]
        {
            let local_bin = home_dir.join(".local").join("bin");
            if local_bin.is_dir() && !dirs.contains(&local_bin) {
                dirs.push(local_bin);
            }
            let npm_global = home_dir.join(".npm-global").join("bin");
            if npm_global.is_dir() && !dirs.contains(&npm_global) {
                dirs.push(npm_global);
            }
        }
    }

    dirs
}

/// Locate an executable across PATH and toolchain directories, returning its absolute path.
pub fn find_executable(executable: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    let extensions: Vec<String> = {
        let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT".to_string());
        pathext.split(';').map(|s| s.to_lowercase()).collect()
    };

    let dirs = search_directories();

    for dir in dirs {
        let direct = dir.join(executable);
        if direct.is_file() {
            return Some(direct);
        }

        #[cfg(windows)]
        {
            for ext in &extensions {
                let with_ext = dir.join(format!("{executable}{ext}"));
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }
    }

    None
}

/// Check whether an executable exists on the system PATH or toolchain directories.
pub fn is_on_path(executable: &str) -> bool {
    find_executable(executable).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_lookup() {
        let test_cases = [
            ("main.rs", "rust"),
            ("app.ts", "typescript"),
            ("app.js", "typescript"),
            ("script.py", "python"),
            ("main.go", "go"),
            ("main.c", "c"),
            ("main.cpp", "cpp"),
            ("Program.cs", "csharp"),
            ("App.java", "java"),
            ("Main.kt", "kotlin"),
            ("index.php", "php"),
            ("app.rb", "ruby"),
            ("main.swift", "swift"),
            ("deploy.sh", "bash"),
            ("query.sql", "sql"),
            ("main.dart", "dart"),
            ("main.zig", "zig"),
            ("init.lua", "lua"),
            ("README.md", "markdown"),
        ];

        for (filename, expected_lang) in test_cases {
            let prof = LspRegistry::profile_for_path(Path::new(filename))
                .unwrap_or_else(|| panic!("profile should be found for {filename}"));
            assert_eq!(prof.language_id, expected_lang, "failed for {filename}");
        }
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
