# AGENTS.md — transcend-cli

## 1. Purpose
The executable binary entrypoint for Transcend.

## 2. Ownership
Owns CLI argument parsing, environment initialization, logging setup (stderr), and runtime boot.

## 3. Local Contracts
- Never print logs or messages to `stdout` in default server mode; `stdout` is strictly reserved for JSON-RPC MCP framing.
- In CLI subcommand modes (`transcend lsp status`, `transcend lsp install`), human-readable formatted output is routed to `stdout` with clear status tables and progress indicators.
- All diagnostics, panic hooks, and server debug traces must route to `stderr`.
- Supports CLI subcommands `lsp status [language]` and `lsp install <language> [--method <manager>]`.

## 4. Work Guidance
- Use `cargo run -p transcend-cli` to execute locally.
- Test with MCP inspector or client via stdio pipe.

## 5. Verification
```sh
cargo test -p transcend-cli
cargo check -p transcend-cli
```

## 6. Child DOX Index
*(empty — no subdirectories registered)*
