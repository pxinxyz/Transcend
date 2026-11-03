![Transcend](banner.png)

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-4c566a" alt="License: MIT"></a>
  <a href="https://www.conventionalcommits.org/en/v1.0.0/"><img src="https://img.shields.io/badge/Conventional_Commits-1.0.0-4c566a?logo=conventionalcommits&amp;logoColor=white" alt="Conventional Commits"></a>
  <a href="https://doc.rust-lang.org/edition-guide/rust-2024/index.html"><img src="https://img.shields.io/badge/Rust-2024-4c566a?logo=rust&amp;logoColor=white" alt="Rust 2024"></a>
</p>

Transcend is a Rust MCP server for code analysis and file operations.
MCP means Model Context Protocol.
The server returns structured results for search, syntax analysis, file changes, and terminal sessions.

## Features

- Search tools find files, text matches, and symbols.
- Reading tools return file ranges, symbol source, and syntax outlines.
- File tools write, patch, and delete paths within the workspace.
- Patch tools check syntax through Tree-sitter before writing supported source files.
- Language server tools resolve definitions, references, types, and diagnostics.
- Terminal tools start commands, stream output, accept input, and manage sessions.
- The Git tool returns structured repository status.

## Build

The build requires a Rust toolchain that supports the 2024 edition.
Run these commands from the repository root:

```sh
cargo build --locked --release -p transcend-cli
```

Cargo writes the executable to `target/release/transcend`.
Windows adds the `.exe` extension.

Add the executable directory to `PATH` before using the commands below.
Start the server without arguments:

```sh
transcend
```

The server uses standard input and standard output for MCP transport.
Logs use standard error.

## MCP configuration

Configure your MCP client to start the executable over `stdio`.
Use the absolute path to the executable.
The client configuration format depends on the client.

Clients that use an `mcpServers` configuration can use this entry on Windows:

```json
{
  "mcpServers": {
    "transcend": {
      "command": "C:\\path\\to\\transcend.exe"
    }
  }
}
```

The server advertises 23 tools.
The server converts tool schemas to the JSON Schema subset used by its transport adapter.

## Tools

| Area | Tools |
| --- | --- |
| Discovery | `find`, `search`, `find_symbol` |
| Reading | `outline`, `read_symbol`, `read_file` |
| File changes | `write_file`, `patch`, `batch_patch`, `delete_path`, `set_workspace` |
| Language servers | `lsp_definition`, `lsp_references`, `lsp_hover`, `lsp_diagnostics`, `lsp_status`, `lsp_install` |
| Execution | `exec`, `terminal_read`, `terminal_write`, `terminal_resize`, `terminal_kill` |
| Git | `git_status` |

The [server router](crates/transcend-server/src/lib.rs) defines the tool interface.
The [protocol crate](crates/transcend-protocol) defines request and response types.

## Language support

Tree-sitter profiles cover these languages:

- Rust, TypeScript, JavaScript, Python, Go, C, and C++.
- C#, Java, Kotlin, PHP, Ruby, Swift, and Dart.
- Zig, Lua, Bash, SQL, and Markdown.

The [language server registry](crates/transcend-core/src/lsp/registry.rs) defines server discovery and file extensions.
Language server operations require a suitable installed server.
Some operations can use syntax-based fallbacks.
Responses identify the engine that produced the result.

Inspect language server availability:

```sh
transcend lsp status
transcend lsp status rust
```

Install a supported server through an available package manager:

```sh
transcend lsp install rust
```

## Schemas

Export the schemas from the server router:

```sh
transcend export-schemas --out ./schemas
```

The command writes one schema file per advertised tool.
The generated files describe the same tools that the server serves.

## Platform support

The terminal implementation provides Windows and Unix paths.
Windows uses ConPTY and Job Objects for interactive sessions and process ownership.
Unix paths use pseudo-terminals and process groups.

The repository records Windows 10 Pro 22H2 as its verified integration platform.
Linux and macOS integration paths still require execution on those platforms.
The repository does not define hosted CI.

## Workspace

| Crate | Responsibility |
| --- | --- |
| [`transcend-protocol`](crates/transcend-protocol) | Defines requests, responses, and schemas. |
| [`transcend-core`](crates/transcend-core) | Implements search, syntax analysis, file changes, language servers, and terminals. |
| [`transcend-server`](crates/transcend-server) | Exposes the MCP tool router and transport. |
| [`transcend-cli`](crates/transcend-cli) | Starts the server and handles command arguments. |

## Checks

```sh
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

Use release mode when measuring search or traversal performance:

```sh
cargo test --locked --workspace --release
```

The [benchmark directory](benchmarks) contains published model comparisons.
Those reports describe their recorded runs.
They do not establish performance for every repository or workload.

## Contributions

Commits follow [Conventional Commits](conventionalcommits.md).

## License

The [MIT license](LICENSE) covers this project.
