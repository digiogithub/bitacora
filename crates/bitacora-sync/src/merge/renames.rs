//! Page renames: titles of renamed files, the post-merge link fix-up and the `alias::` suggestion
//! of a rename/rename conflict (design `git-sync-merge` 5.5, BIT-SP-0006.R19).

use bitacora_config::EffectiveConfig;
use bitacora_core::naming::{derive_title, page_key};

/// `title::` of the first property block of a page, if any.
fn title_property(text: &str) -> Option<String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    for line in text.lines().take(30) {
        let t = line.trim_start_matches(['-', ' ', '\t']).trim();
        if let Some(rest) = t.strip_prefix("title::") {
            return Some(rest.trim().to_owned());
        }
        if !t.contains("::") {
            break;
        }
    }
    None
}

/// Title of the page stored at `path` with `content` (`title::` wins over the file name).
pub fn page_title(path: &str, content: &str, cfg: &EffectiveConfig) -> String {
    derive_title(path, title_property(content).as_deref(), cfg)
}

/// Whether `path` is a page file whose rename changes link targets.
pub fn is_page_path(path: &str) -> bool {
    path.starts_with("pages/") && path.ends_with(".md")
}

fn is_tag_char(c: char) -> bool {
    !(c.is_whitespace()
        || matches!(
            c,
            ',' | '.' | ';' | '!' | '?' | ')' | '(' | '[' | ']' | '"' | '\'' | '#'
        ))
}

/// Rewrites one inline (non-code) segment.
fn rewrite_segment(seg: &str, old_key: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(seg.len());
    let mut rest = seg;
    let mut prev: Option<char> = None;
    while let Some(c) = rest.chars().next() {
        if rest.starts_with("[[")
            && let Some(end) = rest[2..].find("]]")
        {
            let inner = &rest[2..2 + end];
            if page_key(inner) == old_key {
                out.push_str("[[");
                out.push_str(new);
                out.push_str("]]");
            } else {
                out.push_str(&rest[..4 + end]);
            }
            prev = Some(']');
            rest = &rest[4 + end..];
            continue;
        }
        if c == '#' && prev.is_none_or(char::is_whitespace) && !old.contains(char::is_whitespace) {
            let name_len: usize = rest[1..]
                .chars()
                .take_while(|&c| is_tag_char(c))
                .map(char::len_utf8)
                .sum();
            let name = &rest[1..1 + name_len];
            if name_len > 0 && page_key(name) == old_key {
                out.push('#');
                if new.contains(char::is_whitespace) {
                    out.push_str(&format!("[[{new}]]"));
                } else {
                    out.push_str(new);
                }
                prev = Some('x');
                rest = &rest[1 + name_len..];
                continue;
            }
        }
        out.push(c);
        prev = Some(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

fn rewrite_property_line(line: &str, old_key: &str, new: &str) -> Option<String> {
    let t = line.trim_start_matches(['-', ' ', '\t']);
    let (key, value) = t.split_once("::")?;
    if !matches!(key.trim().to_lowercase().as_str(), "tags" | "alias") {
        return None;
    }
    let indent_len = line.len() - t.len();
    let items: Vec<String> = value
        .split(',')
        .map(|item| {
            let it = item.trim();
            if !it.starts_with("[[") && page_key(it) == old_key {
                new.to_owned()
            } else {
                it.to_owned()
            }
        })
        .collect();
    let rebuilt = format!("{}{}:: {}", &line[..indent_len], key, items.join(", "));
    (rebuilt.trim_end() != line.trim_end()).then_some(rebuilt)
}

/// Rewrites links to the page `old` into links to `new`: `[[Old]]`, `#[[Old]]`, `#Old` and
/// `tags::`/`alias::` values. Fenced code blocks and inline code are left alone. Returns `None`
/// when nothing changes. Lines are rewritten one by one, so untouched lines keep their bytes.
pub fn rewrite_links(text: &str, old: &str, new: &str) -> Option<String> {
    let old_key = page_key(old);
    if old_key.is_empty() || old_key == page_key(new) && old == new {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let mut in_fence = false;
    let mut changed = false;
    for line in text.split_inclusive('\n') {
        let (body, eol) = match line.strip_suffix('\n') {
            Some(b) => match b.strip_suffix('\r') {
                Some(b2) => (b2, "\r\n"),
                None => (b, "\n"),
            },
            None => (line, ""),
        };
        if body.trim_start_matches(['-', ' ', '\t']).starts_with("```") {
            in_fence = !in_fence;
            out.push_str(line);
            continue;
        }
        if in_fence {
            out.push_str(line);
            continue;
        }
        let rewritten = match rewrite_property_line(body, &old_key, new) {
            Some(l) => l,
            None => {
                // Even-indexed pieces between backticks are outside inline code.
                let pieces: Vec<&str> = body.split('`').collect();
                if pieces.len().is_multiple_of(2) {
                    out.push_str(line);
                    continue;
                }
                pieces
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        if i % 2 == 0 {
                            rewrite_segment(p, &old_key, old, new)
                        } else {
                            (*p).to_owned()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("`")
            }
        };
        if rewritten != body {
            changed = true;
        }
        out.push_str(&rewritten);
        out.push_str(eol);
    }
    changed.then_some(out)
}

/// `((uuid))` block references in `text`, lower-cased, without duplicates.
pub fn block_refs(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("((") {
        let after = &rest[i + 2..];
        if after.len() >= 38
            && after.is_char_boundary(36)
            && after[36..].starts_with("))")
            && is_uuid(&after[..36])
        {
            let id = after[..36].to_lowercase();
            if !found.contains(&id) {
                found.push(id);
            }
        }
        rest = &rest[i + 2..];
    }
    found
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_from_paths_and_properties() {
        let cfg = EffectiveConfig::from_texts(None, Some("{:file/name-format :triple-lowbar}"));
        assert_eq!(page_title("pages/Old Name.md", "- x\n", &cfg), "Old Name");
        assert_eq!(page_title("pages/a___b.md", "- x\n", &cfg), "a/b");
        assert_eq!(
            page_title("pages/new.md", "title:: New Shiny\n\n- x\n", &cfg),
            "New Shiny"
        );
    }

    #[test]
    fn rewrites_links_tags_and_properties() {
        let t = "- see [[Old]] and [[old]] and [[Older]]\n- #Old and #[[Old]] and #Oldest\n  tags:: Old, [[Other]]\n  alias:: x, old\n";
        let out = rewrite_links(t, "Old", "New").unwrap_or_default();
        assert_eq!(
            out,
            "- see [[New]] and [[New]] and [[Older]]\n- #New and #[[New]] and #Oldest\n  tags:: New, [[Other]]\n  alias:: x, New\n"
        );
        assert!(rewrite_links("- nothing here\n", "Old", "New").is_none());
    }

    #[test]
    fn code_is_left_alone_and_untouched_lines_keep_bytes() {
        let t = "- keep  [[Other]]   \r\n- `[[Old]]` but [[Old]]\r\n```\n[[Old]]\n```\n";
        let out = rewrite_links(t, "Old", "New").unwrap_or_default();
        assert_eq!(
            out,
            "- keep  [[Other]]   \r\n- `[[Old]]` but [[New]]\r\n```\n[[Old]]\n```\n"
        );
    }

    #[test]
    fn tags_with_spaces_in_new_title_use_brackets() {
        assert_eq!(
            rewrite_links("- #Old\n", "Old", "New Page").as_deref(),
            Some("- #[[New Page]]\n")
        );
    }

    #[test]
    fn finds_block_refs() {
        let a = "66500000-0000-4000-8000-0000000000aa";
        let t = format!(
            "- see (({a})) and {{{{embed (({}))}}}} and ((short))\n",
            a.to_uppercase()
        );
        assert_eq!(block_refs(&t), vec![a.to_owned()]);
    }
}
