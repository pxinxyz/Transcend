# AGENTS.md — transcend-protocol

## 1. Purpose
Defines strongly-typed data contracts (requests, responses, error representations) for Transcend MCP tools.

## 2. Ownership
Owns all serializable protocol types, JSON Schema definitions (`schemars::JsonSchema`), and boundary contract validation.

## 3. Local Contracts
- Every public request/response type must derive `serde::Serialize`, `serde::Deserialize`, and `schemars::JsonSchema`.
- Owns data contracts for codebase primitives, LSP operations (`LspStatusRequest`, `LspStatusResponse`, `LspInstallRequest`, `LspInstallResponse`), terminal execution, workspace lifecycle (`SetWorkspaceRequest`, `SetWorkspaceResponse`), and version control (`GitStatusRequest`, `GitStatusResponse`, `GitFileStatus`, `GitFileEntry`).
- Traversal options (`SearchOptions`, `FindOptions`, `OutlineOptions`) expose **two independent axes**: `include_hidden` (dotfiles, default `false`) and `respect_gitignore` (default `true`). They must never be collapsed into one flag — an agent searching vendored or generated code needs the former off without the latter.
- Mutating requests (`WriteFileRequest`, `PatchRequest`, `BatchPatchRequest`, `DeletePathRequest`) carry an optional `workspace_root` boundary. It is a security boundary, not a hint: the engine resolves it to the active workspace root and refuses paths that escape it.
- Zero business logic: only data definitions, validation helpers, and contract fixtures.
- Pure Rust, zero I/O or network dependencies.

## 4. Work Guidance
- Keep symbol and field names discoverable and agent-intuitive.
- Ensure all docstrings clearly describe input constraints and output guarantees for MCP schema generation.
- Adding a field to a request struct is a breaking change for exhaustive struct literals in tests across the workspace; prefer `..Default::default()` in fixtures.

## 5. Verification
```sh
cargo test -p transcend-protocol
cargo check -p transcend-protocol
```

## 6. Child DOX Index
*(empty — no subdirectories registered)*
