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

## 6. Verification
```sh
cargo check --workspace
cargo test --workspace
```

## 7. Child DOX Index
- `crates/transcend-protocol/AGENTS.md` — scope: strongly-typed request/response data contracts and JSON schemas
- `crates/transcend-core/AGENTS.md` — scope: in-process search, traversal, and AST computation engines
- `crates/transcend-server/AGENTS.md` — scope: Model Context Protocol (rmcp) tool router and server implementation
- `crates/transcend-cli/AGENTS.md` — scope: CLI binary entrypoint and stdio transport execution
