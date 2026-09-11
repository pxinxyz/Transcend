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
  <a href="https://modelcontextprotocol.io"><img src="https://img.shields.io/badge/MCP-Standard%20stdio-8A2BE2" alt="MCP Compatible"></a>
  <a href="https://github.com/pxinxyz/Transcend"><img src="https://img.shields.io/badge/tests-53%20passed-brightgreen" alt="Tests"></a>
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

Transcend replaces blind scraping with **5 typed, AST-aware computational primitives**:

```
           ┌────────────────────────┐
           │        find            │  Topological Radar & Extension Census
           └───────────┬────────────┘
                       │
           ┌───────────▼────────────┐
           │       search           │  Clustered Ripgrep Heatmaps & Content Search
           └───────────┬────────────┘
                       │
           ┌───────────▼────────────┐
           │       outline          │  AST Structural Tree or Syntax-Valid Skeletons
           └───────────┬────────────┘
                       │
           ┌───────────▼────────────┐
           │     read_symbol        │  Surgical Semantic Symbol Extraction
           └───────────┬────────────┘
                       │
           ┌───────────▼────────────┐
           │        patch           │  AST-Guarded Indentation-Healed Patching
           └────────────────────────┘
```

---

## Core Primitives

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

### 3. `outline` — AST Symbol Hierarchy & Skeletons
Replaces reading full source files just to understand their high-level structure.
- **Format: `tree`**: Deep semantic symbol hierarchy detailing classes, methods, functions, interfaces, structs, macros, and fields with exact byte and line coordinates.
- **Format: `skeleton`**: Generates **100% syntax-valid code stubs** where function/class bodies are pruned to `{ ... }` while preserving all signatures, types, and docstrings.
- **70–90% Token Reduction**: Reduces 1,500-line source files into ~350 tokens of clean, parseable syntax.

### 4. `read_symbol` — Surgical Semantic Extraction
Replaces manual line slicing and coordinate arithmetic (`sed -n 42,88p`).
- **Qualified Locators**: Inspect symbols by simple name (`login`) or qualified hierarchy (`AuthManager::validate_token`).
- **Context Preservation**: Automatically extracts leading docstrings, annotations, attributes, and surrounding comments.
- **Zero Coordinate Math**: The agent never needs to calculate line numbers or offsets to view a function.

### 5. `patch` — AST-Guarded Surgical Code Modifier
Replaces brittle regex replacements and unverified diff tools.
- **In-Memory Preflight AST Verification**: Before writing anything to disk, parses the modified buffer with Tree-sitter. If syntax errors or missing tokens (`ERROR` or `MISSING` nodes) are detected, the patch is **rejected immediately with exact diagnostics**.
- **Auto-Healing Indentation**: Dynamically calculates surrounding indentation margins, eliminating indentation mismatch errors common in LLM generations.
- **Multi-Modal Targeting**: Target changes via `target_symbol` (AST node replacement), `target_span` (precise coordinates), or `target_text` (guarded substring).
- **Unified Diff Output & Dry Runs**: Generates clean unified diffs and supports safe simulation via `dry_run: true`.

---

## Polyglot AST Engine (18 Languages)

Transcend features native, compiled **Tree-sitter** grammar parsers for 18 industry-standard languages:

| Language | Tree-sitter Grammar | Key Symbols Extracted |
|:---|:---|:---|
| **Rust** | `tree-sitter-rust` | Functions, Impls, Structs, Enums, Traits, Macros, Consts |
| **TypeScript** | `tree-sitter-typescript` | Classes, Interfaces, Methods, Enums, Types, Functions |
| **JavaScript** | `tree-sitter-javascript` | Classes, Methods, Functions, Prototypes |
| **Python** | `tree-sitter-python` | Classes, Methods, Functions, Decorated Items, Docstrings |
| **Go** | `tree-sitter-go` | Functions, Methods (with Receivers), Types, Interfaces, Structs |
| **C** | `tree-sitter-c` | Functions, Structs, Enums, Typedefs, Macros |
| **C++** | `tree-sitter-cpp` | Classes, Namespaces, Methods, Templates, Operators |
| **C#** | `tree-sitter-c-sharp` | Classes, Interfaces, Records, Structs, Methods, Namespaces |
| **Java** | `tree-sitter-java` | Classes, Interfaces, Records, Enums, Methods |
| **Kotlin** | `tree-sitter-kotlin-ng` | Classes, Objects, Functions, Interfaces, Companion Objects |
| **PHP** | `tree-sitter-php` | Classes, Interfaces, Traits, Enums, Functions, Methods |
| **Ruby** | `tree-sitter-ruby` | Classes, Modules, Methods, Singleton Methods |
| **Swift** | `tree-sitter-swift` | Classes, Structs, Protocols, Enums, Extensions, Functions |
| **Dart** | `tree-sitter-dart` | Classes, Mixins, Enums, Extensions, Methods, Functions |
| **Zig** | `tree-sitter-zig` | Functions, Structs, Tests, Enums, Unions |
| **Lua** | `tree-sitter-lua` | Global & Local Functions, Table Methods |
| **Bash / Shell** | `tree-sitter-bash` | Shell Functions, Aliases |
| **SQL** | `tree-sitter-sequel` | Create Table, Create View, Stored Procedures, Functions |
| **Markdown** | `tree-sitter-md` | Document Headings (H1–H6) |

---

## Token Efficiency Benchmarks

Empirical context consumption comparisons measured across popular open-source repositories:

| Target File / Operation | Standard CLI / Cat Workflow | Transcend Primitive | Token Savings |
|:---|:---|:---|:---|
| **Flask `app.py`** (Explore structure) | 1,840 lines (`cat`) &rarr; **~7,400 tokens** | `outline(skeleton)` &rarr; **820 tokens** | **88.9% reduction** |
| **Gin `gin.go`** (Inspect `Engine` type) | 680 lines (`cat`) &rarr; **~3,100 tokens** | `read_symbol("Engine")` &rarr; **290 tokens** | **90.6% reduction** |
| **Hypervisor `VMCS.c`** (Inspect function) | 420 lines (`view_file`) &rarr; **~2,800 tokens** | `read_symbol("SetupVmcs")` &rarr; **310 tokens** | **88.9% reduction** |
| **FastAPI codebase** (Repository map) | Recursive `fd` &rarr; **~9,200 tokens** | `find(radar=true)` &rarr; **440 tokens** | **95.2% reduction** |

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
│   ├── transcend-core/       # Ripgrep-grade search, Tree-sitter AST parser, skeletonizer & patcher
│   ├── transcend-server/     # High-throughput asynchronous MCP stdio server daemon
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
