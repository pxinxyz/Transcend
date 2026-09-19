<p align="center">
  <img src="banner.png" alt="Transcend Banner" width="100%">
</p>

<h1 align="center">Transcend</h1>

<p align="center">
  <strong>Deeper Context. Higher Capabilities.</strong><br>
  <em>The High-Performance Native Codebase Intelligence Engine for AI Coding Agents</em>
</p>

<p align="center">
  <a href="https://conventionalcommits.org"><img src="https://img.shields.io/badge/Conventional%20Commits-1.0.0-%23FE5196?logo=conventionalcommits&logoColor=white" alt="Conventional Commits"></a>
  <a href="https://github.com/pxinxyz/Transcend/releases"><img src="https://img.shields.io/github/v/release/pxinxyz/Transcend?color=blue&label=version" alt="Release"></a>
  <a href="https://modelcontextprotocol.io"><img src="https://img.shields.io/badge/MCP-21%20Tools-8A2BE2" alt="MCP Compatible"></a>
  <a href="https://github.com/pxinxyz/Transcend"><img src="https://img.shields.io/badge/tests-111%20passed-brightgreen" alt="Tests"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-2024%20edition-orange?logo=rust" alt="Rust Edition"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License"></a>
</p>

---

## The Paradigm Shift

Modern AI coding agents (Claude Code, Cursor, Windsurf, Zed, Antigravity) are trapped in an obsolete loop inherited from terminal workflows:

1. Execute generic CLI subprocesses (`grep`, `find`, `cat`, `sed`, `head`, `tail`)
2. Ingest megabytes of human-formatted terminal stdout into the prompt
3. Force the model to act as a **text parser, regex filter, and coordinate calculator**
4. Waste 80%+ of the context budget on whitespace and boilerplate before reasoning begins
5. Attempt brittle string replacements that fail on whitespace drift or silently introduce syntax errors

**Transcend fundamentally replaces this paradigm.**

Instead of launching external shell utilities and parsing unstructured strings, Transcend embeds a high-performance **Rust-native codebase intelligence engine** directly into the agent lifecycle via the **Model Context Protocol (MCP)**. 

Transcend replaces blind scraping with **21 typed, AST-aware, LSP-native, terminal execution, and version control primitives** organized across four computational tiers:

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                                       LLM AGENT                                        │
└───────────────────────────────────────────┬────────────────────────────────────────────┘
                                            │ Model Context Protocol (MCP)
                                            ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                                    TRANSCEND ENGINE                                    │
│                                                                                        │
│  [ Tier 1: Codebase Intelligence & File Lifecycle ]                                    │
│    find               Topological Radar & Extension Census                             │
│    search             Clustered Ripgrep Heatmaps & Content Search                      │
│    find_symbol        Smart Casing & Subsequence Definition Finder                     │
│    outline            AST Structural Tree or Syntax-Valid Skeletons                     │
│    read_symbol        Surgical Semantic Symbol Extraction                              │
│    read_file          Line-Bounded Fast Chunked Reader with Binary-Safety              │
│    write_file         Atomic File Creator with Overwrite & Parent Directory Guards     │
│    delete_path        Workspace-Contained File & Recursive Directory Deletion          │
│    patch              AST-Guarded Patching with Splicing Modes (Insert/Prepend/Append) │
│    batch_patch        Consolidated Multi-File Atomic Changeset with Rollback           │
│    set_workspace      Dynamic Workspace Root Anchor & Auto-Resolution                  │
│                                                                                        │
│  [ Tier 2: Language Server Protocol (LSP) ]                                            │
│    lsp_definition     Exact Cross-File Definition Navigation (Tree-sitter Bridge)      │
│    lsp_references     Semantic Multi-File Usage Extraction                             │
│    lsp_hover          Distilled Type Signatures & Cleaned Documentation                │
│    lsp_diagnostics    Auto-Warming Real-Time Compiler Diagnostics (JSON Fallback)      │
│                                                                                        │
│  [ Tier 3: Hybrid Terminal & PTY Execution ]                                           │
│    exec               Hybrid One-Shot Execution with Auto-Detach (Pipe vs PTY)         │
│    terminal_read      Cursor-Based Incremental Output Streaming                        │
│    terminal_write     Interactive Stdin Delivery & Keystroke Injection                 │
│    terminal_resize    PTY Dimension Adjustment (Rows / Cols)                           │
│    terminal_kill      Process Tree Termination (Windows Job Objects / POSIX PGID)      │
│                                                                                        │
│  [ Tier 4: Version Control & Git Lifecycle ]                                           │
│    git_status         Structured Porcelain v2 Git Repository Inspector                 │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Tier 1: Codebase Intelligence Primitives

### 1. `find` — Topological Radar & File Census
Replaces blind `find` / `fd` commands that return overwhelming flat lists.
- **Directory Radar**: Aggregates directory densities (`{"src/auth": 14, "src/db": 8}`) so agents navigate codebases hierarchically.
- **Extension Census**: Summarizes project composition (`{"rs": 45, "ts": 12, "sql": 4}`).
- **Directory Diversity**: Prevents deep subtrees (like `node_modules` or vendor folders) from starving other project areas of visibility.
- **Budget Control**: Hard byte and file limits with deterministic truncated flags.

### 2. `search` — Density Heatmaps & Clustered Search
Replaces raw `grep` / `ripgrep` shell invocations.
- **Hierarchical Clustering**: Groups matches by directory cluster and file, exposing density heatmaps where patterns concentrate.
- **Line Length Compaction**: Automatically truncates minified lines or SVG paths to prevent context window explosion.
- **Multi-Threaded Traversal**: In-process parallel scanning using `ignore` and `grep-regex` matching kernels.

### 3. `find_symbol` — Global Definition Finder
Bridges the gap between content search and surgical inspection.
- **Two-Stage Hybrid Engine**: Parallel ripgrep word-boundary pre-filtering (sub-millisecond across thousands of files) followed by in-process Tree-sitter AST extraction on matching candidate files.
- **Zero File Guesswork**: Resolves symbols by bare name (`SetupVmcsForProcessor`) or qualified path (`Heartbeat::poll`, `Uart::write_byte`) across entire repositories without knowing which file contains the definition.
- **Classification & Coordinates**: Returns exact declaration signatures, docstrings, spans, visibility, and symbol kinds (functions, structs, classes, macros, typedefs, methods, traits).

### 4. `outline` — AST Symbol Hierarchy & Skeletons
Replaces reading full source files just to understand their high-level structure.
- **Format: `tree`**: Deep semantic symbol hierarchy detailing classes, methods, functions, interfaces, structs, macros, and fields with exact byte and line coordinates.
- **Format: `skeleton`**: Generates **100% syntax-valid code stubs** where function/class bodies are pruned to `{ ... }` while preserving all signatures, types, and docstrings.
- **70–90% Token Reduction**: Reduces 1,500-line source files into ~350 tokens of clean, parseable syntax.

### 5. `read_symbol` — Surgical Semantic Extraction
Replaces manual line slicing and coordinate arithmetic (`sed -n 42,88p`).
- **Qualified Locators**: Inspect symbols by simple name (`login`) or qualified hierarchy (`AuthManager::validate_token`).
- **Context Preservation**: Automatically extracts leading docstrings, annotations, attributes, and surrounding comments.
- **Zero Coordinate Math**: The agent never needs to calculate line numbers or offsets to view a function.

### 6. `read_file` — Structured Bounded Line/Byte Reader
Provides safe, token-bounded inspection of arbitrary non-code and configuration assets (`.env`, `.toml`, `.json`, logs).
- **Line-Bounded Slicing**: Requests specific slices (`start_line`, `end_line`) with 1-based line numbering.
- **Binary Detection Safety**: Probes initial bytes for NUL bytes, omitting raw binary blobs to protect agent context.
- **Strict Byte Caps**: Enforces hard byte budgets (`max_bytes`) to prevent catastrophic multi-megabyte prompt dumps.

### 7. `write_file` — Atomic File Creator
Replaces unverified shell file redirects (`cat << 'EOF' > ...` or `Set-Content`).
- **Atomic Disk Swaps**: Writes to a sibling temporary file, flushes to disk with `sync_all()`, and executes an atomic filesystem rename.
- **Collision Protection**: Default `overwrite: false` fails cleanly if a file already exists, preventing accidental overwrites.
- **Directory Provisioning**: Automatically provisions nested parent directories (`mkdir -p`) when `create_parents: true`.

### 8. `delete_path` — Workspace-Contained File & Directory Deletion
Replaces dangerous `rm -rf` commands with safety-governed workspace deletion.
- **Path-Traversal Containment**: Canonicalizes target paths and verifies they remain strictly within the workspace boundary, blocking any `..` escape attacks.
- **Recursion Guard**: Requires explicit `recursive: true` to delete non-empty directory trees.

### 9. `patch` — AST-Guarded Surgical Code Modifier
Replaces brittle regex replacements and unverified diff tools.
- **In-Memory Preflight AST Verification**: Before writing anything to disk, parses the modified buffer with Tree-sitter. If syntax errors or missing tokens (`ERROR` or `MISSING` nodes) are detected, the patch is **rejected immediately with exact diagnostics**.
- **Auto-Healing Indentation**: Dynamically calculates surrounding indentation margins, eliminating indentation mismatch errors common in LLM generations.
- **Splicing Modes**: Supports `replace`, `insert_before`, `insert_after`, `prepend_to_symbol` (injecting into symbol body), and `append_to_symbol` (adding methods/items right before the closing delimiter).
- **Multi-Modal Targeting**: Target changes via `target_symbol` (AST node replacement), `target_span` (precise coordinates), or `target_text` (guarded substring).
- **Unified Diff Output & Dry Runs**: Generates clean unified diffs and supports safe simulation via `dry_run: true`.

### 10. `batch_patch` — Multi-File Transactional Atomic Changesets
Solves the danger of multi-file refactorings leaving repositories in half-broken intermediate states.
- **Workspace-Wide Preflight**: Validates Tree-sitter AST syntax for **all** target files simultaneously before committing any changes.
- **All-or-Nothing Disk Atomicity**: If any single file in the batch fails AST validation or encounter conflicts, zero files are modified on disk.
- **Automated Rollback**: If an I/O error occurs mid-application, automatically restores all previously modified files from in-memory backups.

### 11. `set_workspace` — Workspace Root Configuration & Auto-Resolution
Dynamically sets or anchors the active project directory for all operations.
- **Anchor Detection**: Auto-detects workspace root via `Cargo.toml`, `.git`, or `package.json` boundaries when unset.
- **Seamless Path Resolution**: All path-based tools (`search`, `find`, `outline`, `patch`, `read_file`, `exec`) resolve relative paths against the active workspace root.

---

## Tier 2: Language Server Protocol (LSP) Subsystem

Transcend integrates official, standardized language servers over asynchronous stdio JSON-RPC 2.0 and distills responses into token-compact models for agents.

```
┌───────────────────────────┐         ┌───────────────────────────────────┐
│   Tree-sitter Bridge      │         │     Token Distiller & Compactor   │
│  (Symbol -> Line/Col)     │         │  (Prune JSON Bloat, Format Spans) │
└─────────────┬─────────────┘         └─────────────────▲─────────────────┘
              │                                         │
              ▼                                         │
┌───────────────────────────────────────────────────────┴─────────────────┐
│                         LspSessionPool                                  │
│  - Auto-Discovery (PATH detection, Workspace root discovery)            │
│  - Process Lifecycle (Spawn, Handshake, Document Sync didOpen/didChange)│
│  - Graceful Fallback (Tree-sitter heuristics when server is uninstalled)│
└─────────────────────────────────────┬───────────────────────────────────┘
                                      │ stdio JSON-RPC 2.0
           ┌──────────────────────────┼──────────────────────────┐
           ▼                          ▼                          ▼
   [ rust-analyzer ]              [ gopls ]                  [ clangd ]
```

### 12. `lsp_definition` — Cross-File Semantic Definition Navigation
- **Tree-sitter Coordinate Bridge**: Accepts symbol queries (`"TerminalEngine::exec"`) and maps them to 0-based `(line, col)` coordinates in microseconds before querying the language server.
- **Semantic Resolution**: Accurately resolves type aliases, trait implementations, macros, and imports across modular boundaries.
- **Zero Crashes via Heuristics**: If a language server binary is not installed locally, Transcend gracefully falls back to Tree-sitter heuristics.

### 13. `lsp_references` — Multi-File Semantic Usages
- **True References**: Distinguishes semantic references from substring collisions, variable shadows, or commented-out code.
- **Clustered Preview**: Returns occurrences grouped by file path with snippet previews and exact line/character coordinates.

### 14. `lsp_hover` — Distilled Type Signatures & Docstrings
- **Distilled Intelligence**: Strips raw HTML, unformatted markdown, and verbose JSON protocol wrappers into clean function signatures, type bounds, and docstrings.
- **Immediate Context**: Inspect complex generic signatures and traits without having to navigate away to the source definition.

### 15. `lsp_diagnostics` — Compiler & Typechecker Diagnostics
- **Live Background Stream**: Subscribes to `textDocument/publishDiagnostics` published by language servers during session edits.
- **Precise Filtering**: Filters diagnostics by file path and severity (`Error`, `Warning`, `Information`, `Hint`).

---

## Tier 3: Hybrid Terminal & PTY Execution Subsystem

Transcend solves the fundamental dilemma of agent shell execution by decoupling **transport** (`pipe` vs. `pty`) from **lifecycle** (`blocking` vs. `detached`).

```
                              ┌───────────────────────────────────┐
                              │            LLM Agent              │
                              └─────────────────┬─────────────────┘
                                                │
                 ┌──────────────────────────────┴──────────────────────────────┐
                 │                                                             │
                 ▼                                                             ▼
       [ Routine / Batch CLI ]                                      [ Interactive / TTY CLI ]
    cargo check, git status, find                                   npm run dev, python -i, REPL
                 │                                                             │
                 ▼                                                             ▼
      Transport: PipeTransport                                     Transport: PtyTransport
    (tokio::process async pipes)                                  (native ConPTY / openpty)
                 │                                                             │
                 └──────────────────────────────┬──────────────────────────────┘
                                                │
                                                ▼
                                    ┌───────────────────────┐
                                    │   CursorRingBuffer    │
                                    │  (Fixed-size circular │
                                    │   monotonic stream)   │
                                    └───────────┬───────────┘
                                                │
                                                ▼
                                   Process Tree Governance
                           Windows Job Objects / POSIX PGID
                                                │
                        ┌───────────────────────┴───────────────────────┐
                        ▼                                               ▼
         [ Exited within timeout_ms ]                     [ Still running at timeout_ms ]
           Return status: "exited"                          TimeoutAction::Detach (default)
           output, exit_code, cursor                        Return status: "detached", session_id
```

### 16. `exec` — Hybrid Execution with Auto-Detach
- **Fast Path (Single Turn)**: Routine commands that exit within `timeout_ms` (e.g. `cargo check`, `git status`) return immediate exit codes and output in a single tool turn without multi-turn polling overhead.
- **Auto-Detach (Long-Running Tasks)**: Long-running servers (`npm run dev`, `cargo watch`, Python REPLs) automatically detach without hanging the turn, returning a persistent `session_id`.
- **Decoupled Transport**: Automatically uses `pipe` for quiet batch utilities and `pty` (ConPTY / openpty) for TTY-aware tools.
- **Direct Raw Mode**: Supports `raw: true` for direct binary execution avoiding shell interpretation.

### 17. `terminal_read` — Cursor-Based Incremental Output Streaming
- **Monotonic Cursor**: Accepts `cursor: usize` and returns `next_cursor: usize`. Subsequent turns read only new output, eliminating repetitive context-wasteful re-reads.
- **Pattern Await**: Optional `wait_for_pattern` waits for specific terminal prompts (e.g. `Ready on http://localhost`) before returning.
- **CR (`\r`) Line Folding**: Interactive CLI progress bars, spinners, and download counters are folded to their final state in-place, slashing token usage by up to 90%.
- **ANSI Sanitization**: In-process terminal escape sequence stripping via `strip-ansi-escapes`.
- **Ring Buffer Bounds**: 1 MB bounded circular buffer with head/tail slicing protects against out-of-memory crashes on runaway output.

### 18. `terminal_write` — Interactive Stdin Delivery
- Injects keystrokes, commands, and interactive responses (`y\n`, Ctrl+C `\x03`) into active detached sessions and REPLs.

### 19. `terminal_resize` — PTY Dimension Control
- Adjusts pseudo terminal column and row geometry (`cols`, `rows`) to adapt output layouts for CLI dashboards and TUIs.

### 20. `terminal_kill` — Process Tree Termination
- **Windows Job Objects**: Binds process trees to kernel Job Objects with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, ensuring that complex child/grandchild processes (`node.exe`, `vite.exe`) are killed atomically without port leaks.
- **Asynchronous PTY Disposal**: Cleans up ConPTY pseudo console handles on dedicated background worker threads to prevent Win32 synchronous pipe-drain deadlocks.

---

## Tier 4: Version Control & Git Lifecycle

### 21. `git_status` — Structured Porcelain v2 Git Inspector
Replaces terminal string scraping like `git status -s` with strongly-typed, token-compact JSON.
- **Full State Breakdown**: Categorizes modified files into `staged`, `unstaged`, `untracked`, and `conflicted` lists.
- **Branch & Tracking Awareness**: Extracts active branch (`branch`), tracking upstream (`upstream`), and precise commit divergence (`ahead`, `behind`).
- **Clean Indicator**: `is_clean: true` allows agents to instantly verify repository state before or after batch modifications.

---

## Polyglot AST & Language Server Engine (18 Languages)

Transcend features native, compiled **Tree-sitter** grammar parsers paired with seamless language server auto-discovery for 18 industry-standard languages:

| Language | Tree-sitter Grammar | Official Language Server | Key Symbols Extracted |
|:---|:---|:---|:---|
| **Rust** | `tree-sitter-rust` | `rust-analyzer` | Functions, Impls, Structs, Enums, Traits, Macros, Consts |
| **TypeScript / JS** | `tree-sitter-typescript` | `typescript-language-server` | Classes, Interfaces, Methods, Enums, Types, Functions |
| **Python** | `tree-sitter-python` | `pyright` / `ruff` | Classes, Methods, Functions, Decorated Items, Docstrings |
| **Go** | `tree-sitter-go` | `gopls` | Functions, Methods (with Receivers), Types, Interfaces, Structs |
| **C / C++** | `tree-sitter-c` / `cpp` | `clangd` | Classes, Namespaces, Methods, Templates, Macros |
| **C#** | `tree-sitter-c-sharp` | `csharp-ls` | Classes, Interfaces, Records, Structs, Methods, Namespaces |
| **Java** | `tree-sitter-java` | `jdtls` | Classes, Interfaces, Records, Enums, Methods |
| **Kotlin** | `tree-sitter-kotlin-ng` | `kotlin-language-server` | Classes, Objects, Functions, Interfaces, Companion Objects |
| **PHP** | `tree-sitter-php` | `intelephense` | Classes, Interfaces, Traits, Enums, Functions, Methods |
| **Ruby** | `tree-sitter-ruby` | `solargraph` | Classes, Modules, Methods, Singleton Methods |
| **Swift** | `tree-sitter-swift` | `sourcekit-lsp` (or Heuristic) | Classes, Structs, Protocols, Enums, Extensions, Functions |
| **Dart** | `tree-sitter-dart` | `dart language-server` | Classes, Mixins, Enums, Extensions, Methods, Functions |
| **Zig** | `tree-sitter-zig` | `zls` | Functions, Structs, Tests, Enums, Unions |
| **Lua** | `tree-sitter-lua` | `lua-language-server` | Global & Local Functions, Table Methods |
| **Bash / Shell** | `tree-sitter-bash` | `bash-language-server` | Shell Functions, Aliases |
| **SQL** | `tree-sitter-sequel` | `sql-language-server` | Create Table, Create View, Stored Procedures, Functions |
| **Markdown** | `tree-sitter-md` | `marksman` | Document Headings (H1–H6) |

---

## Token Efficiency Benchmarks

Empirical context consumption comparisons measured across popular open-source repositories:

| Target File / Operation | Standard CLI / Cat Workflow | Transcend Primitive | Token Savings |
|:---|:---|:---|:---|
| **Flask `app.py`** (Explore structure) | 1,840 lines (`cat`) &rarr; **~7,400 tokens** | `outline(skeleton)` &rarr; **820 tokens** | **88.9% reduction** |
| **Gin `gin.go`** (Inspect `Engine` type) | 680 lines (`cat`) &rarr; **~3,100 tokens** | `read_symbol("Engine")` &rarr; **290 tokens** | **90.6% reduction** |
| **Hypervisor `VMCS.c`** (Inspect function) | 420 lines (`view_file`) &rarr; **~2,800 tokens** | `read_symbol("SetupVmcs")` &rarr; **310 tokens** | **88.9% reduction** |
| **FastAPI codebase** (Repository map) | Recursive `fd` &rarr; **~9,200 tokens** | `find(radar=true)` &rarr; **440 tokens** | **95.2% reduction** |
| **Cross-file Symbol Jump** (Find declaration) | Multiple `grep` + file reads &rarr; **~4,500 tokens** | `lsp_definition` &rarr; **180 tokens** | **96.0% reduction** |
| **Build output / Progress spinner** | Terminal raw logs &rarr; **~5,200 tokens** | `terminal_read` (CR folded) &rarr; **410 tokens** | **92.1% reduction** |

---

## Quickstart & Installation

### Build from Source

```bash
# Clone the repository
git clone https://github.com/pxinxyz/Transcend.git
cd Transcend

# Build the release binary
cargo build --release

# The compiled binary is located at target/release/transcend
```

Run test suite to verify:
```bash
cargo test --workspace
```

---

## Model Context Protocol (MCP) Configuration

Transcend communicates over standard `stdio` JSON-RPC 2.0. Add it to your agent or editor configuration:

### Claude Desktop
Add to `claude_desktop_config.json`:
```json
{
  "mcpServers": {
    "transcend": {
      "command": "/path/to/transcend",
      "args": []
    }
  }
}
```

### Claude Code CLI
Add to `~/.claude.json`:
```json
{
  "mcpServers": {
    "transcend": {
      "command": "/path/to/transcend"
    }
  }
}
```

### Cursor & Windsurf
Add a new MCP server in **Settings &rarr; Features &rarr; MCP**:
- **Name**: `transcend`
- **Type**: `stdio`
- **Command**: `/path/to/transcend`

### Windows Example
```json
{
  "mcpServers": {
    "transcend": {
      "command": "C:\\Projects\\General Workspace\\Idea\\Transcend\\target\\release\\transcend.exe"
    }
  }
}
```

---

## Project Architecture

Transcend is designed with strict boundaries and zero unnecessary dependencies:

```text
Transcend/
├── crates/
│   ├── transcend-protocol/   # Strongly-typed schemas, JSON-RPC contracts, and request/response models
│   ├── transcend-core/       # Core computational engines:
│   │   ├── find.rs / search.rs  # Ripgrep-grade search, directory radar, and diversity sampling
│   │   ├── find_symbol.rs       # Smart casing & token subsequence definition finder
│   │   ├── outline/             # 18 Tree-sitter parsers, semantic hierarchy, and syntax skeletonizer
│   │   ├── patch.rs             # In-memory preflight AST verification, splicing, and batch changesets
│   │   ├── file_ops.rs          # Bounded line/byte reader, atomic file creator, and contained deletion
│   │   ├── git_ops.rs           # In-process porcelain v2 git status inspector
│   │   ├── lsp/                 # LSP stdio JSON-RPC pool, coordinate bridge, and token distillation
│   │   └── terminal/            # Hybrid PTY/Pipe execution, ring buffer, and Job Object process trees
│   ├── transcend-server/     # High-throughput asynchronous MCP stdio server daemon (21 tools)
│   └── transcend-cli/        # Binary entry point and CLI runner
├── banner.png                # Transcend visual identity
└── Cargo.toml                # Workspace definition
```

---

## Contributing

Transcend follows the [Conventional Commits](https://conventionalcommits.org) specification.

```text
<type>[optional scope]: <description>

feat(patch): add in-memory AST syntax validation
fix(outline): resolve method receiver binding in Go
docs(readme): update MCP setup instructions
```

---

## License

This project is licensed under the [MIT License](LICENSE).

<p align="center">
  <sub>Made with &#9829; by <a href="https://github.com/pxinxyz">pxin</a></sub>
</p>
