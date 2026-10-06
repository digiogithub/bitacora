//! Pasted and dropped files: naming and links the way Logseq 0.10.x writes them (BIT-US-0096,
//! BIT-SP-0002.R15; `docs/analysis/logseq/01-file-graph-layout.md` section 5).
//!
//! A file lands in `<graph>/assets/<stem>_<epoch-ms>_<index><ext>`: `' '`, `'%'` and `'/'` of
//! the stem become `_`, runs of `_` collapse into one, and the extension is the last one when
//! it is a known format, otherwise the complete one (`a.tar.gz` -> `.tar.gz`). The inserted link
//! is relative to the file of the page (`../assets/x.png` for `pages/` and `journals/`), a
//! `!` image link for images, audio, video and PDF, a plain link for anything else.
//! This is our own implementation of the documented behaviour (ADR-015).

use crate::graph_path::GraphPath;

/// Directory that holds the attachments.
pub const ASSETS_DIR: &str = "assets";

/// Base file used to compute the relative link when the page has no file yet (Logseq uses
/// `pages/_.md`).
const DEFAULT_PAGE_FILE: &str = "pages/_.md";

const IMAGE: &[&str] = &[
    "png", "jpg", "jpeg", "bmp", "gif", "webp", "svg", "heic", "ico", "avif",
];
const AUDIO: &[&str] = &["mp3", "ogg", "mpeg", "wav", "m4a", "flac", "wma", "aac"];
const VIDEO: &[&str] = &["mp4", "webm", "mov", "flv", "avi", "mkv"];
const OTHER_KNOWN: &[&str] = &[
    "doc",
    "docx",
    "xls",
    "xlsx",
    "ppt",
    "pptx",
    "one",
    "pdf",
    "epub",
    "org",
    "md",
    "markdown",
    "asciidoc",
    "adoc",
    "rst",
    "json",
    "yml",
    "dat",
    "txt",
    "html",
    "js",
    "ts",
    "edn",
    "clj",
    "ml",
    "rb",
    "ex",
    "erl",
    "java",
    "php",
    "c",
    "css",
    "excalidraw",
    "tldr",
    "sh",
];

/// How a link to an asset is written.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AssetKind {
    /// Image: `![name](url)`.
    Image,
    /// Audio or video: `![name](url)`.
    Media,
    /// PDF: `![name](url)`.
    Pdf,
    /// Anything else: `[name](url)`.
    File,
}

impl AssetKind {
    /// The kind of a file by its extension (case-insensitive, with or without the dot).
    #[must_use]
    pub fn of_extension(ext: &str) -> Self {
        let e = ext.trim_start_matches('.').to_ascii_lowercase();
        let e = e.rsplit('.').next().unwrap_or("");
        if IMAGE.contains(&e) {
            Self::Image
        } else if AUDIO.contains(&e) || VIDEO.contains(&e) {
            Self::Media
        } else if e == "pdf" {
            Self::Pdf
        } else {
            Self::File
        }
    }

    /// Whether the link starts with `!`.
    #[must_use]
    pub fn embeds(self) -> bool {
        self != Self::File
    }
}

fn is_known(ext_without_dot: &str) -> bool {
    let e = ext_without_dot.to_ascii_lowercase();
    IMAGE.contains(&e.as_str())
        || AUDIO.contains(&e.as_str())
        || VIDEO.contains(&e.as_str())
        || OTHER_KNOWN.contains(&e.as_str())
}

/// Splits a file name into stem and extension the way Logseq does: the last extension when it
/// is a known format, otherwise everything from the first dot after the first character.
#[must_use]
pub fn split_name(name: &str) -> (&str, &str) {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let last = base
        .rfind('.')
        .filter(|i| *i > 0 && *i + 1 < base.len())
        .map(|i| (&base[..i], &base[i..]));
    let Some((stem, ext)) = last else {
        return (base, "");
    };
    if is_known(&ext[1..]) {
        return (stem, ext);
    }
    // Complete extension: from the first dot that is not the leading character.
    match base
        .char_indices()
        .skip(1)
        .find(|(_, c)| *c == '.')
        .map(|(i, _)| i)
    {
        Some(i) if i + 1 < base.len() => (&base[..i], &base[i..]),
        _ => (stem, ext),
    }
}

/// The file name for the `index`-th pasted file: `Screen Shot.png` at `1731580000000`, index 0 ->
/// `Screen_Shot_1731580000000_0.png`.
#[must_use]
pub fn asset_file_name(original: &str, epoch_ms: u128, index: usize) -> String {
    let (stem, ext) = split_name(original);
    let base = format!("{}_{epoch_ms}_{index}", stem.replace([' ', '%', '/'], "_"));
    let mut out = String::with_capacity(base.len() + ext.len());
    let mut last_underscore = false;
    for c in base.chars() {
        if c == '_' {
            if !last_underscore {
                out.push(c);
            }
            last_underscore = true;
        } else {
            out.push(c);
            last_underscore = false;
        }
    }
    out.push_str(ext);
    out
}

/// The graph path of a new asset (`assets/<file name>`).
///
/// # Errors
/// [`crate::graph_path::GraphPathError`] for a name that cannot be a path segment (empty or only
/// dots cannot happen: the name always carries the timestamp).
pub fn asset_path(
    original: &str,
    epoch_ms: u128,
    index: usize,
) -> Result<GraphPath, crate::graph_path::GraphPathError> {
    GraphPath::new(&format!(
        "{ASSETS_DIR}/{}",
        asset_file_name(original, epoch_ms, index)
    ))
}

/// The prefix that goes from the directory of `page_file` to the graph root: `../` per
/// directory level (`pages/x.md` -> `../`, `pages/sub/x.md` -> `../../`, a file in the root ->
/// empty). A page without a file is measured as `pages/_.md`.
#[must_use]
pub fn relative_prefix(page_file: Option<&GraphPath>) -> String {
    let default;
    let p = match page_file {
        Some(p) => p.as_str(),
        None => {
            default = DEFAULT_PAGE_FILE;
            default
        }
    };
    "../".repeat(p.matches('/').count())
}

/// The Markdown link for the asset at `asset` shown as `name`, relative to `page_file`.
#[must_use]
pub fn asset_link(page_file: Option<&GraphPath>, asset: &GraphPath, name: &str) -> String {
    let kind = AssetKind::of_extension(asset.extension().unwrap_or(""));
    let url = format!(
        "{}{}",
        relative_prefix(page_file),
        escape_url(asset.as_str())
    );
    let label: String = name.chars().filter(|c| !matches!(c, '[' | ']')).collect();
    format!("{}[{label}]({url})", if kind.embeds() { "!" } else { "" })
}

/// Percent-encodes what would end a Markdown link target early (`(`, `)` and whitespace).
fn escape_url(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            c if c.is_whitespace() => {
                let mut b = [0; 4];
                for byte in c.encode_utf8(&mut b).bytes() {
                    out.push_str(&format!("%{byte:02X}"));
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// Whether a link points into the graph's `assets/` folder: `../assets/x`, `./assets/x`,
/// `/assets/x` or `assets/x` (Logseq's `^[./]*assets`). `@alias/x` links are not local.
#[must_use]
pub fn is_local_asset_link(link: &str) -> bool {
    link.trim_start_matches(['.', '/']).starts_with(ASSETS_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gp(s: &str) -> GraphPath {
        GraphPath::new(s).expect("path")
    }

    #[test]
    fn names_follow_the_documented_vectors() {
        assert_eq!(
            asset_file_name("Screen Shot 2024.png", 1_731_580_000_000, 0),
            "Screen_Shot_2024_1731580000000_0.png"
        );
        assert_eq!(
            asset_file_name("report 50%.docx", 1_731_580_000_000, 1),
            "report_50_1731580000000_1.docx"
        );
        assert_eq!(
            asset_file_name("a  b%%c.pdf", 5, 2),
            "a_b_c_5_2.pdf",
            "separators and runs collapse"
        );
        assert_eq!(asset_file_name("image.png", 7, 0), "image_7_0.png");
        assert_eq!(asset_file_name("noext", 7, 3), "noext_7_3");
        // Directory parts of a dropped path are dropped.
        assert_eq!(asset_file_name("/home/u/x y.png", 1, 0), "x_y_1_0.png");
    }

    #[test]
    fn unknown_extensions_keep_the_complete_one() {
        assert_eq!(asset_file_name("a.tar.gz", 1, 0), "a_1_0.tar.gz");
        assert_eq!(asset_file_name("v1.2.png", 1, 0), "v1.2_1_0.png");
        assert_eq!(asset_file_name("data.zip", 1, 0), "data_1_0.zip");
        assert_eq!(asset_file_name(".hidden", 1, 0), ".hidden_1_0");
    }

    #[test]
    fn links_are_relative_to_the_page_file() {
        let img = gp("assets/Screen_Shot_2024_1731580000000_0.png");
        assert_eq!(
            asset_link(
                Some(&gp("journals/2025_11_14.md")),
                &img,
                "Screen Shot 2024.png"
            ),
            "![Screen Shot 2024.png](../assets/Screen_Shot_2024_1731580000000_0.png)"
        );
        let doc = gp("assets/report_50_1731580000000_1.docx");
        assert_eq!(
            asset_link(Some(&gp("pages/sub/x.md")), &doc, "report 50%.docx"),
            "[report 50%.docx](../../assets/report_50_1731580000000_1.docx)"
        );
        assert_eq!(
            asset_link(None, &img, "a.png"),
            "![a.png](../assets/Screen_Shot_2024_1731580000000_0.png)"
        );
        assert_eq!(
            asset_link(Some(&gp("root.md")), &gp("assets/a.pdf"), "a.pdf"),
            "![a.pdf](assets/a.pdf)"
        );
        assert_eq!(
            asset_link(None, &gp("assets/v.mp4"), "v"),
            "![v](../assets/v.mp4)"
        );
        assert_eq!(
            asset_link(None, &gp("assets/a(1).txt"), "a[1]"),
            "[a1](../assets/a%281%29.txt)"
        );
    }

    #[test]
    fn local_links() {
        for l in ["../assets/x.png", "./assets/x", "/assets/x", "assets/x"] {
            assert!(is_local_asset_link(l), "{l}");
        }
        for l in ["@alias/x.pdf", "https://a/assets/x", "pages/assets/x"] {
            assert!(!is_local_asset_link(l), "{l}");
        }
        // The links we write are understood by the recycle code.
        assert_eq!(
            crate::recycle::asset_path_from_link("../../assets/a%281%29.txt")
                .map(|p| p.to_string()),
            Some("assets/a(1).txt".into())
        );
    }
}
