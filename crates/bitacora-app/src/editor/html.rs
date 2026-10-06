//! Basic HTML to Markdown conversion for pasted rich text (BIT-US-0037).
//!
//! Lists become tab-indented `- ` bullets (which the paste classifier turns into a block
//! tree); paragraphs and breaks become line breaks; emphasis, code, links and images become
//! their Markdown forms. Anything else keeps only its text.

/// Whether `text` looks like a fragment of HTML worth converting.
#[must_use]
pub fn looks_like_html(text: &str) -> bool {
    let t = text.trim_start();
    if !t.starts_with('<') {
        return false;
    }
    let lower = t.get(..t.len().min(200)).unwrap_or(t).to_ascii_lowercase();
    ["<!doctype", "<html", "<body", "<ul", "<ol", "<li", "<p", "<div", "<h1", "<h2", "<h3", "<table", "<span", "<meta", "<b>", "<strong", "<a "]
        .iter()
        .any(|tag| lower.starts_with(tag))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece<'a> {
    Text(&'a str),
    Open(String, Vec<(String, String)>),
    Close(String),
}

fn attributes(raw: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = raw.trim();
    while !rest.is_empty() {
        let key_end = rest
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(rest.len());
        let key = rest[..key_end].to_ascii_lowercase();
        rest = rest[key_end..].trim_start();
        let mut value = String::new();
        if let Some(r) = rest.strip_prefix('=') {
            let r = r.trim_start();
            if let Some(q) = r.chars().next().filter(|c| *c == '"' || *c == '\'') {
                let body = &r[1..];
                let end = body.find(q).unwrap_or(body.len());
                value = body[..end].to_owned();
                rest = body.get(end + 1..).unwrap_or("").trim_start();
            } else {
                let end = r.find(char::is_whitespace).unwrap_or(r.len());
                value = r[..end].to_owned();
                rest = r[end..].trim_start();
            }
        }
        if !key.is_empty() {
            out.push((key, value));
        }
    }
    out
}

fn pieces(html: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut rest = html;
    while !rest.is_empty() {
        match rest.find('<') {
            None => {
                out.push(Piece::Text(rest));
                break;
            }
            Some(0) => {
                if let Some(comment) = rest.strip_prefix("<!--") {
                    rest = comment.find("-->").map_or("", |i| &comment[i + 3..]);
                    continue;
                }
                let Some(end) = rest.find('>') else {
                    out.push(Piece::Text(rest));
                    break;
                };
                let inner = &rest[1..end];
                rest = &rest[end + 1..];
                if inner.starts_with('!') || inner.starts_with('?') {
                    continue;
                }
                if let Some(name) = inner.strip_prefix('/') {
                    out.push(Piece::Close(name.trim().to_ascii_lowercase()));
                } else {
                    let inner = inner.trim_end_matches('/');
                    let name_end = inner
                        .find(|c: char| c.is_whitespace())
                        .unwrap_or(inner.len());
                    out.push(Piece::Open(
                        inner[..name_end].to_ascii_lowercase(),
                        attributes(&inner[name_end..]),
                    ));
                }
            }
            Some(i) => {
                out.push(Piece::Text(&rest[..i]));
                rest = &rest[i..];
            }
        }
    }
    out
}

fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|e| *e <= 8) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix('#')
                .and_then(|n| {
                    n.strip_prefix(['x', 'X'])
                        .map_or_else(|| n.parse().ok(), |h| u32::from_str_radix(h, 16).ok())
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Converts the HTML `html` to Markdown.
#[must_use]
pub fn html_to_markdown(html: &str) -> String {
    let mut out = String::new();
    // Open lists (true = ordered) and the href of the open link.
    let mut lists: Vec<bool> = Vec::new();
    let mut hrefs: Vec<Option<String>> = Vec::new();
    let mut skip = 0usize;
    let mut in_pre = false;

    let newline = |out: &mut String| {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
    };
    for piece in pieces(html) {
        match piece {
            Piece::Text(t) => {
                if skip > 0 {
                    continue;
                }
                let text = decode(t);
                if in_pre {
                    out.push_str(&text);
                } else {
                    // Collapse white space like a browser.
                    let mut last_space = out.ends_with(' ') || out.ends_with('\n') || out.is_empty();
                    for c in text.chars() {
                        if c.is_whitespace() {
                            if !last_space {
                                out.push(' ');
                                last_space = true;
                            }
                        } else {
                            out.push(c);
                            last_space = false;
                        }
                    }
                }
            }
            Piece::Open(name, attrs) => match name.as_str() {
                "script" | "style" | "head" | "title" => skip += 1,
                "ul" | "ol" => {
                    newline(&mut out);
                    lists.push(name == "ol");
                }
                "li" => {
                    newline(&mut out);
                    let depth = lists.len().saturating_sub(1);
                    out.push_str(&"\t".repeat(depth));
                    out.push_str("- ");
                }
                "p" | "div" | "tr" | "table" | "blockquote" => {
                    newline(&mut out);
                    if lists.is_empty() && !out.is_empty() && name == "p" {
                        out.push('\n');
                    }
                }
                "br" => out.push('\n'),
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    newline(&mut out);
                    if lists.is_empty() {
                        let level = name[1..].parse::<usize>().unwrap_or(1);
                        out.push_str(&"#".repeat(level));
                        out.push(' ');
                    }
                }
                "b" | "strong" => out.push_str("**"),
                "i" | "em" => out.push('*'),
                "s" | "del" | "strike" => out.push_str("~~"),
                "code" if !in_pre => out.push('`'),
                "pre" => {
                    newline(&mut out);
                    out.push_str("```\n");
                    in_pre = true;
                }
                "a" => {
                    let href = attrs
                        .iter()
                        .find(|(k, _)| k == "href")
                        .map(|(_, v)| decode(v));
                    if href.is_some() {
                        out.push('[');
                    }
                    hrefs.push(href);
                }
                "img" => {
                    let get = |key: &str| {
                        attrs
                            .iter()
                            .find(|(k, _)| k == key)
                            .map(|(_, v)| decode(v))
                            .unwrap_or_default()
                    };
                    let src = get("src");
                    if !src.is_empty() {
                        out.push_str(&format!("![{}]({src})", get("alt")));
                    }
                }
                _ => {}
            },
            Piece::Close(name) => match name.as_str() {
                "script" | "style" | "head" | "title" => skip = skip.saturating_sub(1),
                "ul" | "ol" => {
                    lists.pop();
                    newline(&mut out);
                }
                "li" | "p" | "div" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                | "blockquote" | "table" => newline(&mut out),
                "b" | "strong" => out.push_str("**"),
                "i" | "em" => out.push('*'),
                "s" | "del" | "strike" => out.push_str("~~"),
                "code" if !in_pre => out.push('`'),
                "pre" => {
                    newline(&mut out);
                    out.push_str("```\n");
                    in_pre = false;
                }
                "a" => {
                    if let Some(Some(href)) = hrefs.pop() {
                        out.push_str(&format!("]({href})"));
                    }
                }
                _ => {}
            },
        }
    }
    let mut text = out.trim().to_owned();
    // Trailing spaces inside bullets are noise.
    text = text
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_html_fragments() {
        assert!(looks_like_html("<ul><li>a</li></ul>"));
        assert!(looks_like_html("  <p>hi</p>"));
        assert!(!looks_like_html("a < b and c > d"));
        assert!(!looks_like_html("- a\n- b"));
        assert!(!looks_like_html("<notatag>"));
    }

    #[test]
    fn nested_lists_become_tab_indented_bullets() {
        let md = html_to_markdown("<ul><li>one<ul><li>two</li><li>three</li></ul></li><li>four</li></ul>");
        assert_eq!(md, "- one\n\t- two\n\t- three\n- four");
    }

    #[test]
    fn inline_formatting_links_and_images() {
        let md = html_to_markdown(
            "<p>a <b>bold</b> and <em>it</em> <code>x</code> <a href=\"https://e.org/?a=1&amp;b=2\">site</a></p><p><img src=\"p.png\" alt=\"pic\"></p>",
        );
        assert_eq!(
            md,
            "a **bold** and *it* `x` [site](https://e.org/?a=1&b=2)\n\n![pic](p.png)"
        );
    }

    #[test]
    fn headings_scripts_entities_and_pre() {
        let md = html_to_markdown(
            "<html><head><title>t</title><style>p{}</style></head><body><h2>Head &lt;1&gt;</h2><script>x()</script><pre>l1\nl2</pre></body></html>",
        );
        assert_eq!(md, "## Head <1>\n```\nl1\nl2\n```");
    }

    #[test]
    fn numeric_entities_and_whitespace_collapse() {
        assert_eq!(html_to_markdown("<div>a&#233;  \n  b&#x1F600;&nbsp;c</div>"), "a\u{e9} b\u{1f600} c");
        assert_eq!(html_to_markdown("fish &chips & more"), "fish &chips & more");
    }

    #[test]
    fn garbage_never_panics() {
        for s in ["<", "<a", "<a href=", "</>", "<!--", "<li><li>", "&", "&#;", "&#xZZ;", "<p>\u{e9}</p"] {
            let _ = html_to_markdown(s);
        }
    }
}
