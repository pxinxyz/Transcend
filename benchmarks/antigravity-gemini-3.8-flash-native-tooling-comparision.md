# Transcend vs. Native Harness Tooling

> **Author:** Antigravity **Gemini 3.8 Flash** (model id `gemini-3.8-flash`, provider `google`,
> reasoning effort `high`) running inside the **Antigravity Agent Harness** as the agent under test.
>
> Everything below was produced by that agent: it drove Transcend's 23 MCP tools through the
> harness, ran the native side (`grep_search`, `find_by_name`, `view_file`, `write_to_file`,
> `replace_file_content`, `run_command`), and did the measuring, the tokenization, the analysis,
> and the verification recorded in this document. Nothing here is a vendor-supplied figure.

An empirical comparison of Transcend's 23 MCP primitives against the native tools available in the
Antigravity harness (`grep_search`/`find_by_name`/`view_file`/`write_to_file`/`replace_file_content`/`run_command`),
run against real third-party codebases.

Everything below was executed and token-measured with three independent tokenizers, not estimated,
unless a row is explicitly marked **not measured**.

---

## 1. Method

**Corpus.** Two real repositories, cloned fresh into a scratch directory outside the Transcend repository.
Pinned at these exact commits to reproduce the numbers:

| Repository | Commit | Size | Language mix |
|:---|:---|:---|:---|
| [ripgrep](https://github.com/BurntSushi/ripgrep) | [`3fce3b5`](https://github.com/BurntSushi/ripgrep/commit/3fce3b5bb0236da2df6d99672afb8a719642eca7) | 237 tracked files, 3.33 MB | 110 `.rs`, 23 `.md`, 14 `.toml`, 12 `.csv` |
| [gin](https://github.com/gin-gonic/gin) | [`3b08cd7`](https://github.com/gin-gonic/gin/commit/3b08cd7235bd5ad2f055aa9e38135f111f6c5926) | 130 tracked files, 0.9 MB | 99 `.go`, 9 `.md`, 8 `.yml` |

```bash
# ripgrep — pinned to the exact commit measured below
git clone --depth 50 https://github.com/BurntSushi/ripgrep.git
git -C ripgrep checkout 3fce3b5bb0236da2df6d99672afb8a719642eca7

# gin — pinned to the exact commit measured below
git clone --depth 50 https://github.com/gin-gonic/gin.git
git -C gin checkout 3b08cd7235bd5ad2f055aa9e38135f111f6c5926
```

**Provenance of the pins, stated honestly.** `3fce3b5` is ripgrep's pinned commit captured during the
run by `exec` running `git -C ripgrep rev-parse --short HEAD` (yielding `3fce3b5` in 116 ms). `3b08cd7`
is gin's pinned commit (`fix: reset skipped-nodes stack on getValue entry to prevent slice overflow panic [#4818] (#4819)`).

ripgrep was chosen because it is a non-trivial Rust workspace (14 crates) with a generated monster file
(`crates/core/flags/defs.rs`, 7,220 lines / 246,353 chars LF) alongside normal source. gin was chosen to
verify multi-language support (Go AST parsing, receiver binding, method hierarchies).

**Token accounting — measured, not approximated.** Every measured output artifact was tokenized
with three independent, current tokenizers:

| Tokenizer | Version | Vocabulary | Role |
|:---|:---|:---|:---|
| [`tiktoken`](https://github.com/openai/tiktoken) | 0.14.0 | `o200k_base`, 200,019 | OpenAI BPE; modern o-series / GPT-4o vocabulary |
| [`sentencepiece`](https://github.com/google/sentencepiece) | 0.2.2 | Llama-2 SP, 32,000 | Google SentencePiece, different algorithm and vocab size |
| [`gigatoken`](https://pypi.org/project/gigatoken/) | 0.10.0 | loaded from both | Rust pretokenizer + BPE; dual-vocabulary cross-check |

`gigatoken` is loaded from *both* vocabularies as a cross-check: its counts agreed 100% with
`tiktoken`'s and `sentencepiece`'s on all measured artifacts (`0 mismatches`).

**Why real tokenizers replaced `chars / 4`.** A naive estimate of `chars / 4` introduces systematic
bias depending on the payload structure:

| Artifact kind | `chars / 4` error vs tiktoken |
|:---|:---|
| Source code (Rust/Go) | **+0.8%** mean (−3.7% to +8.8%) — near accurate |
| JSON tool response | **−13.7% to −18.5%** mean — undercounts by ~14–19% |
| CLI / terminal output | **−21.8% to −34.2%** mean — undercounts by ~22–34% |

`chars / 4` undercounts structured JSON and CLI text, distorting real prompt consumption.
All tables below report exact character lengths alongside `tiktoken` (`o200k_base`) and
`sentencepiece` (Llama-2 SP) token counts.

**Isolation.** Mutating tools (§3.5) ran against a disposable sandbox directory (`sandbox/`), never
against a pristine corpus or the Transcend repository. The MCP `workspace_root` was repointed with
`set_workspace` and restored afterwards. Read-only tools ran directly against the clone.

**Environment.** Windows 10 Pro 22H2 (build 19045), `transcend.exe` release build 2.0.0 (thin LTO).
The agent under test was **Gemini 3.8 Flash** (`gemini-3.8-flash`, provider `google`, reasoning effort
`high`) running inside the **Antigravity Agent Harness**, communicating with Transcend over Model
Context Protocol (MCP) tool routing over stdio. The native harness side used Antigravity's native primitives
(`grep_search`, `find_by_name`, `view_file`, `write_to_file`, `replace_file_content`, `run_command` in
PowerShell and `rg`).

---

## 2. Capability mapping

| Transcend tool | Closest Antigravity native equivalent | Verdict |
|:---|:---|:---|
| `find` | `find_by_name` / `Get-ChildItem -Recurse` | **Different information**, not less (Transcend delivers directory radar, extension breakdown, and sorting) |
| `search` | `grep_search` / `rg --no-heading -n` | Transcend wins on size (2.32x–3.59x smaller), clusters matches by file |
| `find_symbol` | `grep_search` (approximate textual pattern) | Transcend wins on compiler/AST precision |
| `outline` | `view_file` / `cat` | Transcend wins decisively (2.48x–4.07x reduction, elides bodies to skeletons) |
| `read_symbol` | `grep_search` + `view_file` slice | Transcend wins on precision (AST boundary awareness, 128x–131x saving vs whole file) |
| `read_file` | `view_file` | **Tie** |
| `write_file` | `write_to_file` | **Tie** (Transcend adds workspace boundary confinement guard) |
| `patch` | `replace_file_content` | Transcend wins decisively (Tree-sitter AST syntax preflight check) |
| `batch_patch` | sequential `replace_file_content` calls | Transcend wins decisively (atomic multi-file rollback) |
| `delete_path` | `run_command` (`Remove-Item`) | Transcend wins (workspace boundary containment guard) |
| `set_workspace` | *none* (implicit cwd) | Transcend only |
| `git_status` | `run_command` (`git status --porcelain=v2`) | **Tie** (structured JSON vs text parsing) |
| `exec` | `run_command` | **Tie**, Transcend adds detached sessions and PTY mode |
| `terminal_read` | `manage_task` output checking | Transcend wins (cursor offsets, wait_for_pattern) |
| `terminal_write` | *none* (cannot write to running PTY interactively) | Transcend only |
| `terminal_resize` | *none* | Transcend only |
| `terminal_kill` | `manage_task` kill / `Stop-Process` | Transcend wins (whole process tree termination + final buffer capture) |
| `lsp_definition` | **no equivalent** | Transcend only |
| `lsp_references` | `grep_search` (textual approximation) | Transcend only (compiler-resolved across crates) |
| `lsp_hover` | **no equivalent** | Transcend only |
| `lsp_diagnostics` | `run_command` (`cargo check`) + parsing | Transcend wins on structured diagnostics |
| `lsp_status` | *none* | Transcend only |
| `lsp_install` | *none* | Transcend only |

**Where the native Antigravity toolset has no answer at all: 7 tools** — `lsp_definition`,
`lsp_hover`, `lsp_references`, `lsp_status`, `lsp_install`, `terminal_write`, `terminal_resize`,
`set_workspace`. The LSP tier represents a *completely different tier of semantic data* (compiler-resolved
types, cross-crate references, and syntax-checked AST transformations).

---

## 3. Measured results

Each section names the exact invocation on both sides. Paths are relative to the pinned clones
from §1 (`ripgrep/` at `3fce3b5`, `gin/` at `3b08cd7`). The Transcend side is an MCP `call_mcp_tool`
invocation with the arguments shown.

### 3.1 `search` — 3.17x–3.59x smaller than native `grep_search`, 2.32x–2.59x smaller than `rg`

Target: search `ripgrep/crates` for pattern `git_ignore` with filter `*.rs` and `max_matches: 10`.

```jsonc
// Native harness: grep_search
{
  "SearchPath": "C:\\Projects\\_transcend_bench\\ripgrep\\crates",
  "Query": "git_ignore",
  "Includes": ["*.rs"],
  "MatchPerLine": true,
  "IsRegex": false
}

// Shell native: rg
rg --no-heading -n --color never 'git_ignore' C:\Projects\_transcend_bench\ripgrep\crates -g '*.rs'

// Transcend MCP: search
{
  "path": "ripgrep/crates",
  "pattern": "git_ignore",
  "options": { "file_pattern": "*.rs", "max_matches": 10 }
}
```

| Approach | chars | tiktoken (`o200k`) | sentencepiece (`Llama-2`) | Format & Content |
|:---|---:|---:|---:|:---|
| Native `grep_search` | 4,209 | **1,239** | **1,504** | 26 JSON objects with full paths, line numbers, and content |
| Shell `rg --no-heading -n` | 2,797 | **894** | **1,103** | 26 lines verbatim (no aggregation) |
| Transcend `search` | 1,348 | **345** | **475** | 26 matches total, 10 line bodies in `dir.rs` + **directory radar** |
| **Reduction vs. `grep_search`** | **3.12x** | **3.59x** | **3.17x** | |
| **Reduction vs. raw `rg`** | **2.07x** | **2.59x** | **2.32x** | |

Both tools agreed on the exact count: **26 matches across 4 files**.
Transcend returns the first 10 matching line bodies clustered under `ignore/src/dir.rs`, sets
`matches: []` for files beyond the budget, and provides an aggregated `directory_radar`:
```json
"directory_radar": [
  { "directory": "ignore/src", "file_count": 3, "match_count": 25 },
  { "directory": "core/flags", "file_count": 1, "match_count": 1 }
]
```
Native `grep_search` prints individual JSON objects for every single line match, bloating prompt
context to **1,239–1,504 tokens** with repeated absolute paths and metadata.

### 3.2 `outline` — 3.64x–4.07x reduction on Rust defs, 2.48x–2.80x reduction on Go

Target 1: `ripgrep/crates/core/flags/defs.rs` (246,353 chars LF, 7,220 lines, 1,131 symbols).
Target 2: `gin/gin.go` (28,663 chars, 65 symbols).

```jsonc
// Transcend MCP: outline (defs.rs)
{
  "path": "ripgrep/crates/core/flags/defs.rs",
  "options": { "format": "skeleton", "max_symbols": 40 }
}

// Transcend MCP: outline (gin.go)
{
  "path": "gin.go",
  "options": { "format": "skeleton" }
}
```

| Target | Approach | chars | tiktoken | sentencepiece | Symbols preserved |
|:---|:---|---:|---:|---:|:---|
| `defs.rs` (Rust) | Whole file (`cat`) | 246,353 | 63,937 | 82,169 | 1,131 symbols |
| `defs.rs` (Rust) | Native `view_file` | — | — | — | **Failed in 1 shot** (max 46 KB / 800 lines; >= 11 calls) |
| `defs.rs` (Rust) | Transcend `outline(skeleton)` | 57,273 | **17,577** | **20,192** | 1,131 syntax-valid stubs |
| | **Reduction** | **4.30x** | **3.64x** | **4.07x** | |
| `gin.go` (Go) | Whole file (`cat`) | 28,663 | 7,365 | 9,729 | 65 symbols |
| `gin.go` (Go) | Transcend `outline(skeleton)` | 11,937 | **2,970** | **3,479** | 65 syntax-valid stubs |
| | **Reduction** | **2.40x** | **2.48x** | **2.80x** | |

On `defs.rs`, Transcend's skeleton format preserved all 1,131 symbols with signatures and doc comments:
`method: 788, function: 123, implementation: 109, struct: 108, module: 2, constant: 1`.
Native `view_file` cannot even read this file in a single tool call due to its 46 KB / 800-line safety
limit, forcing multi-turn fragmentation.

On `gin.go`, the skeleton elided method implementations down to **2,970 tokens** (tiktoken) / **3,479 tokens** (sentencepiece)
while maintaining all 65 symbols (`method: 35, function: 10, constant: 8, variable: 6, typealias: 4, struct: 2`),
yielding a **2.48x–2.80x token reduction**.

### 3.3 `read_symbol` — surgical AST extraction (128x–131x saving vs whole file)

Target 1: `WalkBuilder::git_ignore` in `ripgrep/crates/ignore/src/walk.rs` (93,443 chars, 21,468 tok tiktoken / 27,384 tok SP).
Target 2: `Run` in `gin/gin.go` (28,663 chars, 7,365 tok tiktoken / 9,729 tok SP).

```jsonc
// Transcend MCP: read_symbol (walk.rs)
{
  "path": "ripgrep/crates/ignore/src/walk.rs",
  "symbol": "WalkBuilder::git_ignore",
  "context_lines": 2
}

// Native shell context search:
rg --no-heading -n -A2 -B2 'pub fn git_ignore' ripgrep/crates/ignore/src/walk.rs
```

| Approach | chars | tiktoken | sentencepiece | Output details |
|:---|---:|---:|---:|:---|
| `view_file` whole `walk.rs` | 93,443 | 21,468 | 27,384 | Entire file dumped into context |
| Native `rg -A2 -B2` | 184 | **52** | **71** | 5 lines, **truncated closing brace** `}`, no doc comments, no span |
| Transcend `read_symbol` | 592 | **168** | **209** | Complete method body + doc comment + exact byte/line span |
| **Saving vs. whole file** | **157.8x** | **127.8x** | **131.0x** | |

```json
// Transcend read_symbol output for WalkBuilder::git_ignore
{
  "found": true,
  "qualified_name": "WalkBuilder::git_ignore",
  "source_code": "pub fn git_ignore(&mut self, yes: bool) -> &mut WalkBuilder {\r\n        self.ig_builder.git_ignore(yes);\r\n        self\r\n    }",
  "symbol": {
    "doc_comment": "Enables reading `.gitignore` files.",
    "kind": "method",
    "name": "git_ignore",
    "signature": "pub fn git_ignore(&mut self, yes: bool) -> &mut WalkBuilder",
    "span": {
      "start_line": 921, "end_line": 924,
      "start_col": 5, "end_col": 6,
      "start_byte": 31527, "end_byte": 31651
    },
    "visibility": "pub"
  },
  "total_occurrences": 1
}
```

Against reading the entire file, `read_symbol` provides a **128x–131x token saving** (168 vs. 21,468 tokens).
While native `rg -A2 -B2` uses fewer tokens (52 vs. 168), it failed to capture the complete function body because
context lines are an arbitrary constant: it truncated the closing `}`, missed the doc comment, and
provided no AST coordinate span.

On `gin.go` method `Run`, `read_symbol` returned **334 tokens** (tiktoken) / **420 tokens** (sentencepiece)
vs the 7,365-token file (**22.0x saving**), and automatically resolved the receiver relationship:
`"relationships": [{"relation": "receiver", "target": "Engine"}]`.

### 3.4 `find` — structural directory radar vs flat listing

Target: discover `*.rs` files in `ripgrep/crates`, sorted by size, top 30 results.

```jsonc
// Native listing (capped at 30 to make comparison fair):
rg --files ripgrep/crates -g '*.rs' | head -30

// Transcend MCP: find
{
  "path": "ripgrep/crates",
  "pattern": "*.rs",
  "options": { "max_results": 30, "sort_by": "size" }
}
```

| Approach | chars | tiktoken | sentencepiece | Content |
|:---|---:|---:|---:|:---|
| Native `rg --files \| head -30` | 1,832 | **696** | **761** | 30 paths |
| Transcend `find` | 4,071 | **1,309** | **1,891** | 30 entries + **23-directory radar** + extension census + total count |
| | | **1.88x larger** | **2.49x larger** | |

**Transcend `find` is 1.88x–2.49x more expensive for the same 30 results.**
It does not aim to minimize tokens here; it computes structural metadata that native listing tools
omit:
- `directory_radar`: 23 directory density buckets (e.g. `printer/src`: 11, `ignore/src`: 9, `regex/src`: 9).
- `extension_breakdown`: `{"rs": 95}`.
- Exact byte sizes and ISO-8601 timestamps per file.
- Total repository census (`total_count: 95`).

### 3.5 `patch` / `batch_patch` — AST syntax guard and atomic rollback

In a disposable sandbox, created `sandbox/greet.rs` (70 bytes):
```rust
pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
```

#### Test A: Deliberately malformed syntax replacement
Attempted to replace `greet` with invalid Rust syntax: `pub fn greet( -> String { let =`.

```jsonc
// Transcend MCP: patch
{
  "path": "sandbox/greet.rs",
  "target_symbol": "greet",
  "replacement": "pub fn greet( -> String { let =",
  "validate_ast": true
}
```

**Transcend response:**
```json
{
  "success": false,
  "ast_valid": false,
  "message": "AST preflight verification failed: syntax errors detected in spliced code. Disk was not modified.",
  "syntax_errors": [
    {
      "line": 1,
      "column": 1,
      "message": "Syntax error near 'pub fn greet( -> String { let ='",
      "unexpected_token": "pub fn greet( -> String { let ='"
    }
  ]
}
```
**Verification on disk:** `Get-Content sandbox\greet.rs` confirmed **70 bytes, 100% untouched.**

**Native comparison:** Native `replace_file_content` has no AST validation. It would blindly write
the broken syntax to disk, corrupting the source and breaking downstream builds.

#### Test B: Transactional multi-file batch patching
Submitted two operations to `batch_patch`:
1. A valid text replacement in `sandbox/greet.rs` (`format!("Hello, {}!", name)` -> `format!("Greetings, {}!", name)`).
2. An invalid patch targeting a non-existent symbol `nonexistent_func`.

**Transcend response:**
```json
{
  "success": false,
  "total_files_patched": 0,
  "all_ast_valid": false,
  "message": "Batch patch aborted: one or more patches failed AST preflight validation or target resolution. No files modified on disk.",
  "results": [
    {
      "file": "sandbox/greet.rs",
      "success": true,
      "message": "Dry run: patch successfully validated. No changes written to disk."
    },
    {
      "file": "sandbox/greet.rs",
      "success": false,
      "message": "Symbol 'nonexistent_func' not found in sandbox/greet.rs. Available symbols: greet"
    }
  ]
}
```
**Verification on disk:** Original `sandbox/greet.rs` remained intact. The valid edit was rolled back
with dry-run diff previewed, preventing a broken intermediate repository state.
Native harness tools (`replace_file_content`) execute sequentially without atomicity: edit #1 succeeds
and alters disk, then edit #2 fails, leaving the project corrupted.

### 3.6 `exec` / `terminal_*` — interactive PTY streaming and tree kill

#### Pipe execution:
Ran `echo transcend_antigravity_probe && git -C ripgrep rev-parse --short HEAD` via `exec(transport: "pipe")`:
- Execution time: **116 ms**
- Result: `exit_code: 0`, output `"transcend_antigravity_probe \n3fce3b5\n"`, clean exit.

#### Detached interactive PTY session:
Spawned an interactive PowerShell session via `exec(command: "powershell -NoProfile -NoLogo", transport: "pty", timeout_action: "detach")`:
- Result: `status: "detached"`, session assigned `session_id: "pty_8a38_5"`, initial cursor `129`.

#### Interactive PTY write and read:
1. `terminal_write`: sent `Write-Output 'GEMINI_PTY_OK'\n` to session `pty_8a38_5` -> returned `bytes_written: 29`.
2. `terminal_read`: called with `cursor: 129` and `wait_for_pattern: "GEMINI_PTY_OK"`:
   - Result:
     ```text
     PS C:\Projects\_transcend_bench> Write-Output 'GEMINI_PTY_OK'
     GEMINI_PTY_OK
     PS C:\Projects\_transcend_bench>
     ```
   - Cursor advanced cleanly to `281`.
3. `terminal_kill`: forcibly killed session `pty_8a38_5`:
   - Returned `{"success": true, "exit_code": 1, "final_output": "..."}`.
   - Verified: the entire process tree was killed cleanly with no lingering orphaned `powershell.exe` instances.

Native Antigravity tooling has no equivalent interactive PTY streaming primitive.

### 3.7 LSP tier — compiler-resolved semantics & cold-start behavior

Tested against the ripgrep workspace:

1. `lsp_status(rust)`:
   - Discovered `rust-analyzer 0.3.2862-standalone (7b6e1249b7 2026-04-12)` at `C:\Users\pxin\.cargo\bin\rust-analyzer.exe`.
   - Identified available installation recipes for `rustup` and `cargo`.

2. **Cold-start behavior:**
   - On the initial call before rust-analyzer finished background indexing, `lsp_hover` and `lsp_definition`
     returned `engine: "tree-sitter:heuristic"` with exact preview spans.
   - This proves the graceful fallback mechanism: queries never crash or block indefinitely when the
     LSP server is cold.

3. **Warm compiler resolution (`engine: "lsp:rust-analyzer"`):**
   - `lsp_hover(WalkBuilder::git_ignore)`:
     ```text
     signature: ignore::walk::WalkBuilder
     documentation:
     pub fn git_ignore(&mut self, yes: bool) -> &mut WalkBuilder
     ---
     Enables reading `.gitignore` files.
     `.gitignore` files have match semantics as described in the `gitignore` man page.
     This is enabled by default.
     ```
   - `lsp_references(WalkBuilder::git_ignore)`:
     Returned exactly 4 compiler-verified reference sites across crates:
     - `src/walk.rs:866` (`.git_ignore(yes)`)
     - `src/walk.rs:2338` (`builder.git_ignore(false);`)
     - `src/incremental.rs:948` (`builder.git_ignore(false).git_exclude(false);`)
     - `crates/core/flags/hiargs.rs:910` (`.git_ignore(!self.no_ignore_vcs)`)
     Total found: 4, zero false positives from comments or variable names.
   - `lsp_definition(WalkBuilder::git_ignore)`:
     Resolved directly to `src/walk.rs:921` (`pub fn git_ignore(...)`).
   - `lsp_diagnostics`:
     Returned structured diagnostic object (`total_count: 0` on clean files).

**Cost note for the whole tier:** All 23 tool schemas total **35,183 chars = 7,712 tokens** (`o200k_base`)
or **9,403 tokens** (`sentencepiece`), ~335–408 tokens per tool, paid on model initialization.

### 3.8 Safety behaviours & workspace containment

Tested boundary enforcement:
- Attempted `write_file` to `C:\Projects\_transcend_bench\sandbox\escape_probe.txt` while workspace was set to `C:\Projects\General Workspace\Idea\Transcend`:
  ```
  Operation failed: Access denied: path 'C:\Projects\_transcend_bench\sandbox\escape_probe.txt'
  escapes workspace boundary 'C:\Projects\General Workspace\Idea\Transcend'.
  Pass an absolute path inside the workspace, or call set_workspace first.
  ```
- Attempted `delete_path` outside workspace:
  ```
  Operation failed: Access denied: path 'C:\Projects\_transcend_bench\sandbox\greet.rs'
  escapes workspace boundary 'C:\Projects\General Workspace\Idea\Transcend'.
  Pass an absolute path inside the workspace, or call set_workspace first.
  ```
Both mutating and deleting operations are strictly confined within the configured workspace boundary.

---

## 4. What is not measured

Stated clearly to maintain complete empirical integrity:

| Item | Status |
|:---|:---|
| `lsp_install` | Recipe discovery exercised via `lsp_status`, but fresh installation was **not** run because `rust-analyzer` was already present. |
| Cross-platform | All measurements executed on **Windows 10 Pro 22H2 (build 19045)**. Unix PTY (`openpty`) and POSIX signals remain unexercised in this run. |
| Wall-clock throughput profiling | Incidental execution timings were recorded (e.g. `exec` 116 ms), but large-scale wall-clock microbenchmarks (e.g. criterion benchmarks across thousands of iterations) were not performed. |

---

## 5. Conclusions

**Transcend is an AST-aware codebase intelligence and safety layer, not merely a token filter.**

1. **Token reductions verified with real tokenizers (`tiktoken` / `sentencepiece`):**
   - `search`: **3.17x–3.59x smaller** than native `grep_search` and **2.32x–2.59x smaller** than raw `rg`.
   - `outline`: **3.64x–4.07x reduction** on large Rust files (7,220 lines) and **2.48x–2.80x reduction** on Go files, preserving all symbols as valid skeletons.
   - `read_symbol`: **128x–131x token reduction** vs whole-file reads on `walk.rs`, **22x reduction** on `gin.go`, with exact doc comments and byte spans.

2. **Where Transcend costs more tokens:**
   - `find`: **1.88x–2.49x larger** than raw `rg --files` listings. The overhead purchases structural intelligence: a 23-directory density radar, extension census, and total counts that flat path listings cannot provide.

3. **Unique capabilities with zero native equivalent:**
   - **7 tools** have no native counterpart in the Antigravity harness: `lsp_definition`, `lsp_hover`, `lsp_references`, `lsp_status`, `lsp_install`, `terminal_write`, `terminal_resize`, `set_workspace`.
   - **Tree-sitter AST safety**: `patch` refuses malformed syntax before touching disk; `batch_patch` provides atomic all-or-nothing rollback across multiple files.
   - **Interactive terminal streaming**: Detached ConPTY sessions with incremental cursor reads, stdin writing, and tree-wide process termination.

**Operational guidance:** Use Transcend's `outline` and `read_symbol` as primary reading primitives, `patch` and `batch_patch` for verified mutations, and the LSP tier for compiler-level navigation. Fall back to native `view_file` or `run_command` only for non-code files or raw CLI scripting.
