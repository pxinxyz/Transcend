//! Transcend CLI entrypoint.
//!
//! Default mode runs the MCP server daemon over stdio, where `stdout` is reserved
//! exclusively for JSON-RPC framing. Subcommands emit human-readable output.

use anyhow::{Context, Result};
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::EnvFilter;
use transcend_server::TranscendServer;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();

    // Subcommand dispatch. Anything unrecognized falls through to the MCP daemon so
    // that host launchers passing `--stdio` or similar still boot the server.
    match argv.as_slice() {
        [] => {}
        // `..` so both a lone flag and `--flag extra` match.
        ["-h" | "--help" | "help", ..] => {
            print_help();
            return Ok(());
        }
        ["-v" | "--version", ..] => {
            println!("Transcend v{VERSION}");
            return Ok(());
        }
        ["lsp", rest @ ..] => return cmd_lsp(rest).await,
        ["lsp-status", rest @ ..] => return cmd_lsp_status(rest.first().copied()).await,
        ["lsp-install", rest @ ..] => {
            let method = parse_flag_argv(argv.as_slice(), "--method")
                .or_else(|| parse_flag_argv(argv.as_slice(), "-m"));
            return cmd_lsp_install(rest.first().copied(), method).await;
        }
        ["export-schemas", rest @ ..] => return cmd_export_schemas(rest),
        _ => {}
    }

    run_mcp_server().await
}

/// Run the MCP server daemon over stdio. Logs go to stderr only.
async fn run_mcp_server() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("Starting Transcend MCP Server v{VERSION} over stdio...");

    let service = TranscendServer::default().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

/// `transcend export-schemas [--out <dir>] [--quiet]`
fn cmd_export_schemas(rest: &[&str]) -> Result<()> {
    let out = parse_flag_argv(rest, "--out")
        .or_else(|| parse_flag_argv(rest, "-o"))
        .unwrap_or_else(|| "schemas".to_string());
    let quiet = rest.iter().any(|a| *a == "--quiet" || *a == "-q");

    let out_path = std::path::PathBuf::from(&out);
    let written = TranscendServer::write_schemas(&out_path).with_context(|| {
        format!(
            "failed to export MCP tool schemas to {}",
            out_path.display()
        )
    })?;

    if !quiet {
        println!(
            "Exported {} MCP tool schema(s) to {}\n",
            written.len(),
            out_path.display()
        );
        for entry in &written {
            println!("  {:<18} {}", entry.name, entry.path);
        }
        println!();
    }
    Ok(())
}

/// `transcend lsp <status|install|help>`
async fn cmd_lsp(rest: &[&str]) -> Result<()> {
    let first = rest.first().copied();

    let Some(first) = first else {
        print_lsp_help();
        return Ok(());
    };

    if matches!(first, "-h" | "--help" | "help") || first.starts_with('-') {
        print_lsp_help();
        return Ok(());
    }

    let language = rest.get(1).copied();
    match first {
        "status" => cmd_lsp_status(language).await,
        "install" => {
            let method = parse_flag_argv(rest, "--method").or_else(|| parse_flag_argv(rest, "-m"));
            cmd_lsp_install(language, method).await
        }
        // Shorthand: `transcend lsp typescript`
        other => cmd_lsp_status(Some(other)).await,
    }
}

async fn cmd_lsp_status(language: Option<&str>) -> Result<()> {
    let req = transcend_protocol::LspStatusRequest {
        language: language.map(str::to_string),
    };
    let res = transcend_core::lsp::installer::LspInstaller::check_status(&req).await;

    println!("\nTranscend Language Server Protocol (LSP) Status");
    println!("===============================================");
    println!(
        "Total Profiles: {} | Installed: {} | Missing: {}\n",
        res.total_servers,
        res.total_installed,
        res.total_servers.saturating_sub(res.total_installed)
    );

    println!(
        "{:<12} {:<12} {:<26} LOCATION / INSTALL RECIPE",
        "LANGUAGE", "STATUS", "BINARY"
    );
    println!(
        "{:<12} {:<12} {:<26} --------------------------",
        "--------", "------", "------"
    );

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

        println!(
            "{:<12} {:<12} {:<26} {}",
            s.language, status_str, bin_str, detail
        );
    }
    println!();
    Ok(())
}

async fn cmd_lsp_install(language: Option<&str>, method: Option<String>) -> Result<()> {
    let lang = match language {
        Some(l) if !l.trim().is_empty() => l.to_string(),
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
        Ok(())
    } else {
        eprintln!("[FAILURE] {}", res.message);
        std::process::exit(1);
    }
}

/// Read `--flag value` or `--flag=value` from an argument list.
fn parse_flag_argv(args: &[&str], flag: &str) -> Option<String> {
    for (i, arg) in args.iter().enumerate() {
        if *arg == flag {
            return args.get(i + 1).map(|s| s.to_string());
        }
        if let Some(stripped) = arg.strip_prefix(&format!("{flag}=")) {
            return Some(stripped.to_string());
        }
    }
    None
}

fn print_help() {
    println!(
        r#"Transcend v{VERSION} - Native Codebase Intelligence Engine for AI Coding Agents

USAGE:
    transcend [SUBCOMMAND | OPTIONS]

DEFAULT (no arguments, or unrecognized flags such as --stdio):
    Starts the Transcend MCP Server daemon over stdio (23 native tools).

SUBCOMMANDS:
    export-schemas [-o <dir>] [--quiet]
                                    Write every registered MCP tool schema as JSON
    lsp status [language]           Audit detected language servers, binaries, and install recipes
    lsp install <language> [-m pkg] Install official language server using host package managers
    lsp-status [language]           Shorthand for 'lsp status'
    lsp-install <language> [-m pkg] Shorthand for 'lsp install'
    help, --help, -h                Show this help message
    --version, -v                   Display Transcend version

EXAMPLES:
    transcend                              # Run MCP server daemon for Claude Code / Antigravity
    transcend export-schemas -o ./schemas   # Dump tool schemas for client configuration
    transcend lsp status                   # Show status table of all 18 supported language servers
    transcend lsp status rust              # Show detailed status for Rust analyzer
    transcend lsp install typescript       # Install typescript-language-server via npm
    transcend lsp install python -m pip    # Install pyright via pip
"#
    );
}

fn print_lsp_help() {
    println!(
        r#"Transcend LSP Management

USAGE:
    transcend lsp status [language]
    transcend lsp install <language> [--method <manager>]

SUPPORTED LANGUAGES:
    rust, go, typescript, python, c, cpp, csharp, java, kotlin,
    php, ruby, swift, bash, sql, dart, zig, lua, markdown

FLAGS:
    -m, --method <manager>    Explicit package manager (e.g. npm, pip, rustup, cargo, go)
"#
    );
}
