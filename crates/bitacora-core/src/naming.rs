//! Page title <-> file name codecs (`:triple-lowbar`, `:legacy`, `:legacy-dot`) and the page
//! title derivation pipeline. Spec: `docs/analysis/logseq/01-file-graph-layout.md` §3.
//!
//! Re-implemented from the documented behaviour (ADR-015); no Logseq code is copied.

use bitacora_config::{EffectiveConfig, NameFormat};
use unicode_normalization::UnicodeNormalization;

/// Strip one leading and one trailing `/`, then NFC-normalise. Case is preserved.
pub fn page_name_sanity(title: &str) -> String {
    let s = title.strip_prefix('/').unwrap_or(title);
    let s = s.strip_suffix('/').unwrap_or(s);
    s.nfc().collect()
}

/// Lower-cased page key (`:block/name`): lower-case, then boundary-slash strip and NFC.
pub fn page_key(title: &str) -> String {
    page_name_sanity(&title.to_lowercase())
}

const WINDOWS_RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn is_tri_lb_reserved(c: char) -> bool {
    matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '\\')
}

fn push_pct(out: &mut String, c: char) {
    // Only called for ASCII characters.
    out.push_str(&format!("%{:02X}", c as u32));
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// `%XX` at the start of `b`, returning the byte value.
fn pct_token(b: &[u8]) -> Option<u8> {
    if b.len() >= 3 && b[0] == b'%' {
        Some(hex_val(b[1])? * 16 + hex_val(b[2])?)
    } else {
        None
    }
}

/// `:triple-lowbar` codec.
pub mod triple_lowbar {
    use super::*;

    /// Title -> file body (no extension).
    pub fn encode(title: &str) -> String {
        // 1. boundary slashes + NFC (case preserved)
        let s = page_name_sanity(title);
        // 2. pre-escape existing percent sequences (`%XX` -> `%25XX`)
        let bytes = s.as_bytes();
        let mut escaped = String::with_capacity(s.len());
        let mut i = 0;
        while i < bytes.len() {
            if pct_token(&bytes[i..]).is_some() {
                escaped.push_str("%25");
                i += 1;
            } else {
                let ch = s[i..].chars().next().unwrap_or('\u{FFFD}');
                escaped.push(ch);
                i += ch.len_utf8();
            }
        }
        // 3. reserved characters are percent-encoded (all are ASCII)
        let mut body = String::with_capacity(escaped.len());
        for c in escaped.chars() {
            if is_tri_lb_reserved(c) {
                push_pct(&mut body, c);
            } else {
                body.push(c);
            }
        }
        // 4. leading dot
        if let Some(rest) = body.strip_prefix('.') {
            body = format!("%2E{rest}");
        }
        // 5. Windows reserved names (case-sensitive) and trailing dot
        if WINDOWS_RESERVED.contains(&body.as_str()) || body.ends_with('.') {
            body.push('/');
        }
        // 6. disambiguate underscores, then encode namespace separators
        body.replace("___", "%5F%5F%5F")
            .replace("_/", "%5F/")
            .replace("/_", "/%5F")
            .replace('/', "___")
    }

    /// File body -> title.
    pub fn decode(body: &str) -> String {
        let s = body.replace("___", "/");
        let bytes = s.as_bytes();
        let mut out = String::with_capacity(s.len());
        let mut i = 0;
        while i < bytes.len() {
            match pct_token(&bytes[i..]) {
                // A single token only decodes when it is a complete (ASCII) code point.
                Some(v) if v < 0x80 => {
                    out.push(v as char);
                    i += 3;
                }
                Some(_) => {
                    out.push_str(&s[i..i + 3]);
                    i += 3;
                }
                None => {
                    let ch = s[i..].chars().next().unwrap_or('\u{FFFD}');
                    out.push(ch);
                    i += ch.len_utf8();
                }
            }
        }
        out.split('/')
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("/")
    }
}

/// `:legacy` codec (graphs without `:file/name-format`) and the older `:legacy-dot` scheme.
pub mod legacy {
    use super::*;

    /// Title -> file body (`Projects/Bitacora` -> `Projects%2FBitacora`).
    pub fn encode(title: &str) -> String {
        let s = page_name_sanity(title);
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            if matches!(
                c,
                '\\' | '#' | '|' | '%' | ':' | '*' | '?' | '"' | '<' | '>' | '/'
            ) {
                push_pct(&mut out, c);
            } else {
                out.push(c);
            }
        }
        out
    }

    /// `decodeURIComponent` over the whole string; `None` when it would throw.
    fn decode_uri_component(s: &str) -> Option<String> {
        let b = s.as_bytes();
        let mut out = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'%' {
                out.push(pct_token(&b[i..])?);
                i += 3;
            } else {
                out.push(b[i]);
                i += 1;
            }
        }
        String::from_utf8(out).ok()
    }

    /// File body -> title: every `.` becomes `/`, then URL-decode (raw string on failure).
    pub fn decode(body: &str) -> String {
        let s = body.replace('.', "/");
        decode_uri_component(&s).unwrap_or(s)
    }

    /// Pre-May-2022 `:legacy-dot` file body; only used to detect old files during conversion.
    /// Each run of reserved characters becomes a single `_`, `/` becomes `.`.
    pub fn dot_encode(title: &str) -> String {
        let s = page_name_sanity(title);
        let mut out = String::with_capacity(s.len());
        let mut in_run = false;
        for c in s.chars() {
            if matches!(
                c,
                ':' | '\\' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '%'
            ) {
                if !in_run {
                    out.push('_');
                }
                in_run = true;
            } else {
                in_run = false;
                out.push(if c == '/' { '.' } else { c });
            }
        }
        out
    }
}

/// Encode a title as a file body in the given format.
pub fn file_body_encode(title: &str, format: NameFormat) -> String {
    match format {
        NameFormat::TripleLowbar => triple_lowbar::encode(title),
        NameFormat::Legacy => legacy::encode(title),
    }
}

/// Decode a file body into a title in the given format.
pub fn file_body_decode(body: &str, format: NameFormat) -> String {
    match format {
        NameFormat::TripleLowbar => triple_lowbar::decode(body),
        NameFormat::Legacy => legacy::decode(body),
    }
}

/// Base name of `path` with everything after the last `.` removed.
pub fn path_file_body(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rfind('.') {
        Some(i) => &name[..i],
        None => name,
    }
}

/// Whether a title does not survive an encode -> decode round trip (or its file name would
/// contain reserved characters). Used on rename in any format.
pub fn title_roundtrip_mismatch(title: &str, format: NameFormat) -> bool {
    let body = file_body_encode(title, format);
    file_body_decode(&body, format) != title || body.chars().any(is_tri_lb_reserved)
}

/// Whether a new page needs a `title::` property at creation. Only legacy graphs ever do:
/// in triple-lowbar mode the check is skipped at creation.
pub fn needs_title_property(title: &str, format: NameFormat) -> bool {
    format == NameFormat::Legacy && title_roundtrip_mismatch(title, format)
}

/// Page title from a graph-relative path, an optional `title::` / front-matter value taken from
/// the first properties block (supplied by the markdown layer; stubbed as a plain argument), and
/// the config.
///
/// Order: `pages/contents.*` -> `Contents`; property title; decoded file body.
pub fn derive_title(rel_path: &str, props_title: Option<&str>, cfg: &EffectiveConfig) -> String {
    if rel_path.starts_with("pages/contents.") {
        return "Contents".to_owned();
    }
    if let Some(t) = props_title {
        return t.to_owned();
    }
    file_body_decode(path_file_body(rel_path), cfg.name_format())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const TLB_CASES: &[(&str, &str)] = &[
        ("Projects/Bitacora/Design", "Projects___Bitacora___Design"),
        ("My Page", "My Page"),
        ("What? A: B", "What%3F A%3A B"),
        ("C#", "C%23"),
        ("50% done", "50% done"),
        ("a%2Fb", "a%252Fb"),
        ("a%2fb", "a%252fb"),
        ("%%41", "%%2541"),
        ("foo_bar", "foo_bar"),
        ("foo___bar", "foo%5F%5F%5Fbar"),
        ("foo____bar", "foo%5F%5F%5F_bar"),
        ("a_/b", "a%5F___b"),
        ("a/_b", "a___%5Fb"),
        (".hidden", "%2Ehidden"),
        ("CON", "CON___"),
        ("con", "con"),
        ("COM9", "COM9___"),
        ("ends with.", "ends with.___"),
        ("Version 1.0", "Version 1.0"),
        ("v1.0/notes", "v1.0___notes"),
        ("a*b", "a%2Ab"),
        ("tag|pipe", "tag%7Cpipe"),
        ("<html>", "%3Chtml%3E"),
        ("back\\slash", "back%5Cslash"),
        ("say \"hi\"", "say %22hi%22"),
        ("Cafe\u{301}", "Caf\u{e9}"),
        ("aa?#/bbb/ccc", "aa%3F%23___bbb___ccc"),
        ("a__/bbb/ccc", "a_%5F___bbb___ccc"),
        ("/lead/trail/", "lead___trail"),
    ];

    #[test]
    fn tlb_encode_vectors() {
        for (title, body) in TLB_CASES {
            assert_eq!(triple_lowbar::encode(title), *body, "encode {title:?}");
        }
    }

    #[test]
    fn tlb_decode_vectors() {
        let cases = [
            ("a___b", "a/b"),
            ("a____b", "a/_b"),
            ("a_____b", "a/__b"),
            ("a%3Fb", "a?b"),
            ("a%3fb", "a?b"),
            ("a%252Fb", "a%2Fb"),
            ("caf%E4", "caf%E4"),
            ("%C3%A9", "%C3%A9"),
            ("50% done", "50% done"),
            ("___a___", "a"),
            ("a______b", "a/b"),
            ("%5F%5F%5F", "___"),
            ("%2Ehidden", ".hidden"),
        ];
        for (body, title) in cases {
            assert_eq!(triple_lowbar::decode(body), title, "decode {body:?}");
        }
    }

    #[test]
    fn tlb_examples_roundtrip() {
        for (title, _) in TLB_CASES {
            let t = page_name_sanity(title);
            assert_eq!(triple_lowbar::decode(&triple_lowbar::encode(title)), t);
        }
    }

    proptest! {
        #[test]
        fn tlb_roundtrip(title in "[ -~\u{e9}\u{4e2d}]{0,24}") {
            let t = page_name_sanity(&title);
            prop_assume!(!t.is_empty() && !t.split('/').any(str::is_empty));
            prop_assert_eq!(triple_lowbar::decode(&triple_lowbar::encode(&title)), t);
        }
    }

    #[test]
    fn legacy_vectors() {
        assert_eq!(legacy::encode("Projects/Bitacora"), "Projects%2FBitacora");
        assert_eq!(legacy::encode("a*b? #c|d%"), "a%2Ab%3F %23c%7Cd%25");
        assert_eq!(legacy::decode("Projects%2FBitacora"), "Projects/Bitacora");
        assert_eq!(legacy::decode("Version 1.0"), "Version 1/0");
        assert_eq!(legacy::decode("bad%E4"), "bad%E4");
        assert_eq!(legacy::decode("100% sure"), "100% sure");
        assert_eq!(legacy::decode("a%2Eb"), "a/b".replace('/', "."));
        assert_eq!(legacy::dot_encode("a/b: c"), "a.b_ c");
        assert_eq!(legacy::dot_encode("a?*b"), "a_b");
    }

    #[test]
    fn title_property_predicate() {
        assert!(needs_title_property("Version 1.0", NameFormat::Legacy));
        assert!(!needs_title_property("My Page", NameFormat::Legacy));
        assert!(needs_title_property("a.b", NameFormat::Legacy));
        assert!(!needs_title_property(
            "Version 1.0",
            NameFormat::TripleLowbar
        ));
        assert!(!needs_title_property("a/b", NameFormat::TripleLowbar));
        assert!(!title_roundtrip_mismatch(
            "My Page",
            NameFormat::TripleLowbar
        ));
    }

    fn cfg(src: &str) -> EffectiveConfig {
        EffectiveConfig::from_texts(None, Some(src))
    }

    #[test]
    fn derive_title_pipeline() {
        let tlb = cfg("{:file/name-format :triple-lowbar}");
        let leg = cfg("{}");
        assert_eq!(
            derive_title("pages/contents.md", Some("X"), &tlb),
            "Contents"
        );
        assert_eq!(derive_title("pages/foo.md", Some("Bar"), &tlb), "Bar");
        assert_eq!(
            derive_title("pages/Version 1.0.md", None, &tlb),
            "Version 1.0"
        );
        assert_eq!(
            derive_title("pages/Version 1.0.md", None, &leg),
            "Version 1/0"
        );
        assert_eq!(derive_title("pages/a___b.md", None, &tlb), "a/b");
        assert_eq!(derive_title("pages/sub/dir/x%3F.md", None, &tlb), "x?");
    }
}
