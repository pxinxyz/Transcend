# Cross-platform audit — verified findings

Read-only audit of the Unix/macOS branches, which have never been executed (Windows 10
Pro 22H2 is the only verified platform). Findings marked **confirmed** I re-read and
verified myself; the rest are as reported by the audit with file:line citations.

Method: source inspection, plus `cargo tree --target x86_64-unknown-linux-gnu` /
`--target x86_64-apple-darwin` for dependency gating, plus compiling **verbatim copies**
of the Unix code paths in a scratch crate outside the repo. A full-workspace
`cargo check --target x86_64-unknown-linux-gnu` cannot complete on this machine: cc-rs
needs a Linux C toolchain for the tree-sitter grammars. So the Unix branches *type-check*,
but no Unix *runtime* behaviour has been observed.

---

## HIGH

### 1. Unix process-tree kill: the descendant sweep is a no-op by construction — CONFIRMED
`crates/transcend-core/src/terminal/platform.rs:397-431`, `collect_descendants` at `440-491`

Step 1 signals the process group: `kill(-pgid, SIGTERM)`, sleep 50ms, `kill(-pgid, SIGKILL)`.
Step 2 then sweeps descendants — but by then the root has been SIGKILLed and, during that
50ms sleep, very likely reaped by the tokio waiter (`pipe.rs:154-168`) or the PTY waiter
thread (`pty.rs:170-196`). A reaped process has no children: they are reparented to init.

`collect_descendants` depends exactly on the dead root still existing — it walks
`/proc/{pid}/task/*/children` (Linux) or `pgrep -P <pid>` (other Unixes). Both return
empty for a reaped pid, and the Linux arm `continue`s past an unreadable `/proc/{pid}/task`.
So step 2 can only ever return an empty set. I verified this ordering by reading the code.

The comment at `platform.rs:420` ("Sweep descendants that left the process group") states an
intent the code cannot fulfil. This matters for the specific case it names: anything that
called `setsid()` and left the group (`nohup x &`, `--daemonize`, `docker run -d`). And for
interactive PTY shells: `exec("bash", transport: pty)` reaches
`resolve_shell_spec(.., wrap_in_shell = false)`, running a real interactive shell whose job
control puts **each typed command in its own process group**, so `kill(-shell_pgid)` misses
it and the sweep then walks a dead shell. `terminal_kill` reports success while the job runs on.

**Fix:** snapshot descendants *before* signalling, then group-SIGTERM, then group-SIGKILL,
then SIGKILL anything in the snapshot that survived. Strictly more effective than the
current order and no less safe. Better still on Linux/macOS: sweep by *session* id, since
the child is a session leader (`setsid`).

### 2. Unix shell resolution trusts `$SHELL` blindly — CONFIRMED
`platform.rs:232-239`

```rust
let default_shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
ShellSpec { program: PathBuf::from(default_shell), args: vec!["-c".into(), command.into()] }
```

- `SHELL=""` is `Ok("")` → `program: ""` → every `exec` fails ENOENT.
- `SHELL` pointing at an uninstalled binary → every `exec` fails.
- `SHELL` unset on Alpine/NixOS/distroless → `/bin/bash` does not exist → every `exec`
  fails, although `/bin/sh` is POSIX-guaranteed.
- `$SHELL` is a *login* preference, not a `-c` interpreter (fish/csh differ).

Note the asymmetry: the Windows branch probes `COMSPEC`, runs `find_git_bash()`, and calls
`find_executable_on_path()`; the Unix branch validates nothing.

**Fix:** validate the candidate exists and is executable, then fall back to the passwd
entry, then `/bin/sh`. Add a regression test for `SHELL=""`.

### 3. Linux `lsp_install` recipes need `sudo` with no TTY and closed stdin
`crates/transcend-core/src/lsp/installer.rs:85-92, 116-123, 205-212, 231-238, 256-263`, run at `451-459`

`Command::output()` closes stdin, so `sudo` cannot prompt (it reads `/dev/tty`) → exit 1 on
a normal desktop; in a root container `sudo` may not exist at all → exit 127. `apt-get` is
also invoked with no preceding `apt-get update`, and `jdtls`/`lua-language-server` are not
in the default Ubuntu/Debian repos.

**Fix:** probe privilege (`geteuid() == 0`, `sudo -n true`) and return an actionable
"run this manually" message instead of a confident failure.

## MEDIUM

### 4. Wrong package manager per OS
`installer.rs:142-149` — the Linux branch for kotlin recipes `brew install kotlin-language-server`
(a copy of the macOS branch); `installer.rs:161-165` — swift uses `brew` ungated, so it can
never be installed on Windows or Linux.

### 5. Unused import breaks the clippy gate on Linux/macOS — CONFIRMED
`crates/transcend-core/src/terminal/transport/pipe.rs:14-15`

```rust
#[cfg(unix)]
use std::os::unix::process::CommandExt;
```

`tokio::process::Command` has an **inherent** `process_group`, so the trait import is
unused; on Unix the crate emits `warning: unused import`. AGENTS.md §5 makes
`cargo clippy --workspace --all-targets` with warnings-as-errors a gate, so on Linux/macOS
the documented gate fails on a clean checkout. Verified by compiling the snippet against
`x86_64-unknown-linux-gnu`: that was the only diagnostic produced.

**Fix:** delete lines 14-15. Trivial, and zero risk on Windows.

### 6. PTY sessions never set `TERM`
`pty.rs:100-107` copies the parent environment but `TERM` is never defaulted anywhere in the
workspace. MCP hosts launch servers with piped stdio and often a minimal environment
(launchd/systemd user units commonly lack `TERM`), so on Unix `vim`/`htop`/`less` abort with
"TERM environment variable not set". Windows is unaffected (conhost configures the console).
Terminal *size* is fine — `openpty` applies `TIOCSWINSZ` and `resize()` works.

### 7. patch insert modes emit bare LF into CRLF files
`patch.rs:223-238, 268-273, 288-292`, versus the one CRLF-aware branch at `297-302`

The code knows about CRLF in exactly one place and ignores it in the others, so `patch` on a
CRLF file writes mixed line endings and a whole-line-churn diff. `.gitattributes` does not
govern files the tool edits. The reading side is correct (`file_ops.rs:202-259` counts `\n`
and uses `str::lines()`), so only the splice/write side is affected. This one bites on
Windows too, which is the verified platform.

### 8. LSP discovery misses version-manager dirs and never checks the exec bit
`registry.rs:311-321, 337-341, 358-360`

`~/.nvm/versions/node/*/bin`, `~/.volta/bin`, `~/.bun/bin`, `~/Library/pnpm`,
`~/.pyenv/shims` are never searched, so under a host-minimal PATH every npm/pip server
reports missing and `is_on_path("npm")` is false — meaning a *successful* `npm install -g`
is then reported as "not detected in search paths". Also `is_file()` accepts a
non-executable file on Unix, unlike `which`.

### 9. Non-installer recipes
`installer.rs:183-187` — the `dart` "install" step is `dart language-server --protocol=lsp`,
which launches a server on stdio rather than installing anything (it exits on the closed
stdin or burns the 180s timeout). `pip install pyright`/`ruff` assume a `pip` binary that
Debian/Ubuntu ≥23.04 rejects (PEP 668) and macOS does not ship.

### 10. LSP positions use byte columns, not UTF-16 code units — CONFIRMED, all platforms
`crates/transcend-core/src/lsp/bridge.rs:103-104` and `120-138`

```rust
let point = node.start_position();
return Some((point.row as u32, point.column as u32));   // tree-sitter column = BYTE offset
...
if let Some(col_idx) = line.find(symbol_name) {         // BYTE index
```

LSP `character` is a UTF-16 code-unit offset. Any non-ASCII text earlier on the same line
shifts the position, so `lsp_definition`/`lsp_hover` land on the wrong column — and
`fallback_text_scan`'s `after_idx = col_idx + symbol_name.len()` slices at a byte boundary,
which can panic on a non-char-boundary or miscalculate the word boundary. Not OS-specific:
it fires on a line with an emoji, CJK comment, or accented identifier. This is a definite
bug I verified by reading, and it is the one finding here that is fully fixable and
testable on Windows.

## Verified as correct — do not re-litigate

- **EIO on PTY close is handled**, just not where you would look: `portable-pty-0.9.0/src/unix.rs:93-106`
  maps `EIO` to `Ok(0)` inside `PtyFd::read`, which the reader thread consumes, so
  `pty.rs:150`'s `Ok(0) => break` terminates cleanly.
- **`pgid == pid` holds for both paths**: pipe children get `process_group(0)`
  (`pipe.rs:88`); portable-pty's Unix child calls `setsid()` + `TIOCSCTTY` (`unix.rs:257-274`).
- **Cargo target gating is correct**: `transcend-core/Cargo.toml:44-48` gates `windows-sys`
  behind cfg(windows) and `libc` behind cfg(not(windows)). Both cross-target `cargo tree`
  runs show libc/nix on Unix and **zero** windows/winapi/conpty crates; no Unix-only dep
  leaks into the Windows graph. Linux/macOS builds will not fail for dependency reasons.
- **No stubs**: no `todo!()`, `unimplemented!()`, or error-returning `#[cfg(unix)]` branch
  anywhere in the workspace. The Unix branches are real implementations.
- **Low-impact nuance**: dropping the PTY writer on Unix injects `b"\n"` + `VEOF` into the
  master (`portable-pty` `unix.rs:393-405`), including from `PtyTransport::kill`
  (`pty.rs:290-292`). Windows has no equivalent.

## Cannot be verified without a Linux/macOS machine

1. Real behaviour of `kill(-pgid)`, `setsid` races, `pgrep -P`. Finding 1 is a static
   ordering argument, not an observed leak.
2. Whether macOS `pgrep -P` lists zombie children.
3. Whether a GUI-launched macOS host actually leaves `TERM`/`SHELL` unset.
4. Whether `sudo` has a TTY/NOPASSWD, and whether the named packages exist in the target
   distro's repositories (package names came from reading, not a repo index query).
5. Full-workspace `cargo check/build/test` for Linux/macOS (blocked by the missing cross C
   toolchain for tree-sitter).
