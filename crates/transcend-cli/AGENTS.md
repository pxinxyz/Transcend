# AGENTS.md — transcend-cli

## 1. Purpose
The executable binary entrypoint for Transcend.

## 2. Ownership
Owns CLI argument parsing, environment initialization, logging setup (stderr), and runtime boot.

## 3. Local Contracts
- Never print logs or messages to `stdout` in default server mode; `stdout` is strictly reserved for JSON-RPC MCP framing.
- In CLI subcommand modes (`transcend lsp status`, `transcend lsp install`, `transcend export-schemas`), human-readable formatted output is routed to `stdout` with clear status tables and progress indicators.
- All diagnostics, panic hooks, and server debug traces must route to `stderr`.
- Subcommand dispatch is by explicit `match` on the argument slice. Any unrecognized argument (e.g. a host passing `--stdio`) falls through to the MCP daemon, which is the documented default.
- Supports CLI subcommands `lsp status [language]`, `lsp install <language> [--method <manager>]`, and `export-schemas [--out <dir>] [--quiet]`.
- `export-schemas` delegates to `transcend_server::TranscendServer::write_schemas`, which reads the live tool router. It must never restate tool descriptions or schemas.
- Cross-platform: no `std::process::exit` on a success path; exit codes are reserved for genuine failures so CI can branch on them.

## 4. Work Guidance
- Use `cargo run -p transcend-cli` to execute locally.
- Test with MCP inspector or client via stdio pipe.
- Verify schema export with `cargo run -p transcend-cli -- export-schemas --out target/schemas`.

## 5. Verification
```sh
cargo test -p transcend-cli
cargo check -p transcend-cli
```

## 6. Child DOX Index
*(empty — no subdirectories registered)*
