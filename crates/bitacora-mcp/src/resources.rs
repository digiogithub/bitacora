//! MCP resources: URI scheme, listing, reading and change-to-URI mapping (design section 6).
//!
//! URIs: `bitacora://page/<name>`, `bitacora://graph/<graph>/page/<name>`, `bitacora://block/<uuid>`,
//! `bitacora://journal/<yyyy-mm-dd|today>`, `bitacora://graph/<graph>/config`,
//! `bitacora://sync/status` and `bitacora://asset/<path>`. Names are percent-encoded.

use crate::dates;
use crate::reader::{ChangeEvent, GraphReader, PageInfo};
use crate::render::{Code, ToolError, decode_cursor, encode_cursor, nest, render_tree};
use crate::status::SyncStatus;
use crate::tools::check_graph;

/// Size cap of one asset resource.
pub(crate) const MAX_ASSET_BYTES: u64 = 5 * 1024 * 1024;
/// Resources per `resources/list` page.
pub(crate) const LIST_PAGE: usize = 50;

const SCHEME: &str = "bitacora://";

/// A parsed resource URI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResUri {
    Page { graph: Option<String>, name: String },
    Block(String),
    Journal(Option<String>),
    Config(Option<String>),
    SyncStatus,
    Asset(String),
}

/// Percent-encode everything but unreserved characters (and `/` when `keep_slash`).
pub(crate) fn encode(s: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric()
            || matches!(b, b'-' | b'_' | b'.' | b'~')
            || (keep_slash && b == b'/')
        {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Percent-decode; `None` on malformed escapes or invalid UTF-8.
pub(crate) fn decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Parse a resource URI.
pub(crate) fn parse_uri(uri: &str) -> Option<ResUri> {
    let rest = uri.strip_prefix(SCHEME)?;
    let (head, tail) = rest.split_once('/').unwrap_or((rest, ""));
    match head {
        "page" if !tail.is_empty() => Some(ResUri::Page {
            graph: None,
            name: decode(tail)?,
        }),
        "block" if !tail.is_empty() => Some(ResUri::Block(decode(tail)?)),
        "journal" if !tail.is_empty() => Some(ResUri::Journal(Some(decode(tail)?))),
        "sync" if tail == "status" => Some(ResUri::SyncStatus),
        "asset" if !tail.is_empty() => Some(ResUri::Asset(decode(tail)?)),
        "graph" => {
            let (graph, what) = tail.split_once('/')?;
            let graph = Some(decode(graph)?);
            if what == "config" {
                return Some(ResUri::Config(graph));
            }
            let name = what.strip_prefix("page/")?;
            if name.is_empty() {
                return None;
            }
            Some(ResUri::Page {
                graph,
                name: decode(name)?,
            })
        }
        _ => None,
    }
}

pub(crate) fn page_uri(name: &str) -> String {
    format!("{SCHEME}page/{}", encode(name, false))
}

/// Content of one read resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Content {
    Text {
        uri: String,
        mime: &'static str,
        text: String,
    },
    Blob {
        uri: String,
        mime: &'static str,
        base64: String,
    },
}

/// A listed resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listed {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime: &'static str,
}

/// Static URI templates: `(template, name, description)`.
pub(crate) const TEMPLATES: &[(&str, &str, &str)] = &[
    (
        "bitacora://page/{name}",
        "page",
        "Raw Markdown of a page of the active graph",
    ),
    (
        "bitacora://graph/{graph}/page/{name}",
        "graph-page",
        "Raw Markdown of a page of a named graph",
    ),
    (
        "bitacora://block/{uuid}",
        "block",
        "A block and its descendants as Markdown",
    ),
    (
        "bitacora://journal/{date}",
        "journal",
        "Journal page of a day (`yyyy-mm-dd` or `today`)",
    ),
    (
        "bitacora://graph/{graph}/config",
        "config",
        "`logseq/config.edn` (read-only)",
    ),
    (
        "bitacora://sync/status",
        "sync-status",
        "Git sync status (JSON)",
    ),
    (
        "bitacora://asset/{path}",
        "asset",
        "A file under `assets/` (size-capped)",
    ),
];

/// Resources returned by `resources/list`: fixed entries on the first page, then recent pages.
pub(crate) fn list(
    r: &dyn GraphReader,
    cursor: Option<&str>,
) -> Result<(Vec<Listed>, Option<String>), ToolError> {
    let offset = decode_cursor(cursor)?;
    let mut out = Vec::new();
    if offset == 0 {
        out.push(Listed {
            uri: "bitacora://journal/today".into(),
            name: "Today's journal".into(),
            description: "The journal page of the current day".into(),
            mime: "text/markdown",
        });
        out.push(Listed {
            uri: "bitacora://sync/status".into(),
            name: "Sync status".into(),
            description: "Git sync status".into(),
            mime: "application/json",
        });
    }
    let mut pages = r.recent_pages(offset, LIST_PAGE + 1)?;
    let more = pages.len() > LIST_PAGE;
    pages.truncate(LIST_PAGE);
    let count = pages.len();
    for p in pages {
        out.push(Listed {
            uri: page_uri(&p.original_name),
            name: p.original_name,
            description: "Page".into(),
            mime: "text/markdown",
        });
    }
    Ok((out, more.then(|| encode_cursor(offset + count))))
}

fn page_text(r: &dyn GraphReader, uri: &str, page: &PageInfo) -> Result<Content, ToolError> {
    let text = r
        .page_file_text(page)?
        .ok_or_else(|| ToolError::not_found(format!("file of page `{}`", page.original_name)))?;
    Ok(Content::Text {
        uri: uri.to_owned(),
        mime: "text/markdown",
        text,
    })
}

fn mime_of(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "txt" | "md" => "text/plain",
        _ => "application/octet-stream",
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub(crate) fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(B64[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Read a resource.
pub(crate) fn read(
    r: &dyn GraphReader,
    sync: &SyncStatus,
    uri: &str,
) -> Result<Content, ToolError> {
    let parsed = parse_uri(uri).ok_or_else(|| ToolError::not_found(format!("resource `{uri}`")))?;
    match parsed {
        ResUri::Page { graph, name } => {
            check_graph(r, graph.as_deref())?;
            let page = r
                .page(&name)?
                .ok_or_else(|| ToolError::not_found(format!("page `{name}`")))?;
            page_text(r, uri, &page)
        }
        ResUri::Journal(day) => {
            let day = match day.as_deref() {
                None | Some("today") => r.today(),
                Some(d) => dates::parse(d).ok_or_else(|| {
                    ToolError::invalid("journal date must be yyyy-mm-dd or `today`")
                })?,
            };
            let page = r
                .journal(day)?
                .ok_or_else(|| ToolError::not_found(format!("journal {}", dates::to_iso(day))))?;
            page_text(r, uri, &page)
        }
        ResUri::Block(uuid) => {
            let flat = r.subtree(&uuid)?;
            if flat.is_empty() {
                return Err(ToolError::not_found(format!("block `{uuid}`")));
            }
            Ok(Content::Text {
                uri: uri.to_owned(),
                mime: "text/markdown",
                text: render_tree(&nest(flat)),
            })
        }
        ResUri::Config(graph) => {
            check_graph(r, graph.as_deref())?;
            let text = r
                .config_text()?
                .ok_or_else(|| ToolError::not_found("logseq/config.edn"))?;
            Ok(Content::Text {
                uri: uri.to_owned(),
                mime: "application/edn",
                text,
            })
        }
        ResUri::SyncStatus => {
            let text = serde_json::to_string_pretty(sync)
                .map_err(|e| ToolError::new(Code::Internal, e.to_string()))?;
            Ok(Content::Text {
                uri: uri.to_owned(),
                mime: "application/json",
                text,
            })
        }
        ResUri::Asset(path) => {
            let bytes = r
                .read_asset(&path, MAX_ASSET_BYTES)?
                .ok_or_else(|| ToolError::not_found(format!("asset `{path}`")))?;
            Ok(Content::Blob {
                uri: uri.to_owned(),
                mime: mime_of(&path),
                base64: base64(&bytes),
            })
        }
    }
}

/// Resource URIs affected by a graph change (the subscriber filters them).
pub(crate) fn uris_for_change(graph: &str, today_iso: &str, c: &ChangeEvent) -> Vec<String> {
    let mut out = Vec::new();
    for p in &c.pages {
        out.push(page_uri(p));
        out.push(format!(
            "{SCHEME}graph/{}/page/{}",
            encode(graph, false),
            encode(p, false)
        ));
    }
    for d in &c.journal_days {
        out.push(format!("{SCHEME}journal/{d}"));
        if d == today_iso {
            out.push(format!("{SCHEME}journal/today"));
        }
    }
    for b in &c.blocks {
        out.push(format!("{SCHEME}block/{b}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_round_trip() {
        let u = page_uri("Project X/Sub é");
        assert_eq!(u, "bitacora://page/Project%20X%2FSub%20%C3%A9");
        assert_eq!(
            parse_uri(&u),
            Some(ResUri::Page {
                graph: None,
                name: "Project X/Sub é".into()
            })
        );
        assert_eq!(
            parse_uri("bitacora://graph/my%20g/page/a"),
            Some(ResUri::Page {
                graph: Some("my g".into()),
                name: "a".into()
            })
        );
        assert_eq!(
            parse_uri("bitacora://graph/g/config"),
            Some(ResUri::Config(Some("g".into())))
        );
        assert_eq!(
            parse_uri("bitacora://sync/status"),
            Some(ResUri::SyncStatus)
        );
        assert_eq!(
            parse_uri("bitacora://journal/today"),
            Some(ResUri::Journal(Some("today".into())))
        );
        assert_eq!(
            parse_uri("bitacora://asset/img/a.png"),
            Some(ResUri::Asset("img/a.png".into()))
        );
        assert!(parse_uri("http://x").is_none());
        assert!(parse_uri("bitacora://page/").is_none());
        assert!(parse_uri("bitacora://page/%zz").is_none());
    }

    #[test]
    fn base64_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn change_uris() {
        let c = ChangeEvent {
            pages: vec!["Day".into()],
            journal_days: vec!["2026-01-02".into()],
            blocks: vec!["u1".into()],
            all: false,
        };
        let uris = uris_for_change("g", "2026-01-02", &c);
        assert!(uris.contains(&"bitacora://page/Day".to_owned()));
        assert!(uris.contains(&"bitacora://journal/today".to_owned()));
        assert!(uris.contains(&"bitacora://block/u1".to_owned()));
    }
}
