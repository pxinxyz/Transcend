# Transcend Skill Roadmap

This document tracks the high-performance CLI utilities targeted for Transcend skill integration, mapping the underlying binaries to their typed agent skill contracts.

### Current Implementation Status

| Skill Name | Primary Binary | Fallback Binary | Status | Description |
| :--- | :--- | :--- | :--- | :--- |
| `universal_search` | `rg` (ripgrep) | `grep` | **[x] Live** | Ultra-fast recursive regex file searching with exact byte-offset mapping. |
| `find_replace` | `sd` | `sed` | **[x] Live** | Targeted find-and-replace across multiple files with dry-run support. |
| `codebase_analysis` | `scc` | `tokei` | **[x] Live** | Comprehensive static analysis (cyclomatic complexity, COCOMO, language distribution). |

---

## Target CLI Utilities by Category

### 🔍 Search & Navigation
*The core primitives for an agent to build mental models of large codebases.*

- [x] **ripgrep (`rg`)** → `universal_search` (Recursive regex searching)
- [ ] **fd (`fd`)** → `file_find` (Blazing fast filesystem discovery by name/type/size)
- [ ] **ast-grep (`sg`)** → `structural_search` (AST-aware structural code matching)
- [ ] **zoxide (`z`)** → `directory_jump` (Smart directory navigation)
- [ ] **fzf** / **broot** / **yazi** / **eza**

### 📄 Text & Data Processing
*Offloading heavy string/JSON manipulation from the LLM back to the CPU.*

- [x] **sd (`sd`)** → `find_replace` (Intuitive, fast find-and-replace)
- [ ] **jq / jaq (`jq`)** → `json_query` (Extracting specific fields from massive JSON payloads)
- [ ] **yq (`yq`)** → `yaml_query` (YAML/XML/TOML data extraction)
- [ ] **choose (`choose`)** → `column_extract` (Fast awk-like column extraction)
- [ ] **qsv / xsv (`qsv`)** → `csv_analysis` (Ultra-fast CSV querying and aggregation)
- [ ] **pandoc**

### 👁️ File Viewing
*Extracting surgical context from massive files without blowing out context windows.*

- [ ] **bat (`bat`)** → `file_view` (Surgical, syntax-aware line-range extraction)
- [ ] **helix / hexyl / glow**

### 🌳 Git & Version Control
*Agent-native repository manipulation.*

- [ ] **gh (`gh`)** → `github_api` (PR creation, issue management, code review workflows)
- [ ] **delta / difftastic** → `semantic_diff` (Structural, language-aware diff generation)
- [ ] **gitui / lazygit** 

### 📊 System Monitoring & Profiling
*Allowing agents to debug running processes and performance.*

- [ ] **procs (`procs`)** → `process_list` (Structured process tree analysis)
- [ ] **bottom / bandwhich / dive**

### 🗄️ File Management
*Safe, reversible file operations.*

- [ ] **rip / rnr / ouch / dust / just / watchexec / zellij**

### 🌐 Network
- [ ] **xh / dog / atac**

### 🛠️ Environment & Development
*Automated execution, testing, and formatting.*

- [ ] **hyperfine (`hyperfine`)** → `benchmark_execution` (Statistical command benchmarking)
- [ ] **ruff (`ruff`)** → `python_lint_fix` (Instant Python static analysis and auto-fixing)
- [ ] **biome (`biome`)** → `js_lint_fix` (Instant Web static analysis and auto-fixing)
- [ ] **cargo-nextest** → `rust_test` (Structured test execution and failure extraction)
- [ ] **mise / direnv / starship / tealdeer / typos**
