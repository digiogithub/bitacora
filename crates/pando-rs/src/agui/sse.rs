//! Incremental Server-Sent Events parser.

/// Reassembles SSE frames from arbitrary byte chunks and yields each frame's `data:` payload.
/// Comments (`: keep-alive`), `event:`/`id:`/`retry:` fields and `[DONE]` are ignored.
#[derive(Debug, Default)]
pub struct SseParser {
    line: Vec<u8>,
    data: Vec<String>,
}

impl SseParser {
    /// A fresh parser.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds a chunk; returns the payloads of every frame it completed.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        for &byte in chunk {
            if byte == b'\n' {
                let mut line = std::mem::take(&mut self.line);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                self.handle_line(&line, &mut out);
            } else {
                self.line.push(byte);
            }
        }
        out
    }

    /// Flushes a trailing frame of a stream cut without its final blank line.
    pub fn finish(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.line.is_empty() {
            let mut line = std::mem::take(&mut self.line);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.handle_line(&line, &mut out);
        }
        self.dispatch(&mut out);
        out
    }

    fn handle_line(&mut self, line: &[u8], out: &mut Vec<String>) {
        if line.is_empty() {
            self.dispatch(out);
            return;
        }
        if line[0] == b':' {
            return;
        }
        let text = String::from_utf8_lossy(line);
        let (field, value) = match text.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (text.as_ref(), ""),
        };
        if field == "data" {
            self.data.push(value.to_owned());
        }
    }

    fn dispatch(&mut self, out: &mut Vec<String>) {
        if self.data.is_empty() {
            return;
        }
        let payload = self.data.join("\n");
        self.data.clear();
        if payload != "[DONE]" {
            out.push(payload);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SseParser;

    #[test]
    fn splits_frames_across_chunks_and_ignores_comments() {
        let mut p = SseParser::new();
        assert!(p.push(b": keep-alive\n\ndata: {\"a\"").is_empty());
        let got = p.push(b":1}\n\ndata:{\"b\":2}\r\n\r\n");
        assert_eq!(got, vec!["{\"a\":1}", "{\"b\":2}"]);
    }

    #[test]
    fn joins_multiline_data_and_flushes_tail() {
        let mut p = SseParser::new();
        assert_eq!(
            p.push(b"event: x\ndata: l1\ndata: l2\n\ndata: [DONE]\n\ndata: tail"),
            vec!["l1\nl2"]
        );
        assert_eq!(p.finish(), vec!["tail"]);
    }
}
