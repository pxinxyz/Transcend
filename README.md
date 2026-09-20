<p align="center">
  <img src="banner.png" alt="Transcend Banner" width="100%">
</p>

<h1 align="center">Transcend</h1>

<p align="center">
  <strong>Deeper Context. Higher Capabilities.</strong><br>
  <em>The High-Performance Native Codebase Intelligence Engine for AI Coding Agents</em>
</p>

<p align="center">
  <a href="https://conventionalcommits.org"><img src="https://img.shields.io/badge/Conventional%20Commits-1.0.0-%23FE5196?logo=conventionalcommits&logoColor=white" alt="Conventional Commits"></a>
  <a href="https://github.com/pxinxyz/Transcend/releases"><img src="https://img.shields.io/github/v/release/pxinxyz/Transcend?color=blue&label=version" alt="Release"></a>
  <a href="https://modelcontextprotocol.io"><img src="https://img.shields.io/badge/MCP-23%20Tools-8A2BE2" alt="MCP Compatible"></a>
  <a href="https://github.com/pxinxyz/Transcend"><img src="https://img.shields.io/badge/tests-154%20passing-brightgreen" alt="Tests"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-2024%20edition-orange?logo=rust" alt="Rust Edition"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License"></a>
</p>

---

Transcend embeds a Rust-native codebase intelligence engine directly into the agent
lifecycle over the **Model Context Protocol**. Instead of spawning `grep`, `find`, `cat`
and `sed` and asking the model to act as a regex filter and coordinate calculator, agents
call 23 typed primitives that return structured, token-compact data with exact source
spans.

Two things it does that shell tooling cannot:

- **Compiler-resolved semantics.** Seven tools have no shell equivalent — the LSP tier,
  plus `terminal_write`, `terminal_resize` and `set_workspace`. Text search cannot tell a
  real reference from one in a comment, and cannot infer a type at all.
- **Verified mutations.** `patch` and `batch_patch` parse the spliced buffer with
  Tree-sitter *before* touching disk and refuse invalid syntax with exact line/column;
  `batch_patch` aborts atomically across files.

For measured comparisons against `rg`, `cat` and native file tooling — including where
Transcend is *larger* or slower — see
**[benchmarks/deepseek-flash-native-tooling-comparison.md](benchmarks/deepseek-flash-native-tooling-comparison.md)**.

## Tools

| Tool | Purpose |
|:---|:---|
| **Discovery & search** | |
| `find` | File discovery with a directory-density radar and extension census |
| `search` | Regex search returning per-file clusters plus a directory density radar |
| `find_symbol` | Locate definitions by name, qualified path, or kind across the workspace |
| **Reading** | |
| `outline` | AST symbol hierarchy, or a syntax-valid `skeleton` with bodies elided |
| `read_symbol` | One symbol's exact source, doc comment and byte span |
| `read_file` | Line/byte bounded reader with binary detection |
| **Writing** | |
| `write_file` | Atomic write with collision and parent-directory guards |
| `patch` | Edit by symbol, span or text with Tree-sitter preflight |
| `batch_patch` | Transactional multi-file changeset with all-or-nothing semantics |
| `delete_path` | Workspace-contained deletion with a recursion guard |
| `set_workspace` | Set or auto-detect the workspace root for relative paths |
| **Language servers** | |
| `lsp_definition` | Compiler-resolved definition, with a Tree-sitter fallback |
| `lsp_references` | Compiler-resolved references (no comment or string false positives) |
| `lsp_hover` | Inferred type signature and distilled documentation |
| `lsp_diagnostics` | Compiler diagnostics, with a native `cargo check` JSON fallback |
| `lsp_status` | Audit installed language servers, paths and install recipes |
| `lsp_install` | Install a language server via the host package manager |
| **Execution** | |
| `exec` | Hybrid one-shot execution: pipe or PTY, blocking or detached |
| `terminal_read` | Cursor-based incremental output streaming |
| `terminal_write` | Interactive stdin delivery to a detached session |
| `terminal_resize` | PTY dimension control |
| `terminal_kill` | Process-tree termination (Job Objects / POSIX process groups) |
| **Version control** | |
| `git_status` | Structured porcelain-v2 status instead of scraped text |

## Languages

18 language profiles, each with a compiled Tree-sitter grammar feeding `outline`,
`find_symbol` and `read_symbol`. The LSP tier discovers an installed language server
per profile and otherwise falls back to a Tree-sitter heuristic; the response reports
which engine answered. C and C++ are separate profiles sharing `clangd`.

| | | |
|:---|:---|:---|
| Rust (`rust-analyzer`) | TypeScript / JS (`typescript-language-server`) | Python (`pyright`, `ruff`) |
| Go (`gopls`) | C / C++ (`clangd`) | C# (`csharp-ls`) |
| Java (`jdtls`) | Kotlin (`kotlin-language-server`) | PHP (`intelephense`) |
| Ruby (`solargraph`) | Swift (`sourcekit-lsp`) | Dart (`dart language-server`) |
| Zig (`zls`) | Lua (`lua-language-server`) | Bash (`bash-language-server`) |
| SQL (`sql-language-server`) | Markdown (`marksman`) | |

## Install

```bash
git clone https://github.com/pxinxyz/Transcend.git
cd Transcend
cargo build --release          # binary at target/release/transcend
cargo test --workspace         # 154 tests
```

Transcend runs as an MCP server over stdio by default, and as a CLI for language-server
management:

```bash
transcend                                  # MCP server daemon over stdio
transcend lsp status [language]            # audit servers, binaries and recipes
transcend lsp install <language> [-m pkg]  # install via the host package manager
transcend export-schemas --out ./schemas   # dump tool schemas from the live router
```

`export-schemas` reads the running tool router, so the exported schemas cannot drift from
what the server actually serves.

## MCP configuration

Transcend speaks standard `stdio` JSON-RPC 2.0, so any MCP-capable client can host it.

**The easy way: just ask your agent.** MCP registration differs per client and changes
often, so rather than copying a config snippet that will be stale in six months, point
your coding agent at this repository and let it wire itself up:

> Clone `https://github.com/pxinxyz/Transcend`, build it with `cargo build --release`,
> and register `target/release/transcend` as a `transcend` MCP server over stdio.

That works in Claude Code, Cursor, Zed, Codex, Windsurf, DeepSeek Harness, Hermes Agent,
Kimi, and anything else that speaks MCP. If it can read this README, it can do the setup.

The manual path is the usual one for your client: a `mcpServers` entry in
`claude_desktop_config.json` or `~/.claude.json`, an MCP server under
**Settings → Features → MCP** in Cursor and Windsurf, or an entry in the client's own MCP
config file. Type is always `stdio`; the command is the absolute path to the binary.

Either way, confirm 23 `transcend` tools appear — see
[Verifying MCP integration](#verifying-mcp-integration) if they don't.

> **Schema portability.** Tool schemas are rewritten into a portable JSON Schema subset
> (`$ref` inlined; no `$defs`, `$schema`, `type` arrays, `anyOf` or `format`). Some hosts
> enforce a restricted subset and would otherwise reject *every* tool rather than the one
> that offends — which is why a server advertising raw `schemars` output registers zero
> tools there. Handled by the server; nothing to configure.

## Platform support

The execution subsystem is OS-specific, isolated behind `#[cfg(...)]` in
`transcend-core/src/terminal/platform.rs` and `lsp/installer.rs`:

| Concern | Windows | Linux / macOS |
|:---|:---|:---|
| Interactive terminals | ConPTY via `portable-pty` | `openpty` (`setsid` session leader) |
| Process-tree kill | Job Object (`KILL_ON_JOB_CLOSE`), `taskkill /T /F` fallback | Own process group (`setpgid`), then a descendant sweep (`/proc/<pid>/task/*/children`, or `pgrep -P`) |
| Default shell | PowerShell (`pwsh` if present), `cmd /C` when `&&`/`\|\|` is detected | `$SHELL`, else `/bin/bash`, with `-c` |
| PTY line input | `\n` translated to `\r` | `\n` passed through |

**Windows 10 Pro 22H2 (build 19045) is the verified platform.** The Unix branches are
implemented and unit-tested where testable, but their integration paths — process groups,
the `pgrep` sweep, `openpty` behaviour — have not been exercised on a Linux or macOS host.
Treat them as unproven until someone runs the suite there. There is no hosted CI.

`.gitattributes` pins LF repository-wide; without it a Windows checkout stores CRLF in the
working tree while the index holds LF, so `cargo fmt --check` disagrees between machines.

## Architecture

```text
Transcend/
├── crates/
│   ├── transcend-protocol/   # Typed request/response contracts and JSON schemas
│   ├── transcend-core/       # Engines: search, traversal, outline, patch, lsp, terminal
│   ├── transcend-server/     # MCP tool router (rmcp) and stdio serving
│   └── transcend-cli/        # Binary entry point and CLI subcommands
├── benchmarks/               # Measured comparisons against native tooling
├── .gitattributes            # LF line-ending policy
└── Cargo.toml                # Workspace definition
```

## Development

```bash
cargo check --workspace --all-targets     # type-check
cargo test --workspace                    # unit + integration tests
cargo test --workspace --release          # also exercises thin-LTO release codegen
cargo clippy --workspace --all-targets    # must be warning-free
cargo fmt --all --check                   # formatting gate
```

The release profile enables `lto = "thin"` and `codegen-units = 1`, since search and
traversal throughput is the point of the project.

### Verifying MCP integration

`cargo test` covers the protocol over an in-memory duplex transport
(`test_mcp_stdio_protocol_round_trip` drives `initialize` → `tools/list` → `tools/call`).
For a real end-to-end check, `cargo run -p transcend-cli -- export-schemas --out ./schemas`
must produce 23 files; it reads the live router, so a missing or malformed schema there is
a server defect rather than a client one.

Client-specific registration quirks belong in that client's own documentation. One that
has bitten this project: in DeepSeek Harness a profile patch entry must be nested under
`insert:` — a bare top-level entry is an id-targeted *override*, not an insertion, and the
loader skips it with `entry not found`.

## Contributing

Commits follow the [Conventional Commits](https://conventionalcommits.org) specification:

```text
feat(patch): add in-memory AST syntax validation
fix(outline): resolve method receiver binding in Go
docs(readme): update MCP setup instructions
```

## License

[MIT](LICENSE)

<p align="center">
  <sub>Made with &#9829; by <a href="https://github.com/pxinxyz">pxin</a></sub>
</p>
