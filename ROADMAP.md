# Transcend Skill Roadmap

This document tracks the high-performance CLI utilities targeted for Transcend skill integration, mapping the underlying binaries to their typed agent skill contracts.

### Current Implementation Status

Transcend now ships **29 skills** covering search, text/data processing, Git workflows, file operations, and development tooling. All skills include `.skill.json` contracts, Nunjucks execution templates, normalization pipelines, and comprehensive tests (285 tests across 85 suites).

| Skill Name | Primary Binary | Fallback Binary | Status | Description |
| :--- | :--- | :--- | :--- | :--- |
| `universal_search` | `rg` (ripgrep) | `grep` | **[x] Live** | Ultra-fast recursive regex file searching with exact byte-offset mapping. |
| `find_replace` | `sd` | `sed` | **[x] Live** | Targeted find-and-replace across multiple files with dry-run support. |
| `codebase_analysis` | `scc` | `tokei` | **[x] Live** | Comprehensive static analysis (cyclomatic complexity, COCOMO, language distribution). |
| `file_find` | `fd` | `find` | **[x] Live** | Blazing fast filesystem discovery by name, type, and size. |
| `json_query` | `jq` / `jaq` | — | **[x] Live** | Extract specific fields from massive JSON payloads. |
| `yaml_query` | `yq` | — | **[x] Live** | YAML/XML/TOML data extraction. |
| `column_extract` | `choose` | `awk` | **[x] Live** | Fast awk-like column extraction. |
| `csv_analysis` | `qsv` / `xsv` | — | **[x] Live** | Ultra-fast CSV querying and aggregation. |
| `file_view` | `bat` | `cat` / `head` | **[x] Live** | Surgical, syntax-aware line-range extraction. |
| `directory_jump` | `zoxide` (`z`) | — | **[x] Live** | Smart directory navigation. |
| `structural_search` | `ast-grep` (`sg`) | — | **[x] Live** | AST-aware structural code matching. |
| `benchmark_execution` | `hyperfine` | — | **[x] Live** | Statistical command benchmarking. |
| `python_lint_fix` | `ruff` | — | **[x] Live** | Instant Python static analysis and auto-fixing. |
| `js_lint_fix` | `biome` | — | **[x] Live** | Instant Web static analysis and auto-fixing. |
| `rust_test` | `cargo-nextest` | `cargo test` | **[x] Live** | Structured test execution and failure extraction. |
| `semantic_diff` | `delta` / `difftastic` | `git diff` | **[x] Live** | Structural, language-aware diff generation. |
| `process_list` | `procs` | `ps` | **[x] Live** | Structured process tree analysis. |
| `github_api` | `gh` | — | **[x] Live** | PR creation, issue management, code review workflows. |
| `git_status` | `git` | — | **[x] Live** | Structured working tree status. |
| `git_diff` | `git` | — | **[x] Live** | Normalized patch output with hunks and stats. |
| `git_log` | `git` | — | **[x] Live** | Commit history with decorations and graph support. |
| `git_blame` | `git` | — | **[x] Live** | Line-level attribution with boundary detection. |
| `git_branch` | `git` | — | **[x] Live** | Branch listing with remote tracking and merge status. |
| `git_stash` | `git` | — | **[x] Live** | Stash list with untracked files support. |
| `git_tag` | `git` | — | **[x] Live** | Annotated and lightweight tag listing. |
| `git_remote` | `git` | — | **[x] Live** | Remote configuration and URL display. |
| `git_config` | `git` | — | **[x] Live** | Git configuration variable query. |
| `git_show` | `git` | — | **[x] Live** | Object inspection (commits, tags, trees). |
| `git_cherry_pick` | `git` | — | **[x] Live** | Cherry-pick with conflict detection and resolution hints. |
| `git_merge` | `git` | — | **[x] Live** | Merge execution with status and conflict reporting. |

---

## Target CLI Utilities by Category

### 🔍 Search & Navigation
*The core primitives for an agent to build mental models of large codebases.*

- [x] **ripgrep (`rg`)** → `universal_search` (Recursive regex searching)
- [x] **fd (`fd`)** → `file_find` (Blazing fast filesystem discovery by name/type/size)
- [x] **ast-grep (`sg`)** → `structural_search` (AST-aware structural code matching)
- [x] **zoxide (`z`)** → `directory_jump` (Smart directory navigation)
- [ ] **fzf** / **broot** / **yazi** / **eza**

### 📄 Text & Data Processing
*Offloading heavy string/JSON manipulation from the LLM back to the CPU.*

- [x] **sd (`sd`)** → `find_replace` (Intuitive, fast find-and-replace)
- [x] **jq / jaq (`jq`)** → `json_query` (Extracting specific fields from massive JSON payloads)
- [x] **yq (`yq`)** → `yaml_query` (YAML/XML/TOML data extraction)
- [x] **choose (`choose`)** → `column_extract` (Fast awk-like column extraction)
- [x] **qsv / xsv (`qsv`)** → `csv_analysis` (Ultra-fast CSV querying and aggregation)
- [ ] **pandoc**

### 👁️ File Viewing
*Extracting surgical context from massive files without blowing out context windows.*

- [x] **bat (`bat`)** → `file_view` (Surgical, syntax-aware line-range extraction)
- [ ] **helix / hexyl / glow**

### 🌳 Git & Version Control
*Agent-native repository manipulation.*

- [x] **gh (`gh`)** → `github_api` (PR creation, issue management, code review workflows)
- [x] **delta / difftastic** → `semantic_diff` (Structural, language-aware diff generation)
- [x] **git** → `git_status`, `git_diff`, `git_log`, `git_blame`, `git_branch`, `git_stash`, `git_tag`, `git_remote`, `git_config`, `git_show`, `git_cherry_pick`, `git_merge`
- [ ] **gitui / lazygit**

### 📊 System Monitoring & Profiling
*Allowing agents to debug running processes and performance.*

- [x] **procs (`procs`)** → `process_list` (Structured process tree analysis)
- [ ] **bottom / bandwhich / dive**

### 🗄️ File Management
*Safe, reversible file operations.*

- [ ] **rip / rnr / ouch / dust / just / watchexec / zellij**

### 🌐 Network
- [ ] **xh / dog / atac**

### 🛠️ Environment & Development
*Automated execution, testing, and formatting.*

- [x] **hyperfine (`hyperfine`)** → `benchmark_execution` (Statistical command benchmarking)
- [x] **ruff (`ruff`)** → `python_lint_fix` (Instant Python static analysis and auto-fixing)
- [x] **biome (`biome`)** → `js_lint_fix` (Instant Web static analysis and auto-fixing)
- [x] **cargo-nextest** → `rust_test` (Structured test execution and failure extraction)
- [ ] **mise / direnv / starship / tealdeer / typos**
