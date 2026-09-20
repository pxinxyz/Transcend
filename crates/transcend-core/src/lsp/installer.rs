//! Language Server Installer & Status Auditor
//!
//! Provides recipe-based discovery, version probing, and automated installation
//! of official language servers using host package managers.

use std::path::Path;
use std::time::Duration;
use tokio::process::Command;
use tracing::{info, warn};

use transcend_protocol::{
    LspInstallRecipe, LspInstallRequest, LspInstallResponse, LspServerStatus, LspStatusRequest,
    LspStatusResponse,
};

use super::registry::{KNOWN_SERVERS, LspRegistry, is_on_path};
use crate::CoreError;

/// Raw recipe definition for a language server.
struct RawRecipe {
    manager: &'static str,
    command: &'static str,
    description: &'static str,
}

/// Helper to get official recipes for a given language ID.
fn get_recipes_for_language(language_id: &str) -> Vec<RawRecipe> {
    match language_id {
        "rust" => vec![
            RawRecipe {
                manager: "rustup",
                command: "rustup component add rust-analyzer",
                description: "Official Rustup component",
            },
            RawRecipe {
                manager: "cargo",
                command: "cargo install rust-analyzer",
                description: "Build from crates.io via Cargo",
            },
        ],
        "go" => vec![RawRecipe {
            manager: "go",
            command: "go install golang.org/x/tools/gopls@latest",
            description: "Official Go language server from Google",
        }],
        "typescript" => vec![RawRecipe {
            manager: "npm",
            command: "npm install -g typescript-language-server typescript",
            description: "Official TypeScript language server via npm",
        }],
        "python" => vec![
            RawRecipe {
                manager: "npm",
                command: "npm install -g pyright",
                description: "Microsoft Pyright static type checker via npm",
            },
            RawRecipe {
                manager: "pip",
                command: "pip install pyright",
                description: "Microsoft Pyright via Python pip",
            },
            RawRecipe {
                manager: "pip",
                command: "pip install ruff",
                description: "Astral Ruff Python linter and language server",
            },
        ],
        "c" | "cpp" => {
            #[cfg(windows)]
            {
                vec![RawRecipe {
                    manager: "winget",
                    command: "winget install LLVM.LLVM",
                    description: "LLVM Clangd compiler & language server via WinGet",
                }]
            }
            #[cfg(target_os = "macos")]
            {
                vec![RawRecipe {
                    manager: "brew",
                    command: "brew install llvm",
                    description: "LLVM Clangd via Homebrew",
                }]
            }
            #[cfg(not(any(windows, target_os = "macos")))]
            {
                vec![RawRecipe {
                    manager: "apt",
                    command: "sudo apt-get install clangd",
                    description: "Clangd language server via apt",
                }]
            }
        }
        "csharp" => vec![RawRecipe {
            manager: "dotnet",
            command: "dotnet tool install --global csharp-ls",
            description: "C# Language Server via .NET toolchain",
        }],
        "java" => {
            #[cfg(windows)]
            {
                vec![RawRecipe {
                    manager: "winget",
                    command: "winget install RedHat.JDTLS",
                    description: "Eclipse JDT Language Server via WinGet",
                }]
            }
            #[cfg(target_os = "macos")]
            {
                vec![RawRecipe {
                    manager: "brew",
                    command: "brew install jdtls",
                    description: "Eclipse JDT Language Server via Homebrew",
                }]
            }
            #[cfg(not(any(windows, target_os = "macos")))]
            {
                vec![RawRecipe {
                    manager: "apt",
                    command: "sudo apt-get install jdtls",
                    description: "Eclipse JDT Language Server via apt",
                }]
            }
        }
        "kotlin" => {
            #[cfg(windows)]
            {
                vec![RawRecipe {
                    manager: "winget",
                    command: "winget install fwcd.kotlin-language-server",
                    description: "Kotlin Language Server via WinGet",
                }]
            }
            #[cfg(target_os = "macos")]
            {
                vec![RawRecipe {
                    manager: "brew",
                    command: "brew install kotlin-language-server",
                    description: "Kotlin Language Server via Homebrew",
                }]
            }
            #[cfg(not(any(windows, target_os = "macos")))]
            {
                vec![RawRecipe {
                    manager: "brew",
                    command: "brew install kotlin-language-server",
                    description: "Kotlin Language Server via Homebrew",
                }]
            }
        }
        "php" => vec![RawRecipe {
            manager: "npm",
            command: "npm install -g intelephense",
            description: "Intelephense PHP language server via npm",
        }],
        "ruby" => vec![RawRecipe {
            manager: "gem",
            command: "gem install solargraph",
            description: "Solargraph Ruby language server via gem",
        }],
        "swift" => vec![RawRecipe {
            manager: "brew",
            command: "brew install swift",
            description: "Swift toolchain and SourceKit-LSP",
        }],
        "bash" => vec![RawRecipe {
            manager: "npm",
            command: "npm install -g bash-language-server",
            description: "Bash language server via npm",
        }],
        "sql" => vec![
            RawRecipe {
                manager: "npm",
                command: "npm install -g sql-language-server",
                description: "SQL Language Server via npm",
            },
            RawRecipe {
                manager: "go",
                command: "go install github.com/sqls-server/sqls@latest",
                description: "sqls language server via Go",
            },
        ],
        "dart" => vec![RawRecipe {
            manager: "dart",
            command: "dart language-server --protocol=lsp",
            description: "Built-in Dart SDK language server",
        }],
        "zig" => {
            #[cfg(windows)]
            {
                vec![RawRecipe {
                    manager: "winget",
                    command: "winget install ziglang.zls",
                    description: "Zig Language Server via WinGet",
                }]
            }
            #[cfg(target_os = "macos")]
            {
                vec![RawRecipe {
                    manager: "brew",
                    command: "brew install zls",
                    description: "Zig Language Server via Homebrew",
                }]
            }
            #[cfg(not(any(windows, target_os = "macos")))]
            {
                vec![RawRecipe {
                    manager: "snap",
                    command: "sudo snap install zls --classic",
                    description: "Zig Language Server via Snap",
                }]
            }
        }
        "lua" => {
            #[cfg(windows)]
            {
                vec![RawRecipe {
                    manager: "winget",
                    command: "winget install sumneko.lua-language-server",
                    description: "Lua Language Server via WinGet",
                }]
            }
            #[cfg(target_os = "macos")]
            {
                vec![RawRecipe {
                    manager: "brew",
                    command: "brew install lua-language-server",
                    description: "Lua Language Server via Homebrew",
                }]
            }
            #[cfg(not(any(windows, target_os = "macos")))]
            {
                vec![RawRecipe {
                    manager: "apt",
                    command: "sudo apt-get install lua-language-server",
                    description: "Lua Language Server via apt",
                }]
            }
        }
        "markdown" => {
            #[cfg(windows)]
            {
                vec![RawRecipe {
                    manager: "winget",
                    command: "winget install marksman",
                    description: "Marksman Markdown language server via WinGet",
                }]
            }
            #[cfg(target_os = "macos")]
            {
                vec![RawRecipe {
                    manager: "brew",
                    command: "brew install marksman",
                    description: "Marksman Markdown language server via Homebrew",
                }]
            }
            #[cfg(not(any(windows, target_os = "macos")))]
            {
                vec![RawRecipe {
                    manager: "snap",
                    command: "sudo snap install marksman",
                    description: "Marksman Markdown language server via Snap",
                }]
            }
        }
        _ => Vec::new(),
    }
}

/// Probes an executable for its version string with a bounded timeout.
async fn probe_version(exec_path: &Path) -> Option<String> {
    // 1. Try `--version`
    if let Ok(Ok(output)) = tokio::time::timeout(
        Duration::from_millis(2000),
        Command::new(exec_path).arg("--version").output(),
    )
    .await
        && output.status.success()
    {
        let text = if !output.stdout.is_empty() {
            String::from_utf8_lossy(&output.stdout).to_string()
        } else {
            String::from_utf8_lossy(&output.stderr).to_string()
        };
        let first_line = text.lines().next().unwrap_or("").trim().to_string();
        if !first_line.is_empty()
            && !first_line.to_lowercase().contains("error")
            && !first_line.to_lowercase().contains("exception")
        {
            return Some(first_line);
        }
    }

    // 2. Try `version` (e.g. gopls)
    if let Ok(Ok(output)) = tokio::time::timeout(
        Duration::from_millis(2000),
        Command::new(exec_path).arg("version").output(),
    )
    .await
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        let first_line = text.lines().next().unwrap_or("").trim().to_string();
        if !first_line.is_empty() && !first_line.to_lowercase().contains("error") {
            return Some(first_line);
        }
    }

    Some("installed".to_string())
}

/// Language Server Manager and Installer engine.
pub struct LspInstaller;

impl LspInstaller {
    /// Check current installation status and recipes for requested or all language servers.
    pub async fn check_status(req: &LspStatusRequest) -> LspStatusResponse {
        let lang_filter = req.language.as_deref().map(|s| s.to_lowercase());

        let mut servers = Vec::new();
        let mut total_installed = 0;

        for profile in KNOWN_SERVERS {
            if let Some(ref filter) = lang_filter
                && profile.language_id != filter.as_str()
            {
                continue;
            }

            let primary_binary = profile.binary_candidates[0].to_string();
            let resolved = LspRegistry::resolve_binary_path(profile);

            let (installed, binary, path_str, version) = match resolved {
                Some((bin_name, abs_path)) => {
                    total_installed += 1;
                    let ver = probe_version(&abs_path).await;
                    (
                        true,
                        Some(bin_name),
                        Some(abs_path.to_string_lossy().to_string()),
                        ver,
                    )
                }
                None => (false, None, None, None),
            };

            let recipes = get_recipes_for_language(profile.language_id)
                .into_iter()
                .map(|r| LspInstallRecipe {
                    available: is_on_path(r.manager),
                    manager: r.manager.to_string(),
                    command: r.command.to_string(),
                    description: Some(r.description.to_string()),
                })
                .collect();

            servers.push(LspServerStatus {
                language: profile.language_id.to_string(),
                primary_binary,
                installed,
                binary,
                path: path_str,
                version,
                install_methods: recipes,
            });
        }

        let total_servers = servers.len();
        LspStatusResponse {
            servers,
            total_servers,
            total_installed,
        }
    }

    /// Automatically install a language server using host package managers.
    pub async fn install(req: &LspInstallRequest) -> Result<LspInstallResponse, CoreError> {
        let lang = req.language.to_lowercase();
        let profile = LspRegistry::profile_for_language(&lang).ok_or_else(|| {
            CoreError::InvalidInput(format!(
                "Unsupported language '{lang}'. Supported: rust, go, typescript, python, c, cpp, csharp, java, kotlin, php, ruby, swift, bash, sql, dart, zig, lua, markdown"
            ))
        })?;

        // 1. Check if already installed
        if let Some((bin_name, abs_path)) = LspRegistry::resolve_binary_path(profile) {
            let ver = probe_version(&abs_path).await;
            return Ok(LspInstallResponse {
                success: true,
                language: lang,
                binary: Some(bin_name),
                path: Some(abs_path.to_string_lossy().to_string()),
                version: ver,
                output: "Language server is already installed and discoverable on PATH."
                    .to_string(),
                message: format!("'{}' is already installed.", profile.binary_candidates[0]),
            });
        }

        // 2. Select appropriate recipe
        let raw_recipes = get_recipes_for_language(profile.language_id);
        if raw_recipes.is_empty() {
            return Err(CoreError::InvalidInput(format!(
                "No automated installation recipe available for language '{lang}' on this platform."
            )));
        }

        let selected_recipe = if let Some(ref method) = req.method {
            if method.eq_ignore_ascii_case("auto") {
                raw_recipes.iter().find(|r| is_on_path(r.manager))
            } else {
                raw_recipes
                    .iter()
                    .find(|r| r.manager.eq_ignore_ascii_case(method))
            }
        } else {
            raw_recipes.iter().find(|r| is_on_path(r.manager))
        };

        let recipe = match selected_recipe {
            Some(r) => r,
            None => {
                let options: Vec<String> = raw_recipes
                    .iter()
                    .map(|r| format!("{} ('{}')", r.manager, r.command))
                    .collect();
                return Ok(LspInstallResponse {
                    success: false,
                    language: lang,
                    binary: None,
                    path: None,
                    version: None,
                    output: String::new(),
                    message: format!(
                        "No required package manager found for '{}'. Please install one of: {}",
                        profile.language_id,
                        options.join(", ")
                    ),
                });
            }
        };

        info!(
            language = %lang,
            manager = %recipe.manager,
            command = %recipe.command,
            "Executing LSP installer command"
        );

        // 3. Execute installation command asynchronously with a 180-second timeout
        #[cfg(windows)]
        let mut child = Command::new("cmd");
        #[cfg(windows)]
        child.args(["/C", recipe.command]);

        #[cfg(not(windows))]
        let mut child = Command::new("sh");
        #[cfg(not(windows))]
        child.args(["-c", recipe.command]);

        let exec_result = tokio::time::timeout(Duration::from_secs(180), child.output()).await;

        let output = match exec_result {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => {
                return Ok(LspInstallResponse {
                    success: false,
                    language: lang,
                    binary: None,
                    path: None,
                    version: None,
                    output: format!("Execution failed: {e}"),
                    message: format!("Failed to run install command '{}': {e}", recipe.command),
                });
            }
            Err(_) => {
                return Ok(LspInstallResponse {
                    success: false,
                    language: lang,
                    binary: None,
                    path: None,
                    version: None,
                    output: "Command timed out after 180 seconds".to_string(),
                    message: format!("Installation of '{}' timed out", recipe.command),
                });
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined_log = format!("{stdout}\n{stderr}").trim().to_string();

        if !output.status.success() {
            warn!(status = ?output.status.code(), "LSP installation command exited with error");
            return Ok(LspInstallResponse {
                success: false,
                language: lang,
                binary: None,
                path: None,
                version: None,
                output: combined_log,
                message: format!(
                    "Command '{}' failed with exit code {:?}",
                    recipe.command,
                    output.status.code()
                ),
            });
        }

        // 4. Verify that binary is now discoverable
        let verified = LspRegistry::resolve_binary_path(profile);
        match verified {
            Some((bin_name, abs_path)) => {
                let ver = probe_version(&abs_path).await;
                info!(
                    binary = %bin_name,
                    path = %abs_path.display(),
                    "LSP installation succeeded and verified"
                );
                Ok(LspInstallResponse {
                    success: true,
                    language: lang,
                    binary: Some(bin_name.clone()),
                    path: Some(abs_path.to_string_lossy().to_string()),
                    version: ver,
                    output: combined_log,
                    message: format!(
                        "Successfully installed '{bin_name}' via {}.",
                        recipe.manager
                    ),
                })
            }
            None => Ok(LspInstallResponse {
                success: false,
                language: lang,
                binary: None,
                path: None,
                version: None,
                output: combined_log,
                message: format!(
                    "Command '{}' completed, but candidate binary was not detected in search paths. A terminal restart or PATH refresh may be required.",
                    recipe.command
                ),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_check_status_all() {
        let req = LspStatusRequest::default();
        let res = LspInstaller::check_status(&req).await;
        assert_eq!(res.total_servers, 18);
        assert_eq!(res.servers.len(), 18);
    }

    #[tokio::test]
    async fn test_check_status_single_language() {
        let req = LspStatusRequest {
            language: Some("rust".to_string()),
        };
        let res = LspInstaller::check_status(&req).await;
        assert_eq!(res.total_servers, 1);
        assert_eq!(res.servers.len(), 1);
        assert_eq!(res.servers[0].language, "rust");
        assert_eq!(res.servers[0].primary_binary, "rust-analyzer");
        assert!(!res.servers[0].install_methods.is_empty());
    }

    #[tokio::test]
    async fn test_install_unsupported_language() {
        let req = LspInstallRequest {
            language: "brainfuck".to_string(),
            method: None,
        };
        let res = LspInstaller::install(&req).await;
        assert!(res.is_err());
    }
}
