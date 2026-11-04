# Transcend agent instructions

## Scope

Transcend is a Rust MCP server for code analysis, file operations, language servers, and terminal sessions.
The workspace uses Rust 2024.
Read [README.md](README.md) before changing public behavior.
Read the relevant crate's `AGENTS.md` before editing that crate.

## Repository map

| Crate | Responsibility |
| --- | --- |
| [transcend-protocol](crates/transcend-protocol/AGENTS.md) | Requests, responses, validation types, and schemas. |
| [transcend-core](crates/transcend-core/AGENTS.md) | Search, syntax analysis, file changes, language servers, terminals, and Git. |
| [transcend-server](crates/transcend-server/AGENTS.md) | MCP routing, schema normalization, and transport integration. |
| [transcend-cli](crates/transcend-cli/AGENTS.md) | Executable startup, logging, and CLI subcommands. |

## Implementation rules

- Keep search, traversal, and syntax analysis in Rust.
- Use the existing subprocess integrations for Git, compilers, language servers, and terminal commands.
- Keep protocol definitions in `transcend-protocol` and algorithms in `transcend-core`.
- Route synchronous engine work through the server's blocking offload mechanism.
- Keep default server-mode standard output reserved for MCP traffic. Send diagnostics to standard error.
- Preserve documented CLI subcommand output.
- Derive served and exported schemas from the live tool router.
- Preserve schema normalization, required properties, and nested schema compatibility.
- Preserve independent `include_hidden` and `respect_gitignore` options.
- Enforce workspace boundaries before every filesystem mutation.
- Preserve canonical path checks, symlink handling, and checks for targets that do not exist yet.
- Keep batch patches cumulative and validate them before writing.
- Preserve terminal process ownership, cancellation, output cursors, and interactive input behavior.
- Keep Windows and Unix behavior behind the existing platform boundaries.

## Validation

Run these checks from the repository root before committing:

```sh
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
```

For search or traversal performance changes:

```sh
cargo test --locked --workspace --release
```

For schema or transport changes, run the server tests and inspect exported schemas:

```sh
cargo test --locked -p transcend-server
cargo run --locked -p transcend-cli -- export-schemas --out target/schemas
```

Extend the existing stdio round-trip test when changing wire behavior.
Test workspace escapes and symlinks when changing filesystem mutations.
Test interactive sessions and process cleanup when changing terminal behavior.
Keep test artifacts under the build directory.
Tests must not depend on their starting directory or machine-specific paths.

## Repository hygiene

- Preserve LF line endings.
- Keep `LEGACY`, `rust-sdk`, `IDEAS`, `benches`, and `target` untracked.
- Treat local design notes as proposals, not implementation contracts.
- Reserve [benchmarks](benchmarks) for published model comparisons.
- Keep repository audits and verification harnesses in the ignored `benches` directory.
- Use [Conventional Commits](conventionalcommits.md).
- Push or publish only when the task authorizes it.

## Reporting

The repository has no hosted CI.
Record the platform, commands, and results used for verification.
Windows 10 Pro 22H2 is the recorded integration baseline.
Do not describe Linux or macOS integration as verified without running it there.
Keep benchmark claims tied to their recorded workload and build profile.
