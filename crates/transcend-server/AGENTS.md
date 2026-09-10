# AGENTS.md — transcend-server

## 1. Purpose
Exposes Transcend computational primitives through the Model Context Protocol (MCP) using the official `rmcp` SDK.

## 2. Ownership
Owns tool definitions, router registrations (`#[tool_router]`), JSON schema negotiation, and transport serving logic.

## 3. Local Contracts
- Implements MCP tools strictly using `rmcp` macros (`#[tool]`, `#[tool_router(server_handler)]`).
- All tool outputs are strongly typed via `rmcp::Json<T>`.
- Does not implement business logic directly; delegates to `transcend-core::Engine`.

## 4. Work Guidance
- Keep tool names, descriptions, and parameter docstrings precise for agent discovery.
- Stdio transport must write all logs to `stderr` to avoid corrupting the JSON-RPC stdio stream.

## 5. Verification
```sh
cargo test -p transcend-server
cargo check -p transcend-server
```

## 6. Child DOX Index
*(empty — no subdirectories registered)*
