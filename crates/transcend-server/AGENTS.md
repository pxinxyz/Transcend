# AGENTS.md — transcend-server

## 1. Purpose
Exposes Transcend computational primitives through the Model Context Protocol (MCP) using the official `rmcp` SDK.

## 2. Ownership
Owns tool definitions, router registrations (`#[tool_router]`), JSON schema negotiation, and transport serving logic.

## 3. Local Contracts
- Implements MCP tools strictly using `rmcp` macros (`#[tool]`, `#[tool_router]`, `#[tool_handler]`).
- All tool outputs are strongly typed via `rmcp::Json<T>`.
- Exposes core codebase tools (`find`, `search`, `outline`, `find_symbol`, `read_symbol`, `patch`, `batch_patch`, `read_file`, `write_file`, `delete_path`, `set_workspace`, `git_status`) and LSP tools (`lsp_definition`, `lsp_references`, `lsp_hover`, `lsp_diagnostics`, `lsp_status`, `lsp_install`).
- Exposes hybrid terminal tools (`exec`, `terminal_read`, `terminal_write`, `terminal_resize`, `terminal_kill`).
- 23 total native tools registered in the server router.
- Does not implement business logic directly; delegates to `transcend-core::Engine`.
- **Schema portability is mandatory.** Every advertised input schema passes through `schema::normalize_schema`, which strips `$defs`/`$ref`/`$schema`, collapses `type` arrays and `anyOf`, and inlines definitions. Raw `schemars` output is valid JSON Schema and is accepted by the reference `@modelcontextprotocol/sdk` client, but hosts enforcing a restricted subset (DeepSeek Harness validates `type`/`oneOf`/`properties`/`required`/`additionalProperties`/`items`/`enum`/`const` only) reject the *entire tool*. Without normalization this server registers **zero** tools under such a host. `test_tool_definitions_are_complete` asserts the invariants at every nesting depth.
  - Inside a `properties` map, keys are field names, not keywords. Never filter them against a keyword list: `SearchRequest.pattern` is a property named `pattern`, and treating it as the `pattern` keyword silently deleted it from the contract.
  - Every name in `required` must be declared in `properties`, or strict hosts reject the schema.
- **Blocking discipline**: synchronous `Engine` methods (`search`, `find`, `outline`, `read_symbol`, `patch`, `find_symbol`, `read_file`, `write_file`, `delete_path`, `batch_patch`, `set_workspace`) MUST be dispatched through `TranscendServer::offload`, which wraps them in `tokio::task::spawn_blocking`. Calling them inline stalls a runtime worker and starves concurrent `exec` / `terminal_*` / `lsp_*` calls.
- `initialize` must advertise `SERVER_NAME` and `SERVER_INSTRUCTIONS` (identity + usage guidance). The `#[tool_handler]` attribute requires literal strings, so `test_server_info_advertises_identity_and_instructions` asserts the literals match the constants.
- Registered tool schemas must never be restated by hand: `TranscendServer::tool_definitions()` and `write_schemas()` read the live `ToolRouter` so exported schemas cannot drift from what is served.
- `list_tools` is overridden on the `ServerHandler` impl so the wire `tools/list` response carries normalized schemas. `#[tool_handler]` skips generating it when the impl already defines it; overriding the router's `list_all()` alone is NOT enough, because clients read schemas from the wire.
- Tests MUST NOT write outside the build directory or assume a process cwd. The schema-export test writes only into a temp dir; tests using crate-relative fixtures pin the workspace via `server_pinned_to(&crate_dir())`.

## 4. Work Guidance
- Keep tool names, descriptions, and parameter docstrings precise for agent discovery.
- Stdio transport must write all logs to `stderr` to avoid corrupting the JSON-RPC stdio stream.
- `test_mcp_stdio_protocol_round_trip` exercises `initialize` -> `tools/list` -> `tools/call` over an in-memory duplex transport; extend it rather than adding method-level tests when the concern is wire format or schema negotiation.

## 5. Verification
```sh
cargo test -p transcend-server
cargo check -p transcend-server
```

## 6. Child DOX Index
*(empty — no subdirectories registered)*
