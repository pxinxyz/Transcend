# AGENTS.md — transcend-core

## 1. Purpose
In-process implementation of high-performance codebase search, filesystem discovery, AST outlining, and surgical transformations.

## 2. Ownership
Owns execution algorithms, native traversal, ripgrep/grep-searcher integrations, and Tree-sitter AST parsing.

## 3. Local Contracts
- Pure Rust in-process implementation; no shelling out to external Node/CLI subprocesses.
- Search engine (V1): in-process `grep-searcher`, `grep-regex`, and `ignore::WalkBuilder`.
- Binary skipping via NUL byte detection (`BinaryDetection::quit(0x00)`).
- Lossy UTF-8 decoding with CRLF normalization; searching non-UTF-8 text files must never panic.
- Match budget capping (`max_matches`) with `FileCluster` aggregation to prevent context window overflow.
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
