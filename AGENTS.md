# AGENTS.md — Transcend

## 1. Purpose
High-performance, agent-native computational primitives and Model Context Protocol (MCP) server written in pure Rust. Replaces crude CLI text scraping with typed, token-compact structured data.

## 2. Ownership
Owns the Transcend Cargo workspace and its four constituent crates:
- `crates/transcend-protocol`
- `crates/transcend-core`
- `crates/transcend-server`
- `crates/transcend-cli`

## 3. Local Contracts
- Governed by root `AGENTS.md` directives.
- **Conventional Commits**: All git commit messages MUST strictly adhere to the [Conventional Commits v1.0.0](conventionalcommits.md) specification (see §4).
- `LEGACY/` is gitignored and reserved for untracked legacy reference materials (§14.5 containment).
- `rust-sdk/` is gitignored and reserved for local SDK reference (official `modelcontextprotocol/rust-sdk` clone).
- `IDEAS/` is gitignored and reserved for parked design notes and speculative work. Nothing in it is committed, so treat it as local scratch: it is not a contract, not reviewed, and may contradict the code.
- In-process native Rust implementations; zero external script/CLI binary runtime dependencies.

## 4. Conventional Commits Specification
All commits in this repository must strictly adhere to the [Conventional Commits 1.0.0](conventionalcommits.md) standard located at [`conventionalcommits.md`](file:///c:/Projects/General%20Workspace/Idea/Transcend/conventionalcommits.md).

- **Format**:
  ```text
  <type>[optional scope]: <description>

  [optional body]

  [optional footer(s)]
  ```
- **Allowed Types**:
  - `feat`: adds a new user/agent-facing feature (correlates with SemVer `MINOR`).
  - `fix`: patches a bug in the codebase (correlates with SemVer `PATCH`).
  - `build`: changes that affect the build system or external dependencies (e.g. Cargo, workspace manifests).
  - `chore`: maintenance, tooling setup, routine housekeeping with no production code change.
  - `docs`: documentation changes only (AGENTS.md, markdown files).
  - `refactor`: code change that neither fixes a bug nor adds a feature.
  - `perf`: code change that improves performance.
  - `test`: adding missing tests or correcting existing tests.
  - `ci`: changes to CI configuration files and scripts.
- **Breaking Changes**:
  - Indicated with `!` before the colon (e.g. `feat(protocol)!: ...`) or a footer `BREAKING CHANGE: <description>` (correlates with SemVer `MAJOR`).
- **Description**:
  - Short summary in imperative, present tense ("add", not "added" or "adds").
  - Lowercase, no ending period.
- **Body & Footers**:
  - For substantive changes, include a multi-line body separated by a blank line explaining motivation and architecture.

## 5. Work Guidance
- Use `cargo test --workspace` and `cargo check --workspace` before every commit.
- Use release mode (`cargo test --release`) when verifying search and traversal throughput.
- Clippy and rustfmt are gates, not suggestions: run `cargo clippy --workspace --all-targets` (treat warnings as errors) and `cargo fmt --all --check` before committing. They are not enforced by CI in this repository, so they must be run deliberately.

## 6. Verification
```sh
cargo check --workspace --all-targets
cargo test --workspace
cargo test --workspace --release
cargo clippy --workspace --all-targets
cargo fmt --all --check
```

## 7. Cross-Platform Contract
- **There is no hosted CI.** Verification is manual, on the platform you have. Do not claim a change works on an OS you have not run it on.
- Windows 10 Pro 22H2 (build 19045) is the verified platform. The Unix branches are implemented and unit-tested where testable, but process groups, the `pgrep` descendant sweep and `openpty` have not been exercised on Linux or macOS.
- Platform-specific code belongs behind `#[cfg(...)]` in as few modules as possible: `transcend-core/src/terminal/platform.rs` (shell resolution, process-tree ownership) and `transcend-core/src/lsp/installer.rs` (package-manager recipes).
- `.gitattributes` pins `eol=lf` repository-wide. Do not commit CRLF; it makes `cargo fmt --check` disagree between machines and every diff noisy.
- No test may depend on the process working directory, write outside the build directory, or hardcode a machine-specific absolute path.

## 8. Child DOX Index
- `crates/transcend-protocol/AGENTS.md` — scope: strongly-typed request/response data contracts and JSON schemas
- `crates/transcend-core/AGENTS.md` — scope: in-process search, traversal, and AST computation engines
- `crates/transcend-server/AGENTS.md` — scope: Model Context Protocol (rmcp) tool router and server implementation
- `crates/transcend-cli/AGENTS.md` — scope: CLI binary entrypoint and stdio transport execution
