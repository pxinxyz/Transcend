"""Probe every Transcend tool for confidently-wrong or silently-misleading output.

Each check states an expectation that follows from the tool's own documentation, then
reports whether the live server honours it. Designed to be run against a real server so
the answers come from behaviour, not from reading the code.

Usage: python benchmarks/probe_contracts.py [--binary target/debug/transcend.exe]
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mcp_client import McpSession  # noqa: E402

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

FINDINGS: list[tuple[str, str, str, bool]] = []


def check(tool: str, expectation: str, detail: str, ok: bool) -> None:
    FINDINGS.append((tool, expectation, detail, ok))
    mark = "ok  " if ok else "FAIL"
    print(f"{mark} [{tool}] {expectation}")
    if not ok:
        print(f"       -> {detail}")


def j(payload: str) -> dict:
    try:
        return json.loads(payload)
    except json.JSONDecodeError:
        return {}


def run_probes(s: McpSession, scratch: str) -> None:
    os.makedirs(os.path.join(scratch, "sub"), exist_ok=True)
    sample = os.path.join(scratch, "sample.rs")
    with open(sample, "w", encoding="utf-8") as fh:
        fh.write(
            "pub fn alpha() -> u32 { 1 }\n\n"
            "fn caller() -> u32 { alpha() }\n\n"
            "pub struct Widget { pub size: u32 }\n"
        )
    empty = os.path.join(scratch, "empty.rs")
    open(empty, "w").close()

    # ---- read_file: can a failure be told from an empty file? ------------
    # Before `success` existed these were identical in every field a caller would gate on --
    # empty content, truncated false, zero line counts -- so an agent reading `content` saw a
    # missing file as an empty one.
    missing = j(s.call("read_file", {"path": os.path.join(scratch, "nope.rs")}))
    empty_res = j(s.call("read_file", {"path": empty}))
    directory_res = j(s.call("read_file", {"path": scratch}))
    check(
        "read_file", "a missing file reports success=false",
        f"success={missing.get('success')} content={missing.get('content')!r}",
        missing.get("success") is False,
    )
    check(
        "read_file", "a directory reports success=false",
        f"success={directory_res.get('success')}",
        directory_res.get("success") is False,
    )
    check(
        "read_file", "an empty file reports success=true, not a failure",
        f"success={empty_res.get('success')} content={empty_res.get('content')!r}",
        empty_res.get("success") is True,
    )

    # ---- read_symbol: does a miss look like a hit? -----------------------
    r = j(s.call("read_symbol", {"path": sample, "symbol": "does_not_exist"}))
    check(
        "read_symbol", "a miss sets found=false rather than returning empty source",
        f"found={r.get('found')} source={r.get('source_code')!r}",
        r.get("found") is False,
    )
    check(
        "read_symbol", "a miss does not claim a qualified_name",
        f"qualified_name={r.get('qualified_name')!r}",
        not r.get("qualified_name"),
    )

    # ---- find_symbol: is a zero-result search distinguishable? -----------
    r = j(s.call("find_symbol", {"name": "zzz_absent_symbol", "path": scratch}))
    check(
        "find_symbol", "zero results reports total_found=0",
        f"total_found={r.get('total_found')}",
        r.get("total_found") == 0,
    )

    # ---- search: truncation honesty -------------------------------------
    r = j(s.call("search", {"pattern": "alpha", "path": scratch,
                            "options": {"max_matches": 1}}))
    check(
        "search", "a capped result sets truncated=true",
        f"truncated={r.get('truncated')} total={r.get('total_matches')}",
        r.get("truncated") is True or (r.get("total_matches") or 0) <= 1,
    )

    # ---- search: does a file root report its own name? -------------------
    r = j(s.call("search", {"pattern": "alpha", "path": sample}))
    files = [f.get("file") for f in (r.get("files") or [])]
    check(
        "search", "searching one file names that file in the cluster",
        f"files={files}",
        all(f for f in files) if files else False,
    )

    # ---- outline: does max_files truncation get reported? ----------------
    r = j(s.call("outline", {"path": scratch, "options": {"max_files": 1}}))
    check(
        "outline", "a file-budget cap sets truncated=true",
        f"truncated={r.get('truncated')} files={len(r.get('files') or [])}",
        r.get("truncated") is True or len(r.get("files") or []) <= 1,
    )

    # ---- exec: is max_output_bytes honoured? ----------------------------
    r = j(s.call("exec", {"command": "echo hello", "max_output_bytes": 3}))
    check(
        "exec", "max_output_bytes bounds the returned output",
        f"len={len(r.get('output') or '')} truncated={r.get('truncated')}",
        len(r.get("output") or "") <= 3 or r.get("truncated") is True,
    )

    # ---- git_status: does it name the repo? ------------------------------
    r = j(s.call("git_status", {}))
    check(
        "git_status", "reports whether the target is a git repo",
        f"is_git_repo={r.get('is_git_repo')}",
        "is_git_repo" in r,
    )

    # ---- lsp_status: unknown language ------------------------------------
    raw = s.call("lsp_status", {"language": "javascript"})
    r = j(raw)
    check(
        "lsp_status", "an unknown language is refused, not reported as an empty host",
        f"total_servers={r.get('total_servers')} raw={raw[:120]!r}",
        raw.startswith(("[rpc-error]", "[tool-error]")) or (r.get("total_servers") or 0) > 0,
    )

    # ---- terminal_*: a missing session ------------------------------------
    raw = s.call("terminal_read", {"session_id": "pty_does_not_exist"})
    r = j(raw)
    check(
        "terminal_read", "an unknown session is refused, not returned as an empty read",
        f"raw={raw[:120]!r} status={r.get('status')!r}",
        raw.startswith(("[rpc-error]", "[tool-error]")) or "error" in r,
    )

    # ---- delete_path: a missing target ------------------------------------
    r = j(s.call("delete_path", {"path": os.path.join(scratch, "never_existed.rs")}))
    check(
        "delete_path", "a missing target reports success=false",
        f"success={r.get('success')}",
        r.get("success") is False,
    )

    # ---- write_file: does overwrite:false refuse? -------------------------
    r = j(s.call("write_file", {"path": sample, "content": "clobber"}))
    after = open(sample, encoding="utf-8").read()
    check(
        "write_file", "overwrite defaults to false and does not clobber",
        f"success={r.get('success')} file_intact={'alpha' in after}",
        r.get("success") is False and "alpha" in after,
    )

    # ---- patch dry_run: does it write? -----------------------------------
    before = open(sample, encoding="utf-8").read()
    r = j(s.call("patch", {"path": sample, "target_symbol": "alpha",
                           "replacement": "pub fn alpha() -> u32 { 2 }",
                           "dry_run": True}))
    check(
        "patch", "dry_run leaves the file untouched",
        f"success={r.get('success')} unchanged={open(sample, encoding='utf-8').read() == before}",
        open(sample, encoding="utf-8").read() == before,
    )

    # ---- batch_patch dry_run: count honesty -------------------------------
    r = j(s.call("batch_patch", {"patches": [{"path": sample, "target_symbol": "alpha",
                                              "replacement": "pub fn alpha() -> u32 { 3 }"}],
                                 "dry_run": True}))
    check(
        "batch_patch", "a dry run reports zero files patched",
        f"total_files_patched={r.get('total_files_patched')}",
        r.get("total_files_patched") == 0,
    )

    # ---- lsp_references: unknown symbol ----------------------------------
    r = j(s.call("lsp_references", {"path": sample, "symbol": "zzz_absent"}))
    check(
        "lsp_references", "an unknown symbol returns no references",
        f"total_found={r.get('total_found')}",
        (r.get("total_found") or 0) == 0,
    )

    # ---- lsp_diagnostics: the false negative fixed earlier ---------------
    broken = os.path.join(scratch, "broken_crate")
    os.makedirs(os.path.join(broken, "src"), exist_ok=True)
    with open(os.path.join(broken, "Cargo.toml"), "w", encoding="utf-8") as fh:
        fh.write('[package]\nname = "broken_crate"\nversion = "0.1.0"\nedition = "2021"\n\n'
                 "[workspace]\n")
    lib = os.path.join(broken, "src", "lib.rs")
    with open(lib, "w", encoding="utf-8") as fh:
        fh.write("pub fn b() -> u32 {\n    let s: String = 42;\n    nope(s)\n}\n")
    p = subprocess.run(["cargo", "check", "--message-format=short"], cwd=broken,
                       capture_output=True, text=True)
    truth = "error" in (p.stdout + p.stderr).lower()
    r = j(s.call("lsp_diagnostics", {"path": lib}))
    check(
        "lsp_diagnostics", "a file with real compiler errors is not reported clean",
        f"total_count={r.get('total_count')} ground_truth_has_errors={truth}",
        bool(r.get("total_count")) == truth,
    )


def run_param_probes(s: McpSession, scratch: str) -> None:
    """Do the documented parameters actually change the output?

    A parameter that is accepted, documented, and silently ignored is worse than an absent
    one: the caller receives a filtered-looking result that was never filtered.
    """
    tree = os.path.join(scratch, "tree")
    shutil.rmtree(tree, ignore_errors=True)
    os.makedirs(os.path.join(tree, "deep", "deeper"), exist_ok=True)
    os.makedirs(os.path.join(tree, "alpha"), exist_ok=True)
    for name in ("root_one.rs", "root_two.py"):
        open(os.path.join(tree, name), "w").write("pub fn root_marker() {}\n")
    open(os.path.join(tree, "deep", "mid.rs"), "w").write("pub fn mid_marker() {}\n")
    open(os.path.join(tree, "deep", "deeper", "low.rs"), "w").write("pub fn low_marker() {}\n")
    open(os.path.join(tree, "alpha", "a1.rs"), "w").write("pub fn alpha_marker() {}\n")
    open(os.path.join(tree, "notes.txt"), "w").write("text_marker\n")
    open(os.path.join(tree, ".hidden.rs"), "w").write("pub fn hidden_marker() {}\n")

    # find: max_depth must actually bound the walk
    r = j(s.call("find", {"pattern": "*.rs", "path": tree, "options": {"max_depth": 1}}))
    paths = [e.get("path", "") for e in (r.get("entries") or [])]
    check(
        "find", "max_depth=1 excludes nested files",
        f"paths={paths}",
        bool(paths) and not any(("deep" in p) for p in paths),
    )

    # find: extension must filter
    r = j(s.call("find", {"pattern": "*", "path": tree, "options": {"extension": "rs"}}))
    paths = [e.get("path", "") for e in (r.get("entries") or [])]
    check(
        "find", "extension=rs excludes non-rs files",
        f"paths={paths}",
        bool(paths) and not any(p.endswith(".txt") for p in paths),
    )

    # find: include_hidden
    default_hidden = len(j(s.call("find", {"pattern": "*hidden*", "path": tree})).get("entries") or [])
    with_hidden = len(
        j(s.call("find", {"pattern": "*hidden*", "path": tree,
                          "options": {"include_hidden": True}})).get("entries") or []
    )
    check(
        "find", "include_hidden=true surfaces dotfiles",
        f"default={default_hidden} with_hidden={with_hidden}",
        with_hidden > default_hidden,
    )

    # search: context_lines must add surrounding text. It attaches `context_before` /
    # `context_after` to each match rather than emitting extra matches -- asserting a higher
    # match count here was the probe's error, not the tool's.
    ctx_file = os.path.join(scratch, "ctx.rs")
    open(ctx_file, "w").write("line_a\nline_b\nctx_marker\nline_d\nline_e\n")

    def ctx_of(n):
        res = j(s.call("search", {"pattern": "ctx_marker", "path": ctx_file,
                                  "options": {"context_lines": n}}))
        files = res.get("files") or []
        matches = (files[0].get("matches") if files else []) or []
        return matches[0] if matches else {}

    m0, m2 = ctx_of(0), ctx_of(2)
    check(
        "search", "context_lines=2 attaches two lines either side",
        f"before={m2.get('context_before')} after={m2.get('context_after')}",
        len(m2.get("context_before") or []) == 2 and len(m2.get("context_after") or []) == 2,
    )
    check(
        "search", "context_lines=0 attaches no context",
        f"before={m0.get('context_before')} after={m0.get('context_after')}",
        not (m0.get("context_before") or m0.get("context_after")),
    )

    # search: max_line_length must clip long lines
    long_file = os.path.join(scratch, "long.rs")
    open(long_file, "w").write("long_marker " + ("x" * 400) + "\n")
    r = j(s.call("search", {"pattern": "long_marker", "path": long_file,
                            "options": {"max_line_length": 20}}))
    line = ((r.get("files") or [{}])[0].get("matches") or [{}])[0].get("line_text", "")
    check(
        "search", "max_line_length=20 clips the returned line",
        f"len={len(line)}",
        0 < len(line) <= 60,
    )

    # outline: max_depth must bound the hierarchy
    r = j(s.call("outline", {"path": tree, "options": {"max_depth": 1}}))

    def max_children(node, depth=0):
        kids = node.get("children") or []
        return max([depth] + [max_children(c, depth + 1) for c in kids]) if kids else depth

    deepest = 0
    for f in r.get("files") or []:
        for sym in f.get("symbols") or []:
            deepest = max(deepest, max_children(sym))
    check(
        "outline", "max_depth=1 bounds symbol nesting",
        f"deepest_nesting={deepest}",
        deepest <= 1,
    )

    # outline: include_doc_comments=false must drop doc comments
    doc_file = os.path.join(scratch, "doc.rs")
    open(doc_file, "w").write("/// A documented function.\npub fn documented() {}\n")
    r_on = j(s.call("outline", {"path": doc_file, "options": {"include_doc_comments": True}}))
    r_off = j(s.call("outline", {"path": doc_file, "options": {"include_doc_comments": False}}))

    def has_doc(res):
        return any(sym.get("doc_comment")
                   for f in res.get("files") or [] for sym in f.get("symbols") or [])

    check(
        "outline", "include_doc_comments=false omits doc comments",
        f"on={has_doc(r_on)} off={has_doc(r_off)}",
        has_doc(r_on) and not has_doc(r_off),
    )

    # find_symbol: limit must bound results
    many = os.path.join(scratch, "many.rs")
    open(many, "w").write("".join(f"pub fn lim_{i}() {{}}\n" for i in range(40)))
    r = j(s.call("find_symbol", {"name": "lim_", "path": scratch, "exact": False,
                                 "limit": 3, "fuzzy": True}))
    check(
        "find_symbol", "limit=3 bounds the returned symbols",
        f"returned={len(r.get('symbols') or [])} total={r.get('total_found')}",
        len(r.get("symbols") or []) <= 3,
    )

    # read_file: start_line/end_line must slice
    r = j(s.call("read_file", {"path": ctx_file, "start_line": 2, "end_line": 3,
                               "line_numbers": True}))
    check(
        "read_file", "start_line/end_line slice and line_numbers prefixes",
        f"start={r.get('start_line')} end={r.get('end_line')} content={r.get('content')!r}",
        r.get("start_line") == 2 and r.get("end_line") == 3
        and "ctx_marker" in (r.get("content") or ""),
    )

    # read_symbol: context_lines must add surroundings
    r2 = j(s.call("read_symbol", {"path": os.path.join(scratch, "sample.rs"), "symbol": "alpha",
                                  "context_lines": 2}))
    check(
        "read_symbol", "context_lines adds surrounding text",
        f"keys={sorted(r2.keys())}",
        ("context_before" in r2) or ("context_after" in r2),
    )

    # search: respect_gitignore as an independent axis
    gi_dir = os.path.join(scratch, "gi")
    os.makedirs(gi_dir, exist_ok=True)
    p = subprocess.run(["git", "init", "-q"], cwd=gi_dir, capture_output=True, text=True)
    if p.returncode == 0:
        open(os.path.join(gi_dir, ".gitignore"), "w").write("ignored.rs\n")
        open(os.path.join(gi_dir, "ignored.rs"), "w").write("gi_marker\n")
        open(os.path.join(gi_dir, "kept.rs"), "w").write("gi_marker\n")
        on = j(s.call("search", {"pattern": "gi_marker", "path": gi_dir,
                                 "options": {"respect_gitignore": True}}))
        off = j(s.call("search", {"pattern": "gi_marker", "path": gi_dir,
                                  "options": {"respect_gitignore": False}}))
        check(
            "search", "respect_gitignore=false finds gitignored files",
            f"on={on.get('total_matches')} off={off.get('total_matches')}",
            (off.get("total_matches") or 0) > (on.get("total_matches") or 0),
        )

    # ---- find: the remaining documented options --------------------------
    sizes = os.path.join(scratch, "sizes")
    shutil.rmtree(sizes, ignore_errors=True)
    os.makedirs(sizes)
    open(os.path.join(sizes, "small.txt"), "w").write("x")
    open(os.path.join(sizes, "large.txt"), "w").write("y" * 5000)
    open(os.path.join(sizes, "medium.txt"), "w").write("z" * 500)

    # sort_by=size must order largest first, as documented.
    r = j(s.call("find", {"pattern": "*.txt", "path": sizes, "options": {"sort_by": "size"}}))
    order = [os.path.basename(e.get("path", "")) for e in r.get("entries") or []]
    check(
        "find", "sort_by=size orders largest first",
        f"order={order}",
        order[:1] == ["large.txt"],
    )

    # sort_by=path must be alphabetical.
    r = j(s.call("find", {"pattern": "*.txt", "path": sizes, "options": {"sort_by": "path"}}))
    order = [os.path.basename(e.get("path", "")) for e in r.get("entries") or []]
    check(
        "find", "sort_by=path orders alphabetically",
        f"order={order}",
        order == sorted(order),
    )

    # exclude must actually exclude.
    r = j(s.call("find", {"pattern": "*.txt", "path": sizes,
                          "options": {"exclude": ["large.txt"]}}))
    paths = [os.path.basename(e.get("path", "")) for e in r.get("entries") or []]
    check(
        "find", "exclude removes matching files",
        f"paths={paths}",
        bool(paths) and "large.txt" not in paths,
    )

    # file_type=directory must return directories, not files.
    r = j(s.call("find", {"pattern": "*", "path": tree,
                          "options": {"file_type": "directory"}}))
    paths = [e.get("path", "") for e in r.get("entries") or []]
    check(
        "find", "file_type=directory does not return regular files",
        f"paths={paths}",
        bool(paths) and not any(p.endswith(".rs") or p.endswith(".txt") for p in paths),
    )

    # max_per_dir must bound results from any one directory.
    bulk = os.path.join(scratch, "bulk")
    shutil.rmtree(bulk, ignore_errors=True)
    os.makedirs(bulk)
    for i in range(20):
        open(os.path.join(bulk, f"b{i:02}.rs"), "w").write("pub fn b() {}\n")
    r = j(s.call("find", {"pattern": "*.rs", "path": bulk,
                          "options": {"max_per_dir": 3, "max_results": 50}}))
    n = len(r.get("entries") or [])
    check(
        "find", "max_per_dir=3 bounds results from one directory",
        f"returned={n}",
        n <= 3,
    )

    # ---- find: does a capping mean the caller is told? -------------------
    r = j(s.call("find", {"pattern": "*.rs", "path": bulk, "options": {"max_results": 2}}))
    check(
        "find", "a max_results cap sets truncated=true",
        f"truncated={r.get('truncated')} returned={len(r.get('entries') or [])} "
        f"total={r.get('total_count')}",
        r.get("truncated") is True or (r.get("total_count") or 0) <= 2,
    )

    # ---- terminal: write must reach the child, read must see it ----------
    # The child has to be a SHELL for a typed command to produce output. An earlier version
    # of this probe sent a shell command to `node -e setInterval(...)`, which simply discards
    # its input -- so the check failed for the probe's reason, not the tool's.
    raw = s.call("exec", {"command": "powershell -NoProfile -NoLogo",
                          "transport": "pty", "timeout_action": "detach",
                          "timeout_ms": 3000})
    r = j(raw)
    sid = r.get("session_id")
    if sid:
        # Wait for the prompt rather than assuming a fixed settle time.
        prompt = ""
        deadline = time.time() + 20
        while time.time() < deadline:
            rd = j(s.call("terminal_read", {"session_id": sid, "cursor": 0,
                                            "timeout_ms": 800}))
            prompt = rd.get("output") or ""
            if "PS " in prompt:
                break
            time.sleep(0.3)
        check(
            "terminal_write", "a pty shell reaches an interactive prompt",
            f"output_tail={prompt[-60:]!r}",
            "PS " in prompt,
        )

        marker = "PROBE_ECHO_MARKER"
        wr = j(s.call("terminal_write", {"session_id": sid, "input": f"echo {marker}\r"}))
        seen = ""
        for _ in range(15):
            rd = j(s.call("terminal_read", {"session_id": sid, "cursor": 0,
                                            "timeout_ms": 1000}))
            seen = rd.get("output") or ""
            if marker in seen:
                break
            time.sleep(0.3)
        check(
            "terminal_write", "typed input reaches the child and its output returns",
            f"bytes_written={wr.get('bytes_written')} output_tail={seen[-120:]!r}",
            marker in seen,
        )

        # wait_for_pattern: a pattern that never appears must be reported, not silently
        # skipped. Returning the output as though the wait had succeeded makes "matched" and
        # "gave up" indistinguishable.
        raw_wait = s.call("terminal_read", {
            "session_id": sid, "cursor": 0,
            "wait_for_pattern": "THIS_WILL_NEVER_APPEAR_ZZZ", "timeout_ms": 700,
        })
        check(
            "terminal_read", "wait_for_pattern reports a pattern that never appears",
            f"raw={raw_wait[:140]!r}",
            raw_wait.startswith(("[rpc-error]", "[tool-error]")),
        )

        raw_ok = s.call("terminal_read", {
            "session_id": sid, "cursor": 0,
            "wait_for_pattern": marker, "timeout_ms": 5000,
        })
        check(
            "terminal_read", "wait_for_pattern returns output when the pattern is present",
            f"contains_marker={marker in raw_ok}",
            marker in raw_ok,
        )
        s.call("terminal_kill", {"session_id": sid})
    else:
        check("terminal_write", "a detached pty session is returned", f"raw={raw[:160]!r}", False)

    # ---- terminal_resize: must be accepted for a live session ------------
    raw = s.call("exec", {"command": "node -e \"setInterval(()=>{},1000)\"",
                          "transport": "pty", "timeout_action": "detach",
                          "timeout_ms": 2500})
    r = j(raw)
    sid = r.get("session_id")
    if sid:
        rr = j(s.call("terminal_resize", {"session_id": sid, "cols": 100, "rows": 40}))
        check(
            "terminal_resize", "resize succeeds on a live session",
            f"success={rr.get('success')}",
            rr.get("success") is True,
        )
        s.call("terminal_kill", {"session_id": sid})

    # ---- misspelled options must not silently change the result ----------
    # A typo'd cap falls back to the default, so the caller gets more (or fewer) results than
    # asked for with no signal. Reported rather than fixed: rejecting unknown fields is a
    # breaking change for hosts that send extra metadata.
    typo_dir = os.path.join(scratch, "typo")
    shutil.rmtree(typo_dir, ignore_errors=True)
    os.makedirs(typo_dir)
    for i in range(150):
        open(os.path.join(typo_dir, f"f{i:03}.rs"), "w").write("pub fn t() {}\n")
    r_ok = j(s.call("find", {"pattern": "*.rs", "path": typo_dir,
                             "options": {"max_results": 5}}))
    r_typo = j(s.call("find", {"pattern": "*.rs", "path": typo_dir,
                               "options": {"max_result": 5}}))
    n_ok = len(r_ok.get("entries") or [])
    n_typo = len(r_typo.get("entries") or [])
    check(
        "find", "a misspelled option is rejected rather than silently defaulted",
        f"max_results=5 -> {n_ok} entries; max_result=5 (typo) -> {n_typo} entries",
        n_typo == n_ok,
    )

    # ---- zero and inverted budgets must be refused, not reinterpreted ----
    # Verified live before the fix: read_file {start_line: 8, end_line: 3} reported
    # start_line 8 / end_line 8 with empty content for a 10-line file; outline
    # {max_files: 0} reported summary.total_files 0 for a directory that held a file;
    # search {max_matches: 0} returned every match while find_symbol {limit: 0} returned none.
    num_file = os.path.join(scratch, "counted.txt")
    open(num_file, "w").write("".join(f"line{i}\n" for i in range(1, 11)))

    raw = s.call("read_file", {"path": num_file, "start_line": 8, "end_line": 3})
    check(
        "read_file", "an inverted line range is refused",
        f"raw={raw[:130]!r}",
        raw.startswith(("[rpc-error]", "[tool-error]")),
    )

    raw = s.call("search", {"pattern": "line", "path": num_file,
                            "options": {"max_matches": 0}})
    check(
        "search", "max_matches=0 is refused rather than silently ignored",
        f"raw={raw[:130]!r}",
        raw.startswith(("[rpc-error]", "[tool-error]")),
    )

    raw = s.call("outline", {"path": scratch, "options": {"max_files": 0}})
    check(
        "outline", "max_files=0 is refused rather than yielding a false census",
        f"raw={raw[:130]!r}",
        raw.startswith(("[rpc-error]", "[tool-error]")),
    )

    # ---- reporting paths must be relative to the search root -------------
    # FileCluster.file is documented as "relative to search root". With a file root the root
    # IS the file, so the only relative form is its own name; reporting the absolute path made
    # search disagree with find and outline for the same file.
    rel_dir = os.path.join(scratch, "relativity")
    shutil.rmtree(rel_dir, ignore_errors=True)
    os.makedirs(os.path.join(rel_dir, "sub"))
    open(os.path.join(rel_dir, "root.rs"), "w").write("rel_marker\npub fn root_fn() {}\n")
    open(os.path.join(rel_dir, "sub", "deep.rs"), "w").write("rel_marker\n")

    dir_res = j(s.call("search", {"pattern": "rel_marker", "path": rel_dir}))
    dir_files = sorted(f.get("file", "") for f in dir_res.get("files") or [])
    file_res = j(s.call("search", {"pattern": "rel_marker",
                                   "path": os.path.join(rel_dir, "root.rs")}))
    file_files = [f.get("file", "") for f in file_res.get("files") or []]
    check(
        "search", "cluster paths are relative to the search root, file root included",
        f"directory root -> {dir_files}; file root -> {file_files}",
        bool(file_files)
        and all(not os.path.isabs(p) for p in file_files)
        and file_files == ["root.rs"],
    )

    # The same defect existed in find_symbol and outline: with a file root the former returned
    # an empty path and the latter an ABSOLUTE one, while both report a relative path under a
    # directory root. Three tools disagreeing about the same file forces callers to special-case
    # each, so all three must agree.
    root_file = os.path.join(rel_dir, "root.rs")
    fs_res = j(s.call("find_symbol", {"name": "root_fn", "path": root_file, "exact": True}))
    fs_files = [x.get("file", "") for x in fs_res.get("symbols") or []]
    check(
        "find_symbol", "symbol paths are relative with a file root",
        f"file root -> {fs_files}",
        bool(fs_files) and fs_files == ["root.rs"],
    )

    ol_res = j(s.call("outline", {"path": root_file}))
    ol_files = [f.get("file", "") for f in ol_res.get("files") or []]
    check(
        "outline", "outline paths are relative with a file root",
        f"file root -> {ol_files}",
        bool(ol_files) and ol_files == ["root.rs"],
    )

    # ---- outline summary must describe the returned payload ---------------
    # It previously mixed stages: total_symbols counted parsed symbols before the budget ran
    # while total_files counted files after the file cap, so an 8-file/16-symbol directory
    # reported total_files: 1 with total_symbols: 16 under max_files: 1.
    census = os.path.join(scratch, "census")
    shutil.rmtree(census, ignore_errors=True)
    os.makedirs(census)
    for i in range(8):
        open(os.path.join(census, f"m{i}.rs"), "w").write(
            f"pub fn alpha_{i}() {{}}\npub fn beta_{i}() {{}}\n")

    for label, opts in [("uncapped", {}), ("max_files=1", {"max_files": 1}),
                        ("max_symbols=1", {"max_symbols": 1})]:
        r = j(s.call("outline", {"path": census, "options": opts}))
        files = r.get("files") or []
        shown_f, shown_s = len(files), sum(len(f.get("symbols") or []) for f in files)
        summ = r.get("summary", {})
        check(
            "outline", f"summary matches the payload ({label})",
            f"returned files={shown_f} syms={shown_s}; "
            f"summary files={summ.get('total_files')} syms={summ.get('total_symbols')}; "
            f"truncated={r.get('truncated')}",
            summ.get("total_files") == shown_f and summ.get("total_symbols") == shown_s,
        )

    # ---- coordinate conventions must agree across tools ------------------
    # Cross-tool convention checking is what found the file-root path defect in three tools at
    # once. Off-by-one conventions are the other high-risk shared surface: if search says line 5
    # and outline says line 6 for the same declaration, every chained edit lands on the wrong line.
    coords = os.path.join(scratch, "coords.rs")
    if os.path.exists(coords):
        os.remove(coords)
    with open(coords, "w") as fh:
        fh.write(
            "// line 1\n// line 2\n\nimpl Thing {\n"
            "    pub fn target_fn() -> u32 { 1 }\n}\n\n"
            "fn caller() -> u32 { Thing::target_fn() }\n"
        )
    # target_fn is declared on line 5 at column 5 (1-based, 4-space indent).

    sr = j(s.call("search", {"pattern": "target_fn", "path": coords}))
    search_lines = sorted(
        m.get("line_number")
        for f in sr.get("files") or []
        for m in f.get("matches") or []
    )
    check(
        "search", "line_number is 1-based and matches the real line",
        f"lines={search_lines} (declaration is line 5)",
        search_lines[:1] == [5],
    )

    orr = j(s.call("outline", {"path": coords}))
    spans = []

    def collect(syms):
        for sym in syms:
            spans.append((sym.get("name"), sym.get("span") or {}))
            collect(sym.get("children") or [])

    for f in orr.get("files") or []:
        collect(f.get("symbols") or [])
    target = next((sp for name, sp in spans if name == "target_fn"), {})
    check(
        "outline", "span agrees with search for the same declaration",
        f"outline span=({target.get('start_line')},{target.get('start_col')}) vs search line 5",
        target.get("start_line") == 5 and target.get("start_col") == 5,
    )

    fr2 = j(s.call("find_symbol", {"name": "target_fn", "path": scratch, "exact": True,
                                   "case_sensitive": True}))
    fsym = next((x for x in fr2.get("symbols") or [] if x.get("name") == "target_fn"), {})
    fspan = fsym.get("span") or {}
    check(
        "find_symbol", "span agrees with search and outline",
        f"find_symbol span=({fspan.get('start_line')},{fspan.get('start_col')})",
        fspan.get("start_line") == 5 and fspan.get("start_col") == 5,
    )

    if target:
        pr = j(s.call("patch", {"path": coords, "target_span": target,
                                "replacement": "pub fn target_fn() -> u32 { 99 }",
                                "dry_run": True}))
        check(
            "patch", "accepts the span that outline reported",
            f"success={pr.get('success')} ast_valid={pr.get('ast_valid')}",
            pr.get("success") is True,
        )

    # ---- encodings, BOMs and line endings --------------------------------
    # UTF-16 is legitimately binary (NUL bytes), so the interesting cases are the ones that
    # must NOT be treated as binary, and the line-counting conventions.
    enc = os.path.join(scratch, "encodings")
    shutil.rmtree(enc, ignore_errors=True)
    os.makedirs(enc)

    def blob(name, data):
        p = os.path.join(enc, name)
        with open(p, "wb") as fh:
            fh.write(data)
        return p

    bom_txt = blob("bom.txt", b"\xef\xbb\xbfalpha\nbravo\n")
    crlf = blob("crlf.txt", b"alpha\r\nbravo\r\ncharlie\r\n")
    mixed = blob("mixed.txt", b"alpha\r\nbravo\ncharlie\r\n")
    utf16 = blob("utf16.txt", "alpha\nbravo\n".encode("utf-16"))
    bom_rs = blob("bom.rs", b"\xef\xbb\xbfpub fn alpha_one() -> u32 { 1 }\n\n"
                             b"pub fn beta_two() -> u32 { 2 }\n")

    r = j(s.call("read_file", {"path": crlf, "line_numbers": True}))
    check(
        "read_file", "CRLF file reports 3 lines, not 6 or 1",
        f"total_lines={r.get('total_lines')}",
        r.get("total_lines") == 3,
    )
    r = j(s.call("read_file", {"path": mixed, "line_numbers": True}))
    check(
        "read_file", "mixed line endings still count 3 lines",
        f"total_lines={r.get('total_lines')}",
        r.get("total_lines") == 3,
    )
    r = j(s.call("read_file", {"path": utf16}))
    check(
        "read_file", "UTF-16 is reported as binary rather than as text",
        f"is_binary={r.get('is_binary')}",
        r.get("is_binary") is True,
    )

    # A BOM must not attach itself to a symbol name, a signature, or a search match, or every
    # caller comparing those strings sees a zero-width character it never wrote.
    r = j(s.call("outline", {"path": bom_rs}))
    names = [x.get("name", "") for f in r.get("files") or [] for x in f.get("symbols") or []]
    sigs = [x.get("signature", "") for f in r.get("files") or [] for x in f.get("symbols") or []]
    check(
        "outline", "a UTF-8 BOM does not attach to symbol names or signatures",
        f"names={names} sigs={sigs}",
        names == ["alpha_one", "beta_two"]
        and all(not t.startswith("\ufeff") for t in names + sigs),
    )
    r = j(s.call("search", {"pattern": "alpha_one", "path": bom_rs}))
    line = ((r.get("files") or [{}])[0].get("matches") or [{}])[0].get("line_text", "")
    check(
        "search", "a BOM does not leak into the first matched line",
        f"line={line!r}",
        line.lstrip("\ufeff").startswith("pub fn alpha_one"),
    )
    r = j(s.call("read_symbol", {"path": bom_rs, "symbol": "beta_two"}))
    src = r.get("source_code") or ""
    check(
        "read_symbol", "returned source is free of a leading BOM",
        f"source={src[:40]!r}",
        not src.startswith("\ufeff"),
    )

    # ---- exit codes: success vs failure must be distinguishable ----
    # Exact codes are degraded through the default shell (PowerShell collapses some nested
    # non-zero codes to 1), but the property a caller actually branches on is whether zero
    # means success, so that is what is asserted. See the audit's item 24.
    ok = j(s.call("exec", {"command": "rg --version"}))
    bad = j(s.call("exec", {"command": "cmd /c exit 42"}))
    check(
        "exec", "a failing command is not reported with exit code 0",
        f"rg --version -> {ok.get('exit_code')}; cmd /c exit 42 -> {bad.get('exit_code')}",
        ok.get("exit_code") == 0 and bad.get("exit_code") not in (0, None),
    )
    exact = j(s.call("exec", {"command": "cmd /c exit 42", "shell": "cmd"}))
    check(
        "exec", "naming the shell preserves the exact exit code",
        f"shell=cmd -> {exact.get('exit_code')} (expected 42)",
        exact.get("exit_code") == 42,
    )

    # ---- lsp_definition / lsp_hover on a known symbol --------------------
    r = j(s.call("lsp_definition", {"path": os.path.join(scratch, "sample.rs"),
                                    "symbol": "alpha"}))
    check(
        "lsp_definition", "a defined symbol yields a target",
        f"targets={len(r.get('targets') or [])} engine={r.get('engine')!r}",
        len(r.get("targets") or []) > 0,
    )

    r = j(s.call("lsp_hover", {"path": os.path.join(scratch, "sample.rs"),
                               "symbol": "alpha"}))
    check(
        "lsp_hover", "a symbol yields a signature or documentation",
        f"signature={r.get('signature')!r} engine={r.get('engine')!r}",
        bool(r.get("signature") or r.get("documentation")),
    )


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--binary", default=os.path.join(REPO, "target", "debug", "transcend.exe"))
    args = ap.parse_args()

    if not os.path.isfile(args.binary):
        sys.exit(f"server binary not found: {args.binary} (cargo build --bin transcend)")

    scratch = os.path.join(REPO, "target", "probe-contracts")
    shutil.rmtree(scratch, ignore_errors=True)
    os.makedirs(scratch, exist_ok=True)

    session = McpSession(args.binary, REPO, workspace=scratch)
    try:
        run_probes(session, scratch)
        run_param_probes(session, scratch)
    finally:
        session.close()

    failed = [f for f in FINDINGS if not f[3]]
    print(f"\n{len(FINDINGS) - len(failed)}/{len(FINDINGS)} expectations honoured")
    if failed:
        print("\nFAILURES:")
        for tool, expectation, detail, _ in failed:
            print(f"  [{tool}] {expectation}\n      {detail}")
    shutil.rmtree(scratch, ignore_errors=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()

