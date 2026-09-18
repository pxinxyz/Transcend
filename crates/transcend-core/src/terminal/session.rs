//! Terminal Execution Session
//!
//! Represents a live or exited command execution context holding its output ring buffer,
//! transport handle, and lifecycle metadata.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use chrono::{DateTime, Utc};

use super::buffer::SharedCursorRingBuffer;
use super::projection::TerminalProjection;
use super::transport::ActiveTransport;
use transcend_protocol::TerminalSessionStatus;

/// An active or exited execution session.
pub struct TerminalSession {
    pub session_id: String,
    pub command: String,
    pub cwd: PathBuf,
    pub shell: String,
    pub transport_type: String,
    pub started_at: DateTime<Utc>,
    pub last_activity: Arc<AtomicU64>,
    pub buffer: SharedCursorRingBuffer,
    pub transport: ActiveTransport,
}

impl TerminalSession {
    /// Create a new session wrapping an active transport.
    pub fn new(
        session_id: String,
        command: String,
        cwd: PathBuf,
        shell: String,
        transport_type: String,
        transport: ActiveTransport,
        buffer: SharedCursorRingBuffer,
    ) -> Self {
        let now_epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            session_id,
            command,
            cwd,
            shell,
            transport_type,
            started_at: Utc::now(),
            last_activity: Arc::new(AtomicU64::new(now_epoch)),
            buffer,
            transport,
        }
    }

    /// Touch last activity timestamp.
    pub fn touch(&self) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_activity.store(now, Ordering::Relaxed);
    }

    /// Read incremental output from cursor offset, applying terminal normalization.
    pub fn read(&self, from_cursor: usize, max_bytes: usize) -> (String, usize, bool, Option<i32>) {
        self.touch();
        let (raw_chunk, next_cursor, truncated_raw, _) = {
            let buf = self.buffer.lock().unwrap();
            buf.read_from(from_cursor, max_bytes)
        };

        let (normalized_text, truncated_proj) = TerminalProjection::project(&raw_chunk, max_bytes);
        let truncated = truncated_raw || truncated_proj;
        let exit_code = self.transport.exit_code();

        (normalized_text, next_cursor, truncated, exit_code)
    }

    /// Send interactive input to the session stdin.
    pub async fn write(&self, input: &str) -> Result<usize, String> {
        self.touch();
        self.transport.write(input.as_bytes()).await
    }

    /// Resize terminal dimensions.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        self.touch();
        self.transport.resize(cols, rows)
    }

    /// Forcibly terminate the session and entire child process tree.
    pub async fn kill(&self) -> (Option<i32>, String) {
        self.transport.kill().await;

        // Drain any remaining output
        let raw = {
            let buf = self.buffer.lock().unwrap();
            buf.snapshot()
        };
        let (final_text, _) = TerminalProjection::project(&raw, 65536);
        let exit_code = self.transport.exit_code();

        (exit_code, final_text)
    }

    /// Whether the process is still running.
    pub fn is_running(&self) -> bool {
        self.transport.is_running()
    }

    /// Current lifecycle status.
    pub fn status(&self) -> TerminalSessionStatus {
        if self.is_running() {
            TerminalSessionStatus::Running
        } else if let Some(code) = self.transport.exit_code() {
            if code == 0 {
                TerminalSessionStatus::Exited
            } else {
                TerminalSessionStatus::Failed
            }
        } else {
            TerminalSessionStatus::Exited
        }
    }
}
