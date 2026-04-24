# Transcend

### The agent-native CLI skill framework.

Transcend transforms high-performance CLI tools into typed computational primitives with structured JSON contracts. Agents reason over structured data — not terminal text.

```text
Standard Agent:  Tool → Raw Text → LLM Parsing → Reconstructed Structure → Reasoning
Transcend:       Tool → Structured JSON → Reasoning
```

---

## The Problem

Every AI coding agent today runs the same broken loop:

1. Call a CLI tool (`grep`, `find`, `cat`)
2. Receive kilobytes of human-formatted terminal output
3. Dump it into the context window
4. Parse it with the LLM — the world's most expensive regex engine
5. Repeat

This works on toy projects. On real codebases, it **physically cannot work**.

A single `grep` for "FastAPI" across the FastAPI repository returns ~24,000 matches. The standard agent workflow (search + read matched files) produces **6.5-7.2 million tokens** depending on tokenizer. No model on Earth has a context window that large. The task is impossible.

Transcend makes it trivial.

---

## "But tools already have --json flags"

Some do. Most don't. And even when they do, JSON format is not the same as a typed contract.

`rg --json` outputs NDJSON with envelope messages, redundant path repetitions per match, nested stat objects, and begin/end wrappers that carry zero information for the agent. Multiply by 24,000 matches and you have megabytes of verbose, tool-specific JSON that the agent still has to parse, understand, and relay between tool calls through its context window.

Three gaps that `--json` flags don't solve:

1. **No normalisation.** Every tool has a different JSON schema. `rg --json` looks nothing like `fd --json`. Transcend normalises all of them to a universal output contract — same shape, every tool.
2. **No chaining without context window relay.** Even with JSON output, the agent reads results into its context, extracts file paths, then generates a new tool call. Transcend chains skills in-process. The agent says "search then replace" once; the file list never enters the context window.
3. **No cross-tool contract.** An agent using raw tool JSON handles N different schemas and completely different fallback behaviour when a tool isn't installed. Transcend gives every tool the same contract, with declared fallbacks that normalise to the same schema regardless of which binary actually executed.

---

## MCP Integration Guide (Instant Setup)

Transcend exposes its ultra-fast codebase analysis and refactoring skills through the **Model Context Protocol (MCP)**. Because it adheres to the standard MCP `stdio` transport, Transcend is instantly compatible out-of-the-box with **any CLI agent, IDE, or GUI tool** that supports MCP.

### The MCP Architecture

Transcend acts as a standard MCP server. Your agent acts as the MCP client.

```text
┌──────────────────────┐     MCP (stdio)     ┌──────────────────────┐     Native Subprocess    ┌───────────────┐
│ MCP Client           │◄───────────────────►│ Transcend MCP Server │◄────────────────────────►│ ripgrep / sd  │
│ (Claude Code, Cursor,│   JSON-RPC 2.0      │ (Nunjucks Engine)    │     Zero-overhead pipe   │ scc           │
│  Zed, Windsurf, etc) │                     └──────────────────────┘                          └───────────────┘
└──────────────────────┘
```

1. **Discovery:** The agent queries `tools/list`. Transcend advertises its typed schema.
2. **Execution:** The agent calls a tool via `tools/call` with JSON arguments.
3. **Processing:** Transcend compiles the input using the blazing-fast **Nunjucks** template engine and pipes it directly to native Rust/Go binaries.
4. **Structured Return:** Raw stdout is instantly structured into typed JSON and returned to the agent. No terminal parsing required.

### Standard MCP Configuration Object

If your tool supports MCP, you only need three pieces of information to connect it to Transcend:
1. **Transport Type:** `stdio`
2. **Command:** `npx`
3. **Arguments:** `transcend`, `--mcp`

You can drop this block into almost any standard MCP config (like Claude Desktop, Cursor, or Zed):

```json
{
  "mcpServers": {
    "transcend": {
      "command": "npx",
      "args": ["-y", "github:PXINXYZ/Transcend", "--mcp"]
    }
  }
}
```

### Client-Specific Quick Starts

*   **Claude Code (CLI):** Add the server block to your global `~/.claude.json` or project-local `.claude.json`. Run `/mcp tools` in the CLI to verify.
*   **Claude Desktop (GUI):** Add the server block to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows).
*   **Cursor (IDE):** Open **Cursor Settings -> Features -> MCP Servers**. Add New MCP Server (`stdio`), set Command to `npx @pxinxyz/transcend --mcp`.
*   **Zed (IDE):** Add to `~/.config/zed/settings.json` under `context_servers`.

---

## Skill Anatomy

A Transcend skill is a `.skill.json` file. The snippet below is a simplified overview — the actual specifications are significantly more detailed.

```jsonc
{
  "name": "universal_search",
  "version": "1.0.1",
  "stability": "stable",

  // Typed inputs with validation
  "inputs": {
    "pattern": { "type": "string", "required": true },
    "path": { "type": "string", "default": "." }
  },

  // Typed outputs — the contract the agent relies on
  "outputs": {
    "success": {
      "properties": {
        "total_matches": { "type": "integer" },
        "matches": [{
          "file": "string",
          "line_number": "integer",
          "match_text": "string"
        }]
      }
    }
  },

  // Execution — how the underlying CLI tool is invoked via Node.js
  "execution": {
    "command": "rg",
    "args": ["--json", "--", "{{ pattern }}", "{{ path }}"],
    "template_engine": "nunjucks"
  },

  // Normalisation — raw tool output → typed JSON
  "normalization": {
    "pipeline": [
      "filter_message_types",
      "extract_matches",
      "assemble_output"
    ]
  }
}
```

Every field is a contract. The agent knows what goes in, what comes out, what can chain next, and what happens if the tool isn't installed. No guessing. No parsing.

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

At scale, the standard approach doesn't degrade — it becomes **physically impossible**. 814K context tokens exceeds every production model. Transcend bypasses this bottleneck entirely.

---

## License

All rights reserved. &copy; 2026 pxin
