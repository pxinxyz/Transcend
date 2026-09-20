# AGENTS.md — transcend-core

## 1. Purpose
In-process implementation of high-performance codebase search, filesystem discovery, AST outlining, and surgical transformations.

## 2. Ownership
Owns execution algorithms, native traversal, ripgrep/grep-searcher integrations, and Tree-sitter AST parsing.

## 3. Local Contracts
- Pure Rust in-process implementation; no shelling out to external Node/CLI subprocesses.
- Search engine (V3): in-process `grep-searcher`, `grep-regex`, and multi-threaded `ignore::WalkParallel`.
- Thread-local `Searcher` instances per worker with lockless atomic match accounting (`AtomicUsize`).
- Adaptive cross-file diversity sampling: `max_per_file` prevents monster files from starving search results.
- Unicode-safe line length bounding: `max_line_length` (default 500) truncates oversized lines with character counts.
- Macro-level Directory Radar: generates `DirectoryCluster` aggregates grouped by directory and sorted by density.
- Context lines: optional `context_lines` captures `context_before` and `context_after` around matches.
- Deterministic result ordering: matches sorted by `(file, line_number)`, clusters sorted by `match_count` descending.
- Binary skipping via NUL byte detection (`BinaryDetection::quit(0x00)`).
- Lossy UTF-8 decoding with CRLF normalization; searching non-UTF-8 text files must never panic.
- Match budget capping (`max_matches`) with `FileCluster` aggregation and optional empty cluster pruning (`max_empty_clusters`).
- Ignore control: `respect_gitignore` (default `true`) is an independent axis from `include_hidden` across `search`, `find`, and `outline`. Disabling it must NOT pull in hidden files or `.git/`.
- Workspace root management (`workspace_root`, `set_workspace`) with anchor detection (`Cargo.toml`, `.git`, `package.json`) and automatic relative path resolution across all operations.
- Anchor discovery (`outermost_anchor`) climbs to the **outermost** project marker, not the nearest, so a Cargo workspace is not silently scoped to a single crate.
- `resolve_path` accepts `None`, `""`, and `"."` as "the workspace root"; absolute paths pass through unchanged.
- **Workspace boundary enforcement**: every mutating primitive (`write_file`, `patch`, `batch_patch`, `delete_path`) runs `ensure_within` before touching disk. Comparison is on canonicalized paths (symlinks resolved); a not-yet-created target is checked via its nearest existing ancestor. Unresolvable boundaries fail closed.
- Transactional cumulative batch patching with in-memory buffer threading, consolidated single-file unified diff generation, and single-write disk commits.
- Indentation-aware splicing for `prepend_to_symbol` and `append_to_symbol`.
- Streaming line reader in `read_file` with $O(1)$ memory overhead and fast 64KB raw chunk newline counting once `end_line` is reached.
- Smart casing and token subsequence matching in `find_symbol` across snake_case $\leftrightarrow$ camelCase/PascalCase boundaries.
- In-process native `GitEngine` parsing `git status --porcelain=v2 --branch` into typed, token-compact JSON.
- Native compiler JSON diagnostics fallback (`cargo check --message-format=json`) when LSP language servers are absent.
- Dynamic LSP discovery across system `PATH` and toolchain directories (`~/.cargo/bin`, `%APPDATA%\npm`, `~/.transcend/bin`, `~/go/bin`, `~/.local/bin`) with zero hardcoded paths.
- LSP root markers may be literal filenames (`Cargo.toml`) or globs (`*.sln`, `*.csproj`); glob markers are matched against directory entries, case-insensitively.
- LSP session pool never holds its map lock across a cold spawn: the first caller publishes a `Spawning` placeholder, and concurrent callers await it instead of launching a duplicate server. Idle sessions are reclaimed after `IDLE_SESSION_TTL`.
- Recipe-based language server status auditing (`lsp_status`) and automated host package manager installation (`lsp_install`) with bounded execution timeouts and version verification.
- All errors map into `CoreError`.
- Must satisfy the `Engine` trait.
- Terminal subsystem (`terminal::TerminalEngine`):
  - Decoupled transport (`PipeTransport` for fast one-shot/quiet CLI, `PtyTransport` for interactive/TTY CLI via `portable-pty`).
  - Decoupled lifecycle (`blocking` for one-shot exit within `timeout_ms`, `detached` returning persistent `session_id`).
  - Process tree governance: Windows Job Objects (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) with root process and `taskkill /T /F` fallback; POSIX own process group (`setpgid` via `process_group(0)` for pipe children, `setsid` for PTY children) signalled with SIGTERM then SIGKILL, followed by a descendant sweep (`/proc/<pid>/task/*/children` on Linux, `pgrep -P` elsewhere).
  - Interactive-command detection keys off the resolved program token, never substrings elsewhere in the command line.
  - Asynchronous PTY disposal on dedicated background worker thread to prevent `ClosePseudoConsole` deadlocks during pipe drain.
  - Cursor-based ring buffer (`CursorRingBuffer`) with monotonic read cursor and head/tail byte slicing.
  - Terminal projection (`TerminalProjection`): ANSI escape code stripping and CR (`\r`) in-place line folding.

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
