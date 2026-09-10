# AGENTS.md — transcend-core

## 1. Purpose
In-process implementation of high-performance codebase search, filesystem discovery, AST outlining, and surgical transformations.

## 2. Ownership
Owns execution algorithms, native traversal, ripgrep/grep-searcher integrations, and Tree-sitter AST parsing.

## 3. Local Contracts
- Pure Rust in-process implementation; no shelling out to external Node/CLI subprocesses.
- All errors map into `CoreError`.
- Must satisfy the `Engine` trait.

## 4. Work Guidance
- Use release builds (`cargo test --release`) when verifying search and traversal benchmarks.
- Prioritize memory efficiency and token-aware compaction for results.

## 5. Verification
```sh
cargo test -p transcend-core
cargo check -p transcend-core
```

## 6. Child DOX Index
*(empty — no subdirectories registered)*
