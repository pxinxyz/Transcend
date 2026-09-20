# Tool defect audit — 23 MCP tools

Systematic review: every request field in `crates/transcend-protocol/src/lib.rs` was traced
into `transcend-core`, then verified empirically against the live server and fixtures.
Findings marked **CONFIRMED** were independently reproduced by me (not just reported).

Note on ownership: none of these are caught by the existing suite.

## Status

| # | Defect | Status |
|---|---|---|
| 1 | `batch_patch` persisted in-memory buffers | **fixed** (`c414c8d`) |
| 2 | `outline` panicked truncating a multibyte skeleton | **fixed** (`520f9d4`) |
| 3 | `lsp_diagnostics` with `path` always returns zero | **fixed** (`8c22e92`) |
| 4 | `outline` drops files past `max_files`, reports `truncated:false` | **fixed** (`2d85618`) |
| 5 | `find_symbol` can omit the exact match (cap before sort) | **fixed** (`92c6c90`) |
| 6 | `find_symbol.case_sensitive` default | **withdrawn** — doc was wrong, see below |
| 7 | `exec` ignored `max_output_bytes` on the kill path | **fixed** (`b867a5e`) |
| 8 | `lsp_references.include_declaration` ignored on fallback | **fixed** (`7d163c8`) |
| 9 | `read_file.max_bytes` enforced as a char budget | **fixed** (`19323ab`) |
| 10 | `search` on a single file reports `file:""` | **fixed** (`74a0413`) |
| 11 | `search` reports `truncated:false` when `max_per_file` capped | **fixed** (`74a0413`) |
| 12 | `symbol_kinds`/`exported_only` no-ops for some languages | **fixed** (`2414bb6`) |
| 13 | `ast_valid:true` when no grammar exists | **documented** (`1161274`) — behaviour change needs a contract decision |
| 14 | `lsp_status` silently empty for an unknown language | **fixed** (`85f8aae`) |
| 15 | Shared workspace root makes results order-dependent | open |
| — | `exec.raw` silently ignored on the PTY path | **fixed** (`b867a5e`) |
| — | `lsp_diagnostics` fell back to `cargo check` on an empty LSP verdict | **fixed** (`8c22e92`) |

13 of 16 fixed, 1 documented, 2 open. Every fix carries a regression test that was
confirmed to fail against the pre-fix code. Two findings were corrected during the work:
item 6 was withdrawn (the code was right, the doc was unsatisfiable), and the
`lsp_diagnostics` fallback plus the `total_files_patched` / dry-run miscounts were found
while fixing items 3 and 1.

### Remaining

- **13** `ast_valid: true` for extensions with no grammar. The doc now states the
  limitation and a test pins it, so this is honest. Making it *accurate* needs a decision:
  either a new `ast_validated` field, or rejecting `validate_ast` when no grammar exists.
  Both change a contract.
- **15** Shared workspace root is process-global; read-only requests expose no
  `workspace_root` to pin, so concurrent calls can resolve against different roots. The
  audit reproduced this with `set_workspace` racing a no-path `find_symbol`. Most invasive
  of the set and the least likely to be hit by a single-agent session.

  A related layering hazard was found while investigating this and is now documented and
  tested (`b974f33`): `FileOps::ensure_within` returns `Ok(())` for a `None` boundary instead
  of failing closed. Enforcement actually happens one layer up, where `NativeEngine`
  overwrites `workspace_root` on every mutating dispatch with the boundary it resolved from
  its own active root. That layering is correct today and a test now pins it, but
  `FileOps::write_file` performs no boundary check at all when called directly with
  `workspace_root: None`, and nothing previously said so.

## Boundary budgets were reinterpreted instead of refused (fixed)

Found by probing error paths and boundaries, all verified live first:

| input | before | after |
|---|---|---|
| `read_file {start_line: 8, end_line: 3}` on a 10-line file | `start_line: 8, end_line: 8`, **empty content**, no message | refused, both bounds named |
| `outline {max_files: 0}` | empty census, `summary.total_files: 0` for a directory holding a file | refused |
| `search {max_matches: 0}` | budget silently ignored, every match returned | refused |
| `find_symbol {limit: 0}` | no results | refused |

The `read_file` one is the worst of the three: the response *claimed* to cover line 8 while
returning nothing, so a caller trusting the reported range would read the wrong lines.
The `outline` one reported a false census — a directory with a file in it described as
having none, flagged `truncated: true`, which implies something was capped rather than that
the caller asked for nothing. And zero meant *opposite* things in sibling tools: ignored by
`search`, taken literally by `find_symbol`.

Fixed in `e02d48e`: all five count budgets (`search.max_matches`, `search.max_per_file`,
`outline.max_files`, `outline.max_symbols`, `find_symbol.limit`) plus the inverted line range
are now refused, with the error naming the default. Rejecting is the honest option because
"none" and "unlimited" are opposite readings of zero and a caller who passed 0 cannot be
assumed to have meant either.

## Misspelled options are silently defaulted (reported, not fixed)

An option name that does not exist is accepted without complaint, so the default applies and
the caller cannot tell. Verified live on a directory of 150 files:

| request | returned |
|---|---|
| `find max_results: 5` | 5 entries |
| `find max_result: 5` (typo) | **100 entries** |
| `search max_matches: 3` | 3 matches |
| `search max_match: 3` (typo) | **50 matches** |

Asking for a *smaller* budget than the default silently returns up to 20x more output than
intended. The reverse is worse: asking for more results than the default returns fewer, so
relevant results are missing with no signal — the same confident-wrong-answer shape as the
other defects here.

The protocol carries **56 `alias` attributes**, so being forgiving about naming is clearly
deliberate. Forgiving *silently* is not the same as forgiving usefully.

**Not fixed**: making it loud needs `deny_unknown_fields` on the option structs, which rejects
any unexpected key and is therefore a breaking change for hosts that send extra metadata.
Pinned by `misspelled_option_names_are_silently_ignored`; the contract probe asserts the
desirable behaviour and fails until the decision is taken.

## Defect found by the contract probe (fixed)

**`terminal_read` gave up on `wait_for_pattern` without saying so.** `wait_for_pattern` is
documented as "pattern to await in the terminal output stream before returning"; the poll
loop fell out of its deadline and returned the buffer as though the wait had succeeded, so
"the pattern appeared" and "I waited and gave up" were indistinguishable. An agent waiting
for a build to print a completion line would proceed on a build that never got there.

Verified live before the fix: a read with `wait_for_pattern` set to a string that cannot
occur returned `Ok` with the shell banner and no failure indication. Fixed in `aaec581`; the
wait now distinguishes a match from a timeout and a process exit, and returns an error naming
the pattern and the reason. Regression test drives a real PTY and was confirmed to fail
against the previous behaviour.

Also checked and found **correct**, so they do not need re-litigating:

- `search.max_line_length` marks the clip inline (`... [truncated N chars]`) rather than via a
  flag; `truncated` stays false because the *match budget* was not what clipped the line. The
  inline marker makes the omission visible, so this is a defensible reading of the contract.
- `git_status` distinguishes staged, unstaged and untracked correctly, verified against
  `git status --porcelain=v2` on a repo constructed with all three states.

## Defect found by the efficiency harness (fixed)

**`lsp_diagnostics` reported a broken file as clean.**

Reproducer: a standalone crate with two genuine compiler errors
(`let s: String = 42;` and a call to a missing function). `cargo check` reports both.

| condition | before | after |
|---|---|---|
| fresh MCP session, `lsp_diagnostics {path: <lib.rs>}` | 2 | 2 |
| session already used by `search`/`read_symbol`/`find`/`outline` | **0** | **2** |
| `lsp_diagnostics {path: <directory>}` | 0 | 0 (known limitation, below) |

Three independent causes, found in that order:

1. **An empty LSP cache was treated as a verdict.** Diagnostics arrive as
   `publishDiagnostics` notifications and nothing sets `warmed` for them, so "not indexed
   yet" and "genuinely clean" were indistinguishable. Fixed: a cold session reports no
   verdict and defers to the compiler fallback.
2. **Substring path matching.** `"src/lib.rs".contains("/abs/proj/src")` is false, so a
   DIRECTORY filter -- which the request contract documents -- always returned zero. Fixed:
   matching is segment-based.
3. **The compiler fallback only looked for `Cargo.toml` at the workspace root.** With the
   workspace scoped to a checkout and the file in a sibling crate, *no compiler ran at all*
   and the empty result was returned as "no problems". The filter is applied only after a
   successful cargo run, so nothing could be surfaced. Fixed: the manifest is now discovered
   by climbing from the file's own directory, so the file is checked against ITS crate.

Cause 1 was a regression introduced earlier in this session by the commit that stopped a
correct LSP verdict being replaced by another engine's output. Causes 2 and 3 were
pre-existing. The harness found all three, which is the argument for having built it.

### Known limitation: directory filters with a relative cache
rust-analyzer reports paths relative to the session root (`src/lib.rs`) while the engine
resolves the filter absolutely (`/abs/proj/src`). A FILE filter matches, because the
relative cache is the absolute filter's tail. A DIRECTORY filter cannot be resolved,
because placing a relative path under an absolute directory needs the session root, which
is not threaded into the filter. Pinned by
`test_diagnostics_relative_cache_matches_absolute_filter_by_tail`.

## Decisions outstanding


Two items are parked rather than fixed because they change a public contract:

- **13** `ast_valid` is vacuous without a registered grammar. Options and a recommendation
  are written up in `IDEAS/ast-valid-contract.md` (gitignored, local scratch). The doc and a
  pinning test landed in `1161274`; making the field *accurate* needs the decision.
- **15** Adding a per-request root override to read-only tools would let a caller pin the
  root and close the ordering dependence without removing `set_workspace`. It is additive
  but touches six request structs across 23 tools, so the schema cost and the naming need
  agreeing first.

### Deliberately not fixed

`read_file` returning no `success` discriminator (a missing file, a directory and an empty
file are all `content: "", truncated: false`) is a real defect, but the fix adds a field to
a response contract and deserves a decision rather than a drive-by change.


---

## HIGH

### 1. `batch_patch` writes in-memory buffer content to disk, and reports it did not — CONFIRMED, FIXED
`crates/transcend-core/src/patch.rs:478-479` (Phase 1 seeds `working_buffers` from
`content`), `:590-619` (Phase 2 writes **every** buffer), contrast `:415` where single
`patch` guards with `req.content.is_none()`.

`PatchRequest.content` is documented "In-memory code buffer (for unsaved buffer testing)".
Single `patch` honours that; `batch_patch` does not. Reproduced directly:

```
batch_patch {content:"fn main() { BATCH_INMEMORY_MARKER }", target_symbol:"main"}
  -> disk: "fn main() { BATCH_REPLACED }"      <-- file OVERWRITTEN
  -> response: "Dry run: patch successfully validated. No changes written to disk."
patch {content:"fn main() { SINGLE_INMEMORY_MARKER }", ...}
  -> disk: unchanged
  -> response: "Patch successfully applied in-memory."
```

The tool mutated real files from an unsaved buffer **and asserted the opposite**, so a caller
doing an in-memory batch simulation silently corrupted files — including creating files that
did not previously exist.

**Fixed.** Phase 1 records paths that supplied `content`; Phase 2 skips them. Two further
honesty defects were fixed in the same pass:
- `total_files_patched` counted *requested* files, so a dry run claimed N files patched. It
  now reports files actually written (0 for a dry run).
- The success message now names the in-memory paths that were validated but not persisted.

Three regression tests added; the in-memory one was confirmed to fail against the pre-fix
code. One existing server test (`test_server_batch_patch_tool`) asserted
`total_files_patched == 1` for a dry run over an in-memory buffer — it was codifying the bug
and was updated to assert 0.

### 2. `outline` panics with `format:"skeleton"` + `max_output_bytes` on non-ASCII source — CONFIRMED
`crates/transcend-core/src/outline/scanner.rs:443`

`skel.truncate(bytes_remaining)` used a byte budget as a `String` index; `String::truncate`
panics off a char boundary. Reproduced by driving `OutlineScanner::scan` across every budget
in `51..2800` on a multi-byte fixture: **1,779 of 2,749 budgets (65%) panicked** with
`assertion failed: self.is_char_boundary(new_len)`. The call returns an opaque
`engine task failed: task N panicked` and no data.

`max_output_bytes` is a documented byte budget, and `SERVER_INSTRUCTIONS` actively recommends
`format: "skeleton"`, so any file with a non-ASCII doc comment, string literal or identifier
can trip this.

**Fixed:** `floor_char_boundary` walks the cut back to a real boundary. Fixing it exposed a
second defect my test caught: the `"...[truncated]"` marker was appended *after* truncating to
the budget, so the result exceeded the caller's budget; the marker length is now reserved
first. Two regression tests added, both confirmed to fail against the pre-fix code.

### 3. `lsp_diagnostics` with `path` always returns zero diagnostics from the LSP cache
`lsp/mod.rs:209`, `lsp/distiller.rs:281`, `lsp/session.rs:161-165`; contrast `lsp/fallback.rs:245-248`

The filter compares the **absolute** resolved path (the engine resolves it) against the cache's
**relative** keys using `contains` — arithmetically impossible to match. The compiler fallback
in the same crate uses `ends_with` and therefore works.

*Reported verification:* warm rust-analyzer session, unfiltered → `total_count=5` (keys are
relative, e.g. `src/lib.rs`); same session with `path` as a file or directory → `0`. An agent
asking "does this file compile?" is told there are no problems.

**Fix:** suffix-normalized comparison, or store absolute paths in the cache.

## MEDIUM

4. **`outline` drops files past `max_files` while reporting `truncated:false`** —
   `scanner.rs:258-260`, `:270`. The walk caps at `max_files*2` candidates and parses
   `max_files`; `truncated` only reflects the symbol/byte budget. 25-file dir → 20 files,
   `summary.total_files=20`, `truncated=false`. `OutlineResponse.truncated` is documented
   "capped by symbol **or file budget**".
5. **`find_symbol` can omit the exact match** — `find_symbol.rs:200`, `:252`, `:271-280`.
   The `(limit*10).max(200)` candidate cap fills in traversal order *before* the exact-first
   sort, so a late-discovered exact match is discarded. 250 partials + 1 exact →
   `{exact:false, limit:5}` returned 5 partials, exact absent.
6. **`find_symbol.case_sensitive` — WITHDRAWN as a behaviour bug; the doc was wrong.**
   Reported as a default contradicting its contract (`find_symbol.rs:70` uses
   `unwrap_or(false)`, doc said "Defaults to true for exact matches"). I attempted the
   suggested fix (`unwrap_or(exact)`) and it broke
   `test_find_symbol_smart_casing_and_fuzzy_subsequence`: resolving `setup_vmcs` →
   `SetupVmcs` *requires* case-insensitivity in exact mode. Smart casing is a deliberate,
   tested feature, so the implementation is right and the **documentation** was
   unsatisfiable. Corrected the doc comment in `crates/transcend-protocol/src/lib.rs`
   instead of changing behaviour. Leaving the code alone was the correct call.
7. **`exec` ignores `max_output_bytes` on the `timeout_action:"kill"` path** —
   `terminal/mod.rs:174-186` hardcodes `truncated:false` over an uncapped snapshot. 12,690
   chars returned for a 100-byte budget, vs 97 chars + `truncated:true` on the normal path.
8. **`lsp_references.include_declaration` ignored on the fallback** — `lsp/mod.rs:143-145`,
   `lsp/fallback.rs:84-104`. Default (documented `false`) and `true` return byte-identical sets
   including the definition.
9. **`read_file.max_bytes` is enforced as a *char* budget** — `file_ops.rs:220-221` compares
   `content.len()` (bytes) but consumes it via `chars().take(remaining_budget)`, so a
   multi-byte file can return up to 4x the documented bytes. `max_bytes:10` on a file of `é`
   returned 20 UTF-8 bytes.
10. **`search` on a single file reports `file:""`** — `search.rs:141-145`. The root *is* the
    file, so `strip_prefix` yields `""`. `outline` handles this case deliberately
    (`scanner.rs:294-298`), so the omission looks accidental.
11. **`search` reports `truncated:false` while `max_per_file` capped matches** — `search.rs:253`
    only compares the global count to `max_matches`. Per-cluster `matches_truncated` carries the
    truth, so the top-level flag is inconsistent rather than fatal.
12. **`OutlineOptions.symbol_kinds`/`exported_only` are no-ops for some languages** —
    `outline/sql.rs`, `markdown.rs` read neither; `ruby.rs`, `bash.rs` lack `exported_only`.
    SQL with `symbol_kinds:["function"]` returned `[struct, function]`; the Rust control
    filtered correctly. **Fix:** apply the filter centrally in `parse_bytes`.

## LOW

13. **`ast_valid`/`all_ast_valid: true` when no grammar exists** — `patch.rs:340`, `:381`, `:645`.
    For `.json`/`.toml`/`.txt` the preflight is skipped but validity is still asserted:
    `data.json` corrupted to `{"a":(((((` with `all_ast_valid:true`.
14. **`lsp_status` returns an empty list for an unknown language** while `lsp_install` errors —
    `installer.rs:324-328` vs `:379-383`. A typo is indistinguishable from "no servers installed".
15. **Shared workspace root makes read-only results order-dependent** — `lib.rs:180`,
    `:236-254`, `:370-394`. Read-only requests expose no `workspace_root` to pin, and rmcp
    dispatches concurrently. Observed: a no-path `find_symbol` raced `set_workspace` and
    answered from the previous root while a concurrent `search` saw the new one.

## Not yet traced

Per-language `exported_only`/`include_relationships` semantics across all 20 outline adapters;
`git_ops` porcelain-v2 parsing of quoted/special-character paths (`git_ops.rs:121-125`,
`:156-170`); `terminal_read.wait_for_pattern` timeout-without-match reporting;
`ExecRequest.raw` on the PTY path and its whitespace tokenizer; `batch_patch.total_files_patched`
counting requested rather than written files.
