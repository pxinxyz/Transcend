//! Platform-Specific Shell Resolution & Process-Tree Ownership
//!
//! Enforces bulletproof process cleanup (via Windows Job Objects and Unix Process Groups)
//! so that child/grandchild processes (node, vite, cargo, rustc) are never orphaned.

use std::path::PathBuf;

/// Resolved shell invocation details.
#[derive(Debug, Clone)]
pub struct ShellSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// Resolve the invocation for `command` as a single `program` plus its arguments.
///
/// `wrap_in_shell` distinguishes the two callers:
///
/// - `true` (pipe execution): every command runs through a shell, so shell syntax
///   (`&&`, pipes, globs, `$VAR`) works. This is what a one-shot CLI invocation wants.
/// - `false` (interactive PTY): a bare shell or REPL is invoked **directly** so it stays
///   interactive. Wrapping it would run the program as a one-shot argument and exit
///   immediately — `powershell -Command "powershell -NoProfile -NoLogo"` prints a banner
///   and returns, leaving a dead session that accepts writes and silently ignores them.
///   Compound or argument-bearing commands are still wrapped, since those need a shell.
pub fn resolve_shell_spec(
    shell_override: Option<&str>,
    command: &str,
    wrap_in_shell: bool,
) -> ShellSpec {
    if !wrap_in_shell && let Some(spec) = bare_repl_spec(command) {
        return spec;
    }
    resolve_shell(shell_override, command)
}

/// Build a direct invocation for a command that is nothing but a shell or REPL program.
///
/// Returns `None` for anything compound (`a && b`), pipe or redirect syntax, or a
/// program given work to do, because those require a shell to interpret.
fn bare_repl_spec(command: &str) -> Option<ShellSpec> {
    let tokens = tokenize_command(command);
    let (first, rest) = tokens.split_first()?;

    // Strip a directory prefix and a Windows executable suffix so `C:\...\pwsh.exe`
    // and `pwsh` resolve identically.
    let program = first.rsplit(['/', '\\']).next().unwrap_or(first);
    let lowered = program.to_ascii_lowercase();
    let stem = lowered.strip_suffix(".exe").unwrap_or(&lowered);

    const INTERACTIVE_PROGRAMS: &[&str] = &[
        "bash",
        "cmd",
        "fish",
        "irb",
        "mongosh",
        "mysql",
        "node",
        "nu",
        "nushell",
        "powershell",
        "psql",
        "pwsh",
        "python",
        "python2",
        "python3",
        "redis-cli",
        "sh",
        "sqlite3",
        "zsh",
    ];
    if !INTERACTIVE_PROGRAMS.contains(&stem) {
        return None;
    }

    // Session modifiers such as `-NoProfile` leave the shell interactive and must be
    // preserved. Anything else (`-Command`, `-c`, `-e`, a script path) is work to run,
    // which belongs in a shell.
    const SESSION_MODIFIERS: &[&str] = &[
        "-nologo",
        "-noprofile",
        "-noninteractive",
        "-nol",
        "-nop",
        "-noexit",
    ];
    for token in rest {
        if !SESSION_MODIFIERS.contains(&token.to_ascii_lowercase().as_str()) {
            return None;
        }
    }

    Some(ShellSpec {
        program: PathBuf::from(first),
        args: rest.to_vec(),
    })
}

/// Split a command line into tokens, honouring single and double quotes.
///
/// Used only for classification; the result is never executed. Shell operators are
/// emitted as their own tokens so a compound command is never mistaken for a bare one.
fn tokenize_command(cmd: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for ch in cmd.chars() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '\'' | '"' => quote = Some(ch),
                c if c.is_whitespace() => {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                }
                '&' | '|' | ';' | '<' | '>' => {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                    tokens.push(ch.to_string());
                }
                _ => current.push(ch),
            },
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Resolves the optimal shell command for the current platform and optional user override.
pub fn resolve_shell(shell_override: Option<&str>, command: &str) -> ShellSpec {
    #[cfg(windows)]
    {
        match shell_override.map(|s| s.to_lowercase()).as_deref() {
            Some("cmd") => {
                let cmd_path = std::env::var_os("COMSPEC")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(r"C:\Windows\System32\cmd.exe"));
                ShellSpec {
                    program: cmd_path,
                    args: vec!["/C".to_string(), command.to_string()],
                }
            }
            Some("bash") | Some("sh") => {
                let bash_path = find_git_bash().unwrap_or_else(|| PathBuf::from("bash.exe"));
                ShellSpec {
                    program: bash_path,
                    args: vec!["-c".to_string(), command.to_string()],
                }
            }
            Some("pwsh") => {
                let pwsh = find_executable_on_path("pwsh.exe")
                    .unwrap_or_else(|| PathBuf::from("pwsh.exe"));
                ShellSpec {
                    program: pwsh,
                    args: vec![
                        "-NoProfile".to_string(),
                        "-NonInteractive".to_string(),
                        "-Command".to_string(),
                        command.to_string(),
                    ],
                }
            }
            Some("powershell") | None => {
                if let Some(pwsh) = find_executable_on_path("pwsh.exe") {
                    ShellSpec {
                        program: pwsh,
                        args: vec![
                            "-NoProfile".to_string(),
                            "-NonInteractive".to_string(),
                            "-Command".to_string(),
                            command.to_string(),
                        ],
                    }
                } else {
                    let has_chaining = command.contains("&&") || command.contains("||");
                    if has_chaining {
                        let cmd_path = std::env::var_os("COMSPEC")
                            .map(PathBuf::from)
                            .unwrap_or_else(|| PathBuf::from(r"C:\Windows\System32\cmd.exe"));
                        ShellSpec {
                            program: cmd_path,
                            args: vec!["/C".to_string(), command.to_string()],
                        }
                    } else {
                        let ps_path =
                            find_executable_on_path("powershell.exe").unwrap_or_else(|| {
                                PathBuf::from(
                                    r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
                                )
                            });
                        ShellSpec {
                            program: ps_path,
                            args: vec![
                                "-NoProfile".to_string(),
                                "-NonInteractive".to_string(),
                                "-Command".to_string(),
                                command.to_string(),
                            ],
                        }
                    }
                }
            }
            Some(custom) => ShellSpec {
                program: PathBuf::from(custom),
                args: vec!["-Command".to_string(), command.to_string()],
            },
        }
    }

    #[cfg(not(windows))]
    {
        match shell_override {
            Some("sh") => ShellSpec {
                program: PathBuf::from("/bin/sh"),
                args: vec!["-c".to_string(), command.to_string()],
            },
            Some(custom) => ShellSpec {
                program: PathBuf::from(custom),
                args: vec!["-c".to_string(), command.to_string()],
            },
            None => {
                let default_shell =
                    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
                ShellSpec {
                    program: PathBuf::from(default_shell),
                    args: vec!["-c".to_string(), command.to_string()],
                }
            }
        }
    }
}

#[cfg(windows)]
fn find_git_bash() -> Option<PathBuf> {
    let candidates = [
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files (x86)\Git\bin\bash.exe",
        r"C:\msys64\usr\bin\bash.exe",
    ];
    for cand in candidates {
        let p = PathBuf::from(cand);
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    find_executable_on_path("bash.exe")
}

#[cfg(windows)]
fn find_executable_on_path(executable: &str) -> Option<PathBuf> {
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(executable);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

// =========================================================================
// Process Tree Ownership (Windows Job Objects / Unix PGID)
// =========================================================================

/// RAII Process Tree Owner.
/// When killed or dropped, enforces termination of all child and descendant processes.
pub struct ProcessTreeOwner {
    #[cfg(windows)]
    job_handle: Option<windows_sys::Win32::Foundation::HANDLE>,
    #[cfg(not(windows))]
    pgid: Option<i32>,
    pid: Option<u32>,
}

impl ProcessTreeOwner {
    /// Create a new process tree manager.
    pub fn new() -> Self {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::JobObjects::*;

            let job_handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            let handle_opt = if !job_handle.is_null() {
                // Set KILL_ON_JOB_CLOSE limit so all processes die when job handle closes
                let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

                let ok = unsafe {
                    SetInformationJobObject(
                        job_handle,
                        JobObjectExtendedLimitInformation,
                        &info as *const _ as *const std::ffi::c_void,
                        std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                    )
                };
                if ok != 0 {
                    Some(job_handle)
                } else {
                    unsafe { windows_sys::Win32::Foundation::CloseHandle(job_handle) };
                    None
                }
            } else {
                None
            };

            Self {
                job_handle: handle_opt,
                pid: None,
            }
        }

        #[cfg(not(windows))]
        {
            Self {
                pgid: None,
                pid: None,
            }
        }
    }

    /// Assign a spawned process PID to this process tree owner.
    pub fn attach_pid(&mut self, pid: u32) {
        self.pid = Some(pid);

        #[cfg(windows)]
        {
            if let Some(job) = self.job_handle {
                use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
                use windows_sys::Win32::System::Threading::{
                    OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
                };

                let proc_handle =
                    unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid) };
                if !proc_handle.is_null() {
                    unsafe {
                        AssignProcessToJobObject(job, proc_handle);
                        windows_sys::Win32::Foundation::CloseHandle(proc_handle);
                    }
                }
            }
        }

        #[cfg(not(windows))]
        {
            self.pgid = Some(pid as i32);
        }
    }

    /// Forcibly terminate the entire process tree.
    ///
    /// Windows relies on the Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`), which
    /// covers every descendant even if intermediate processes have exited. Unix has no
    /// equivalent, so termination is layered: the process group is signalled first
    /// (covering children that stayed in the group), and any descendants that escaped
    /// the group are then enumerated and signalled directly.
    pub fn kill_tree(&mut self) {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::JobObjects::TerminateJobObject;
            use windows_sys::Win32::System::Threading::{
                OpenProcess, PROCESS_TERMINATE, TerminateProcess,
            };

            if let Some(job) = self.job_handle.take() {
                unsafe {
                    TerminateJobObject(job, 1);
                    windows_sys::Win32::Foundation::CloseHandle(job);
                }
            }
            if let Some(pid) = self.pid.take() {
                unsafe {
                    let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
                    if !h.is_null() {
                        TerminateProcess(h, 1);
                        windows_sys::Win32::Foundation::CloseHandle(h);
                    }
                }
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .output();
            }
        }

        #[cfg(not(windows))]
        {
            let pid = self.pid.take();
            let pgid = self.pgid.take();

            // 1. Signal the whole group. Only valid when the child is a group leader:
            //    pipe children get `setpgid` via `process_group(0)`, and PTY children
            //    become session leaders through portable-pty's `setsid()`.
            let group = pgid.or(pid.map(|p| p as i32));
            if let Some(pgid) = group {
                // A negative pid targets the group; guard against the pathological
                // pgid of 0/1, which would signal our own group or init.
                if pgid > 1 {
                    unsafe {
                        libc::kill(-pgid, libc::SIGTERM);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    unsafe {
                        libc::kill(-pgid, libc::SIGKILL);
                    }
                }
            }

            // 2. Sweep descendants that left the process group.
            if let Some(pid) = pid {
                for descendant in collect_descendants(pid) {
                    unsafe {
                        libc::kill(descendant, libc::SIGKILL);
                    }
                }
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        }
    }
}

/// Enumerate descendant PIDs of `root` that are still alive.
///
/// Linux reads the kernel's `children` file, which is authoritative and cheap. Other
/// Unixes fall back to `pgrep`, and if neither is available the caller still has the
/// process-group signal, so an empty result is safe rather than fatal.
#[cfg(not(windows))]
fn collect_descendants(root: u32) -> Vec<i32> {
    #[cfg(target_os = "linux")]
    {
        let mut found = Vec::new();
        let mut queue = vec![root];
        while let Some(pid) = queue.pop() {
            // A process may span several threads; children are listed per task.
            let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
                continue;
            };
            for task in tasks.flatten() {
                let children_path = task.path().join("children");
                let Ok(children) = std::fs::read_to_string(&children_path) else {
                    continue;
                };
                for raw in children.split_whitespace() {
                    if let Ok(child) = raw.parse::<u32>() {
                        if child != root && !found.contains(&child) {
                            found.push(child);
                            queue.push(child);
                        }
                    }
                }
            }
        }
        return found.into_iter().map(|p| p as i32).collect();
    }

    #[cfg(not(target_os = "linux"))]
    {
        let mut found = Vec::new();
        let mut frontier = vec![root];
        while let Some(pid) = frontier.pop() {
            let Ok(output) = std::process::Command::new("pgrep")
                .args(["-P", &pid.to_string()])
                .output()
            else {
                break;
            };
            for raw in String::from_utf8_lossy(&output.stdout).split_whitespace() {
                if let Ok(child) = raw.parse::<u32>() {
                    if child != root && !found.contains(&child) {
                        found.push(child);
                        frontier.push(child);
                    }
                }
            }
        }
        found.into_iter().map(|p| p as i32).collect()
    }
}

impl Default for ProcessTreeOwner {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ProcessTreeOwner {
    fn drop(&mut self) {
        self.kill_tree();
    }
}

// Windows HANDLE is a raw pointer (*mut c_void) which Rust marks as !Send and !Sync.
// Windows Job Object handles are thread-safe kernel handles that can safely be moved across threads.
#[cfg(windows)]
unsafe impl Send for ProcessTreeOwner {}
#[cfg(windows)]
unsafe impl Sync for ProcessTreeOwner {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every supported shell override must produce a `program` plus a command-passing
    /// flag, and the command must never be dropped.
    #[test]
    fn test_resolve_shell_shapes_are_well_formed() {
        let command = "echo cross_platform_marker";
        let overrides: [Option<&str>; 7] = [
            None,
            Some("sh"),
            Some("bash"),
            Some("pwsh"),
            Some("powershell"),
            Some("cmd"),
            Some("/bin/dash"),
        ];

        for shell in overrides {
            let spec = resolve_shell(shell, command);

            // A shell that cannot be resolved would silently break every `exec` call.
            #[cfg(not(windows))]
            {
                let program = spec.program.to_string_lossy();
                assert!(
                    program == "/bin/sh"
                        || std::path::Path::new(spec.program.as_os_str()).exists()
                        || matches!(
                            shell,
                            Some("bash") | Some("pwsh") | Some("powershell") | Some("cmd")
                        ),
                    "unresolvable shell for override {shell:?}: {program}"
                );
            }

            assert!(
                !spec.args.is_empty(),
                "shell override {shell:?} produced no arguments"
            );
            assert!(
                spec.args.iter().any(|a| a == command),
                "shell override {shell:?} did not forward the command: {:?}",
                spec.args
            );
        }
    }

    /// On Unix a command must be passed to `-c`; on Windows to `-Command` or `/C`.
    /// Getting this wrong yields "unknown option" failures at runtime, on one OS only.
    #[test]
    fn test_resolve_shell_uses_platform_flag() {
        let spec = resolve_shell(None, "echo hi");

        #[cfg(unix)]
        assert!(
            spec.args.iter().any(|a| a == "-c"),
            "unix shells must receive -c, got {:?}",
            spec.args
        );

        #[cfg(windows)]
        assert!(
            spec.args.iter().any(|a| a == "-Command" || a == "/C"),
            "windows shells must receive -Command or /C, got {:?}",
            spec.args
        );
    }

    /// An explicit override must win over the platform default.
    #[test]
    fn test_resolve_shell_honours_explicit_override() {
        let spec = resolve_shell(Some("sh"), "echo hi");
        let program = spec.program.to_string_lossy().to_ascii_lowercase();

        #[cfg(unix)]
        assert!(program.contains("sh"), "expected sh, got {program}");

        #[cfg(windows)]
        assert!(
            program.contains("bash") || program.contains("sh"),
            "expected a POSIX shell on Windows for override 'sh', got {program}"
        );
    }

    /// The process-tree owner must be constructible and droppable without a child.
    #[test]
    fn test_process_tree_owner_drop_without_child_is_safe() {
        let mut owner = ProcessTreeOwner::new();
        owner.kill_tree();
        owner.kill_tree(); // idempotent
        drop(ProcessTreeOwner::default());
    }

    /// Attaching a bogus pid must not kill anything or panic; `kill_tree` on a
    /// non-existent process is a no-op.
    #[test]
    fn test_process_tree_owner_tolerates_dead_pid() {
        let mut owner = ProcessTreeOwner::new();
        // A pid that cannot be running (way above any plausible pid_max).
        owner.attach_pid(999_999);
        owner.kill_tree();
    }

    /// Descendant enumeration must return nothing for a pid that does not exist,
    /// rather than erroring or looping.
    #[test]
    #[cfg(not(windows))]
    fn test_collect_descendants_handles_missing_pid() {
        assert!(collect_descendants(999_999).is_empty());
    }
}
