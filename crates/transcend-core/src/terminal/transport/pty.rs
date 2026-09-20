//! PTY-Based Transport Runner
//!
//! Executes interactive and TTY-aware commands inside a native pseudoterminal
//! (ConPTY on Windows, openpty on Unix) using portable-pty.

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use super::super::buffer::SharedCursorRingBuffer;
use super::super::platform::{ProcessTreeOwner, resolve_shell_spec};

/// Active PTY Transport Session.
pub struct PtyTransport {
    pub pid: u32,
    pub master: Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>,
    pub writer: Arc<Mutex<Option<Box<dyn std::io::Write + Send>>>>,
    pub is_running: Arc<AtomicBool>,
    pub exit_code: Arc<AtomicI32>,
    pub tree_owner: Arc<tokio::sync::Mutex<ProcessTreeOwner>>,
}

/// Device Status Report request (`ESC [ 6 n`).
///
/// ConPTY emits this during startup and **blocks until the terminal answers** with a
/// Cursor Position Report. Nothing in the harness is a real terminal, so without a reply
/// the conhost never finishes initialising, the shell never prints a prompt or banner,
/// and the session looks alive while producing no output at all.
const CURSOR_POSITION_REQUEST: &[u8] = b"\x1b[6n";

/// Cursor Position Report (`ESC [ <row> ; <col> R`), 1-based.
fn cursor_position_report(rows: u16, cols: u16) -> Vec<u8> {
    format!("\x1b[{};{}R", rows.max(1), cols.max(1)).into_bytes()
}

/// Rewrite line endings to `\r` for PTY input.
///
/// A `\n` is emitted as `\r`, except when it directly follows a `\r` (a CRLF pair),
/// where the `\r` already submits the line and the `\n` is dropped. Two consecutive
/// `\n` therefore remain two submits, which matters for blank-line input.
fn normalize_input_newlines(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    for (index, &byte) in input.iter().enumerate() {
        match byte {
            b'\n' if index > 0 && input[index - 1] == b'\r' => {}
            b'\n' => out.push(b'\r'),
            other => out.push(other),
        }
    }
    out
}

/// Scan `chunk` for terminal queries that require a reply and return the responses.
///
/// Only queries ConPTY actually blocks on are answered; inventing replies for other
/// sequences would corrupt the session's input.
fn terminal_query_responses(chunk: &[u8], rows: u16, cols: u16) -> Vec<Vec<u8>> {
    let mut responses = Vec::new();
    let mut index = 0;
    while index + CURSOR_POSITION_REQUEST.len() <= chunk.len() {
        if &chunk[index..index + CURSOR_POSITION_REQUEST.len()] == CURSOR_POSITION_REQUEST {
            responses.push(cursor_position_report(rows, cols));
            index += CURSOR_POSITION_REQUEST.len();
        } else {
            index += 1;
        }
    }
    responses
}

impl PtyTransport {
    pub fn spawn(
        command: &str,
        cwd: &Path,
        shell_override: Option<&str>,
        buffer: SharedCursorRingBuffer,
        cols: u16,
        rows: u16,
    ) -> Result<Self, String> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to open PTY: {e}"))?;

        // PTY sessions must be genuinely interactive, so a bare shell or REPL is
        // invoked directly rather than as a shell argument. Routing it through
        // `resolve_shell` turned `powershell -NoProfile -NoLogo` into
        // `powershell -Command "powershell -NoProfile -NoLogo"`, which ran the inner
        // command, printed its banner and exited — leaving a session that reported
        // successful writes while no process was listening.
        let spec = resolve_shell_spec(shell_override, command, false);

        let mut cmd = CommandBuilder::new(&spec.program);
        for (k, v) in std::env::vars_os() {
            cmd.env(k, v);
        }
        for arg in &spec.args {
            cmd.arg(arg);
        }
        cmd.cwd(cwd);

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn PTY child: {e}"))?;

        // Crucial: drop the slave in the parent process immediately after spawning.
        // Otherwise, ConPTY and pseudo-terminals keep the connection open,
        // child.wait() blocks forever, and EOF is never signaled.
        drop(pair.slave);

        let pid = child.process_id().unwrap_or(0);

        let mut tree_owner = ProcessTreeOwner::new();
        if pid > 0 {
            tree_owner.attach_pid(pid);
        }
        let tree_owner = Arc::new(tokio::sync::Mutex::new(tree_owner));

        let is_running = Arc::new(AtomicBool::new(true));
        let exit_code = Arc::new(AtomicI32::new(-1));

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Failed to clone PTY reader: {e}"))?;

        // Query responses must go back to the child, but the writer is owned by the
        // caller-facing struct. The reader thread therefore forwards detected queries
        // over a channel and a pump writes them, splitting a borrow that would
        // otherwise be a cycle (reader needs writer; writer lives in `Self`).
        let (query_tx, mut query_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        // Background reader thread draining ConPTY / openpty into CursorRingBuffer
        let buf_clone = Arc::clone(&buffer);
        let running_reader = Arc::clone(&is_running);
        std::thread::Builder::new()
            .name("transcend-pty-reader".to_string())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                while running_reader.load(Ordering::Relaxed) {
                    match reader.read(&mut buf) {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            let chunk = &buf[..n];
                            for response in terminal_query_responses(chunk, rows, cols) {
                                // Best effort: a dropped reply only costs a stalled
                                // shell, and the session is already being torn down.
                                let _ = query_tx.send(response);
                            }
                            let mut lock = buf_clone.lock().unwrap();
                            lock.write(chunk);
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn PTY reader thread: {e}"))?;

        // Background child wait thread using non-blocking try_wait polling
        let running_waiter = Arc::clone(&is_running);
        let code_waiter = Arc::clone(&exit_code);
        std::thread::Builder::new()
            .name("transcend-pty-waiter".to_string())
            .spawn(move || {
                loop {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            let code = if status.success() { 0 } else { 1 };
                            code_waiter.store(code, Ordering::SeqCst);
                            running_waiter.store(false, Ordering::SeqCst);
                            break;
                        }
                        Ok(None) => {
                            if !running_waiter.load(Ordering::Relaxed) {
                                // Process was killed or cancelled externally
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(25));
                        }
                        Err(_) => {
                            code_waiter.store(1, Ordering::SeqCst);
                            running_waiter.store(false, Ordering::SeqCst);
                            break;
                        }
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn PTY waiter thread: {e}"))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to obtain PTY writer: {e}"))?;
        let writer = Arc::new(Mutex::new(Some(writer)));

        let master = Arc::new(Mutex::new(Some(pair.master)));

        // Pump thread: forwards reader-detected terminal queries back to the child's
        // stdin. Without it, ConPTY's startup `ESC[6n` is never answered and the shell
        // never initialises.
        let writer_for_queries = Arc::clone(&writer);
        let running_pump = Arc::clone(&is_running);
        std::thread::Builder::new()
            .name("transcend-pty-reply".to_string())
            .spawn(move || {
                while running_pump.load(Ordering::Relaxed) {
                    match query_rx.blocking_recv() {
                        Some(response) => {
                            let mut guard = writer_for_queries.lock().unwrap();
                            if let Some(w) = guard.as_mut() {
                                if w.write_all(&response).is_err() || w.flush().is_err() {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                        None => break,
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn PTY reply thread: {e}"))?;

        Ok(Self {
            pid,
            master,
            writer,
            is_running,
            exit_code,
            tree_owner,
        })
    }

    /// Write input data to the PTY master.
    ///
    /// A bare `\n` is translated to `\r`. A pseudo-terminal delivers input to the child
    /// as terminal keystrokes, and Windows console hosts only treat carriage return as
    /// "submit this line" — with `\n` alone the shell buffers the text, shows a
    /// continuation prompt, and never executes it. Callers naturally send `\n`, so the
    /// translation belongs here rather than in every caller.
    pub fn write_all(&self, input: &[u8]) -> Result<(), String> {
        let translated = normalize_input_newlines(input);
        let mut writer_guard = self.writer.lock().unwrap();
        if let Some(writer) = writer_guard.as_mut() {
            writer
                .write_all(&translated)
                .map_err(|e| format!("Failed to write to PTY: {e}"))?;
            writer
                .flush()
                .map_err(|e| format!("Failed to flush PTY: {e}"))?;
            Ok(())
        } else {
            Err("PTY writer has been closed".to_string())
        }
    }

    /// Resize PTY terminal dimensions.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        let master_guard = self.master.lock().unwrap();
        if let Some(master) = master_guard.as_ref() {
            master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| format!("Failed to resize PTY: {e}"))
        } else {
            Err("PTY master has been closed".to_string())
        }
    }

    /// Forcibly terminate process tree and release PTY master to signal EOF to reader thread.
    pub async fn kill(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        {
            let mut owner = self.tree_owner.lock().await;
            owner.kill_tree();
        }
        {
            let mut writer_guard = self.writer.lock().unwrap();
            writer_guard.take();
        }
        {
            // ClosePseudoConsole on Windows is a blocking call that can deadlock if closed
            // synchronously while the reader thread is in a pending ReadFile.
            // Dispatching the master drop onto a detached background cleanup thread
            // prevents thread blocking and allows ConPTY cleanup to proceed cleanly.
            let master_opt = {
                let mut master_guard = self.master.lock().unwrap();
                master_guard.take()
            };
            if let Some(m) = master_opt {
                std::thread::Builder::new()
                    .name("transcend-pty-closer".to_string())
                    .spawn(move || {
                        drop(m);
                    })
                    .ok();
            }
        }
    }
}

impl Drop for PtyTransport {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        if let Ok(mut writer_guard) = self.writer.lock() {
            writer_guard.take();
        }
        if let Ok(mut master_guard) = self.master.lock()
            && let Some(m) = master_guard.take()
        {
            std::thread::Builder::new()
                .name("transcend-pty-closer".to_string())
                .spawn(move || {
                    drop(m);
                })
                .ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_position_report_is_one_based() {
        assert_eq!(cursor_position_report(30, 120), b"\x1b[30;120R");
        // A zero-sized terminal must still produce a valid report.
        assert_eq!(cursor_position_report(0, 0), b"\x1b[1;1R");
    }

    #[test]
    fn test_detects_conpty_startup_query() {
        // The exact sequence ConPTY emits at startup; `ESC[6n` is the blocking one.
        let chunk = b"\x1b[?9001h\x1b[?1004h\x1b[6n";
        let responses = terminal_query_responses(chunk, 30, 120);
        assert_eq!(responses.len(), 1, "expected exactly one reply");
        assert_eq!(responses[0], b"\x1b[30;120R");
    }

    #[test]
    fn test_ignores_unrelated_sequences() {
        // Private mode set/reset and SGR colour must NOT be answered.
        for chunk in [
            &b"\x1b[?9001h"[..],
            &b"\x1b[?1004h"[..],
            &b"\x1b[0m\x1b[1;32mhello"[..],
            &b"plain text with no escapes"[..],
            &b""[..],
        ] {
            assert!(
                terminal_query_responses(chunk, 30, 120).is_empty(),
                "should not reply to {chunk:?}"
            );
        }
    }

    #[test]
    fn test_replies_to_every_query_in_a_batch() {
        let chunk = b"\x1b[6n\x1b[6n";
        assert_eq!(terminal_query_responses(chunk, 10, 10).len(), 2);
    }

    #[test]
    fn test_partial_query_at_chunk_boundary_is_not_matched() {
        // A truncated escape must not be misread as a full query; a false reply would
        // inject stray bytes into the child's stdin.
        assert!(terminal_query_responses(b"\x1b[6", 30, 120).is_empty());
        assert!(terminal_query_responses(b"\x1b[", 30, 120).is_empty());
    }

    /// PTY input must submit on `\n`: Windows console hosts treat only carriage return
    /// as "run this line", so an un-translated `\n` makes the shell print a continuation
    /// prompt and never execute the command.
    #[test]
    fn test_input_newlines_are_translated_to_carriage_return() {
        assert_eq!(normalize_input_newlines(b"echo hi\n"), b"echo hi\r");
        // CRLF must submit exactly once, not twice.
        assert_eq!(normalize_input_newlines(b"echo hi\r\n"), b"echo hi\r");
        // An existing CR is left alone.
        assert_eq!(normalize_input_newlines(b"echo hi\r"), b"echo hi\r");
        // Two LFs are two submits (blank line), so they must not be collapsed.
        assert_eq!(normalize_input_newlines(b"a\n\nb\n"), b"a\r\rb\r");
        // Control characters such as Ctrl+C are not line endings and pass through.
        assert_eq!(normalize_input_newlines(b"\x03"), b"\x03");
        assert!(normalize_input_newlines(b"").is_empty());
    }

    /// A bare REPL must be invoked directly for a PTY, never wrapped in a shell.
    #[test]
    fn test_bare_repl_is_not_shell_wrapped() {
        let spec = resolve_shell_spec(None, "powershell -NoProfile -NoLogo", false);
        let program = spec.program.to_string_lossy().to_ascii_lowercase();
        assert!(
            program.contains("powershell"),
            "bare REPL must resolve to the program itself, got {program}"
        );
        assert!(
            !spec.args.iter().any(|a| a == "-Command" || a == "/C"),
            "bare REPL must not be wrapped as a one-shot command: {:?}",
            spec.args
        );

        // Compound commands still need a shell.
        let compound = resolve_shell_spec(None, "echo a && echo b", false);
        assert!(
            compound
                .args
                .iter()
                .any(|a| a == "-Command" || a == "/C" || a == "-c"),
            "compound command must still be shell-wrapped: {:?}",
            compound.args
        );

        // Pipe execution always wraps, even for a bare REPL.
        let piped = resolve_shell_spec(None, "python", true);
        assert!(
            piped
                .args
                .iter()
                .any(|a| a == "-Command" || a == "/C" || a == "-c"),
            "pipe execution must always shell-wrap: {:?}",
            piped.args
        );
    }
}
