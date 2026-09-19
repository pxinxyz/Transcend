//! LSP Framing Codec & JSON-RPC 2.0 Types
//!
//! Handles encoding and decoding of standard LSP header-framed messages
//! (`Content-Length: <n>\r\n\r\n<payload>`) over asynchronous stdio streams.

use serde_json::Value;

/// Format a JSON payload with standard LSP HTTP-like headers.
pub fn format_lsp_message(value: &Value) -> Result<Vec<u8>, serde_json::Error> {
    let json_bytes = serde_json::to_vec(value)?;
    let header = format!("Content-Length: {}\r\n\r\n", json_bytes.len());
    let mut out = Vec::with_capacity(header.len() + json_bytes.len());
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(&json_bytes);
    Ok(out)
}

/// Buffer for reading framed LSP messages from a byte stream.
#[derive(Default, Debug)]
pub struct LspMessageReader {
    buffer: Vec<u8>,
}

impl LspMessageReader {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
        }
    }

    /// Feed chunk of raw bytes into the reader buffer.
    pub fn feed(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
    }

    /// Attempt to parse the next complete JSON-RPC message from the buffer.
    /// Returns `Some(Value)` if a complete message is available, or `None` if more data is needed.
    pub fn next_message(&mut self) -> Result<Option<Value>, String> {
        let header_end_delim = b"\r\n\r\n";
        let Some(header_end_idx) = self.buffer.windows(4).position(|w| w == header_end_delim) else {
            return Ok(None);
        };

        let header_str = std::str::from_utf8(&self.buffer[..header_end_idx])
            .map_err(|e| format!("Invalid UTF-8 in LSP headers: {e}"))?;

        let mut content_length: Option<usize> = None;
        for line in header_str.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("Content-Length:") {
                content_length = val.trim().parse::<usize>().ok();
            }
        }

        let Some(len) = content_length else {
            return Err(format!("LSP headers missing Content-Length: {header_str}"));
        };

        let body_start = header_end_idx + 4;
        let body_end = body_start + len;

        if self.buffer.len() < body_end {
            // Incomplete body, wait for more data
            return Ok(None);
        }

        let body_bytes = &self.buffer[body_start..body_end];
        let message: Value = serde_json::from_slice(body_bytes)
            .map_err(|e| format!("Failed to parse JSON-RPC body: {e}"))?;

        // Drain the parsed message and its headers from the buffer
        self.buffer.drain(..body_end);

        Ok(Some(message))
    }
}

/// Helper to build a JSON-RPC 2.0 Request.
pub fn make_request(id: u64, method: &str, params: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params
    })
}

/// Helper to build a JSON-RPC 2.0 Notification.
pub fn make_notification(method: &str, params: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params
    })
}

/// Convert an absolute file path to a file:// URI.
pub fn path_to_uri(path: &std::path::Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let clean = crate::clean_path(&canonical);
    let path_str = clean.to_string_lossy().replace('\\', "/");
    let trimmed = path_str.trim_start_matches('/');
    let uri_path = trimmed.replace(' ', "%20");
    format!("file:///{uri_path}")
}

/// Convert a file:// URI to a local PathBuf.
pub fn uri_to_path(uri: &str) -> std::path::PathBuf {
    let path_str = uri
        .strip_prefix("file:///")
        .or_else(|| uri.strip_prefix("file://"))
        .unwrap_or(uri);

    // Decode URL-encoded characters (e.g. %20 -> space)
    let decoded = urlencoding_decode(path_str);
    crate::clean_path(&std::path::PathBuf::from(decoded))
}

fn urlencoding_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex_byte) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                out.push(hex_byte as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_and_parse_lsp_message() {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": 1234
            }
        });

        let formatted = format_lsp_message(&msg).expect("format should succeed");
        assert!(formatted.starts_with(b"Content-Length: "));

        let mut reader = LspMessageReader::new();
        // Feed in two chunks to test stream buffering
        let split_idx = formatted.len() / 2;
        reader.feed(&formatted[..split_idx]);
        assert_eq!(reader.next_message().unwrap(), None);

        reader.feed(&formatted[split_idx..]);
        let parsed = reader.next_message().unwrap().expect("should parse message");
        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["method"], "initialize");
        assert_eq!(parsed["params"]["processId"], 1234);

        // Buffer should now be empty
        assert_eq!(reader.next_message().unwrap(), None);
    }

    #[test]
    fn test_uri_path_roundtrip() {
        let win_path = std::path::PathBuf::from(r"C:\Projects\Test\main.rs");
        let uri = path_to_uri(&win_path);
        assert!(uri.starts_with("file:///"));
        let back = uri_to_path(&uri);
        assert_eq!(back.file_name().unwrap(), "main.rs");
    }
}
