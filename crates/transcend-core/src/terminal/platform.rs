//! Platform-Specific Shell Resolution & Process-Tree Ownership
//!
//! Enforces bulletproof process cleanup (via Windows Job Objects and Unix Process Groups)
//! so that child/grandchild processes (node, vite, cargo, rustc) are never orphaned.

use std::path::{Path, PathBuf};

/// Resolved shell invocation details.
#[derive(Debug, Clone)]
pub struct ShellSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
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
                let pwsh = find_executable_on_path("pwsh.exe")
                    .or_else(|| find_executable_on_path("powershell.exe"))
                    .unwrap_or_else(|| PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"));
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
                let default_shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
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
        let p = Path::new(cand);
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
                use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

                let proc_handle = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid) };
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
    pub fn kill_tree(&mut self) {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::JobObjects::TerminateJobObject;
            use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

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
            if let Some(pgid) = self.pgid.take() {
                unsafe {
                    libc::killpg(pgid, libc::SIGKILL);
                }
            } else if let Some(pid) = self.pid {
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        }
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
unsafe impl Send for ProcessTreeOwner {}
unsafe impl Sync for ProcessTreeOwner {}
