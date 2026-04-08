# Transcend

### The agent-native CLI skill framework.

Transcend transforms CLI tools into typed computational primitives with structured JSON contracts. Agents reason over structured data — not terminal text.

```
Standard Agent:  Tool → Raw Text → LLM Parsing → Reconstructed Structure → Reasoning
Transcend:       Tool → Structured JSON → Reasoning
```

---

## The Problem

Every AI coding agent today runs the same broken loop:

1. Call a CLI tool (grep, find, cat)
2. Receive kilobytes of human-formatted terminal output
3. Dump it into the context window
4. Parse it with the LLM — the world's most expensive regex engine
5. Repeat

This works on toy projects. On real codebases, it **physically cannot work**.

A single `grep` for "FastAPI" across the FastAPI repository returns ~24,000 matches. The standard agent workflow (search + read matched files) produces **6.5-7.2 million tokens** depending on tokenizer. No model on Earth has a context window that large. The task is impossible.

Transcend makes it trivial.

---

## What Transcend Does

Transcend wraps CLI tools in **skill specifications** — typed contracts that define:

- Strictly typed inputs with validation
- Deterministic, structured JSON outputs
- Normalisation pipelines that transform raw tool output into clean data
- Platform-aware execution with automatic fallbacks
- Composable chaining — skills pass typed data directly to downstream skills

The agent never parses text. The agent never relays intermediate data. The agent **orchestrates**.

```
universal_search(pattern="fetchUserData", path="./src")
    ↓  typed file list flows directly — zero tokens through the LLM
find_replace(pattern="fetchUserData", replacement="resolveUser", files_from=<search>)
    → structured change manifest
```

Two skill calls. One JSON manifest returned. The LLM decided what to do, not how to do it.

---

## Benchmarks

### Token Reduction — FastAPI (2,944 files, 409,037 lines)

Measured with **4 real tokenizers** from 4 vendors. Not heuristics.

| Tokenizer | Transcend | Standard Agent | Reduction | Ratio |
|-----------|-----------|----------------|-----------|-------|
| o200k_base (GPT) | 112,691 | 10,222,088 | **98.9%** | 91x |
| Gemma 3 (Google) | 140,344 | 11,430,610 | **98.8%** | 81x |
| Llama 4 Scout (Meta) | 111,914 | 9,735,463 | **98.9%** | 87x |
| Qwen 3.5 (Alibaba) | 128,950 | 10,943,987 | **98.8%** | 85x |

Cross-tokenizer spread: **0.1 percentage points**. The reduction is tokenizer-invariant.

Results independently confirmed by Claude Opus 4.6 and Gemini 3.1 Pro running identical scripts.

### Speed — Cached Runtime

| Codebase | Per-Search (Cached) | Notes |
|----------|-------------------|-------|
| 46 files | **377 microseconds** | Warm V8, JIT-compiled regex |
| 2,944 files (FastAPI) | **15ms** | 28% faster than raw ripgrep (JSON output is cheaper than text formatting) |

### Real-World Agent Performance — 15-File Codebase

Rename `fetchUserData` → `resolveUser` across a realistic Node.js project (91 occurrences, 13 files).

| Metric | Transcend | Standard Agent |
|--------|-----------|----------------|
| Agent tool calls | **1** | **27** |
| Context tokens | **709** | **6,493** |
| Estimated wall clock | **414ms** | **5,416ms** |
| Correctness | 91/91 | 91/91 |

### Real-World Agent Performance — FastAPI

| Metric | Transcend | Standard Agent |
|--------|-----------|----------------|
| Agent tool calls | **1** | **537** |
| Context tokens | **12,508** | **814,461** |
| Estimated wall clock | **469ms** | **~107 seconds** |
| Effective speedup | **~230x** | — |

At scale, the standard approach doesn't degrade — it becomes **physically impossible**. 814K tokens exceeds every production context window.

---

## Skill Anatomy

A Transcend skill is a `.skill.json` file. Here's the structure:

```jsonc
{
  "name": "universal_search",
  "version": "1.0.1",
  "stability": "stable",

  // Typed inputs with validation
  "inputs": {
    "pattern": { "type": "string", "required": true },
    "path": { "type": "string", "default": "." },
    "max_matches": { "type": "integer", "default": 1000 }
  },

  // Typed outputs — the contract the agent relies on
  "outputs": {
    "success": {
      "properties": {
        "total_matches": { "type": "integer" },
        "files_searched": { "type": "integer" },
        "matches": [{
          "file": "string",
          "line_number": "integer",
          "match_text": "string"
        }]
      }
    }
  },

  // Execution — how the underlying CLI tool is invoked
  "execution": {
    "command": "rg",
    "args": ["--json", "--", "{{ pattern }}", "{{ path }}"],
    "template_engine": "tera"
  },

  // Normalisation — raw tool output → typed JSON
  "normalization": {
    "pipeline": [
      "filter_message_types",
      "extract_matches",
      "attach_context",
      "extract_summary",
      "detect_truncation",
      "assemble_output"
    ]
  },

  // Resilience — fallbacks when the primary tool is missing
  "resilience": {
    "fallback": {
      "condition": "command_not_found",
      "command": "grep",
      "normalization": "grep_plaintext_to_transcend_schema"
    }
  },

  // Chaining — typed data flows between skills
  "chains": {
    "produces": ["file_path", "line_number", "match_text"],
    "compatible_downstream": [
      { "skill": "find_replace", "via": ["file", "match_text"] },
      { "skill": "file_view", "via": ["file", "line_number"] }
    ]
  }
}
```

Every field is a contract. The agent knows what goes in, what comes out, what can chain next, and what happens if the tool isn't installed. No guessing. No parsing.

---

## Skill Chaining

Skills declare typed compatibility. Data flows directly between them without passing through the LLM context window.

```
directory_list ──→ universal_search ──→ find_replace ──→ file_view
      (path)            (file, match_text)         (file)

file_find ──→ universal_search ──→ structural_search
   (path)          (file)               (AST analysis)
```

Each arrow is a typed field mapping defined in the skill spec. The agent decides the pipeline; the runtime executes it. Intermediate data never touches the context window.

---

## Supported Tools

Transcend targets high-performance CLI utilities — primarily Rust and Go — across 9 categories:

| Category | Tools |
|----------|-------|
| **Search & Navigation** | ripgrep, fd, fzf, zoxide, broot, yazi, eza, ast-grep |
| **Text & Data Processing** | jq/jaq, yq, sd, choose, qsv/xsv, tokei, pandoc |
| **File Viewing** | bat, helix, hexyl, glow |
| **Git & Version Control** | gitui, lazygit, delta, difftastic, gh |
| **System Monitoring** | procs, bottom, bandwhich, dive |
| **File Management** | rip, rnr, ouch, dust, just, watchexec, zellij |
| **Network** | xh, dog, atac |
| **Environment** | mise, direnv, starship, tealdeer |
| **Development** | hyperfine, ruff, uv, biome, typos, cargo-nextest |

The `transcend-init` installer handles the full toolchain setup with an interactive TUI.

---

## How It's Different

This is not a prompt compression tool. This is not an output filter. This is not a CLI wrapper.

| | RTK | CLI-Anything | PolyMCP | **Transcend** |
|---|---|---|---|---|
| Approach | Proxy — strips CLI output | Generates CLIs for GUI apps | Groups MCP tool schemas | **Typed skill specifications** |
| Token reduction | 60-90% | Marginal | Marginal | **98.8%** |
| Structured output | No | Per-app JSON | MCP schemas | **Universal typed contracts** |
| Normalisation pipeline | No | No | No | **Yes** |
| Skill chaining | No | No | No | **Yes — typed field mapping** |
| Fallback system | No | No | No | **Yes — per-skill** |
| Platform-aware | No | No | No | **Yes — Linux/macOS/Windows/WSL** |

Others compress a bad format. Transcend replaces the format.

---

## Architecture

```
┌─────────────────────────────────────────────┐
│                   Agent (LLM)               │
│                                             │
│  Decides WHAT to do. Writes one invocation. │
│  Receives structured JSON. Reasons.         │
└──────────────────┬──────────────────────────┘
                   │ skill invocation
┌──────────────────▼──────────────────────────┐
│              Transcend Runtime               │
│                                             │
│  ┌─────────────┐  ┌─────────────────────┐   │
│  │ Skill Spec  │  │ Normalisation       │   │
│  │ (.skill.json│  │ Pipeline            │   │
│  │  contract)  │→ │ raw output → typed  │   │
│  └─────────────┘  │ JSON contract       │   │
│                    └──────────┬──────────┘   │
│  ┌─────────────┐             │              │
│  │ Chaining    │◄────────────┘              │
│  │ Engine      │ typed data flows between   │
│  │             │ skills without touching     │
│  │             │ the context window          │
│  └─────────────┘                            │
└──────────────────┬──────────────────────────┘
                   │ subprocess
┌──────────────────▼──────────────────────────┐
│           CLI Tools (rg, fd, sd, bat...)     │
│           Execute. Return raw output.        │
└─────────────────────────────────────────────┘
```

---

## Quick Start

### Install the toolchain

```bash
# macOS / Linux / WSL
curl -sSf https://raw.githubusercontent.com/pxinxyz/Transcend/main/Scripts/transcend-init.sh | bash

# Windows (PowerShell)
irm https://raw.githubusercontent.com/pxinxyz/Transcend/main/Scripts/transcend-init.ps1 | iex
```

### Use a skill

Skills are self-contained `.skill.json` files. Point your agent framework at them.

```bash
# Browse available skills
ls Skills/*.skill.json
```

---

## Project Structure

```
Transcend/
├── Skills/                     # Skill specifications
│   ├── universal_search.skill.json
│   ├── find_replace.skill.json
│   └── Archive/                # Previous versions
├── Scripts/                    # Toolchain installers
│   ├── transcend-init.sh       # macOS/Linux/WSL
│   └── transcend-init.ps1      # Windows
├── Internal/                   # Design docs
│   ├── Vision.md
│   ├── Tools.md
│   └── Example Skill.md
└── Transcend Case Studies/     # Benchmark data
    ├── Claude/                 # Claude Opus 4.6 benchmarks
    └── Gemini/                 # Gemini 3.1 Pro benchmarks
```

---

## Status

Transcend is in active development. The skill specification format is stable. The runtime (single warm process serving skills with in-process chaining) is the next milestone.

**Current:**
- Skill spec v1 — stable
- `universal_search` and `find_replace` skills — implemented
- Cross-platform toolchain installer — implemented
- Benchmarks validated across 4 tokenizers and 2 frontier models

**Next:**
- Runtime implementation (Node.js, warm V8 process)
- Additional skill implementations across all 9 tool categories
- Agent framework integrations

---

## Vanquish Integration

Transcend is designed to pair with [Vanquish](https://github.com/pxinxyz/Vanquish), a Rust-native Corrective RAG runtime. Together:

- **Vanquish** provides validated, semantic context
- **Transcend** provides structured tool execution
- **The LLM** acts purely as the reasoning engine

---

## License

All rights reserved. &copy; 2026 pxin

---

<p align="center">
<sub>Built by <a href="https://github.com/pxinxyz">pxin</a></sub>
</p>
