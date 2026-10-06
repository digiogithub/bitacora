//! `set_property` / `remove_property`: minimal-diff property edits.
//!
//! Behaviour documented in `docs/analysis/logseq/02-markdown-block-syntax.md` §3.3, minus the
//! hoisting Logseq does on save. Only the first (effective) property group is edited; property-like
//! lines inside fences, quotes and `#+BEGIN_` blocks are never touched (the scanner skips them).

use crate::canonical::convert_drawers;
use crate::edit::{insert_lines, line_index, remove_line, replace_line, title_block_len};
use crate::lines::ParserOptions;
use crate::properties::{
    GroupOrigin, PropLine, PropLineKind, PropertyGroup, normalize_key, scan_properties,
};
use crate::span::Span;

fn scan(content: &str) -> crate::properties::PropertyScan {
    scan_properties(
        content.as_bytes(),
        Span::new(0, content.len()),
        ParserOptions::default(),
    )
}

fn find_line<'a>(group: &'a PropertyGroup, norm: &str) -> Option<&'a PropLine> {
    group
        .lines
        .iter()
        .find(|l| l.kind == PropLineKind::Property && l.valid && l.key_norm == norm)
}

/// The value of property `key` in the effective group (`None` when absent). Drawer properties count.
#[must_use]
pub fn get_property(content: &str, key: &str) -> Option<String> {
    let norm = normalize_key(key);
    let scan = scan(content);
    let g = scan.effective()?;
    g.lines
        .iter()
        .find(|l| l.valid && l.key_norm == norm)
        .map(|l| l.value_raw.clone())
}

/// Sets `key:: value`: replaces the value of an existing line in place, else appends to the end of
/// the first property group, else inserts after the title (and its SCHEDULED/DEADLINE lines), else
/// at the top. The key is lowercased and the value trimmed. A `:PROPERTIES:` drawer in the block is
/// converted to `key:: value` lines first (the block is being edited).
#[must_use]
pub fn set_property(content: &str, key: &str, value: &str) -> String {
    let key = key.trim().to_lowercase();
    let value = value.trim();
    let norm = normalize_key(&key);
    let mut content = content.to_owned();
    if scan(&content)
        .effective()
        .is_some_and(|g| g.origin == GroupOrigin::Drawer)
    {
        content = convert_drawers(&content);
    }
    let new_line = if value.is_empty() {
        format!("{key}::")
    } else {
        format!("{key}:: {value}")
    };

    let sc = scan(&content);
    if let Some(g) = sc.effective() {
        if let Some(line) = find_line(g, &norm) {
            if line.value_raw == value {
                return content;
            }
            let from = line.key_span.end;
            let to = line.value_span.end;
            let tail = if value.is_empty() {
                "::".to_owned()
            } else {
                format!(":: {value}")
            };
            content.replace_range(from..to, &tail);
            return content;
        }
        let last = g.lines.last().map_or(0, |l| l.span.start);
        let idx = line_index(&content, last) + 1;
        return insert_lines(&content, idx, &[new_line]);
    }
    insert_lines(&content, title_block_len(&content), &[new_line])
}

/// Sets a list-valued property as `[[a]], [[b]]`.
#[must_use]
pub fn set_property_values(content: &str, key: &str, pages: &[&str]) -> String {
    let value = pages
        .iter()
        .map(|p| format!("[[{p}]]"))
        .collect::<Vec<_>>()
        .join(", ");
    set_property(content, key, &value)
}

/// Removes the first `key:: ...` line of the effective group. A drawer is converted first.
/// Content without that property is returned unchanged.
#[must_use]
pub fn remove_property(content: &str, key: &str) -> String {
    let norm = normalize_key(key.trim());
    let sc = scan(content);
    let Some(g) = sc.effective() else {
        return content.to_owned();
    };
    if g.origin == GroupOrigin::Drawer {
        let converted = convert_drawers(content);
        return remove_property(&converted, key);
    }
    match find_line(g, &norm) {
        Some(l) => remove_line(content, line_index(content, l.span.start)),
        None => content.to_owned(),
    }
}

/// Front-matter variant for a YAML pre-block (`---` ... `---`): sets `key: value` in place or just
/// before the closing `---`. Content that is not front matter is returned unchanged.
#[must_use]
pub fn set_front_matter_property(content: &str, key: &str, value: &str) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    if lines.first().map(|l| l.trim_end()) != Some("---") {
        return content.to_owned();
    }
    let Some(close) = (1..lines.len()).find(|&i| lines[i].trim_end() == "---") else {
        return content.to_owned();
    };
    let key = key.trim().to_lowercase();
    let line = format!("{key}: {}", value.trim());
    for (i, l) in lines.iter().enumerate().take(close).skip(1) {
        if l.split_once(':')
            .is_some_and(|(k, _)| k.trim().to_lowercase() == key)
        {
            return replace_line(content, i, &line);
        }
    }
    insert_lines(content, close, &[line])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_value_only_on_that_line() {
        assert_eq!(
            set_property("task\nb:: 1\na:: 2", "b", "3"),
            "task\nb:: 3\na:: 2"
        );
        assert_eq!(set_property("task\nB:: 1", "b", "1"), "task\nB:: 1");
        assert_eq!(set_property("task\nb::", "b", "x"), "task\nb:: x");
        assert_eq!(set_property("task\nb:: x", "b", ""), "task\nb::");
    }

    #[test]
    fn appends_to_first_group_not_after_body() {
        assert_eq!(
            set_property("task\nb:: 1\nbody text", "Status", " open "),
            "task\nb:: 1\nstatus:: open\nbody text"
        );
    }

    #[test]
    fn inserts_after_title_and_planning_lines() {
        assert_eq!(set_property("title", "k", "v"), "title\nk:: v");
        assert_eq!(set_property("title\nbody", "k", "v"), "title\nk:: v\nbody");
        assert_eq!(
            set_property("title\nSCHEDULED: <2024-01-01 Mon>\nbody", "k", "v"),
            "title\nSCHEDULED: <2024-01-01 Mon>\nk:: v\nbody"
        );
        assert_eq!(set_property("", "k", "v"), "k:: v");
        assert_eq!(
            set_property("```\ncode\n```", "k", "v"),
            "k:: v\n```\ncode\n```"
        );
        assert_eq!(set_property("a\n", "k", "v"), "a\nk:: v\n");
    }

    #[test]
    fn fenced_property_lines_are_never_modified() {
        let c = "title\n```\nk:: 1\n```";
        assert_eq!(set_property(c, "k", "2"), "title\nk:: 2\n```\nk:: 1\n```");
        assert_eq!(remove_property(c, "k"), c);
    }

    #[test]
    fn remove_restores_original() {
        let c = "title\nbody";
        assert_eq!(remove_property(&set_property(c, "k", "v"), "k"), c);
        assert_eq!(remove_property("a\nk:: v", "k"), "a");
        assert_eq!(remove_property("a\nk:: v\nz:: 1\n", "k"), "a\nz:: 1\n");
        assert_eq!(remove_property("a\nx:: 1", "k"), "a\nx:: 1");
    }

    #[test]
    fn property_on_first_line() {
        assert_eq!(set_property("a:: 1", "b", "2"), "a:: 1\nb:: 2");
        assert_eq!(remove_property("a:: 1\nb:: 2", "a"), "b:: 2");
    }

    #[test]
    fn list_values_and_get() {
        assert_eq!(
            set_property_values("t", "tags", &["a", "b c"]),
            "t\ntags:: [[a]], [[b c]]"
        );
        assert_eq!(get_property("t\nTags:: x", "tags").as_deref(), Some("x"));
    }

    #[test]
    fn drawer_is_converted_on_edit() {
        assert_eq!(
            set_property("t\n:PROPERTIES:\n:a: 1\n:END:", "b", "2"),
            "t\na:: 1\nb:: 2"
        );
    }

    #[test]
    fn front_matter() {
        let fm = "---\ntitle: x\n---";
        assert_eq!(
            set_front_matter_property(fm, "tags", "a"),
            "---\ntitle: x\ntags: a\n---"
        );
        assert_eq!(
            set_front_matter_property(fm, "Title", "y"),
            "---\ntitle: y\n---"
        );
        assert_eq!(set_front_matter_property("plain", "a", "b"), "plain");
    }
}
