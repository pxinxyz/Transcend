# Transcend

### The agent-native CLI skill framework.

Transcend transforms high-performance CLI tools into typed computational primitives with structured JSON contracts. Agents reason over structured data — not terminal text.

```text
Standard Agent:  Tool → Raw Text → LLM Parsing → Reconstructed Structure → Reasoning
Transcend:       Tool → Structured JSON → Reasoning
```

---

## ⚡ Quick Install (Zero-Install MCP)

You do not need to clone this repository to use Transcend. It can be run instantly as a zero-install MCP Server using `npx`. Add the following to your agent's MCP configuration (e.g., Claude Desktop, Cursor, Zed):

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

---

## The Problem

Every AI coding agent today runs the same broken loop:

1. Call a CLI tool (`grep`, `find`, `cat`)
2. Receive kilobytes of human-formatted terminal output
3. Dump it into the context window
4. Parse it with the LLM — the world's most expensive regex engine
5. Repeat

This works on toy projects. On real codebases, it **physically cannot work**.

A single `grep` across a large repository can return tens of thousands of matches. The standard agent workflow (search + read matched files) produces **millions of tokens** depending on the tokenizer. No model on Earth has a context window that large. The task is impossible.

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

## Supported Skills

Transcend currently ships **29 typed skills** powered by high-performance CLI utilities — primarily Rust and Go — across 9 categories. Every skill has a strict JSON contract, Nunjucks execution templates, normalization pipelines, and comprehensive tests (**285 tests** across 85 suites).

| Category | Skills | Primary Tools |
|----------|--------|---------------|
| **Search & Navigation** | `universal_search`, `file_find`, `structural_search`, `directory_jump` | ripgrep, fd, ast-grep, zoxide |
| **Text & Data Processing** | `json_query`, `yaml_query`, `column_extract`, `csv_analysis`, `find_replace` | jq/jaq, yq, choose, qsv/xsv, sd |
| **File Viewing** | `file_view` | bat |
| **Git & Version Control** | `git_status`, `git_diff`, `git_log`, `git_blame`, `git_branch`, `git_stash`, `git_tag`, `git_remote`, `git_config`, `git_show`, `git_cherry_pick`, `git_merge`, `semantic_diff`, `github_api` | git, delta, difftastic, gh |
| **System Monitoring** | `process_list` | procs |
| **File Management** | `find_replace` | sd |
| **Network** | `github_api` | gh |
| **Environment** | `directory_jump` | zoxide |
| **Development** | `benchmark_execution`, `python_lint_fix`, `js_lint_fix`, `rust_test`, `codebase_analysis` | hyperfine, ruff, biome, cargo-nextest, scc |

---

## Benchmarks

From internal testing, I, and the agents I utilize, have measured drastic token reduction and speedup, while retaining full performance.

At scale, the standard approach doesn't degrade — it becomes **physically impossible**. Massive context dumps exceed production model limits. Transcend bypasses this bottleneck entirely.

*You can run tests and comparisons yourself on your codebase, and ask your favorite agent to compare it.*

---

## License

All rights reserved. &copy; 2026 pxin
