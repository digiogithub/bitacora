//! Shared tool plumbing: outputs, error codes, pagination cursors, size caps and Markdown rendering
//! of blocks (Logseq outline syntax: tab indentation, `- ` bullets).

use std::fmt::Write as _;

use schemars::JsonSchema;
use serde::Serialize;

use crate::reader::{BlockInfo, ReaderError, ReaderErrorKind};

/// Cap of the Markdown rendering of one tool result, in characters (design section 5.4).
pub(crate) const MAX_TEXT_CHARS: usize = 20_000;

/// Machine-readable error codes (design section 5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Code {
    NotFound,
    InvalidArgument,
    InvalidQuery,
    NotSupported,
    ForbiddenScope,
    ReadOnly,
    Internal,
}

impl Code {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "NOT_FOUND",
            Self::InvalidArgument => "INVALID_ARGUMENT",
            Self::InvalidQuery => "INVALID_QUERY",
            Self::NotSupported => "NOT_SUPPORTED",
            Self::ForbiddenScope => "FORBIDDEN_SCOPE",
            Self::ReadOnly => "READ_ONLY",
            Self::Internal => "INTERNAL",
        }
    }
}

/// A tool failure, reported as an `isError` result with `{code, message}`.
#[derive(Debug, Clone)]
pub(crate) struct ToolError {
    pub code: Code,
    pub message: String,
}

impl ToolError {
    pub(crate) fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub(crate) fn not_found(what: impl std::fmt::Display) -> Self {
        Self::new(Code::NotFound, format!("{what} not found"))
    }
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::new(Code::InvalidArgument, message)
    }
}

impl From<ReaderError> for ToolError {
    fn from(e: ReaderError) -> Self {
        let code = match e.kind {
            ReaderErrorKind::NotSupported => Code::NotSupported,
            ReaderErrorKind::Invalid => Code::InvalidArgument,
            ReaderErrorKind::InvalidQuery => Code::InvalidQuery,
            ReaderErrorKind::Internal => Code::Internal,
        };
        Self::new(code, e.message)
    }
}

/// A successful tool result: structured JSON plus a Markdown rendering.
#[derive(Debug, Clone)]
pub(crate) struct ToolOutput {
    pub value: serde_json::Value,
    pub markdown: String,
}

pub(crate) type ToolResult = Result<ToolOutput, ToolError>;

/// Build a [`ToolOutput`]; the Markdown text is capped at [`MAX_TEXT_CHARS`].
pub(crate) fn output<T: Serialize + JsonSchema>(value: &T, markdown: String) -> ToolResult {
    let value = serde_json::to_value(value)
        .map_err(|e| ToolError::new(Code::Internal, format!("cannot encode result: {e}")))?;
    let (markdown, _) = cap_text(markdown, MAX_TEXT_CHARS);
    Ok(ToolOutput { value, markdown })
}

/// Truncate to `max` characters on a char boundary; the flag says whether text was cut.
pub(crate) fn cap_text(mut s: String, max: usize) -> (String, bool) {
    if s.chars().count() <= max {
        return (s, false);
    }
    let cut = s.char_indices().nth(max).map_or(s.len(), |(i, _)| i);
    s.truncate(cut);
    s.push_str("\n\n[truncated]");
    (s, true)
}

/// Offset cursor: opaque `o:<n>`.
pub(crate) fn encode_cursor(offset: usize) -> String {
    format!("o:{offset}")
}

/// Decode an offset cursor (`None` = first page).
pub(crate) fn decode_cursor(cursor: Option<&str>) -> Result<usize, ToolError> {
    match cursor {
        None | Some("") => Ok(0),
        Some(c) => c
            .strip_prefix("o:")
            .and_then(|n| n.parse().ok())
            .ok_or_else(|| ToolError::invalid("invalid cursor")),
    }
}

/// Clamp an optional limit to `1..=max`.
pub(crate) fn limit(requested: Option<u32>, default: usize, max: usize) -> usize {
    requested.map_or(default, |n| n as usize).clamp(1, max)
}

/// Is this continuation line a `key:: value` property line?
fn is_property_line(line: &str) -> bool {
    let t = line.trim_start();
    match t.split_once("::") {
        Some((key, rest)) => {
            !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
                && (rest.is_empty() || rest.starts_with(' '))
        }
        None => false,
    }
}

/// Remove `key:: value` lines after the first line (properties are reported separately).
pub(crate) fn strip_properties(block: &mut BlockInfo) {
    let mut lines = block.content.lines();
    let first = lines.next().unwrap_or("").to_owned();
    let rest: Vec<&str> = lines.filter(|l| !is_property_line(l)).collect();
    block.content = if rest.is_empty() {
        first
    } else {
        format!("{first}\n{}", rest.join("\n"))
    };
    block.properties.clear();
}

/// Turn a flat pre-order list (with `depth`) into a tree. Blocks deeper than their predecessor's
/// children chain attach to the nearest shallower open block; a first block deeper than later ones
/// simply becomes a root.
pub(crate) fn nest(flat: Vec<BlockInfo>) -> Vec<BlockInfo> {
    fn close(stack: &mut Vec<BlockInfo>, roots: &mut Vec<BlockInfo>) {
        if let Some(done) = stack.pop() {
            match stack.last_mut() {
                Some(parent) => parent.children.push(done),
                None => roots.push(done),
            }
        }
    }
    let mut roots = Vec::new();
    let mut stack: Vec<BlockInfo> = Vec::new();
    for b in flat {
        while stack.last().is_some_and(|top| top.depth >= b.depth) {
            close(&mut stack, &mut roots);
        }
        stack.push(b);
    }
    while !stack.is_empty() {
        close(&mut stack, &mut roots);
    }
    roots
}

/// Drop descendants of collapsed blocks from a flat subtree (the first block is the root and is
/// never treated as collapsed).
pub(crate) fn drop_collapsed(flat: Vec<BlockInfo>) -> Vec<BlockInfo> {
    let mut skip_below: Option<u32> = None;
    let mut out = Vec::with_capacity(flat.len());
    for (i, b) in flat.into_iter().enumerate() {
        if let Some(d) = skip_below {
            if b.depth > d {
                continue;
            }
            skip_below = None;
        }
        if i > 0 && b.collapsed {
            skip_below = Some(b.depth);
        }
        out.push(b);
    }
    out
}

/// Render nested blocks as a Logseq outline.
pub(crate) fn render_tree(blocks: &[BlockInfo]) -> String {
    let mut out = String::new();
    render_level(blocks, 0, &mut out);
    out
}

fn render_level(blocks: &[BlockInfo], level: usize, out: &mut String) {
    for b in blocks {
        let indent = "\t".repeat(level);
        let mut lines = b.content.lines();
        let _ = writeln!(out, "{indent}- {}", lines.next().unwrap_or(""));
        for l in lines {
            let _ = writeln!(out, "{indent}  {l}");
        }
        render_level(&b.children, level + 1, out);
    }
}

/// One line per block for list-style results.
pub(crate) fn render_block_line(b: &BlockInfo) -> String {
    let first = b.content.lines().next().unwrap_or("");
    format!("- {first}  _(page: [[{}]], id: {})_\n", b.page, b.uuid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blk(uuid: &str, depth: u32, content: &str) -> BlockInfo {
        BlockInfo {
            uuid: uuid.into(),
            content: content.into(),
            page: "P".into(),
            properties: Default::default(),
            marker: None,
            priority: None,
            scheduled: None,
            deadline: None,
            collapsed: false,
            depth,
            version: String::new(),
            children: Vec::new(),
            is_pre_block: false,
        }
    }

    #[test]
    fn nest_and_render_round_trip() {
        let flat = vec![
            blk("a", 1, "A\nid:: 1"),
            blk("b", 2, "B"),
            blk("c", 3, "C"),
            blk("d", 2, "D"),
            blk("e", 1, "E"),
        ];
        let tree = nest(flat);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].children.len(), 2);
        assert_eq!(tree[0].children[0].children[0].uuid, "c");
        assert_eq!(
            render_tree(&tree),
            "- A\n  id:: 1\n\t- B\n\t\t- C\n\t- D\n- E\n"
        );
    }

    #[test]
    fn nest_starting_deep_makes_roots() {
        let tree = nest(vec![blk("a", 3, "A"), blk("b", 2, "B"), blk("c", 3, "C")]);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[1].children.len(), 1);
    }

    #[test]
    fn collapsed_descendants_dropped() {
        let mut root = blk("r", 1, "R");
        root.collapsed = true;
        let mut mid = blk("m", 2, "M");
        mid.collapsed = true;
        let flat = vec![root, mid, blk("x", 3, "X"), blk("y", 2, "Y")];
        let kept: Vec<_> = drop_collapsed(flat).into_iter().map(|b| b.uuid).collect();
        assert_eq!(kept, ["r", "m", "y"]);
    }

    #[test]
    fn strips_property_lines_but_not_text() {
        let mut b = blk("a", 1, "TODO x\nid:: 1\nnote: not a property\nmore");
        b.properties.insert("id".into(), "1".into());
        strip_properties(&mut b);
        assert_eq!(b.content, "TODO x\nnote: not a property\nmore");
        assert!(b.properties.is_empty());
    }

    #[test]
    fn cap_and_cursor() {
        let (s, cut) = cap_text("é".repeat(30), 10);
        assert!(cut && s.starts_with(&"é".repeat(10)));
        assert_eq!(decode_cursor(Some("o:40")).ok(), Some(40));
        assert!(decode_cursor(Some("zz")).is_err());
        assert_eq!(limit(Some(0), 20, 100), 1);
        assert_eq!(limit(Some(9999), 20, 100), 100);
        assert_eq!(limit(None, 20, 100), 20);
    }
}
