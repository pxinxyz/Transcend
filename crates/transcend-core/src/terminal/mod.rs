//! Hybrid Terminal & PTY Execution Subsystem
//!
//! Orchestrates process transport (Pipe vs PTY), lifecycle management (blocking vs detached),
//! process-tree ownership, and token-compact terminal projection.

pub mod buffer;
pub mod platform;
pub mod projection;
pub mod registry;
pub mod session;
pub mod transport;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use buffer::{CursorRingBuffer, DEFAULT_BUFFER_CAPACITY};
use registry::TerminalRegistry;
use session::TerminalSession;
use transport::{ActiveTransport, PipeTransport, PtyTransport};
use transcend_protocol::{
    ExecRequest, ExecResponse, ExecStatus, ExecTransport, TerminalKillRequest, TerminalKillResponse,
    TerminalReadRequest, TerminalReadResponse, TerminalResizeRequest, TerminalResizeResponse,
    TerminalWriteRequest, TerminalWriteResponse, TimeoutAction,
};

use crate::CoreError;

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Central execution engine for terminal operations.
#[derive(Clone)]
pub struct TerminalEngine {
    registry: TerminalRegistry,
}

impl Default for TerminalEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalEngine {
    /// Create a new TerminalEngine instance.
    pub fn new() -> Self {
        Self {
            registry: TerminalRegistry::new(),
        }
    }

    /// Execute a command using the hybrid model.
    pub async fn exec(&self, req: &ExecRequest) -> Result<ExecResponse, CoreError> {
        let start_time = Instant::now();
        let timeout_ms = req.timeout_ms.unwrap_or(10_000);
        let max_output_bytes = req.max_output_bytes.unwrap_or(32_768);
        let timeout_action = req.timeout_action.unwrap_or_default();

        let cwd = match &req.cwd {
            Some(p) => PathBuf::from(p),
            None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        };

        if !cwd.exists() {
            return Err(CoreError::General(format!(
                "Working directory does not exist: {}",
                cwd.display()
            )));
        }

        // Decide transport (Auto, Pipe, or Pty)
        let transport_mode = req.transport.unwrap_or_default();
        let use_pty = match transport_mode {
            ExecTransport::Pty => true,
            ExecTransport::Pipe => false,
            ExecTransport::Auto => is_interactive_command(&req.command),
        };

        let buffer = Arc::new(Mutex::new(CursorRingBuffer::new(DEFAULT_BUFFER_CAPACITY)));

        let session_id = format!(
            "pty_{:x}_{:x}",
            std::process::id(),
            SESSION_COUNTER.fetch_add(1, Ordering::SeqCst)
        );

        let active_transport = if use_pty {
            let pty = PtyTransport::spawn(
                &req.command,
                &cwd,
                req.shell.as_deref(),
                Arc::clone(&buffer),
                120,
                30,
            )
            .map_err(CoreError::General)?;
            ActiveTransport::Pty(pty)
        } else {
            let pipe = PipeTransport::spawn(
                &req.command,
                &cwd,
                req.shell.as_deref(),
                Arc::clone(&buffer),
                req.raw.unwrap_or(false),
            )
            .await
            .map_err(CoreError::General)?;
            ActiveTransport::Pipe(pipe)
        };

        let transport_name = if use_pty { "pty" } else { "pipe" }.to_string();
        let resolved_shell = req.shell.clone().unwrap_or_else(|| "default".to_string());

        let session = Arc::new(TerminalSession::new(
            session_id.clone(),
            req.command.clone(),
            cwd,
            resolved_shell,
            transport_name,
            active_transport,
            buffer,
        ));

        // Poll for completion up to timeout_ms
        let timeout_dur = Duration::from_millis(timeout_ms);
        let poll_interval = Duration::from_millis(50);
        let deadline = Instant::now() + timeout_dur;

        let mut exited = false;
        while Instant::now() < deadline {
            if !session.is_running() {
                exited = true;
                break;
            }
            tokio::time::sleep(poll_interval).await;
        }

        // Pre-detach race elimination: do one final check
        if !exited && !session.is_running() {
            exited = true;
        }

        let elapsed_ms = start_time.elapsed().as_millis() as u64;

        if exited {
            // Process exited within timeout window
            let (output, cursor, truncated, exit_code) = session.read(0, max_output_bytes);
            Ok(ExecResponse {
                status: ExecStatus::Exited,
                exit_code,
                session_id: None,
                output,
                cursor,
                truncated,
                elapsed_ms,
            })
        } else {
            // Process is still running at timeout deadline
            match timeout_action {
                TimeoutAction::Detach => {
                    let (output, cursor, truncated, _) = session.read(0, max_output_bytes);
                    self.registry.insert(Arc::clone(&session)).await;

                    Ok(ExecResponse {
                        status: ExecStatus::Detached,
                        exit_code: None,
                        session_id: Some(session_id),
                        output,
                        cursor,
                        truncated,
                        elapsed_ms,
                    })
                }
                TimeoutAction::Kill => {
                    let (exit_code, output) = session.kill().await;
                    let cursor = session.buffer.lock().unwrap().write_cursor();
                    Ok(ExecResponse {
                        status: ExecStatus::Exited,
                        exit_code: exit_code.or(Some(137)),
                        session_id: None,
                        output,
                        cursor,
                        truncated: false,
                        elapsed_ms,
                    })
                }
                TimeoutAction::Error => {
                    session.kill().await;
                    Err(CoreError::General(format!(
                        "Command timed out after {timeout_ms}ms"
                    )))
                }
            }
        }
    }

    /// Read incremental output from a detached session using a cursor.
    pub async fn read(&self, req: &TerminalReadRequest) -> Result<TerminalReadResponse, CoreError> {
        let session = self
            .registry
            .get(&req.session_id)
            .await
            .ok_or_else(|| CoreError::General(format!("Session not found: {}", req.session_id)))?;

        let max_bytes = req.max_bytes.unwrap_or(16_384);
        let from_cursor = req.cursor.unwrap_or(0);

        // Optional wait if requested: either for pattern match or for any new data
        if let Some(ref pattern) = req.wait_for_pattern {
            let wait_ms = req.timeout_ms.unwrap_or(5_000);
            let deadline = Instant::now() + Duration::from_millis(wait_ms);
            while Instant::now() < deadline {
                let (peek_output, _, _, _) = session.read(from_cursor, max_bytes);
                if peek_output.contains(pattern) || !session.is_running() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        } else if let Some(wait_ms) = req.timeout_ms {
            let deadline = Instant::now() + Duration::from_millis(wait_ms);
            while Instant::now() < deadline {
                let current_cursor = session.buffer.lock().unwrap().write_cursor();
                if current_cursor > from_cursor || !session.is_running() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        let (output, next_cursor, truncated, exit_code) = session.read(from_cursor, max_bytes);
        let status = session.status();

        Ok(TerminalReadResponse {
            session_id: req.session_id.clone(),
            status,
            output,
            next_cursor,
            exit_code,
            truncated,
        })
    }

    /// Write input to a detached session's stdin.
    pub async fn write(&self, req: &TerminalWriteRequest) -> Result<TerminalWriteResponse, CoreError> {
        let session = self
            .registry
            .get(&req.session_id)
            .await
            .ok_or_else(|| CoreError::General(format!("Session not found: {}", req.session_id)))?;

        let bytes_written = session
            .write(&req.input)
            .await
            .map_err(CoreError::General)?;

        Ok(TerminalWriteResponse {
            session_id: req.session_id.clone(),
            bytes_written,
        })
    }

    /// Resize a detached session's terminal dimensions.
    pub async fn resize(&self, req: &TerminalResizeRequest) -> Result<TerminalResizeResponse, CoreError> {
        let session = self
            .registry
            .get(&req.session_id)
            .await
            .ok_or_else(|| CoreError::General(format!("Session not found: {}", req.session_id)))?;

        session
            .resize(req.cols, req.rows)
            .map_err(CoreError::General)?;

        Ok(TerminalResizeResponse {
            session_id: req.session_id.clone(),
            success: true,
        })
    }

    /// Kill an active detached session and terminate its process tree.
    pub async fn kill(&self, req: &TerminalKillRequest) -> Result<TerminalKillResponse, CoreError> {
        let session = self
            .registry
            .get(&req.session_id)
            .await
            .ok_or_else(|| CoreError::General(format!("Session not found: {}", req.session_id)))?;

        let (exit_code, final_output) = session.kill().await;

        Ok(TerminalKillResponse {
            session_id: req.session_id.clone(),
            success: true,
            exit_code,
            final_output: Some(final_output),
        })
    }

    /// Read output from an active or exited detached session (alias).
    pub async fn terminal_read(&self, req: &TerminalReadRequest) -> Result<TerminalReadResponse, CoreError> {
        self.read(req).await
    }

    /// Write input to a detached session's stdin (alias).
    pub async fn terminal_write(&self, req: &TerminalWriteRequest) -> Result<TerminalWriteResponse, CoreError> {
        self.write(req).await
    }

    /// Resize a detached session's terminal dimensions (alias).
    pub async fn terminal_resize(&self, req: &TerminalResizeRequest) -> Result<TerminalResizeResponse, CoreError> {
        self.resize(req).await
    }

    /// Kill an active detached session and terminate its process tree (alias).
    pub async fn terminal_kill(&self, req: &TerminalKillRequest) -> Result<TerminalKillResponse, CoreError> {
        self.kill(req).await
    }
}

/// Heuristics to detect whether a command likely requires PTY terminal emulation.
fn is_interactive_command(cmd: &str) -> bool {
    let lower = cmd.to_lowercase();
    let tokens: Vec<&str> = lower.split_whitespace().collect();

    let interactive_starters = [
        "npm run dev",
        "npm run start",
        "yarn dev",
        "yarn start",
        "pnpm dev",
        "vite",
        "cargo watch",
        "python -i",
        "python",
        "node",
        "ipython",
        "irb",
        "bash -i",
        "sh -i",
        "docker run -it",
        "docker-compose up",
        "watch",
        "htop",
        "top",
    ];

    for starter in interactive_starters {
        if lower.starts_with(starter) || lower.contains(starter) {
            return true;
        }
    }

    // Single token REPL invocation (e.g. `python`, `node`)
    if tokens.len() == 1 {
        let repls = ["python", "python3", "node", "irb", "bash", "sh", "pwsh", "powershell"];
        if repls.contains(&tokens[0]) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use transcend_protocol::{ExecStatus, ExecTransport, TerminalKillRequest, TerminalReadRequest, TimeoutAction};

    #[tokio::test]
    async fn test_terminal_exec_one_shot_pipe() {
        let engine = TerminalEngine::new();
        let res = engine
            .exec(&ExecRequest {
                command: "echo one_shot_pipe_success".to_string(),
                transport: Some(ExecTransport::Pipe),
                timeout_ms: Some(5000),
                ..Default::default()
            })
            .await
            .expect("exec should succeed");

        assert_eq!(res.status, ExecStatus::Exited);
        assert_eq!(res.exit_code, Some(0));
        assert!(res.output.contains("one_shot_pipe_success"));
        assert!(res.session_id.is_none());
    }

    #[tokio::test]
    async fn test_terminal_exec_detach_and_stream_and_kill() {
        let engine = TerminalEngine::new();

        #[cfg(windows)]
        let cmd = "powershell -NoProfile -Command \"Write-Output start_long; Start-Sleep -Seconds 10; Write-Output finish_long\"";
        #[cfg(not(windows))]
        let cmd = "echo start_long && sleep 10 && echo finish_long";

        let res = engine
            .exec(&ExecRequest {
                command: cmd.to_string(),
                transport: Some(ExecTransport::Pipe),
                timeout_ms: Some(400),
                timeout_action: Some(TimeoutAction::Detach),
                ..Default::default()
            })
            .await
            .expect("exec should detach");

        assert_eq!(res.status, ExecStatus::Detached);
        assert!(res.session_id.is_some());
        let session_id = res.session_id.unwrap();

        // 1. Read initial output
        let read_res = engine
            .terminal_read(&TerminalReadRequest {
                session_id: session_id.clone(),
                cursor: Some(0),
                ..Default::default()
            })
            .await
            .expect("terminal_read should succeed");

        assert_eq!(read_res.session_id, session_id);

        // 2. Kill the session
        let kill_res = engine
            .terminal_kill(&TerminalKillRequest {
                session_id: session_id.clone(),
            })
            .await
            .expect("terminal_kill should succeed");

        assert!(kill_res.success);

        // 3. Verify session status is now exited
        let post_kill_read = engine
            .terminal_read(&TerminalReadRequest {
                session_id: session_id.clone(),
                cursor: None,
                ..Default::default()
            })
            .await
            .expect("read after kill should succeed");

        assert_eq!(post_kill_read.status, transcend_protocol::TerminalSessionStatus::Exited);
    }

    #[tokio::test]
    async fn test_terminal_exec_pty() {
        let engine = TerminalEngine::new();
        #[cfg(windows)]
        let cmd = "cmd.exe";
        #[cfg(not(windows))]
        let cmd = "sh";

        let res = engine
            .exec(&ExecRequest {
                command: cmd.to_string(),
                transport: Some(ExecTransport::Pty),
                timeout_ms: Some(300),
                timeout_action: Some(TimeoutAction::Detach),
                ..Default::default()
            })
            .await
            .expect("pty exec should detach into interactive session");

        assert_eq!(res.status, ExecStatus::Detached);
        assert!(res.session_id.is_some());
        let session_id = res.session_id.unwrap();

        // Send input into PTY
        let write_res = engine
            .terminal_write(&transcend_protocol::TerminalWriteRequest {
                session_id: session_id.clone(),
                input: "echo pty_interactive_success\r\n".to_string(),
            })
            .await
            .expect("writing to PTY should succeed");

        assert!(write_res.bytes_written > 0);

        // Give PTY short time to process input
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Read incremental output
        let read_res = engine
            .terminal_read(&TerminalReadRequest {
                session_id: session_id.clone(),
                cursor: Some(0),
                ..Default::default()
            })
            .await
            .expect("reading from PTY should succeed");

        assert_eq!(read_res.session_id, session_id);

        // Kill the PTY session
        let kill_res = engine
            .terminal_kill(&TerminalKillRequest {
                session_id: session_id.clone(),
            })
            .await
            .expect("killing PTY session should succeed");

        assert!(kill_res.success);
    }
}
