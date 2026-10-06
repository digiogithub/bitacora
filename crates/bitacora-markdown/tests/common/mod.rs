//! Placeholder helpers shared by the test/bench templates.
//!
//! These stand in for the real parser (BIT-EP-0003), which replaces them.

/// Split text into lines, keeping each line terminator (`\n` or `\r\n`).
pub fn split_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// Inverse of [`split_lines`].
#[allow(dead_code)] // used by the proptest template only
pub fn join_lines(lines: &[&str]) -> String {
    lines.concat()
}
