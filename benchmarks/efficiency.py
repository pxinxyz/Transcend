#!/usr/bin/env python3
"""Measure Transcend against the CLI equivalents an agent would otherwise scrape.

Two questions, both of which the first comparison in this repository answered only
partially:

1. **Cost** -- how many tokens does the payload actually cost? Measured on the text a host
   hands the model, so protocol framing is excluded. Tokenised with a real tokenizer,
   because `chars/4` is not a safe approximation for JSON (it overestimates structured
   output) or for CLI text.

2. **Overhead share** -- how much of that cost is signal? Transcend bundles a directory
   radar, an extension breakdown and per-symbol spans into responses; some of that is
   genuinely useful (it tells you where matches cluster) and some is not. Reported as a
   separate column rather than assumed in either direction.

3. **Answerability** -- for tasks where the harness knows the ground truth, does the
   payload contain it? A cheaper tool that omits the answer is not cheaper.

Usage:
    python benchmarks/efficiency.py --corpus <path-to-a-rust-checkout>
    python benchmarks/efficiency.py --corpus <path> --binary target/release/transcend.exe
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mcp_client import McpSession  # noqa: E402

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ENCODING = "o200k_base"


def tokenizer():
    try:
        import tiktoken
    except ImportError:
        sys.exit("tiktoken is required: pip install tiktoken")
    enc = tiktoken.get_encoding(ENCODING)

    def count(text: str) -> int:
        return len(enc.encode(text, disallowed_special=()))

    return count


def run(cmd: list[str], cwd: str, shell: bool = False) -> str:
    """Run a CLI equivalent and return stdout -- what an agent would scrape."""
    p = subprocess.run(
        cmd, cwd=cwd, shell=shell, capture_output=True, text=True, encoding="utf-8", errors="replace"
    )
    return p.stdout


def run_both(cmd: list[str], cwd: str) -> tuple[str, int]:
    """Return stdout+stderr and the exit code.

    Compilers report on stderr, so a measurement that only reads stdout sees an empty
    result for a failing build -- which is exactly the case a diagnostics task cares about.
    """
    p = subprocess.run(
        cmd, cwd=cwd, capture_output=True, text=True, encoding="utf-8", errors="replace"
    )
    return (p.stdout + p.stderr), p.returncode


# ---------------------------------------------------------------------------
# Overhead classification
# ---------------------------------------------------------------------------

# Keys whose contents are navigation/aggregate metadata rather than the answer body.
OVERHEAD_KEYS = ("directory_radar", "extension_breakdown", "kind_breakdown",
                 "language_breakdown", "severity_breakdown", "summary", "size_bytes",
                 "modified", "qualified_name", "signature", "doc_comment", "visibility",
                 "span", "children", "total_lines", "next_cursor", "elapsed_ms")


def overhead_tokens(payload: str, count) -> int:
    """Tokens spent on metadata fields, measured by re-serialising without them.

    Approximate by construction: it reports the cost of the named fields, not a claim that
    every one of them is useless. `directory_radar` is a real feature; it is simply not the
    answer to "find me five results".
    """
    try:
        doc = json.loads(payload)
    except (json.JSONDecodeError, TypeError):
        return 0
    removed = 0

    def strip(node):
        nonlocal removed
        if isinstance(node, dict):
            for k in list(node):
                if k in OVERHEAD_KEYS:
                    removed += count(json.dumps(node[k], separators=(",", ":")))
                    del node[k]
                else:
                    strip(node[k])
        elif isinstance(node, list):
            for item in node:
                strip(item)

    strip(doc)
    return removed


# ---------------------------------------------------------------------------
# Tasks
# ---------------------------------------------------------------------------

class Result:
    def __init__(self, task, approach, payload, tokens, overhead, answerable, note=""):
        self.task = task
        self.approach = approach
        self.payload = payload
        self.tokens = tokens
        self.overhead = overhead
        self.answerable = answerable
        self.note = note


def eval_tasks(session: McpSession, corpus: str, count) -> list[Result]:
    out: list[Result] = []

    # --- T1: locate every occurrence of a pattern -------------------------
    pat = "from_utf8_lossy"
    t = session.call("search", {"pattern": pat, "options": {"max_matches": 30}})
    tt = count(t)
    doc = json.loads(t) if t.startswith("{") else {}
    out.append(Result(
        "count+locate matches", "transcend search", t, tt, overhead_tokens(t, count),
        bool(doc.get("files")), f"{doc.get('total_matches')} matches",
    ))
    c = run(["rg", "-n", "--no-heading", pat], corpus)
    out.append(Result(
        "count+locate matches", "rg -n", c, count(c), 0,
        bool(c.strip()), f"{len(c.splitlines())} lines",
    ))

    # --- T2: scoped search, small budget ---------------------------------
    t = session.call("search", {"pattern": pat, "options": {"max_matches": 5}})
    out.append(Result(
        "5 matches only", "transcend search", t, count(t), overhead_tokens(t, count), True,
    ))
    c = run(["rg", "-n", "--no-heading", "-m", "5", pat], corpus)
    out.append(Result("5 matches only", "rg -m 5", c, count(c), 0, True))

    # --- T3: read one function by name -----------------------------------
    sym = "quit"
    sym_file = "crates/searcher/src/searcher/mod.rs"
    t = session.call("read_symbol", {"path": sym_file, "symbol": sym})
    d = json.loads(t) if t.startswith("{") else {}
    c = run(["rg", "-n", "-A", "20", rf"\bfn {sym}\b", sym_file], corpus)
    out.append(Result("read a known function", "transcend read_symbol", t, count(t),
                      overhead_tokens(t, count), bool(d.get("found")),
                      f"found={d.get('found')}"))
    out.append(Result("read a known function", "rg -A20", c, count(c), 0, bool(c.strip()),
                      f"{len(c.splitlines())} lines"))

    # --- T3b: same task, but the name does not exist ----------------------
    # The failure path is a real agent cost and the earlier comparison ignored it: the
    # agent still pays for the answer, then pays again for the retry.
    t = session.call("read_symbol", {"path": sym_file, "symbol": "no_such_symbol_xyz"})
    d = json.loads(t) if t.startswith("{") else {}
    out.append(Result("look up a symbol that isn't there", "transcend read_symbol", t, count(t),
                      overhead_tokens(t, count), bool(d.get("message")),
                      "returns the file's available symbol names"))
    c = run(["rg", "-n", r"\bno_such_symbol_xyz\b", sym_file], corpus)
    c = c.strip() or "(no output)"
    out.append(Result("look up a symbol that isn't there", "rg (empty result)", c, count(c), 0,
                      True, "silent: cannot distinguish absent from misspelled"))

    # --- T4: file discovery ----------------------------------------------
    t = session.call("find", {"pattern": "*.rs", "options": {"max_results": 20}})
    doc = json.loads(t) if t.startswith("{") else {}
    out.append(Result("list 20 rust files", "transcend find", t, count(t),
                      overhead_tokens(t, count), len(doc.get("entries", [])) == 20))
    c = run(["rg", "--files", "-g", "*.rs"], corpus)
    lines = c.splitlines()[:20]
    c20 = "\n".join(lines)
    out.append(Result("list 20 rust files", "rg --files | head -20", c20, count(c20), 0,
                      len(lines) == 20))

    # --- T5: structural overview -----------------------------------------
    # Both sides capped to the same symbol budget, so this compares shape rather than how
    # much each tool was allowed to return.
    budget = 40
    t = session.call("outline", {"path": "crates/searcher/src/searcher",
                                 "options": {"max_symbols": budget}})
    doc = json.loads(t) if t.startswith("{") else {}
    c = run(["rg", "-n", r"^\s*(pub )?(fn|struct|enum|impl|trait) ",
             "crates/searcher/src/searcher"], corpus)
    c_capped = "\n".join(c.splitlines()[:budget])
    out.append(Result("outline a module", "transcend outline", t, count(t),
                      overhead_tokens(t, count), bool(doc.get("files")),
                      f"max_symbols={budget}"))
    out.append(Result("outline a module", "rg declarations (same cap)", c_capped,
                      count(c_capped), 0, bool(c_capped.strip()),
                      f"first {budget} lines"))

    # --- T6: git status ---------------------------------------------------
    t = session.call("git_status", {})
    out.append(Result("repo dirty state", "transcend git_status", t, count(t),
                      overhead_tokens(t, count), t.startswith("{")))
    c = run(["git", "status", "--porcelain=v2", "--branch"], corpus)
    out.append(Result("repo dirty state", "git status --porcelain=v2", c, count(c), 0, True))

    return out


def diagnostic_task(session: McpSession, corpus: str, workdir: str, count) -> list[Result]:
    """T7: does a file actually compile? Ground truth is established independently.

    This is the task where a wrong answer is worse than an expensive one: `cargo check`
    gives ground truth, so a tool that reports no problems on a broken file is scored as
    *unanswerable* however few tokens it costs.
    """
    out: list[Result] = []
    probe = os.path.join(workdir, "diag_probe")
    os.makedirs(os.path.join(probe, "src"), exist_ok=True)
    with open(os.path.join(probe, "Cargo.toml"), "w", encoding="utf-8") as fh:
        # The empty [workspace] table is load-bearing: without it cargo treats this crate as
        # a member of the enclosing Transcend workspace and refuses to build it, which
        # silently destroys the ground truth this task depends on.
        fh.write(
            '[package]\nname = "diag_probe"\nversion = "0.1.0"\nedition = "2021"\n\n'
            "[workspace]\n"
        )
    # A genuine type error, plus a call to a function that does not exist.
    with open(os.path.join(probe, "src", "lib.rs"), "w", encoding="utf-8") as fh:
        fh.write(
            "pub fn build() -> u32 {\n"
            '    let s: String = 42;\n'          # mismatched types
            "    missing_helper(s)\n"            # cannot find function
            "}\n"
        )

    # Ground truth. If cargo cannot run, the task is meaningless -- say so rather than
    # scoring every approach against a false negative.
    cargo, cargo_exit = run_both(["cargo", "check", "--message-format=short"], probe)
    truth_has_error = "error" in cargo.lower()
    if not cargo.strip():
        raise RuntimeError(
            "cargo check produced no output for the probe crate; ground truth is unavailable "
            f"(cwd={probe})"
        )

    # What a naive agent does: check the exit code alone.
    _, naive_exit = run_both(["cargo", "check"], probe)
    exitcode_view = f"exit={naive_exit}"
    naive_ok = naive_exit == 0

    # What a naive agent does: check the exit code. `cargo check` reports on stderr.
    exitcode_view = f"exit={naive_exit}"

    t = session.call("lsp_diagnostics", {"path": os.path.join(probe, "src", "lib.rs")})
    doc = json.loads(t) if t.startswith("{") else {}
    lsp_total = doc.get("total_count")
    lsp_ok = bool(lsp_total)

    out.append(Result(
        "does this file compile?", "transcend lsp_diagnostics (with path)", t, count(t),
        overhead_tokens(t, count), lsp_ok,
        f"total_count={lsp_total}; ground truth: {truth_has_error} errors exist",
    ))
    out.append(Result(
        "does this file compile?", "cargo check --message-format=short", cargo, count(cargo),
        0, truth_has_error, "ground truth source",
    ))
    out.append(Result(
        "does this file compile?", "exit code only (naive)", exitcode_view, count(exitcode_view),
        0, not naive_ok,
        "WRONG: exit code alone cannot distinguish 'clean' from 'not run'",
    ))
    return out


# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------

def report(results: list[Result]) -> None:
    print(f"\n{'task':<28}{'approach':<36}{'tokens':>8}{'overhd':>8}{'answer?':>9}")
    print("-" * 89)
    last = None
    for r in results:
        flag = "yes" if r.answerable else "NO"
        over = f"{r.overhead / r.tokens * 100:.0f}%" if r.tokens and r.overhead else "-"
        task = r.task if r.task != last else ""
        last = r.task
        print(f"{task:<28}{r.approach:<36}{r.tokens:>8,}{over:>8}{flag:>9}")
        if r.note:
            print(f"{'':<28}  note: {r.note}")

    print("\n--- where Transcend wins or loses (same task, both answerable) ---")
    by_task: dict[str, list[Result]] = {}
    for r in results:
        by_task.setdefault(r.task, []).append(r)
    for task, rs in by_task.items():
        good = [r for r in rs if r.answerable]
        if len(good) < 2:
            bad = [r for r in rs if not r.answerable]
            if bad:
                print(f"  {task}: {len(bad)} approach(es) could not answer")
            continue
        good.sort(key=lambda r: r.tokens)
        best, worst = good[0], good[-1]
        print(f"  {task}: {best.approach} {best.tokens:,} vs {worst.approach} {worst.tokens:,}"
              f"  -> {worst.tokens / best.tokens:.2f}x")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--corpus", required=True, help="path to a Rust checkout to measure against")
    ap.add_argument("--binary", default=os.path.join(REPO, "target", "debug", "transcend.exe"))
    ap.add_argument("--out", default=os.path.join(REPO, "target", "efficiency.json"))
    ap.add_argument("--run-diagnostics", action="store_true",
                    help="also run the compile-check task (needs cargo; uses a scratch crate)")
    args = ap.parse_args()

    if not os.path.isdir(args.corpus):
        sys.exit(f"corpus not found: {args.corpus}")
    if not os.path.isfile(args.binary):
        sys.exit(f"server binary not found: {args.binary} (cargo build --bin transcend)")

    count = tokenizer()
    print(f"tokenizer: {ENCODING}")
    print(f"corpus   : {args.corpus}")
    print(f"binary   : {args.binary}")

    session = McpSession(args.binary, REPO, workspace=args.corpus)
    try:
        results = eval_tasks(session, args.corpus, count)
        if args.run_diagnostics:
            scratch = os.path.join(REPO, "target", "efficiency-scratch")
            os.makedirs(scratch, exist_ok=True)
            try:
                results += diagnostic_task(session, args.corpus, scratch, count)
            except Exception as exc:  # noqa: BLE001 - a failing probe must not lose the rest
                print(f"\n[diagnostic task skipped: {exc}]")
    finally:
        session.close()

    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as fh:
        json.dump([
            {"task": r.task, "approach": r.approach, "tokens": r.tokens,
             "overhead_tokens": r.overhead, "answerable": r.answerable, "note": r.note}
            for r in results
        ], fh, indent=2)
    report(results)
    print(f"\nwrote {os.path.relpath(args.out, REPO)}")


if __name__ == "__main__":
    main()
