use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::EnvFilter;
use transcend_server::TranscendServer;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Check if subcommand was requested
    if args.len() > 1 {
        match args[1].as_str() {
            "lsp" => {
                let sub = args.get(2).map(|s| s.as_str()).unwrap_or("status");
                match sub {
                    "status" => {
                        let filter = args.get(3).cloned();
                        return cmd_lsp_status(filter).await;
                    }
                    "install" => {
                        let lang = args.get(3).cloned();
                        let method = parse_flag(&args, "--method").or_else(|| parse_flag(&args, "-m"));
                        return cmd_lsp_install(lang, method).await;
                    }
                    "-h" | "--help" | "help" => {
                        print_lsp_help();
                        return Ok(());
                    }
                    unknown => {
                        // Could be shorthand: `transcend lsp typescript` or `transcend lsp status`
                        if unknown.starts_with('-') {
                            print_lsp_help();
                            return Ok(());
                        }
                        return cmd_lsp_status(Some(unknown.to_string())).await;
                    }
                }
            }
            "lsp-status" => {
                let filter = args.get(2).cloned();
                return cmd_lsp_status(filter).await;
            }
            "lsp-install" => {
                let lang = args.get(2).cloned();
                let method = parse_flag(&args, "--method").or_else(|| parse_flag(&args, "-m"));
                return cmd_lsp_install(lang, method).await;
            }
            "-h" | "--help" | "help" => {
                print_help();
                return Ok(());
            }
            "--version" | "-v" => {
                println!("Transcend v{}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => {
                // Unknown argument or flags like --stdio: fall through to MCP server daemon
            }
        }
    }

    // Default mode: Initialize logging to stderr and run MCP Server over stdio
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("Starting Transcend MCP Server over stdio...");

    let server = TranscendServer::default();
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}

fn parse_flag(args: &[String], flag: &str) -> Option<String> {
    for (i, arg) in args.iter().enumerate() {
        if arg == flag {
            return args.get(i + 1).cloned();
        }
        if let Some(stripped) = arg.strip_prefix(&format!("{flag}=")) {
            return Some(stripped.to_string());
        }
    }
    None
}

fn print_help() {
    println!(r#"Transcend - Native Codebase Intelligence Engine for AI Coding Agents

USAGE:
    transcend [SUBCOMMAND | OPTIONS]

DEFAULT (No arguments or with stdio harness):
    Starts the high-performance Transcend MCP Server daemon over stdio (23 native tools).

SUBCOMMANDS:
    lsp status [language]           Audit detected language servers, binaries, and install recipes
    lsp install <language> [-m pkg] Install official language server using host package managers
    lsp-status [language]           Shorthand for 'lsp status'
    lsp-install <language> [-m pkg] Shorthand for 'lsp install'
    help, --help, -h                Show this help message
    --version, -v                   Display Transcend version

EXAMPLES:
    transcend                       # Run MCP server daemon for Claude Code / Antigravity
    transcend lsp status            # Show status table of all 18 supported language servers
    transcend lsp status rust       # Show detailed status for Rust analyzer
    transcend lsp install typescript # Automatically install typescript-language-server via npm
    transcend lsp install python -m pip # Install pyright via pip
"#);
}

fn print_lsp_help() {
    println!(r#"Transcend LSP Management

USAGE:
    transcend lsp status [language]
    transcend lsp install <language> [--method <manager>]

SUPPORTED LANGUAGES:
    rust, go, typescript, python, c, cpp, csharp, java, kotlin,
    php, ruby, swift, bash, sql, dart, zig, lua, markdown

FLAGS:
    -m, --method <manager>    Explicit package manager (e.g. npm, pip, rustup, cargo, go)
"#);
}

async fn cmd_lsp_status(language: Option<String>) -> Result<()> {
    let req = transcend_protocol::LspStatusRequest { language };
    let res = transcend_core::lsp::installer::LspInstaller::check_status(&req).await;

    println!("\nTranscend Language Server Protocol (LSP) Status");
    println!("===============================================");
    println!(
        "Total Profiles: {} | Installed: {} | Missing: {}\n",
        res.total_servers,
        res.total_installed,
        res.total_servers.saturating_sub(res.total_installed)
    );

    println!("{:<12} {:<12} {:<26} {}", "LANGUAGE", "STATUS", "BINARY", "LOCATION / INSTALL RECIPE");
    println!("{:<12} {:<12} {:<26} {}", "--------", "------", "------", "--------------------------");

    for s in &res.servers {
        let status_str = if s.installed { "Installed" } else { "Missing" };
        let bin_str = s.binary.as_deref().unwrap_or("-");
        let detail = if s.installed {
            let ver = s.version.as_deref().unwrap_or("");
            if let Some(ref p) = s.path {
                if ver.is_empty() || ver == "installed" {
                    p.clone()
                } else {
                    format!("{p} ({ver})")
                }
            } else {
                ver.to_string()
            }
        } else {
            let available_recipe = s.install_methods.iter().find(|r| r.available);
            if let Some(r) = available_recipe {
                format!("{} (run: '{}')", r.manager, r.command)
            } else if let Some(first) = s.install_methods.first() {
                format!("{} (need '{}')", first.command, first.manager)
            } else {
                "No recipe".to_string()
            }
        };

        println!("{:<12} {:<12} {:<26} {}", s.language, status_str, bin_str, detail);
    }
    println!();
    Ok(())
}

async fn cmd_lsp_install(language: Option<String>, method: Option<String>) -> Result<()> {
    let lang = match language {
        Some(l) if !l.trim().is_empty() => l,
        _ => {
            eprintln!("Error: Language name required. Example: 'transcend lsp install typescript'");
            std::process::exit(1);
        }
    };

    println!("\n[Transcend] Installing language server for '{lang}'...");
    let req = transcend_protocol::LspInstallRequest {
        language: lang.clone(),
        method,
    };

    let res = transcend_core::lsp::installer::LspInstaller::install(&req).await?;

    if !res.output.is_empty() {
        println!("\n--- Execution Output ---");
        println!("{}", res.output);
        println!("------------------------\n");
    }

    if res.success {
        println!("[SUCCESS] {}", res.message);
        if let Some(path) = res.path {
            println!("  Binary Path: {path}");
        }
        if let Some(ver) = res.version {
            println!("  Version:     {ver}");
        }
    } else {
        eprintln!("[FAILURE] {}", res.message);
        std::process::exit(1);
    }

    Ok(())
}
