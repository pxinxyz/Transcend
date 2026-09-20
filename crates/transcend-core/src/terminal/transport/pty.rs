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
use super::super::platform::{ProcessTreeOwner, resolve_shell};

/// Active PTY Transport Session.
pub struct PtyTransport {
    pub pid: u32,
    pub master: Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>,
    pub writer: Arc<Mutex<Option<Box<dyn std::io::Write + Send>>>>,
    pub is_running: Arc<AtomicBool>,
    pub exit_code: Arc<AtomicI32>,
    pub tree_owner: Arc<tokio::sync::Mutex<ProcessTreeOwner>>,
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

        let spec = resolve_shell(shell_override, command);

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
                            let mut lock = buf_clone.lock().unwrap();
                            lock.write(&buf[..n]);
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
    pub fn write_all(&self, input: &[u8]) -> Result<(), String> {
        let mut writer_guard = self.writer.lock().unwrap();
        if let Some(writer) = writer_guard.as_mut() {
            writer
                .write_all(input)
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
