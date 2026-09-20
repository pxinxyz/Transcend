# Transcend vs. Native Harness Tooling

> **Author:** DeepSeek **DeepSeek-V41-Flash** (model id `deepseek-flash`, provider
> `deepseek-official`, reasoning effort `max`) running inside the **DeepSeek Harness**
> (`@deepseek-ai/dsh` 0.1.5-rc.2) as the agent under test.
>
> Everything below was produced by that agent: it drove Transcend's MCP tools through the
> harness, ran the native side, and did the measuring, the analysis, and the error
> recorded in §4.1. Nothing here is a vendor-supplied figure.

An empirical comparison of Transcend's 23 MCP primitives against the native tools
available in this harness (ripgrep/`pwsh`/`read`/`write`/`edit`/`glob`/`grep`), run
against real third-party codebases.

Everything below was executed, not estimated, unless a row is explicitly marked
**not measured**.

---

## 1. Method

**Corpus.** Two real repositories, cloned fresh into a scratch directory outside the
Transcend repository. Clone them at these exact commits to reproduce the numbers:

| Repository | Commit | Size | Language mix |
|:---|:---|:---|:---|
| [ripgrep](https://github.com/BurntSushi/ripgrep) | [`3fce3b5`](https://github.com/BurntSushi/ripgrep/commit/3fce3b5bb0236da2df6d99672afb8a719642eca7) | 237 tracked files, 3.33 MB | 110 `.rs`, 23 `.md`, 14 `.toml` |
| [gin](https://github.com/gin-gonic/gin) | [`3b08cd7`](https://github.com/gin-gonic/gin/commit/3b08cd7235bd5ad2f055aa9e38135f111f6c5926) | 99 tracked files, 0.9 MB | 99 `.go` |

```bash
# ripgrep — pinned to the exact commit measured below
git clone https://github.com/BurntSushi/ripgrep.git
git -C ripgrep checkout 3fce3b5bb0236da2df6d99672afb8a719642eca7

# gin
git clone https://github.com/gin-gonic/gin.git
git -C gin checkout 3b08cd7235bd5ad2f055aa9e38135f111f6c5926
```

**Provenance of the pins, stated honestly.** `3fce3b5` is the commit the run actually
measured — it was captured during the run by `exec` running `git rev-parse --short HEAD`
inside the clone. The gin commit was *not* captured during the run; `3b08cd7` is gin's
default-branch head resolved afterwards, so its numbers should be treated as indicative
of that snapshot rather than exactly reproducible.

ripgrep was chosen because it is a real, non-trivial Rust workspace (14 crates) with a
genuinely large generated file (`crates/core/flags/defs.rs`, 7,220 lines) alongside
normal source, which separates tools that scale from tools that merely work on toys.

**Token accounting.** Tokens are approximated as `chars / 4`. This is a documented
heuristic, **not** a tokenizer. Both sides are measured identically, so only *ratios*
should be read, never absolute token counts. Byte and character counts are exact.

**Isolation.** Mutating tools (§3.5) ran against a disposable copy of the clone, never
against a pristine corpus or the Transcend repository. The MCP `workspace_root` was
repointed with `set_workspace` and restored afterwards. Read-only tools ran directly
against the clone.

**Environment.** Windows 10 Pro 22H2 (build 19045), `transcend.exe` release build 2.0.0
(thin LTO). The agent under test was **DeepSeek-V41-Flash** (model id `deepseek-flash`,
provider `deepseek-official`, reasoning effort `max`) running inside the **DeepSeek
Harness** (`@deepseek-ai/dsh` 0.1.5-rc.2), which reaches Transcend through
[`@deepseek-ai/dsh-mcp-client`](https://github.com/deepseek-ai/deepseek-harness) over
stdio. The native side used `pwsh` 7 and `rg`.

The harness is the *client* here, not part of the measurement: it forwards each
`tools/call` to Transcend over stdio. Any MCP client would produce the same Transcend
side; the native side is plain shell. Model choice therefore affects how the tools were
*driven and interpreted*, not the byte counts.

**Reproducing a row.** Start the Transcend MCP server against a clone
(`transcend export-schemas --out ./schemas` confirms all 23 tools are live), then run the
native side with the command named in that row's section below. Costs are character
counts of the raw response on each side.

---

## 2. Capability mapping

| Transcend tool | Closest native equivalent | Verdict |
|:---|:---|:---|
| `find` | `Get-ChildItem -Recurse` / `fd` | **Different information**, not less |
| `search` | `rg --no-heading -n` | Transcend wins on size, loses on fidelity |
| `find_symbol` | `rg 'fn NAME'` (approximate) | Transcend wins |
| `outline` | *none* (read whole file) | Transcend wins decisively |
| `read_symbol` | `rg -A/-B` then `read` | Transcend wins on precision |
| `read_file` | `read` with offset/limit | **Tie** |
| `write_file` | `write` | **Tie** (different guarantees) |
| `patch` | `edit` | Transcend wins (AST guard) |
| `batch_patch` | several `edit` calls | Transcend wins (atomicity) |
| `delete_path` | `Remove-Item` | Transcend wins (guard) |
| `set_workspace` | *none* — implicit cwd | Transcend only |
| `git_status` | `git status --porcelain=v2` | **Tie** (structure vs. parsing) |
| `exec` | `pwsh` | **Tie**, Transcend adds detach |
| `terminal_read` | background job + `job_output` | **Tie** |
| `terminal_write` | *none* — cannot write to a running proc | Transcend only |
| `terminal_resize` | *none* | Transcend only |
| `terminal_kill` | `Stop-Process` / `job_kill` | **Tie** (tree semantics differ) |
| `lsp_definition` | **no equivalent** | Transcend only |
| `lsp_references` | `rg` (textual approximation) | Transcend wins on precision |
| `lsp_hover` | **no equivalent** | Transcend only |
| `lsp_diagnostics` | `cargo check` + parse | Transcend wins on structure |
| `lsp_status` | *none* | Transcend only |
| `lsp_install` | *none* | Transcend only |

**Where my own toolset has no answer at all: 7 tools** — `lsp_definition`,
`lsp_hover`, `lsp_status`, `lsp_install`, `terminal_write`, `terminal_resize`,
`set_workspace`. This is the honest core of the comparison: the LSP tier is not a
faster way to do something I can already do, it is a *different class of information*
(compiler-resolved rather than textual).

---

## 3. Measured results

Each section names the exact invocation on both sides. Paths are relative to the pinned
clone from §1 (`ripgrep/` at `3fce3b5`); the Transcend side is an MCP `tools/call` with
the arguments shown.

### 3.1 `search` — 2.33x smaller than `rg` on the same query

```bash
rg --no-heading -n --color never 'git_ignore' ripgrep/crates
```
```jsonc
// tools/call
{ "name": "search",
  "arguments": { "pattern": "git_ignore", "path": "ripgrep/crates",
                 "options": { "file_pattern": "*.rs", "max_matches": 10 } } }
```

| | Output |
|:---|:---|
| `rg --no-heading -n` | 26 lines, **2,796 chars** (~699 tok) |
| `search` (max_matches=10) | 26 matches found, 10 with text, clustered by file, **~1,200 chars** |

Both reported **26 matches** — no divergence in the count. Transcend returns fewer
line bodies (budget-capped) plus a `directory_radar` locating density
(`ignore/src` 25, `core/flags` 1). `rg` returns every line verbatim but no aggregation.

**Where `search` loses:** it returns clusters with empty match arrays once the budget is
hit (`walk.rs {match_count: 5, matches: []}`). A consumer that wants the actual lines
must re-query. **Mitigated:** each cluster now carries `matches_truncated`, so an
exhausted cluster is distinguishable from a file with no matches, and `match_count` stays
exact for the radar either way. `rg` still returns every line verbatim in one shot.

### 3.2 `outline` — 4.44x reduction on a 7,220-line file

Target: `ripgrep/crates/core/flags/defs.rs`, 254,514 bytes, 7,220 lines, 1,131 symbols.

```bash
wc -c ripgrep/crates/core/flags/defs.rs   # the "cat" baseline
```
```jsonc
// tools/call
{ "name": "outline",
  "arguments": { "path": "ripgrep/crates/core/flags/defs.rs",
                 "options": { "format": "skeleton", "max_symbols": 40 } } }
```

| Approach | Cost |
|:---|:---|
| `cat` the file | 254,514 bytes (~63,629 tok) |
| `outline(format="skeleton")` | 57,273 chars (**~14,318 tok**) |
| Reduction | **4.44x** |

The skeleton preserved all 1,131 symbols as syntax-valid stubs with signatures and doc
comments. It reported `parse_status: complete` and a kind breakdown
(`method: 788, function: 123, implementation: 109, struct: 108, module: 2, constant: 1`).

**Caveat, and it matters:** a 4.44x reduction on a *pathological* generated file is the
best case. On normal files the win is smaller, and on a 1,145-line file the skeleton can
approach the source size. The real argument for `outline` is not the ratio — it is that
it answers "what is in this file?" without a full read at all.

### 3.3 `read_symbol` — precise, but not always smaller than a targeted `rg`

Target: `WalkBuilder::git_ignore` in `ripgrep/crates/ignore/src/walk.rs` (96,183 bytes,
~24,046 tok to read whole).

```bash
rg --no-heading -n -A2 -B2 'pub fn git_ignore' ripgrep/crates/ignore/src/walk.rs
```
```jsonc
// tools/call
{ "name": "read_symbol",
  "arguments": { "path": "ripgrep/crates/ignore/src/walk.rs",
                 "symbol": "WalkBuilder::git_ignore",
                 "context_before": 2, "context_after": 2 } }
```

| Approach | Cost | What you get |
|:---|:---|:---|
| `read` whole file | ~24,046 tok | everything |
| `rg -A2 -B2 'pub fn git_ignore'` | 183 chars (~46 tok) | 8 lines, no doc comment, no span |
| `read_symbol` | 541 chars (~135 tok) | body + `doc_comment` + exact byte span |

`read_symbol` is **~3x larger than a bare `rg` context search** — and still worth it,
because it also returns `span: {start_byte: 31527, end_byte: 31651}` and the doc comment,
which `rg` does not, and it cannot accidentally match a commented-out or string-literal
occurrence. Against the *whole-file read* it is a 178x saving.

### 3.4 `find` — costs more, delivers aggregates instead of a list

```bash
# native: the closest analogue, recursive discovery + metadata + sort + top 30
find ripgrep/crates -name '*.rs' -printf '%s\t%p\n' | sort -rn | head -30
```
```jsonc
// tools/call
{ "name": "find",
  "arguments": { "pattern": "*.rs", "path": "ripgrep/crates",
                 "options": { "max_results": 30, "sort_by": "size" } } }
```

| | Cost | Content |
|:---|:---|:---|
| `Get-ChildItem \| Sort Length \| Select -First 30` | 844 chars | 30 path+size lines |
| `find(sort_by="size")` | ~2,300 chars | 30 entries + **23-directory radar** + extension census + `total_count: 95` |

**Transcend `find` is 2.7x *more* expensive here.** It is not a compaction win. Its value
is the directory-density radar and the total census, which the native listing does not
compute — the agent learns *where* code lives, not just what exists. On a large
repository this is the difference between a 50,000-line listing and a fixed-size
summary; on a 237-file repository it is mostly overhead.

### 3.5 `patch` / `batch_patch` — the AST guard actually fires

Created `greet.rs`, then applied a deliberately malformed replacement:

```json
{"success": false, "ast_valid": false,
 "message": "AST preflight verification failed: syntax errors detected in spliced code. Disk was not modified.",
 "syntax_errors": [{"line": 1, "column": 1,
   "message": "Syntax error near 'pub fn greet( -> String { let ='",
   "unexpected_token": "pub fn greet( -> String { let ='"}]}
```

Verified on disk: **98 bytes, unchanged.** The guard reports exact line/column and the
offending token.

`batch_patch` with one valid and one unresolvable patch:

```json
{"success": false, "total_files_patched": 0,
 "message": "Batch patch aborted: one or more patches failed AST preflight validation or target resolution. No files modified on disk."}
```

Verified on disk: original content intact. The valid patch's diff was still returned
(marked `"Dry run: patch successfully validated"`), so a failed batch is *diagnosable*,
not just refused. This is a genuine capability my `edit` tool lacks: I would have
written the first edit before discovering the second target was missing.

### 3.6 `exec` / `terminal_*` — process-tree kill verified

Pipe exec: `echo ... && git rev-parse --short HEAD` → `exit_code: 0`, 117 ms, clean output.

Detached session (`pty_4310_2`), then `terminal_kill`:

| | Before | After |
|:---|:---|:---|
| Child `powershell.exe` (PID 30560, parent = the Transcend session) | alive | **gone** |
| Total `powershell.exe` processes | 5 | 3 |
| `terminal_kill` result | — | `{"success": true, "exit_code": 1, "final_output": "child_tree_started\n"}` |

The process-tree extension worked, including returning the unread buffered output
(`final_output`) at kill time.

**`terminal_write` over a PowerShell PTY — found broken, now fixed.** At the time of
the original run, three successive `terminal_write` calls (52 bytes total, including
`echo pty_echo_probe` and a newline-terminated `Write-Output`) produced **zero** output,
and `terminal_read` timed out with `next_cursor` unmoved at 20.

The cause turned out to be two independent defects, neither PowerShell-specific — the
existing `cmd.exe` test passed only because it never asserted on output:

1. **Every PTY command was shell-wrapped.** `PtyTransport::spawn` routed the command
   through `resolve_shell`, turning `powershell -NoProfile -NoLogo` into
   `powershell -Command "powershell -NoProfile -NoLogo"`, which ran the inner command,
   printed its banner and exited. A bare shell or REPL is now invoked directly
   (`resolve_shell_spec(.., wrap_in_shell = false)`), while compound commands and pipe
   execution are still wrapped.
2. **ConPTY's startup query was never answered.** The child emitted
   `ESC[?9001h ESC[?1004h ESC[6n` and blocked, because `ESC[6n` is a *Device Status
   Request* that requires a Cursor Position Report before the console host finishes
   initialising. Nothing replied, so no prompt or banner was ever produced. The PTY
   reader now detects that query and a pump thread writes the report back.

A third issue surfaced once output flowed: a bare `\n` was being buffered rather than
submitted, because Windows console hosts accept only carriage return as "run this line"
(the shell showed a `>>` continuation prompt). PTY input now translates line endings to
`\r` while preserving `\r\n` and blank lines.

Verified live through the MCP path on Windows 10 Pro 22H2 (build 19045):

| Call | Result |
|:---|:---|
| `exec(transport=pty, cmd="powershell -NoProfile -NoLogo")` | `detached`, 222 bytes — the prompt |
| `terminal_write("Write-Output 'PTY_STDIN_WORKS'\n")` | `{bytes_written: 31}` |
| `terminal_read(cursor=222, wait_for_pattern="PTY_STDIN_WORKS")` | `PTY_STDIN_WORKS` echoed **and** its output returned |

`test_pty_session_is_actually_interactive` now guards this, asserting the session stays
running and that written input actually reaches the child and comes back.

### 3.7 LSP tier — real compiler semantics, with a cold-start caveat

`lsp_status(rust)` correctly reported `rust-analyzer 0.3.2862-standalone`, its resolved
absolute path, and two available install recipes.

`lsp_hover(WalkBuilder::git_ignore)` returned genuine rust-analyzer output:

```
signature: ignore::walk::WalkBuilder
documentation: ```rust pub fn git_ignore(&mut self, yes: bool) -> &mut WalkBuilder ```
  Enables reading `.gitignore` files. ... This is enabled by default.
engine: "lsp:rust-analyzer"
```

`lsp_references` returned 4 compiler-resolved sites with `engine: "lsp:rust-analyzer"` —
no false positives from comments or strings.

**Cold-start caveat, measured, and now mitigated.** The *first two* `lsp_definition`
calls on a freshly-cloned repository returned `engine: "tree-sitter:heuristic"` instead
of LSP results, because rust-analyzer had not finished indexing and the request deadline
was a fixed 5 seconds. Semantic requests now use a 120-second deadline until the server
has answered once, and `goto_definition` retries for up to 90 seconds while the session
reports itself cold, so the first query after a fresh clone gets compiler precision
instead of silently degrading. The `engine` field still reports which engine answered, so
a genuine heuristic fallback remains visible.

**Cost note for the whole tier:** all 23 tool schemas total **35,161 bytes (~8,790
tokens)** of context, paid on every model request. That is the price of the verbose,
well-documented schemas — a real cost against my own leaner native tool definitions.

### 3.8 Safety behaviours confirmed

`write_file` outside the configured workspace root was refused with an actionable message:

```
Access denied: path 'C:\Projects\_transcend_bench\sandbox\greet.rs' escapes
workspace boundary 'C:\Projects\General Workspace\Idea\Transcend'.
Pass an absolute path inside the workspace, or call set_workspace first.
```

Following that instruction (`set_workspace`) made the write succeed. The guard and its
error message are both correct.

---

## 4. What is not measured

Stated plainly rather than glossed:

| Item | Status |
|:---|:---|
| `delete_path` | Exercised (deleted a file, returned `deleted_count: 1`), but the recursive/boundary-escape paths were **not** timed or cost-measured |
| `git_status` | Exercised on both repos (`branch: main/master`, `is_clean: true`, ahead/behind). No native timing comparison |
| `lsp_install` | Exercised (correctly short-circuited: `'marksman' is already installed`) but the actual install path was **not** run |
| `lsp_diagnostics` | Exercised — returned 0 diagnostics on a clean file. The LSP-backed path was not differentiated from the `cargo check` fallback |
| Cross-platform | Everything here is **Windows 10 Pro 22H2 (build 19045)** only. The Linux/macOS paths (process groups, `pgrep`, openpty) remain unverified. |
| Throughput | No wall-clock benchmark beyond incidental `elapsed_ms` (e.g. `exec` 117 ms). Search/outline latency was not profiled |
| Token accuracy | `chars/4` is an approximation. No real tokenizer was used |

### 4.1 A false alarm, recorded

An early query — `fn is_gitignore|\.gitignore\(` — returned **0 matches** where a plain
substring search for `git_ignore` returned 26, which looked like a regex or escape bug.
It was not. Investigating properly:

| Query | Result |
|:---|:---|
| `git_ignore` | 26 matches |
| `gitignore\|walk_parallel` (plain alternation) | 268 matches — alternation works |
| `\.gitignore` (escaped literal dot) | 62 matches — escapes work |
| `gitignore\(` (escaped literal paren) | 11 matches — escapes work |
| `is_gitignore` | **0 matches** |
| `rg -c 'is_gitignore'` over the whole repo | **no matches** |

The identifier `is_gitignore` does not exist anywhere in ripgrep. The original regex was
correct and the **0 result was correct behaviour**. No defect exists; the initial
suspicion was my own error and is recorded here so the negative result is not mistaken
for an open issue.

Also resolved: the GitHub Actions workflow added in `5636cb4` has been **removed**
(`cf4332f`). It was never requested, it consumed the account's Actions quota, and every
run failed before its first step with "the job was not started because your account is
locked due to a billing issue". Nothing is lost: `actionlint` confirmed the workflow
itself was valid, and the verification commands it encoded remain in `README.md` and
`AGENTS.md`. There is now no hosted CI, so cross-platform verification is manual.

---

## 5. Conclusions

**Transcend is not a token-compaction layer, and comparing it as one understates it.**

On raw size it wins on `search` (2.33x) and `outline` (4.44x on a 7,220-line file), ties
on `read_file`/`write_file`/`git_status`/`exec`, and *loses* on `find` (2.7x larger) and
`read_symbol` versus a targeted `rg` context search (3x larger). Anyone expecting uniform
reduction will be disappointed by that spread, and the schema overhead (~8,790 tokens)
is charged up front.

The real differentiator is **a different class of answer**:

1. **Compiler-resolved semantics (7 tools with no native equivalent).** `lsp_hover` and
   `lsp_references` return rust-analyzer output. Text search cannot distinguish a real
   reference from one in a comment or a string, and cannot infer a type at all.
2. **Verified mutations.** The AST preflight rejected invalid syntax with exact
   line/column and left the file byte-identical; `batch_patch` aborted atomically
   across files. Sequential `edit` calls cannot offer that — they fail halfway.
3. **Structural aggregates.** `find`'s directory radar and `search`'s density clusters
   answer "where is this concentrated?" in one call.

**Against that, the problems found — and their status:**

1. **PTY stdin on Windows did not work.** Three compounding defects (shell-wrapped bare
   REPLs, an unanswered ConPTY `ESC[6n` startup query, and un-submitted `\n` line
   endings). All three are **fixed and verified live**; see §3.6. The old test passed
   only because it never asserted on output.
2. **LSP queries degraded on a cold repository.** **Fixed**: semantic requests now use a
   cold-start deadline and `goto_definition` retries while the server indexes (§3.7).
3. **Budget-capped results include empty match arrays**, so a consumer must re-query to
   get line text, unlike `rg`. **Mitigated**: clusters now report `matches_truncated`, so
   the omission is explicit rather than indistinguishable from an empty file. The
   re-query is still required — keeping `directory_radar` counts truthful depends on it.

Plus one unexplained observation: the regex-alternation query returning 0 matches.

**Practical guidance.** Use Transcend for `outline`/`read_symbol` exploration and for the
LSP tier, where it is strictly better than anything textual. Keep native `rg` for
pattern hunting where verbatim lines matter, and native `read`/`write`/`edit` for
ordinary file work — they are smaller, and the AST guard only pays for itself when
editing code by symbol rather than by line.
