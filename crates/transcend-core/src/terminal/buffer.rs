//! Cursor-Based Bounded Circular Ring Buffer
//!
//! Provides a memory-bounded byte storage mechanism (protecting against OOM crashes)
//! while tracking linear monotonic write cursors for streaming, non-redundant client reads.

use std::sync::{Arc, Mutex};

/// Default capacity for the in-memory ring buffer (2 MiB).
pub const DEFAULT_BUFFER_CAPACITY: usize = 2 * 1024 * 1024;

/// A thread-safe, memory-bounded ring buffer with linear monotonic cursors.
#[derive(Debug)]
pub struct CursorRingBuffer {
    capacity: usize,
    buffer: Vec<u8>,
    head: usize,         // Current write pointer in circular buffer (0..capacity)
    write_cursor: usize, // Monotonic total bytes ever written
    is_wrapped: bool,
}

impl CursorRingBuffer {
    /// Create a new ring buffer with the specified byte capacity.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "Capacity must be positive");
        Self {
            capacity,
            buffer: vec![0u8; capacity],
            head: 0,
            write_cursor: 0,
            is_wrapped: false,
        }
    }

    /// Append a slice of raw bytes to the ring buffer.
    pub fn write(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }

        if data.len() >= self.capacity {
            // Write only the tail that fits in capacity
            let tail = &data[data.len() - self.capacity..];
            self.buffer[..self.capacity].copy_from_slice(tail);
            self.head = 0;
            self.is_wrapped = true;
            self.write_cursor += data.len();
            return;
        }

        let space_to_end = self.capacity - self.head;
        if data.len() <= space_to_end {
            self.buffer[self.head..self.head + data.len()].copy_from_slice(data);
            self.head += data.len();
            if self.head == self.capacity {
                self.head = 0;
                self.is_wrapped = true;
            }
        } else {
            self.buffer[self.head..self.capacity].copy_from_slice(&data[..space_to_end]);
            let remaining = data.len() - space_to_end;
            self.buffer[..remaining].copy_from_slice(&data[space_to_end..]);
            self.head = remaining;
            self.is_wrapped = true;
        }

        self.write_cursor += data.len();
    }

    /// Current monotonic total bytes written.
    pub fn write_cursor(&self) -> usize {
        self.write_cursor
    }

    /// Oldest monotonic byte position still stored in memory.
    pub fn oldest_available_cursor(&self) -> usize {
        if !self.is_wrapped {
            0
        } else {
            self.write_cursor.saturating_sub(self.capacity)
        }
    }

    /// Read bytes from a specified cursor offset up to `max_bytes`.
    /// Returns: `(data, next_cursor, truncated, dropped_before)`.
    pub fn read_from(&self, from_cursor: usize, max_bytes: usize) -> (Vec<u8>, usize, bool, usize) {
        if from_cursor >= self.write_cursor {
            return (Vec::new(), self.write_cursor, false, 0);
        }

        let oldest = self.oldest_available_cursor();
        let dropped_before = oldest.saturating_sub(from_cursor);

        let effective_start = from_cursor.max(oldest);
        let available_bytes = self.write_cursor - effective_start;
        let bytes_to_read = available_bytes.min(max_bytes);
        let truncated = available_bytes > max_bytes;

        let mut out = Vec::with_capacity(bytes_to_read);

        // Convert effective_start to circular buffer offset
        let buffer_start = if !self.is_wrapped {
            effective_start
        } else {
            // In wrapped buffer, oldest byte is at self.head
            (self.head + (effective_start - oldest)) % self.capacity
        };

        let first_chunk = (self.capacity - buffer_start).min(bytes_to_read);
        out.extend_from_slice(&self.buffer[buffer_start..buffer_start + first_chunk]);

        let remaining = bytes_to_read - first_chunk;
        if remaining > 0 {
            out.extend_from_slice(&self.buffer[..remaining]);
        }

        let next_cursor = effective_start + bytes_to_read;
        (out, next_cursor, truncated, dropped_before)
    }

    /// Get all currently buffered bytes as a contiguous byte vector.
    pub fn snapshot(&self) -> Vec<u8> {
        let (data, _, _, _) = self.read_from(0, self.capacity);
        data
    }
}

/// Shared, thread-safe cursor ring buffer.
pub type SharedCursorRingBuffer = Arc<Mutex<CursorRingBuffer>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_basic_write_read() {
        let mut rb = CursorRingBuffer::new(10);
        rb.write(b"hello");
        assert_eq!(rb.write_cursor(), 5);

        let (data, next_cursor, truncated, dropped) = rb.read_from(0, 100);
        assert_eq!(&data, b"hello");
        assert_eq!(next_cursor, 5);
        assert!(!truncated);
        assert_eq!(dropped, 0);

        // Read again from next_cursor -> empty
        let (data2, next2, _, _) = rb.read_from(next_cursor, 100);
        assert!(data2.is_empty());
        assert_eq!(next2, 5);
    }

    #[test]
    fn test_ring_buffer_wrapping_and_dropping() {
        let mut rb = CursorRingBuffer::new(10);
        rb.write(b"0123456789"); // filled
        assert_eq!(rb.write_cursor(), 10);

        rb.write(b"ABCDE"); // overwritten 01234, now holds 56789ABCDE
        assert_eq!(rb.write_cursor(), 15);
        assert_eq!(rb.oldest_available_cursor(), 5);

        // Reading from cursor 0 should report 5 dropped bytes
        let (data, next_cursor, _, dropped) = rb.read_from(0, 100);
        assert_eq!(dropped, 5);
        assert_eq!(&data, b"56789ABCDE");
        assert_eq!(next_cursor, 15);
    }

    #[test]
    fn test_ring_buffer_max_bytes_truncation() {
        let mut rb = CursorRingBuffer::new(20);
        rb.write(b"0123456789abcdefghij");

        let (data, next_cursor, truncated, _) = rb.read_from(0, 5);
        assert_eq!(&data, b"01234");
        assert_eq!(next_cursor, 5);
        assert!(truncated);

        let (data2, next_cursor2, truncated2, _) = rb.read_from(next_cursor, 5);
        assert_eq!(&data2, b"56789");
        assert_eq!(next_cursor2, 10);
        assert!(truncated2);
    }
}
