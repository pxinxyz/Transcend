#!/usr/bin/env python3
"""Measure the MCP tool-schema token tax, reproducibly.

Why this exists
---------------
`transcend export-schemas` writes *pretty-printed* JSON for human reading, but that is
NOT what an agent pays for. The live `tools/list` wire payload is serialised compactly by
`rmcp`. Measuring the pretty files overstates the tax by ~33% and made an earlier audit of
this repository report 7,712 tokens for a payload that actually costs 5,077.

This harness therefore always re-serialises to compact form before counting, and reports
both numbers side by side so the discrepancy stays visible rather than silently misleading.

Usage
-----
    python benchmarks/schema_tax.py                    # measure current build
    python benchmarks/schema_tax.py --save baseline    # store a baseline
    python benchmarks/schema_tax.py --compare baseline # diff against a stored baseline

Requires `tiktoken` (pip install tiktoken). The exact tokenizer matters: `chars/4` is not
a safe approximation for JSON schemas -- it overestimates them by ~14% because of the
repeated structural punctuation.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ARTIFACTS = os.path.join(REPO, "target", "schema-tax")
ENCODING = "o200k_base"


def find_binary() -> str:
    """Prefer the release binary; fall back to debug so this works mid-development."""
    for profile in ("release", "debug"):
        for name in ("transcend.exe", "transcend"):
            path = os.path.join(REPO, "target", profile, name)
            if os.path.isfile(path):
                return path
    sys.exit("no transcend binary found -- run `cargo build --release` first")


def load_tiktoken():
    try:
        import tiktoken
    except ImportError:
        sys.exit("tiktoken is required: pip install tiktoken")
    return tiktoken.get_encoding(ENCODING)


def export_schemas(binary: str, out_dir: str) -> None:
    if os.path.isdir(out_dir):
        shutil.rmtree(out_dir)
    os.makedirs(out_dir)
    subprocess.run(
        [binary, "export-schemas", "--out", out_dir, "--quiet"],
        check=True,
        stdout=subprocess.DEVNULL,
    )


def load(out_dir: str) -> dict[str, dict]:
    tools = {}
    for name in sorted(os.listdir(out_dir)):
        if name.endswith(".json"):
            with open(os.path.join(out_dir, name), encoding="utf-8") as fh:
                tools[name[:-5]] = json.load(fh)
    return tools


def compact(obj) -> str:
    return json.dumps(obj, separators=(",", ":"), ensure_ascii=False)


def describe(raw: str, enc) -> tuple[int, int]:
    """Split a compact tool schema into description-text vs everything-else tokens."""
    descs = re.findall(r'"description"\s*:\s*"((?:[^"\\]|\\.)*)"', raw)
    desc = sum(len(enc.encode(d, disallowed_special=())) for d in descs)
    total = len(enc.encode(raw, disallowed_special=()))
    return desc, total - desc


def measure(out_dir: str, enc) -> dict:
    tools = load(out_dir)
    per_tool = {}
    pretty_chars = 0
    for name, schema in tools.items():
        raw = compact(schema)
        desc, other = describe(raw, enc)
        per_tool[name] = {
            "tokens": desc + other,
            "desc_tokens": desc,
            "other_tokens": other,
            "chars": len(raw),
        }
        with open(os.path.join(out_dir, f"{name}.json"), encoding="utf-8") as fh:
            pretty_chars += len(fh.read())

    total = sum(t["tokens"] for t in per_tool.values())
    compact_chars = sum(t["chars"] for t in per_tool.values())
    return {
        "tool_count": len(per_tool),
        "total_tokens": total,
        "desc_tokens": sum(t["desc_tokens"] for t in per_tool.values()),
        "pretty_chars": pretty_chars,
        "compact_chars": compact_chars,
        "per_tool": per_tool,
    }


def report(result: dict) -> None:
    per_tool = result["per_tool"]
    total = result["total_tokens"]
    desc = result["desc_tokens"]

    print(f"tokenizer: {ENCODING}   tools: {result['tool_count']}")
    print(
        f"pretty export : {result['pretty_chars']:>8,} chars"
        f"   (what `export-schemas` writes -- DO NOT measure this)"
    )
    print(f"compact wire  : {result['compact_chars']:>8,} chars   (what a host receives)")
    print()
    print(f"{'tool':<20}{'tokens':>8}{'desc':>8}{'other':>8}{'desc%':>7}")
    print("-" * 51)
    for name, t in sorted(per_tool.items(), key=lambda kv: -kv[1]["tokens"]):
        share = t["desc_tokens"] / t["tokens"] * 100 if t["tokens"] else 0
        print(
            f"{name:<20}{t['tokens']:>8,}{t['desc_tokens']:>8,}"
            f"{t['other_tokens']:>8,}{share:>6.0f}%"
        )
    print("-" * 51)
    print(f"{'TOTAL':<20}{total:>8,}{desc:>8,}{total - desc:>8,}{desc / total * 100:>6.0f}%")


def compare(baseline: dict, current: dict) -> None:
    before, after = baseline["total_tokens"], current["total_tokens"]
    delta = after - before
    pct = delta / before * 100 if before else 0
    print(
        f"\nvs baseline: {before:,} -> {after:,} tok  "
        f"({delta:+,} / {pct:+.2f}%)"
    )
    print(f"\n{'tool':<20}{'before':>8}{'after':>8}{'delta':>8}")
    print("-" * 44)
    for name in sorted(current["per_tool"]):
        b = baseline["per_tool"].get(name, {}).get("tokens")
        a = current["per_tool"][name]["tokens"]
        if b is None:
            print(f"{name:<20}{'--':>8}{a:>8,}{'(new)':>8}")
        elif a != b:
            print(f"{name:<20}{b:>8,}{a:>8,}{a - b:>+8,}")
    for name in sorted(set(baseline["per_tool"]) - set(current["per_tool"])):
        print(f"{name:<20}{baseline['per_tool'][name]['tokens']:>8,}{'--':>8}{'(gone)':>8}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--save", metavar="NAME", help="store this measurement under NAME")
    ap.add_argument("--compare", metavar="NAME", help="diff against a stored measurement")
    ap.add_argument(
        "--baseline-dir",
        default=os.path.join(ARTIFACTS, "baselines"),
        help="where named measurements are stored",
    )
    args = ap.parse_args()

    enc = load_tiktoken()
    binary = find_binary()
    with tempfile.TemporaryDirectory(dir=ARTIFACTS if os.path.isdir(ARTIFACTS) else None) as tmp:
        out_dir = os.path.join(tmp, "schemas")
        export_schemas(binary, out_dir)
        result = measure(out_dir, enc)

    os.makedirs(ARTIFACTS, exist_ok=True)
    result["binary"] = os.path.relpath(binary, REPO)
    with open(os.path.join(ARTIFACTS, "current.json"), "w", encoding="utf-8") as fh:
        json.dump(result, fh, indent=2)

    report(result)

    if args.save:
        os.makedirs(args.baseline_dir, exist_ok=True)
        dest = os.path.join(args.baseline_dir, f"{args.save}.json")
        with open(dest, "w", encoding="utf-8") as fh:
            json.dump(result, fh, indent=2)
        print(f"\nsaved baseline -> {os.path.relpath(dest, REPO)}")

    if args.compare:
        src = os.path.join(args.baseline_dir, f"{args.compare}.json")
        if not os.path.isfile(src):
            sys.exit(f"no baseline named {args.compare!r} at {src}")
        with open(src, encoding="utf-8") as fh:
            compare(json.load(fh), result)


if __name__ == "__main__":
    main()
