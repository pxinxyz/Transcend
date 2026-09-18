//! Terminal Projection & Normalization Engine
//!
//! Transforms raw, noisy terminal byte streams into token-compact, normalized text
//! suitable for LLM context consumption:
//! - Strips ANSI escape sequences and cursor movement controls.
//! - Folds carriage return (`\r`) progress bars into their final visual state.
//! - Enforces byte budgets with safe UTF-8 head/tail slicing.

/// Normalizes terminal output bytes into clean, token-efficient text.
pub struct TerminalProjection;

impl TerminalProjection {
    /// Full normalization pipeline: ANSI strip -> \r line folding -> UTF-8 validation -> head/tail slicing.
    pub fn project(raw_bytes: &[u8], max_bytes: usize) -> (String, bool) {
        if raw_bytes.is_empty() {
            return (String::new(), false);
        }

        // 1. Strip ANSI escape sequences
        let clean_bytes = strip_ansi_escapes::strip(raw_bytes);
        let raw_text = String::from_utf8_lossy(&clean_bytes);

        // 2. Fold carriage returns (\r)
        let folded = Self::fold_carriage_returns(&raw_text);

        // 3. Apply byte budget with head/tail slicing if needed
        Self::slice_head_tail(&folded, max_bytes)
    }

    /// Folds lines with carriage returns (`\r`) to emulate terminal terminal-line overwrites.
    /// Emulates cursor rewinds where text after `\r` overwrites previous text on the line.
    pub fn fold_carriage_returns(text: &str) -> String {
        let mut lines = Vec::new();

        for raw_line in text.split('\n') {
            if !raw_line.contains('\r') {
                lines.push(raw_line.to_string());
                continue;
            }

            // Line contains carriage returns: process segments
            let mut line_buf = Vec::<char>::new();
            let mut cursor_col = 0;

            let chars: Vec<char> = raw_line.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let ch = chars[i];
                if ch == '\r' {
                    cursor_col = 0;
                } else {
                    if cursor_col < line_buf.len() {
                        line_buf[cursor_col] = ch;
                    } else {
                        // Pad with spaces if cursor jumped ahead
                        while line_buf.len() < cursor_col {
                            line_buf.push(' ');
                        }
                        line_buf.push(ch);
                    }
                    cursor_col += 1;
                }
                i += 1;
            }

            let folded_line: String = line_buf.into_iter().collect();
            let trimmed_end = folded_line.trim_end_matches(' ');
            lines.push(trimmed_end.to_string());
        }

        lines.join("\n")
    }

    /// Slices text by byte budget preserving head and tail if truncation is necessary.
    pub fn slice_head_tail(text: &str, max_bytes: usize) -> (String, bool) {
        if text.len() <= max_bytes {
            return (text.to_string(), false);
        }

        let head_budget = (max_bytes / 4).min(4096);
        let tail_budget = max_bytes.saturating_sub(head_budget);

        // Find clean UTF-8 boundary for head
        let head_idx = Self::floor_char_boundary(text, head_budget);
        let head = &text[..head_idx];

        // Find clean UTF-8 boundary for tail
        let tail_start_raw = text.len().saturating_sub(tail_budget);
        let tail_idx = Self::ceil_char_boundary(text, tail_start_raw);
        let tail = &text[tail_idx..];

        if head_idx >= tail_idx {
            return (text.to_string(), false);
        }

        let omitted_bytes = text.len() - head.len() - tail.len();
        let formatted = format!(
            "{head}\n\n... [Transcend: omitted {omitted_bytes} bytes of middle output] ...\n\n{tail}"
        );

        (formatted, true)
    }

    fn floor_char_boundary(s: &str, index: usize) -> usize {
        if index >= s.len() {
            return s.len();
        }
        let mut i = index;
        while !s.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    fn ceil_char_boundary(s: &str, index: usize) -> usize {
        if index >= s.len() {
            return s.len();
        }
        let mut i = index;
        while !s.is_char_boundary(i) {
            i += 1;
        }
        i
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_carriage_return_folding() {
        let input = "Progress: [    ] 0%\rProgress: [==  ] 50%\rProgress: [====] 100%\nDone!";
        let folded = TerminalProjection::fold_carriage_returns(input);
        assert_eq!(folded, "Progress: [====] 100%\nDone!");
    }

    #[test]
    fn test_ansi_and_projection() {
        let raw = b"\x1b[32mBuild started\x1b[0m\r\nStep 1\rStep 2\r\nFinished\x1b[0m";
        let (output, truncated) = TerminalProjection::project(raw, 1000);
        assert!(!truncated);
        assert!(output.contains("Build started"));
        assert!(output.contains("Step 2"));
        assert!(!output.contains("\x1b[32m"));
    }

    #[test]
    fn test_head_tail_truncation() {
        let long_text = "HEADER LINE 1\nHEADER LINE 2\n".to_string()
            + &"middle noise line\n".repeat(100)
            + "FOOTER FINAL ERROR\n";

        let (sliced, truncated) = TerminalProjection::slice_head_tail(&long_text, 250);
        assert!(truncated);
        assert!(sliced.contains("HEADER LINE 1"));
        assert!(sliced.contains("FOOTER FINAL ERROR"));
        assert!(sliced.contains("omitted"));
    }
}
