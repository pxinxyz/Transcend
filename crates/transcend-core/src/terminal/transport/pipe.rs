//! Pipe-Based Transport Runner
//!
//! Executes non-interactive commands using standard OS pipes.
//! Captures stdout and stderr concurrently without PTY overhead or terminal escape codes.

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::mpsc;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use super::super::buffer::SharedCursorRingBuffer;
use super::super::platform::{ProcessTreeOwner, resolve_shell};

/// Active Pipe Transport Session.
pub struct PipeTransport {
    pub pid: u32,
    pub stdin_tx: mpsc::UnboundedSender<Vec<u8>>,
    pub is_running: Arc<AtomicBool>,
    pub exit_code: Arc<AtomicI32>,
    pub tree_owner: Arc<tokio::sync::Mutex<ProcessTreeOwner>>,
}

fn tokenize_command(cmd: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;

    for ch in cmd.chars() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            ' ' | '\t' if !in_single && !in_double => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

impl PipeTransport {
    pub async fn spawn(
        command: &str,
        cwd: &Path,
        shell_override: Option<&str>,
        buffer: SharedCursorRingBuffer,
        raw: bool,
    ) -> Result<Self, String> {
        let (program, args) = if raw {
            let tokens = tokenize_command(command);
            if tokens.is_empty() {
                return Err("Cannot execute empty command in raw mode".to_string());
            }
            (std::path::PathBuf::from(&tokens[0]), tokens[1..].to_vec())
        } else {
            let spec = resolve_shell(shell_override, command);
            (spec.program, spec.args)
        };

        let mut cmd = Command::new(&program);
        cmd.args(&args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Put the child in its own process group so the whole tree can be signalled
        // as a unit. Without this, `ProcessTreeOwner` would try to `killpg` a pid that
        // is not a group leader, which fails with ESRCH and silently leaves
        // grandchildren (node, cargo, vite, ...) running.
        //
        // PTY children already become session leaders via `setsid()` inside
        // portable-pty, so only the pipe path needs this.
        #[cfg(unix)]
        cmd.process_group(0);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn pipe command '{}': {e}", program.display()))?;

        let pid = child.id().unwrap_or(0);

        let mut tree_owner = ProcessTreeOwner::new();
        if pid > 0 {
            tree_owner.attach_pid(pid);
        }
        let tree_owner = Arc::new(tokio::sync::Mutex::new(tree_owner));

        let mut stdin = child.stdin.take().ok_or("Failed to open child stdin")?;
        let mut stdout = child.stdout.take().ok_or("Failed to open child stdout")?;
        let mut stderr = child.stderr.take().ok_or("Failed to open child stderr")?;

        let is_running = Arc::new(AtomicBool::new(true));
        let exit_code = Arc::new(AtomicI32::new(-1));

        // Background stdin writer task
        let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        tokio::spawn(async move {
            while let Some(bytes) = stdin_rx.recv().await {
                if stdin.write_all(&bytes).await.is_err() || stdin.flush().await.is_err() {
                    break;
                }
            }
        });

        // Background stdout reader task
        let buf_clone1 = Arc::clone(&buffer);
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut lock = buf_clone1.lock().unwrap();
                        lock.write(&buf[..n]);
                    }
                    Err(_) => break,
                }
            }
        });

        // Background stderr reader task
        let buf_clone2 = Arc::clone(&buffer);
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut lock = buf_clone2.lock().unwrap();
                        lock.write(&buf[..n]);
                    }
                    Err(_) => break,
                }
            }
        });

        // Background waiter task
        let running_clone = Arc::clone(&is_running);
        let code_clone = Arc::clone(&exit_code);
        tokio::spawn(async move {
            match child.wait().await {
                Ok(status) => {
                    let code = status
                        .code()
                        .unwrap_or(if status.success() { 0 } else { 1 });
                    code_clone.store(code, Ordering::SeqCst);
                    running_clone.store(false, Ordering::SeqCst);
                }
                Err(_) => {
                    code_clone.store(1, Ordering::SeqCst);
                    running_clone.store(false, Ordering::SeqCst);
                }
            }
        });

        Ok(Self {
            pid,
            stdin_tx,
            is_running,
            exit_code,
            tree_owner,
        })
    }
}
