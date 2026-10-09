//! Print view (BIT-US-0181): the active page or zoomed block as one self-contained HTML
//! document that the system browser prints (or saves as PDF).
//!
//! GPUI has no print API, so the app renders the already parsed [`Row`]s (the same model the
//! page view draws) to HTML, writes the file into the app *cache* directory (never into the
//! graph: user files are sacred) and opens it in the default browser; an inline script calls
//! `window.print()` on load. Native print dialogs are out of scope.
//!
//! Everything here is GPUI-free and pure except [`write_print_file`].

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::render::inline::{ImageRef, NavTarget, Role, TextLayout};
use crate::render::model::{BlockModel, BodyItem, Row};
use crate::ui::Hsla;
use crate::ui::theme::BitacoraTheme;
use crate::views::block_view::resolve_asset;

/// Cache sub-directory that holds the generated print files.
pub const PRINT_DIR: &str = "print";

/// What to print.
#[derive(Debug, Clone, Copy)]
pub struct PrintDoc<'a> {
    /// Page title (or the zoomed block's page), shown as the heading.
    pub title: &'a str,
    /// The outline, in document order (the page-properties pre-block first, when it has one).
    pub rows: &'a [Row],
    /// The graph folder, to resolve `assets/` images.
    pub graph_root: &'a Path,
}

/// Escapes text for HTML element content and quoted attribute values.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// `#rrggbb` of a theme colour (alpha ignored: print is opaque).
fn css(color: Hsla) -> String {
    let rgb = color.to_rgb();
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(rgb.r), byte(rgb.g), byte(rgb.b))
}

/// The print stylesheet, derived from the light palette ([`BitacoraTheme::print`]); the page is
/// always black on white whatever theme the window shows.
pub fn stylesheet() -> String {
    let theme = BitacoraTheme::print();
    let c = &theme.colors;
    format!(
        "@page{{margin:18mm 16mm}}\
         *{{box-sizing:border-box}}\
         html,body{{background:#fff;color:{text};margin:0}}\
         body{{font:11pt/1.5 -apple-system,'Segoe UI',Roboto,'Helvetica Neue',Arial,sans-serif}}\
         h1{{font-size:22pt;line-height:1.2;margin:0 0 .6em;color:#000}}\
         ul{{list-style:disc;margin:0;padding-left:1.5em}}\
         li::marker{{color:{bullet}}}\
         li{{margin:.15em 0}}\
         .blk{{overflow-wrap:anywhere}}\
         .h1{{font-size:1.6em;font-weight:700}}.h2{{font-size:1.4em;font-weight:700}}\
         .h3{{font-size:1.25em;font-weight:700}}.h4{{font-size:1.1em;font-weight:700}}\
         .h5,.h6{{font-weight:700}}\
         .cb{{font-size:1.05em}}\
         .mk{{font-size:.8em;font-weight:700;letter-spacing:.04em;color:{text2};margin-right:.4em}}\
         .pri{{font-weight:700;color:{text2};margin-right:.4em}}\
         .strike{{text-decoration:line-through;color:{muted}}}\
         .done{{color:{muted}}}\
         .ref,.tag,.link{{color:{accent}}}\
         .link{{text-decoration:underline}}\
         .dim,.muted{{color:{muted}}}\
         code{{font-family:ui-monospace,'SFMono-Regular',Menlo,Consolas,monospace;font-size:.9em;\
         background:{edit};border-radius:3px;padding:0 .25em}}\
         pre{{font-family:ui-monospace,'SFMono-Regular',Menlo,Consolas,monospace;font-size:.88em;\
         background:{edit};border:1px solid {line};border-radius:4px;padding:.6em .8em;\
         margin:.4em 0;white-space:pre-wrap;overflow-wrap:anywhere;break-inside:avoid}}\
         pre code{{background:none;padding:0}}\
         blockquote{{margin:.4em 0;padding-left:.8em;border-left:3px solid {line2};color:{text2}}}\
         .callout{{margin:.4em 0;padding:.4em .8em;border:1px solid {line2};border-left-width:4px;\
         border-radius:4px}}\
         .center{{text-align:center}}.verse{{white-space:pre-wrap}}\
         .props,.plan{{font-size:.85em;color:{muted};margin:.1em 0}}\
         .pk{{font-weight:600}}\
         img{{max-width:100%;height:auto;display:block;margin:.4em 0;break-inside:avoid}}\
         .page-props{{margin:0 0 1em;padding-bottom:.5em;border-bottom:1px solid {line}}}",
        text = css(c.text),
        text2 = css(c.text_2),
        muted = css(c.muted),
        bullet = css(c.bullet),
        accent = css(c.accent),
        edit = css(c.edit_bg),
        line = css(c.line),
        line2 = css(c.line_2),
    )
}

/// True for the URL schemes the print view turns into anchors.
fn is_safe_url(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")
}

/// `file://` URL of a local path (percent-encoded, forward slashes).
pub fn file_url(path: &Path) -> String {
    let mut raw = path.to_string_lossy().replace('\\', "/");
    if !raw.starts_with('/') {
        raw.insert(0, '/');
    }
    let mut out = String::from("file://");
    for byte in raw.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.~:".contains(&byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Inline HTML of a rendered text: emphasis, code, refs, tags and links; line breaks kept.
pub fn layout_html(layout: &TextLayout) -> String {
    let text = &layout.text;
    let mut cuts: BTreeSet<usize> = BTreeSet::from([0, text.len()]);
    let ranges = layout
        .styled
        .iter()
        .map(|s| &s.range)
        .chain(layout.links.iter().map(|(r, _)| r));
    for r in ranges {
        for at in [r.start, r.end] {
            if at <= text.len() && text.is_char_boundary(at) {
                cuts.insert(at);
            }
        }
    }
    let cuts: Vec<usize> = cuts.into_iter().collect();
    let mut out = String::new();
    for pair in cuts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let segment = &text[from..to];
        if segment.is_empty() {
            continue;
        }
        let style = layout
            .styled
            .iter()
            .find(|s| s.range.start <= from && from < s.range.end);
        let link = layout
            .links
            .iter()
            .find(|(r, _)| r.start <= from && from < r.end);
        let mut html = escape(segment).replace('\n', "<br>");
        let mut open = String::new();
        let mut close = String::new();
        if let Some(style) = style {
            let e = style.emphasis;
            for (on, tag) in [
                (e.bold, "strong"),
                (e.italic, "em"),
                (e.strike, "s"),
                (e.highlight, "mark"),
            ] {
                if on {
                    open.push_str(&format!("<{tag}>"));
                    close.insert_str(0, &format!("</{tag}>"));
                }
            }
            let class = match style.role {
                Role::Plain => None,
                Role::PageRef | Role::BlockRef => Some("ref"),
                Role::Tag => Some("tag"),
                Role::Link => Some("link"),
                Role::Code => Some("code"),
                Role::Math | Role::BlockRefDangling | Role::Placeholder | Role::Dim => Some("dim"),
            };
            match class {
                Some("code") => {
                    open.push_str("<code>");
                    close.insert_str(0, "</code>");
                }
                Some(class) => {
                    open.push_str(&format!("<span class=\"{class}\">"));
                    close.insert_str(0, "</span>");
                }
                None => {}
            }
        }
        if let Some((_, NavTarget::Url(url))) = link
            && is_safe_url(url)
        {
            open.insert_str(0, &format!("<a href=\"{}\">", escape(url)));
            close.push_str("</a>");
        }
        html.insert_str(0, &open);
        html.push_str(&close);
        out.push_str(&html);
    }
    out
}

fn image_html(image: &ImageRef, graph_root: &Path) -> Option<String> {
    let src = match resolve_asset(graph_root, &image.src) {
        Some(path) => file_url(&path),
        None if is_safe_url(&image.src) && !image.src.starts_with("mailto:") => image.src.clone(),
        None => return None,
    };
    let mut tag = format!(
        "<img src=\"{}\" alt=\"{}\"",
        escape(&src),
        escape(&image.alt)
    );
    if let Some(width) = image.width.filter(|w| w.is_finite() && *w > 0.0) {
        let _ = write!(tag, " width=\"{}\"", width.round() as u32);
    }
    tag.push('>');
    Some(tag)
}

fn images_html(layout: &TextLayout, graph_root: &Path, out: &mut String) {
    for image in &layout.images {
        if let Some(tag) = image_html(image, graph_root) {
            out.push_str(&tag);
        }
    }
}

/// Properties and planning chips under a block (hidden properties are already filtered out).
fn meta_html(block: &BlockModel, out: &mut String) {
    for chip in &block.planning {
        let _ = write!(
            out,
            "<div class=\"plan\">{}: {}</div>",
            escape(chip.keyword),
            escape(&chip.text)
        );
    }
    for prop in &block.properties {
        let _ = write!(
            out,
            "<div class=\"props\"><span class=\"pk\">{}</span>: {}</div>",
            escape(&prop.key),
            layout_html(&prop.value)
        );
    }
}

fn body_html(block: &BlockModel, graph_root: &Path, out: &mut String) {
    for item in &block.body {
        match item {
            BodyItem::Text(layout) => {
                let _ = write!(out, "<div class=\"blk\">{}</div>", layout_html(layout));
                images_html(layout, graph_root, out);
            }
            BodyItem::Code(code) => {
                match code.language.as_deref().filter(|l| !l.is_empty()) {
                    Some(lang) => {
                        let _ = write!(
                            out,
                            "<pre data-lang=\"{lang}\"><code>{}</code></pre>",
                            escape(&code.text),
                            lang = escape(lang)
                        );
                    }
                    None => {
                        let _ = write!(out, "<pre><code>{}</code></pre>", escape(&code.text));
                    }
                };
            }
            BodyItem::Quote(layout) => {
                let _ = write!(out, "<blockquote>{}</blockquote>", layout_html(layout));
            }
            BodyItem::Callout { kind, layout } => {
                let name = format!("{kind:?}").to_ascii_lowercase();
                let _ = write!(
                    out,
                    "<div class=\"callout {name}\">{}</div>",
                    layout_html(layout)
                );
            }
            BodyItem::Center(layout) => {
                let _ = write!(out, "<div class=\"center\">{}</div>", layout_html(layout));
            }
            BodyItem::Verse(layout) => {
                let _ = write!(out, "<div class=\"verse\">{}</div>", layout_html(layout));
            }
            // Live queries and embeds have no static text to print.
            BodyItem::Widget(_) => {}
        }
    }
}

/// One block: task marker, priority, title, body, planning and visible properties.
fn block_html(block: &BlockModel, graph_root: &Path, out: &mut String) {
    let mut class = String::from("blk");
    if let Some(level) = block.heading {
        let _ = write!(class, " h{level}");
    }
    if let Some(marker) = &block.marker {
        if marker.finished {
            class.push_str(" done");
        }
        if marker.cancelled {
            class.push_str(" strike");
        }
    }
    let _ = write!(out, "<div class=\"{class}\">");
    if let Some(marker) = &block.marker {
        match marker.checkbox {
            Some(true) => out.push_str("<span class=\"cb\">&#9745;</span> "),
            Some(false) => out.push_str("<span class=\"cb\">&#9744;</span> "),
            None => {}
        }
        if marker.show_label {
            let _ = write!(out, "<span class=\"mk\">{}</span>", escape(&marker.label));
        }
    }
    if let Some(priority) = block.priority {
        let _ = write!(
            out,
            "<span class=\"pri\">[#{}]</span>",
            escape(&priority.to_string())
        );
    }
    out.push_str(&layout_html(&block.title));
    out.push_str("</div>");
    images_html(&block.title, graph_root, out);
    body_html(block, graph_root, out);
    meta_html(block, out);
}

/// The rows to print: every block expanded (collapse state is ignored), depths shifted so the
/// shallowest outline row is level 0 (a zoomed block).
fn outline_rows(rows: &[Row]) -> (Option<&Row>, Vec<(usize, &Row)>) {
    let pre = rows.first().filter(|r| r.block_index.is_none());
    let rest: Vec<&Row> = rows.iter().skip(usize::from(pre.is_some())).collect();
    let base = rest.iter().map(|r| r.depth).min().unwrap_or(0);
    (pre, rest.into_iter().map(|r| (r.depth - base, r)).collect())
}

/// Renders the whole document.
pub fn render_html(doc: &PrintDoc<'_>) -> String {
    let mut body = String::new();
    let (pre, outline) = outline_rows(doc.rows);
    if let Some(pre) = pre {
        body.push_str("<div class=\"page-props\">");
        block_html(&pre.block, doc.graph_root, &mut body);
        body.push_str("</div>");
    }
    // `open` counts the `<ul>` levels currently open; each has one open `<li>` (the last block
    // written at that level, which stays open until a sibling or its parent's close arrives).
    let mut open = 0usize;
    for (depth, row) in outline {
        let level = depth + 1;
        if level > open {
            while open < level {
                body.push_str("<ul>");
                open += 1;
            }
        } else {
            while open > level {
                body.push_str("</li></ul>");
                open -= 1;
            }
            body.push_str("</li>");
        }
        body.push_str("<li>");
        block_html(&row.block, doc.graph_root, &mut body);
    }
    while open > 0 {
        body.push_str("</li></ul>");
        open -= 1;
    }
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{title}</title><style>{css}</style></head>\
         <body><h1>{title}</h1>{body}\
         <script>window.addEventListener('load',()=>window.print())</script>\
         </body></html>\n",
        title = escape(doc.title),
        css = stylesheet(),
    )
}

/// File stem for a page title: lower-case ASCII letters and digits joined by `-`.
fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 60 {
            break;
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    if out.is_empty() {
        "page".to_owned()
    } else {
        out
    }
}

/// Where the print file of `title` goes: `<cache_dir>/print/<slug>-<hash>.html`. The hash keeps
/// pages whose titles share a slug apart.
pub fn print_path(cache_dir: &Path, title: &str) -> PathBuf {
    let hash = crate::paths::graph_hash(Path::new(title));
    cache_dir
        .join(PRINT_DIR)
        .join(format!("{}-{}.html", slug(title), &hash[..8]))
}

/// Writes `html` atomically (temp file in the same directory, fsync, rename) to
/// [`print_path`]; creates the directory. Returns the final path.
pub fn write_print_file(cache_dir: &Path, title: &str, html: &str) -> std::io::Result<PathBuf> {
    let target = print_path(cache_dir, title);
    let dir = target.parent().unwrap_or(cache_dir);
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        std::process::id()
    ));
    let written = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(html.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, &target)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written.map(|()| target)
}

#[cfg(test)]
mod tests {
    use bitacora_markdown::properties::PropertyConfig;

    use super::*;
    use crate::render::inline::NoBlocks;
    use crate::render::model::PageModel;

    const PAGE: &str = r#"title:: Sample

- Intro with **bold**, *italic*, `code` and [[Other Page]] #tag
  id:: 6500c1a4-0000-4000-8000-000000000001
  status:: open
  - TODO write the report
    - DONE nested child
  - DOING [#A] in progress
- Plain [site](https://example.com/a?b=1&c=2)
  ```rust
  fn main() { if 1 < 2 {} }
  ```
- <script>alert(1)</script> & "quotes"
- ![pic](../assets/my pic.png)
- Last block
"#;

    fn model() -> PageModel {
        PageModel::from_source(PAGE.as_bytes(), &PropertyConfig::default(), &NoBlocks)
    }

    fn render(root: &Path) -> String {
        let model = model();
        render_html(&PrintDoc {
            title: "Sample <Page>",
            rows: &model.rows,
            graph_root: root,
        })
    }

    #[test]
    fn document_is_self_contained_and_prints_on_load() {
        let html = render(Path::new("/g"));
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<title>Sample &lt;Page&gt;</title>"));
        assert!(html.contains("<h1>Sample &lt;Page&gt;</h1>"));
        assert!(html.contains("@page{margin"));
        assert!(html.contains("window.addEventListener('load',()=>window.print())"));
        // Black on white whatever the window theme is.
        assert!(html.contains("background:#fff"));
        assert!(!html.contains("http-equiv"));
    }

    #[test]
    fn nesting_is_balanced_and_blocks_are_expanded() {
        let html = render(Path::new("/g"));
        assert_eq!(html.matches("<ul>").count(), html.matches("</ul>").count());
        assert_eq!(html.matches("<li>").count(), html.matches("</li>").count());
        // Three levels: Intro > TODO > DONE.
        let intro = html.find("Intro with").expect("intro");
        let todo = html.find("write the report").expect("todo");
        let done = html.find("nested child").expect("done");
        assert!(intro < todo && todo < done);
        assert_eq!(html.matches("<ul>").count(), 3);
    }

    #[test]
    fn collapsed_blocks_are_still_printed() {
        let mut model = model();
        for row in &mut model.rows {
            row.view_collapsed = Some(true);
        }
        let html = render_html(&PrintDoc {
            title: "T",
            rows: &model.rows,
            graph_root: Path::new("/g"),
        });
        assert!(html.contains("nested child"));
    }

    #[test]
    fn inline_markup_tasks_and_refs() {
        let html = render(Path::new("/g"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
        assert!(html.contains("<code>code</code>"));
        assert!(html.contains("<span class=\"ref\">[[Other Page]]</span>"));
        assert!(html.contains("<span class=\"tag\">#tag</span>"));
        assert!(html.contains("&#9744;</span> write the report"));
        assert!(html.contains("&#9745;</span> nested child"));
        assert!(html.contains("<span class=\"mk\">DOING</span>"));
        assert!(html.contains("<span class=\"pri\">[#A]</span>"));
        assert!(html.contains("<a href=\"https://example.com/a?b=1&amp;c=2\">"));
    }

    #[test]
    fn properties_hidden_like_the_editor() {
        let html = render(Path::new("/g"));
        assert!(!html.contains("6500c1a4"));
        assert!(!html.contains("id::"));
        assert!(html.contains("<span class=\"pk\">status</span>"));
    }

    #[test]
    fn code_block_is_escaped_and_labelled() {
        let html = render(Path::new("/g"));
        assert!(
            html.contains(
                "<pre data-lang=\"rust\"><code>fn main() { if 1 &lt; 2 {} }</code></pre>"
            )
        );
    }

    #[test]
    fn user_text_is_html_escaped() {
        let html = render(Path::new("/g"));
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(
            html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
            "{html}"
        );
        assert!(html.contains("&amp;") && html.contains("&quot;quotes&quot;"));
        // The only script in the document is the print trigger.
        assert_eq!(html.matches("<script>").count(), 1);
    }

    #[test]
    fn javascript_links_never_become_anchors() {
        let model = PageModel::from_source(
            b"- [x](javascript:alert(1))\n",
            &PropertyConfig::default(),
            &NoBlocks,
        );
        let html = render_html(&PrintDoc {
            title: "T",
            rows: &model.rows,
            graph_root: Path::new("/g"),
        });
        assert!(!html.contains("href=\"javascript"));
    }

    #[test]
    fn assets_become_absolute_file_urls() {
        let html = render(Path::new("/graph root"));
        assert!(
            html.contains("src=\"file:///graph%20root/assets/my%20pic.png\""),
            "{html}"
        );
        assert!(html.contains("alt=\"pic\""));
    }

    #[test]
    fn zoomed_rows_start_at_level_one() {
        let model = model();
        let sub: Vec<Row> = model
            .rows
            .iter()
            .filter(|r| r.depth >= 1)
            .cloned()
            .collect();
        let html = render_html(&PrintDoc {
            title: "T",
            rows: &sub,
            graph_root: Path::new("/g"),
        });
        assert_eq!(html.matches("<ul>").count(), html.matches("</ul>").count());
        assert_eq!(html.matches("<ul>").count(), 2);
    }

    #[test]
    fn output_path_is_under_the_cache_dir_never_the_graph() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let graph = tmp.path().join("graph");
        std::fs::create_dir_all(graph.join("pages")).expect("graph");
        let dirs = crate::paths::AppDirs::with_root(&tmp.path().join("app"));
        let html = render(&graph);
        let path = write_print_file(&dirs.cache_dir, "My Page / 2026", &html).expect("written");
        assert!(path.starts_with(dirs.cache_dir.join(PRINT_DIR)));
        assert!(!path.starts_with(&graph));
        assert_eq!(std::fs::read_to_string(&path).expect("read"), html);
        // Atomic write leaves no temp file behind and overwriting works.
        write_print_file(&dirs.cache_dir, "My Page / 2026", &html).expect("rewritten");
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().expect("dir"))
            .expect("dir")
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
        assert_eq!(
            std::fs::read_dir(&graph).expect("graph").count(),
            1,
            "graph untouched"
        );
        assert!(file_url(&path).starts_with("file:///"));
    }

    #[test]
    fn slugs_are_safe_file_stems() {
        assert_eq!(slug("My Page / 2026"), "my-page-2026");
        assert_eq!(slug("../../etc"), "etc");
        assert_eq!(slug("日本"), "page");
    }
}
