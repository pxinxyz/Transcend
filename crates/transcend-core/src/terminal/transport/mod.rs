//! Terminal Transport Layer
//!
//! Exposes both PTY-based and Pipe-based process runners behind a unified interface.

pub mod pipe;
pub mod pty;

pub use pipe::PipeTransport;
pub use pty::PtyTransport;

/// Enum representing the active transport backend of a session.
pub enum ActiveTransport {
    Pipe(PipeTransport),
    Pty(PtyTransport),
}

impl ActiveTransport {
    pub fn pid(&self) -> u32 {
        match self {
            Self::Pipe(p) => p.pid,
            Self::Pty(p) => p.pid,
        }
    }

    pub fn is_running(&self) -> bool {
        match self {
            Self::Pipe(p) => p.is_running.load(std::sync::atomic::Ordering::Relaxed),
            Self::Pty(p) => p.is_running.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    pub fn exit_code(&self) -> Option<i32> {
        let raw = match self {
            Self::Pipe(p) => p.exit_code.load(std::sync::atomic::Ordering::Relaxed),
            Self::Pty(p) => p.exit_code.load(std::sync::atomic::Ordering::Relaxed),
        };
        if raw >= 0 {
            Some(raw)
        } else {
            None
        }
    }

    pub async fn write(&self, input: &[u8]) -> Result<usize, String> {
        match self {
            Self::Pipe(p) => {
                p.stdin_tx
                    .send(input.to_vec())
                    .map_err(|e| format!("Failed to dispatch to pipe stdin: {e}"))?;
                Ok(input.len())
            }
            Self::Pty(p) => {
                p.write_all(input)?;
                Ok(input.len())
            }
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        match self {
            Self::Pipe(_) => Err("Resize is only supported on PTY transports".to_string()),
            Self::Pty(p) => p.resize(cols, rows),
        }
    }

    pub async fn kill(&self) {
        match self {
            Self::Pipe(p) => {
                p.is_running.store(false, std::sync::atomic::Ordering::SeqCst);
                let mut owner = p.tree_owner.lock().await;
                owner.kill_tree();
            }
            Self::Pty(p) => {
                p.kill().await;
            }
        }
    }
}
