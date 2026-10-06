//! Logseq-compatible recycle naming (BIT-US-0089, `docs/analysis/logseq/01-file-graph-layout.md`
//! section 5): a deleted page or asset is moved, never erased, to `logseq/.recycle/` under its
//! graph-relative path with every `/` and `\` replaced by `_`. An existing entry is overwritten.
//! References to the deleted file are not rewritten.

use crate::graph_path::GraphPath;

/// Directory that holds deleted files.
pub const RECYCLE_DIR: &str = "logseq/.recycle";

/// Where a deleted file goes: `pages/sub/bar.md` -> `logseq/.recycle/pages_sub_bar.md`.
///
/// A recycle path built from a valid [`GraphPath`] is always valid; the fallback only guards
/// against pathological input (an over-long name) and returns the input unchanged.
#[must_use]
pub fn recycle_path(path: &GraphPath) -> GraphPath {
    let flat: String = path
        .as_str()
        .chars()
        .map(|c| if c == '/' || c == '\\' { '_' } else { c })
        .collect();
    GraphPath::new(&format!("{RECYCLE_DIR}/{flat}")).unwrap_or_else(|_| path.clone())
}

/// The graph-relative asset path a block link points to: `../assets/x.png`, `./assets/x.png`
/// and `assets/x.png` (with `%xx` escapes decoded) map to `assets/x.png`. Links to other places
/// (URLs, other directories, `..` escapes) give `None`.
#[must_use]
pub fn asset_path_from_link(link: &str) -> Option<GraphPath> {
    let link = link.trim();
    if link.contains("://") || link.contains('\\') {
        return None;
    }
    let mut rest = link;
    while let Some(r) = rest.strip_prefix("../").or_else(|| rest.strip_prefix("./")) {
        rest = r;
    }
    if !rest.starts_with("assets/") {
        return None;
    }
    let decoded = percent_decode(rest)?;
    GraphPath::new(&decoded).ok()
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Asset links of a block text: targets of `![alt](target)` and `[label](target)` that point
/// into `assets/`, in order of appearance.
#[must_use]
pub fn asset_links(text: &str) -> Vec<GraphPath> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("](") {
        let after = &rest[i + 2..];
        let Some(end) = after.find(')') else { break };
        if let Some(p) = asset_path_from_link(&after[..end])
            && !out.contains(&p)
        {
            out.push(p);
        }
        rest = &after[end + 1..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gp(s: &str) -> GraphPath {
        GraphPath::new(s).expect("path")
    }

    #[test]
    fn names_flatten_separators() {
        assert_eq!(
            recycle_path(&gp("pages/foo.md")).as_str(),
            "logseq/.recycle/pages_foo.md"
        );
        assert_eq!(
            recycle_path(&gp("pages/sub/bar.md")).as_str(),
            "logseq/.recycle/pages_sub_bar.md"
        );
        assert_eq!(
            recycle_path(&gp("assets/x.png")).as_str(),
            "logseq/.recycle/assets_x.png"
        );
    }

    #[test]
    fn links_to_assets() {
        assert_eq!(
            asset_path_from_link("../assets/x.png").map(|p| p.to_string()),
            Some("assets/x.png".into())
        );
        assert_eq!(
            asset_path_from_link("../assets/my%20pic.png").map(|p| p.to_string()),
            Some("assets/my pic.png".into())
        );
        assert_eq!(asset_path_from_link("https://e.com/assets/x.png"), None);
        assert_eq!(asset_path_from_link("../other/x.png"), None);
        assert_eq!(asset_path_from_link("../assets/../../x"), None);
        let t = "![a](../assets/a.png){:height 10} and [pdf](../assets/b.pdf) [w](https://x.y)";
        let got: Vec<String> = asset_links(t).iter().map(ToString::to_string).collect();
        assert_eq!(got, ["assets/a.png", "assets/b.pdf"]);
    }
}
