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
use transcend_protocol::{
    ExecRequest, ExecResponse, ExecStatus, ExecTransport, TerminalKillRequest,
    TerminalKillResponse, TerminalReadRequest, TerminalReadResponse, TerminalResizeRequest,
    TerminalResizeResponse, TerminalWriteRequest, TerminalWriteResponse, TimeoutAction,
};
use transport::{ActiveTransport, PipeTransport, PtyTransport};

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
        let session =
            self.registry.get(&req.session_id).await.ok_or_else(|| {
                CoreError::General(format!("Session not found: {}", req.session_id))
            })?;

        let max_bytes = req.max_bytes.unwrap_or(16_384);
        let from_cursor = req.cursor.unwrap_or(0);

        // Optional wait if requested: either for pattern match or for any new data
        if let Some(ref pattern) = req.wait_for_pattern {
            let wait_ms = req.timeout_ms.unwrap_or(5_000);
            let deadline = Instant::now() + Duration::from_millis(wait_ms);
            while Instant::now() < deadline {
                let (peek_output, _, _, _) = session.read(from_cursor, usize::MAX);
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
    pub async fn write(
        &self,
        req: &TerminalWriteRequest,
    ) -> Result<TerminalWriteResponse, CoreError> {
        let session =
            self.registry.get(&req.session_id).await.ok_or_else(|| {
                CoreError::General(format!("Session not found: {}", req.session_id))
            })?;

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
    pub async fn resize(
        &self,
        req: &TerminalResizeRequest,
    ) -> Result<TerminalResizeResponse, CoreError> {
        let session =
            self.registry.get(&req.session_id).await.ok_or_else(|| {
                CoreError::General(format!("Session not found: {}", req.session_id))
            })?;

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
        let session =
            self.registry.get(&req.session_id).await.ok_or_else(|| {
                CoreError::General(format!("Session not found: {}", req.session_id))
            })?;

        let (exit_code, final_output) = session.kill().await;

        Ok(TerminalKillResponse {
            session_id: req.session_id.clone(),
            success: true,
            exit_code,
            final_output: Some(final_output),
        })
    }

    /// Read output from an active or exited detached session (alias).
    pub async fn terminal_read(
        &self,
        req: &TerminalReadRequest,
    ) -> Result<TerminalReadResponse, CoreError> {
        self.read(req).await
    }

    /// Write input to a detached session's stdin (alias).
    pub async fn terminal_write(
        &self,
        req: &TerminalWriteRequest,
    ) -> Result<TerminalWriteResponse, CoreError> {
        self.write(req).await
    }

    /// Resize a detached session's terminal dimensions (alias).
    pub async fn terminal_resize(
        &self,
        req: &TerminalResizeRequest,
    ) -> Result<TerminalResizeResponse, CoreError> {
        self.resize(req).await
    }

    /// Kill an active detached session and terminate its process tree (alias).
    pub async fn terminal_kill(
        &self,
        req: &TerminalKillRequest,
    ) -> Result<TerminalKillResponse, CoreError> {
        self.kill(req).await
    }
}

/// Command prefixes that require a real TTY: full-screen UIs, watch loops, and
/// bare REPLs. Matched against the resolved program token plus its arguments, never
/// against arbitrary text elsewhere in the command line.
const INTERACTIVE_PREFIXES: &[&str] = &[
    "docker run -it",
    "docker compose up",
    "docker-compose up",
    "npm run dev",
    "npm run start",
    "yarn dev",
    "yarn start",
    "pnpm dev",
    "pnpm start",
    "cargo watch",
    "vite",
    "vitest",
    "webpack serve",
    "ng serve",
    "rails console",
    "irb",
    "ipython",
    "htop",
    "top",
    "less",
    "more",
    "vim",
    "nvim",
    "nano",
    "emacs",
];

/// Programs that drop into an interactive prompt when invoked without arguments.
const BARE_REPLS: &[&str] = &[
    "python",
    "python3",
    "python2",
    "node",
    "irb",
    "pry",
    "bash",
    "sh",
    "zsh",
    "fish",
    "pwsh",
    "powershell",
    "cmd",
    "cmd.exe",
    "sqlite3",
    "psql",
    "mysql",
    "redis-cli",
    "mongosh",
];

/// Heuristics to detect whether a command likely requires PTY terminal emulation.
///
/// Only the command's *program token and its own arguments* are considered. Scanning
/// the whole string for substrings misroutes ordinary commands: `git commit -m "fix
/// node handling"` would otherwise be forced onto a PTY, which is slower and loses
/// clean exit-code semantics.
fn is_interactive_command(cmd: &str) -> bool {
    let tokens = tokenize_for_detection(cmd);
    let Some(program) = tokens.first() else {
        return false;
    };

    // Strip any directory prefix and a Windows executable suffix so `C:\...\node.exe`
    // and `node` classify identically.
    let program = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program)
        .to_ascii_lowercase();
    let program = program.strip_suffix(".exe").unwrap_or(&program).to_string();

    // A bare REPL is interactive only when invoked without arguments.
    if tokens.len() == 1 && BARE_REPLS.contains(&program.as_str()) {
        return true;
    }

    // Multi-token prefixes are compared against the command's own opening tokens.
    let normalized = tokens
        .iter()
        .map(|t| t.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");

    for prefix in INTERACTIVE_PREFIXES {
        // `vite`/`vitest` etc. must be the program token, not an argument to something else.
        if prefix.contains(' ') {
            if normalized.starts_with(prefix) {
                return true;
            }
        } else if program == *prefix {
            return true;
        }
    }

    false
}

/// Split a command into tokens for heuristic classification only.
///
/// Unlike the `raw` execution path this never needs to be lossless: it exists purely
/// to find the program token, so quotes are stripped and shell operators are treated
/// as ordinary separators.
fn tokenize_for_detection(cmd: &str) -> Vec<String> {
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
                '&' | '|' | ';' | '>' | '<' => {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                    // Keep the operator so `a && b` does not look like one command.
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

#[cfg(test)]
mod tests {
    use super::*;
    use transcend_protocol::{
        ExecStatus, ExecTransport, TerminalKillRequest, TerminalReadRequest, TimeoutAction,
    };

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

        assert_eq!(
            post_kill_read.status,
            transcend_protocol::TerminalSessionStatus::Exited
        );
    }

    /// Interactive detection must key off the program token, not substrings anywhere
    /// in the command line. Misclassifying ordinary commands as interactive routes
    /// them through a PTY, which is slower and loses clean exit-code semantics.
    #[test]
    fn test_is_interactive_command_classification() {
        // Genuinely interactive: bare REPLs.
        assert!(is_interactive_command("python"));
        assert!(is_interactive_command("node"));
        assert!(is_interactive_command("python3"));
        assert!(is_interactive_command("pwsh"));
        assert!(is_interactive_command("cmd.exe"));
        assert!(is_interactive_command(
            r#""C:\Program Files\nodejs\node.exe""#
        ));
        assert!(is_interactive_command("/usr/local/bin/python3"));

        // Genuinely interactive: dev servers and watch loops.
        assert!(is_interactive_command("npm run dev"));
        assert!(is_interactive_command("npm run dev -- --port 3000"));
        assert!(is_interactive_command("vite"));
        assert!(is_interactive_command("cargo watch -x test"));
        assert!(is_interactive_command("docker run -it ubuntu bash"));
        assert!(is_interactive_command("htop"));

        // NOT interactive: the old substring scan misrouted every one of these.
        assert!(!is_interactive_command(
            "git commit -m \"fix node handling\""
        ));
        assert!(!is_interactive_command("echo python"));
        assert!(!is_interactive_command("grep -rn top ./src"));
        assert!(!is_interactive_command("npm install"));
        assert!(!is_interactive_command("cargo build --release"));
        assert!(!is_interactive_command("node --version"));
        assert!(!is_interactive_command("python -c 'print(1)'"));
        assert!(!is_interactive_command("docker run --rm alpine echo hi"));
        assert!(!is_interactive_command(""));
        assert!(!is_interactive_command("   "));
    }

    /// Tokenization must not let an operator fuse two commands into one program token.
    #[test]
    fn test_tokenize_for_detection_handles_quotes_and_operators() {
        assert_eq!(
            tokenize_for_detection("git commit -m 'a b'"),
            vec!["git", "commit", "-m", "a b"]
        );
        assert_eq!(
            tokenize_for_detection("npm install && npm run dev"),
            vec!["npm", "install", "&", "&", "npm", "run", "dev"]
        );
        // A quoted program name still resolves to a single token.
        assert_eq!(
            tokenize_for_detection("\"my tool\" --flag"),
            vec!["my tool", "--flag"]
        );
        assert!(tokenize_for_detection("").is_empty());
    }

    /// One-shot pipe execution must capture stdout and a real exit code.
    #[tokio::test]
    async fn test_terminal_pipe_captures_output_and_exit_code() {
        let engine = TerminalEngine::new();
        let res = engine
            .exec(&ExecRequest {
                command: "echo pipe_exit_check".to_string(),
                transport: Some(ExecTransport::Pipe),
                timeout_ms: Some(10_000),
                ..Default::default()
            })
            .await
            .expect("exec should succeed");

        assert_eq!(res.status, ExecStatus::Exited);
        assert_eq!(res.exit_code, Some(0), "output was: {}", res.output);
        assert!(res.output.contains("pipe_exit_check"));
    }

    /// A non-zero exit must be reported, not swallowed, so agents can branch on it.
    #[tokio::test]
    async fn test_terminal_pipe_reports_failure_exit_code() {
        let engine = TerminalEngine::new();
        #[cfg(windows)]
        let cmd = "exit 7";
        #[cfg(not(windows))]
        let cmd = "exit 7";

        let res = engine
            .exec(&ExecRequest {
                command: cmd.to_string(),
                transport: Some(ExecTransport::Pipe),
                timeout_ms: Some(10_000),
                ..Default::default()
            })
            .await
            .expect("exec should succeed");

        assert_eq!(res.status, ExecStatus::Exited);
        assert_eq!(res.exit_code, Some(7), "output was: {}", res.output);
    }

    /// Detached sessions must be discoverable and killable on every platform; this
    /// exercises the process-tree ownership path end to end.
    #[tokio::test]
    async fn test_terminal_detached_session_process_tree_is_killed() {
        let engine = TerminalEngine::new();

        #[cfg(windows)]
        let cmd = "powershell -NoProfile -Command \"Start-Sleep -Seconds 30\"";
        #[cfg(not(windows))]
        let cmd = "sleep 30";

        let res = engine
            .exec(&ExecRequest {
                command: cmd.to_string(),
                transport: Some(ExecTransport::Pipe),
                timeout_ms: Some(300),
                timeout_action: Some(TimeoutAction::Detach),
                ..Default::default()
            })
            .await
            .expect("exec should detach");

        assert_eq!(res.status, ExecStatus::Detached);
        let session_id = res.session_id.expect("detached session must have an id");

        let kill = engine
            .terminal_kill(&TerminalKillRequest {
                session_id: session_id.clone(),
            })
            .await
            .expect("kill should succeed");
        assert!(kill.success);

        let after = engine
            .terminal_read(&TerminalReadRequest {
                session_id,
                cursor: None,
                ..Default::default()
            })
            .await
            .expect("read after kill should succeed");
        assert!(!matches!(
            after.status,
            transcend_protocol::TerminalSessionStatus::Running
        ));
    }

    /// A PTY session must actually be interactive: input written to stdin has to
    /// reach the child and its output has to come back through the ring buffer.
    ///
    /// Regression test. `PtyTransport::spawn` used to route every command through
    /// `resolve_shell`, so a bare REPL became `powershell -Command "powershell
    /// -NoProfile -NoLogo"`: the wrapper ran the inner command, which printed its
    /// banner and exited, leaving a dead session. `terminal_write` then reported
    /// `bytes_written` while nothing reached any process — a silent failure at the
    /// byte level. `cmd.exe` masked the bug because `cmd /C cmd.exe` happens to stay
    /// interactive, which is why only that shell was covered before.
    #[tokio::test]
    async fn test_pty_session_is_actually_interactive() {
        let engine = TerminalEngine::new();

        #[cfg(windows)]
        let (cmd, marker) = (
            "powershell -NoProfile -NoLogo",
            "pty_stdin_roundtrip_marker",
        );
        #[cfg(not(windows))]
        let (cmd, marker) = ("sh", "pty_stdin_roundtrip_marker");

        let res = engine
            .exec(&ExecRequest {
                command: cmd.to_string(),
                transport: Some(ExecTransport::Pty),
                timeout_ms: Some(2000),
                timeout_action: Some(TimeoutAction::Detach),
                ..Default::default()
            })
            .await
            .expect("pty exec should start an interactive session");

        let session_id = res
            .session_id
            .expect("detached PTY session must have an id");

        // The session must still be running: a shell that already exited cannot
        // accept input, and that is exactly the bug this guards.
        assert_eq!(
            res.status,
            ExecStatus::Detached,
            "PTY session exited immediately; the command was run instead of an \
             interactive shell being started. Output was: {:?}",
            res.output
        );

        #[cfg(windows)]
        let input = format!("Write-Output '{marker}'\r\n");
        #[cfg(not(windows))]
        let input = format!("echo {marker}\n");

        let write = engine
            .terminal_write(&TerminalWriteRequest {
                session_id: session_id.clone(),
                input,
            })
            .await
            .expect("terminal_write should succeed");
        assert!(write.bytes_written > 0);

        // Poll rather than sleep a fixed amount: ConPTY latency varies by machine.
        let mut seen = String::new();
        for _ in 0..40 {
            let read = engine
                .terminal_read(&TerminalReadRequest {
                    session_id: session_id.clone(),
                    cursor: Some(0),
                    ..Default::default()
                })
                .await
                .expect("terminal_read should succeed");
            seen = read.output;
            if seen.contains(marker) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let _ = engine
            .terminal_kill(&TerminalKillRequest {
                session_id: session_id.clone(),
            })
            .await;

        assert!(
            seen.contains(marker),
            "stdin never reached the PTY child. terminal_write reported success but \
             no command ran. Captured output: {seen:?}"
        );
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
