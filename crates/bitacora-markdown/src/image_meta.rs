//! Image metadata: the EDN map that follows `![alt](url)`, e.g. `{:height 100, :width 200}`.
//!
//! Logseq writes it with `pr-str`: keywords as keys, `", "` between entries, and for a resize the
//! `:height` entry before `:width` in a new map. Resizing rewrites only the map; unknown keys and
//! the order of existing keys are kept. Nothing else in the text is touched.

use crate::span::Span;

/// An ordered EDN map of keyword keys to raw values (kept exactly as written).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImageMeta {
    /// `(key without the leading colon, raw value)` in order.
    pub entries: Vec<(String, String)>,
}

impl ImageMeta {
    /// Value of `key` (without colon).
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Sets `key`, keeping its position when present, appending otherwise.
    pub fn set(&mut self, key: &str, value: &str) {
        match self.entries.iter_mut().find(|(k, _)| k == key) {
            Some(e) => e.1 = value.to_owned(),
            None => self.entries.push((key.to_owned(), value.to_owned())),
        }
    }

    /// `pr-str` style: `{:height 150, :width 300}`.
    #[must_use]
    pub fn print(&self) -> String {
        let body = self
            .entries
            .iter()
            .map(|(k, v)| format!(":{k} {v}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{{body}}}")
    }
}

/// Parses an EDN map starting at `text[0] == '{'`. Returns the map and the number of bytes used.
#[must_use]
pub fn parse_meta(text: &str) -> Option<(ImageMeta, usize)> {
    let b = text.as_bytes();
    if b.first() != Some(&b'{') {
        return None;
    }
    let mut i = 1;
    let mut meta = ImageMeta::default();
    let skip = |i: &mut usize| {
        while *i < b.len() && (b[*i].is_ascii_whitespace() || b[*i] == b',') {
            *i += 1;
        }
    };
    loop {
        skip(&mut i);
        match b.get(i)? {
            b'}' => return Some((meta, i + 1)),
            b':' => {}
            _ => return None,
        }
        let ks = i + 1;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b',' && b[i] != b'}' {
            i += 1;
        }
        let key = &text[ks..i];
        if key.is_empty() {
            return None;
        }
        skip(&mut i);
        let vs = i;
        i = value_end(b, i)?;
        meta.entries.push((key.to_owned(), text[vs..i].to_owned()));
    }
}

/// End offset of the EDN value starting at `i`.
fn value_end(b: &[u8], mut i: usize) -> Option<usize> {
    match *b.get(i)? {
        b'"' => {
            i += 1;
            while i < b.len() {
                match b[i] {
                    b'\\' => i += 2,
                    b'"' => return Some(i + 1),
                    _ => i += 1,
                }
            }
            None
        }
        open @ (b'{' | b'[' | b'(') => {
            let close = match open {
                b'{' => b'}',
                b'[' => b']',
                _ => b')',
            };
            let mut depth = 0usize;
            while i < b.len() {
                if b[i] == open {
                    depth += 1;
                } else if b[i] == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i + 1);
                    }
                }
                i += 1;
            }
            None
        }
        b'}' | b',' => None,
        _ => {
            while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b',' && b[i] != b'}' {
                i += 1;
            }
            Some(i)
        }
    }
}

/// End offset (just after the closing `)`) of the Markdown image starting at `text[start..]`
/// (`![alt](url)`), balancing brackets and parentheses.
#[must_use]
pub fn image_end(text: &str, start: usize) -> Option<usize> {
    let b = text.as_bytes();
    if b.get(start..start + 2)? != b"![" {
        return None;
    }
    let mut i = start + 2;
    let mut depth = 1usize;
    while i < b.len() && depth > 0 {
        match b[i] {
            b'[' => depth += 1,
            b']' => depth -= 1,
            b'\\' => i += 1,
            _ => {}
        }
        i += 1;
    }
    if b.get(i) != Some(&b'(') {
        return None;
    }
    let mut depth = 0usize;
    while i < b.len() {
        match b[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Span and value of the metadata map directly after the image at `start`, if any.
#[must_use]
pub fn image_meta_at(text: &str, start: usize) -> Option<(Span, ImageMeta)> {
    let end = image_end(text, start)?;
    let (meta, used) = parse_meta(&text[end..])?;
    Some((Span::new(end, end + used), meta))
}

/// Sets the size of the image at `start`: rewrites `:height`/`:width` in the existing map (keeping
/// unknown keys and the existing order) or appends `{:height H, :width W}`. Only the map changes.
#[must_use]
pub fn set_size(text: &str, start: usize, width: u32, height: u32) -> String {
    let Some(end) = image_end(text, start) else {
        return text.to_owned();
    };
    let mut out = text.to_owned();
    match image_meta_at(text, start) {
        Some((span, mut meta)) => {
            meta.set("height", &height.to_string());
            meta.set("width", &width.to_string());
            out.replace_range(span.range(), &meta.print());
        }
        None => {
            let meta = ImageMeta {
                entries: vec![
                    ("height".into(), height.to_string()),
                    ("width".into(), width.to_string()),
                ],
            };
            out.insert_str(end, &meta.print());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_rewrites_only_the_map() {
        let t = "before ![a.png](../assets/a.png){:height 100, :width 200} after";
        let at = t.find('!').expect("image");
        assert_eq!(
            set_size(t, at, 300, 150),
            "before ![a.png](../assets/a.png){:height 150, :width 300} after"
        );
    }

    #[test]
    fn unknown_keys_and_order_are_kept() {
        let t = "![a](u){:width 2, :x \"y z\", :height 1}";
        assert_eq!(
            set_size(t, 0, 20, 10),
            "![a](u){:width 20, :x \"y z\", :height 10}"
        );
        let t = "![a](u){:height 1, :width 2, :x \"y\"}";
        let (_, m) = image_meta_at(t, 0).expect("meta");
        assert_eq!(m.get("x"), Some("\"y\""));
        assert_eq!(set_size(t, 0, 2, 1), t);
    }

    #[test]
    fn adds_a_map_when_missing() {
        assert_eq!(
            set_size("x ![a](u) y", 2, 300, 150),
            "x ![a](u){:height 150, :width 300} y"
        );
        assert_eq!(
            set_size("![a [b]](u(1))", 0, 1, 2),
            "![a [b]](u(1)){:height 2, :width 1}"
        );
    }

    #[test]
    fn malformed_input_is_left_alone() {
        assert_eq!(set_size("![a](u", 0, 1, 2), "![a](u");
        assert_eq!(set_size("no image", 0, 1, 2), "no image");
        assert!(parse_meta("{:a}").is_none());
        assert!(parse_meta("{a 1}").is_none());
    }
}
