//! Reader for Markdown `:PROPERTIES:` ... `:END:` drawers.
//!
//! Logseq converts such a drawer into `key:: value` lines when it parses a block (and persists that
//! on the next write). Bitacora does **not** convert on read: the drawer is exposed as a
//! [`PropertyGroup`] with [`GroupOrigin::Drawer`] and the bytes stay untouched; a serializer may
//! convert it when the block is edited.
//!
//! Key mapping (documented in `docs/analysis/logseq/02-markdown-block-syntax.md` §2.4): lower-case,
//! `_` and spaces become `-`, `custom_id` / `custom-id` become `id`, `last-modified-at` becomes
//! `updated-at`.

use crate::lines::{Line, is_ws};
use crate::properties::scan::{
    GroupOrigin, PropLine, PropLineKind, PropertyGroup, make_line, normalize_key,
};
use crate::span::Span;

fn trimmed(content: &[u8]) -> &[u8] {
    let start = content.iter().take_while(|&&b| is_ws(b)).count();
    let end = content.len()
        - content[start..]
            .iter()
            .rev()
            .take_while(|&&b| is_ws(b))
            .count();
    &content[start..end]
}

/// Drawer key normalisation: the `::` rules plus `last-modified-at` -> `updated-at`.
fn normalize_drawer_key(raw: &str) -> String {
    let k = normalize_key(raw);
    if k == "last-modified-at" {
        "updated-at".to_owned()
    } else {
        k
    }
}

/// If `lines[idx]` opens a `:PROPERTIES:` drawer that is closed by an `:END:` line (both
/// case-insensitive, outside any region), returns the group and the index of the line after the
/// drawer. `base` is the offset of the scanned slice inside `input`.
pub(crate) fn read_drawer(
    input: &[u8],
    base: usize,
    lines: &[Line<'_>],
    idx: usize,
) -> Option<(PropertyGroup, usize)> {
    let open = &lines[idx];
    if open.region.is_some() || !trimmed(open.content).eq_ignore_ascii_case(b":PROPERTIES:") {
        return None;
    }
    let end_idx = (idx + 1..lines.len()).find(|&j| {
        lines[j].region.is_none() && trimmed(lines[j].content).eq_ignore_ascii_case(b":END:")
    })?;

    let mut props: Vec<PropLine> = Vec::new();
    for line in &lines[idx + 1..end_idx] {
        let lead = line.indent.len();
        let text = &line.content[lead..];
        let Some(rest) = text.strip_prefix(b":") else {
            continue;
        };
        let Some(colon) = rest.iter().position(|&b| b == b':') else {
            continue;
        };
        let abs = base + line.start + lead + 1;
        let key = Span::new(abs, abs + colon);
        let value_text = &rest[colon + 1..];
        let vs = value_text.iter().take_while(|&&b| is_ws(b)).count();
        let ve = value_text.len()
            - value_text[vs..]
                .iter()
                .rev()
                .take_while(|&&b| is_ws(b))
                .count();
        let value_abs = abs + colon + 1;
        let value = Span::new(value_abs + vs, value_abs + ve.max(vs));
        props.push(make_line(
            input,
            Span::new(base + line.start, base + line.end),
            key,
            value,
            PropLineKind::Drawer,
            normalize_drawer_key,
        ));
    }

    let span = Span::new(base + open.start, base + lines[end_idx].end);
    Some((
        PropertyGroup {
            span,
            lines: props,
            origin: GroupOrigin::Drawer,
            effective: false,
        },
        end_idx + 1,
    ))
}

#[cfg(test)]
mod tests {
    use crate::lines::ParserOptions;
    use crate::outline::split;
    use crate::properties::scan::{GroupOrigin, scan_properties};

    fn scan(input: &str) -> crate::properties::PropertyScan {
        let o = split(input.as_bytes());
        scan_properties(input.as_bytes(), o.blocks[0].span, ParserOptions::default())
    }

    #[test]
    fn custom_id_becomes_id_and_bytes_are_untouched() {
        let input =
            "- a\n  :PROPERTIES:\n  :custom_id: 6500c1a4-0000-4000-8000-000000000001\n  :END:";
        let s = scan(input);
        let g = s.effective().expect("drawer group");
        assert_eq!(g.origin, GroupOrigin::Drawer);
        assert!(g.effective);
        assert_eq!(g.lines.len(), 1);
        assert_eq!(g.lines[0].key_raw, "custom_id");
        assert_eq!(g.lines[0].key_norm, "id");
        assert_eq!(g.lines[0].value_raw, "6500c1a4-0000-4000-8000-000000000001");
        assert!(g.lines[0].valid);
        // The group span covers the drawer exactly; the input itself is never modified.
        assert_eq!(g.span.slice(input.as_bytes()), &input.as_bytes()[4..]);
        assert_eq!(
            g.lines[0].value_span.slice(input.as_bytes()),
            b"6500c1a4-0000-4000-8000-000000000001"
        );
    }

    #[test]
    fn end_is_case_insensitive_and_keys_are_mapped() {
        let s =
            scan("- a\n  :properties:\n  :Last_Modified_At: 1\n  :Some_Key: v\n  :end:\n  k:: v");
        assert_eq!(s.groups.len(), 2);
        let d = &s.groups[0];
        assert_eq!(d.lines[0].key_norm, "updated-at");
        assert_eq!(d.lines[1].key_norm, "some-key");
        assert_eq!(s.groups[1].origin, GroupOrigin::Inline);
        assert!(!s.groups[1].effective);
    }

    #[test]
    fn unclosed_or_fenced_drawers_are_ignored() {
        assert!(scan("- a\n  :PROPERTIES:\n  :k: v").groups.is_empty());
        assert!(
            scan("- a\n  ```\n  :PROPERTIES:\n  :k: v\n  :END:\n  ```")
                .groups
                .is_empty()
        );
    }

    #[test]
    fn drawer_value_may_be_empty_and_crlf_is_excluded() {
        let input = "- a\r\n  :PROPERTIES:\r\n  :k:\r\n  :j: x\r\n  :END:\r\n";
        let s = scan(input);
        let g = s.effective().expect("group");
        assert_eq!(g.lines[0].value_raw, "");
        assert_eq!(g.lines[1].value_raw, "x");
        assert_eq!(g.span.end, input.len());
    }
}
