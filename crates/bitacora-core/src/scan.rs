//! Graph discovery: ignore rules, `:hidden` prefixes, the scanner with Logseq's `filter-files`
//! ordering and the UTF-8/BOM reader. Spec: `01-file-graph-layout.md` §1.1.
//!
//! Scanning is strictly read-only: nothing is ever created inside the graph.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use bitacora_config::EffectiveConfig;
use walkdir::WalkDir;

use crate::graph_path::{GraphPath, GraphPathError};

/// Extensions recognised when walking a graph (case-sensitive, like Logseq's directory walk).
pub const ALLOWED_EXTENSIONS: [&str; 9] = [
    "org",
    "markdown",
    "md",
    "edn",
    "json",
    "js",
    "css",
    "excalidraw",
    "tldr",
];

/// Extensions (lower-cased) that reach the page parser.
pub const PARSER_EXTENSIONS: [&str; 5] = ["edn", "css", "org", "markdown", "md"];

const IGNORED_PREFIXES: [&str; 4] = [".", "logseq/.recycle", "logseq/bak", "logseq/version-files"];

/// Whether a graph-relative path is ignored by Logseq's directory-walk rules.
pub fn is_ignored_path(rel: &str) -> bool {
    let rel = rel.trim_start_matches('/');
    if IGNORED_PREFIXES.iter().any(|p| rel.starts_with(p)) {
        return true;
    }
    if matches!(rel, "logseq/graphs-txid.edn" | "logseq/pages-metadata.edn") {
        return true;
    }
    if rel.contains("/node_modules/") || rel.ends_with(".DS_Store") {
        return true;
    }
    // Any hidden segment: `/.x` or leading `.x`, where `x` is a non-dot character.
    rel.split('/').any(|seg| {
        let mut cs = seg.chars();
        cs.next() == Some('.') && cs.next().is_some_and(|c| c != '.')
    })
}

/// Whether the file extension is in the walk whitelist.
pub fn has_allowed_extension(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    match name.rfind('.') {
        Some(i) => ALLOWED_EXTENSIONS.contains(&&name[i + 1..]),
        None => false,
    }
}

/// A file found by the scanner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    /// Normalised graph-relative path.
    pub path: GraphPath,
    /// The path as found on disk (may differ from `path` in Unicode form).
    pub abs: PathBuf,
}

/// Scan errors.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    /// Filesystem error while walking.
    #[error("walking {path}: {source}")]
    Walk {
        /// Directory being walked.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// A path could not be represented as a graph path.
    #[error(transparent)]
    Path(#[from] GraphPathError),
}

/// UTF-16 code unit ordering, which is how JavaScript sorts strings.
fn cmp_js(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// Walk `root` and return every recognised file (Logseq `get-files`), sorted by path.
/// Symlinks and dot-entries are skipped; `:hidden` prefixes from `cfg` are applied.
pub fn scan_graph(root: &Path, cfg: &EffectiveConfig) -> Result<Vec<ScannedFile>, ScanError> {
    let mut out = Vec::new();
    let walker = WalkDir::new(root).follow_links(false).into_iter();
    let walker = walker.filter_entry(|e| {
        if e.depth() == 0 {
            return true;
        }
        if e.path_is_symlink() || e.file_name().to_string_lossy().starts_with('.') {
            return false;
        }
        if e.file_type().is_dir() {
            // Prune only subtrees whose every file would be ignored anyway.
            let Ok(rel) = GraphPath::from_abs(root, e.path()) else {
                return true;
            };
            let r = rel.as_str();
            let pruned = IGNORED_PREFIXES[1..]
                .iter()
                .any(|p| r == *p || r.starts_with(&format!("{p}/")));
            return !(pruned || (cfg.is_hidden(r) && cfg.is_hidden(&format!("{r}/"))));
        }
        true
    });
    for entry in walker {
        let entry = entry.map_err(|e| ScanError::Walk {
            path: e
                .path()
                .map_or_else(|| root.to_path_buf(), Path::to_path_buf),
            source: e.into(),
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = GraphPath::from_abs(root, entry.path())?;
        let r = path.as_str();
        if is_ignored_path(r) || !has_allowed_extension(r) || cfg.is_hidden(r) {
            continue;
        }
        out.push(ScannedFile {
            path,
            abs: entry.into_path(),
        });
    }
    out.sort_by(|a, b| cmp_js(a.path.as_str(), b.path.as_str()));
    Ok(out)
}

/// Logseq `filter-files`: keep parser formats, sort by path, then journals (reverse-sorted),
/// then built-ins (`contents.`, `.edn`, `custom.css`), then everything else.
/// The order decides which file wins on duplicate page titles.
pub fn parse_order(files: &[ScannedFile]) -> Vec<ScannedFile> {
    let mut supported: Vec<&ScannedFile> = files
        .iter()
        .filter(|f| {
            f.path
                .extension()
                .is_some_and(|e| PARSER_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        })
        .collect();
    supported.sort_by(|a, b| cmp_js(a.path.as_str(), b.path.as_str()));
    let (journals, others): (Vec<_>, Vec<_>) = supported
        .into_iter()
        .partition(|f| f.path.as_str().contains("journals/"));
    let (built_in, rest): (Vec<_>, Vec<_>) = others.into_iter().partition(|f| {
        let p = f.path.as_str();
        p.contains("contents.") || p.contains(".edn") || p.contains("custom.css")
    });
    journals
        .into_iter()
        .rev()
        .chain(built_in)
        .chain(rest)
        .cloned()
        .collect()
}

/// Text of a file prepared for parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFile {
    /// Decoded text without a leading BOM.
    pub text: String,
    /// The file started with a UTF-8 BOM (the bytes on disk are never rewritten by reading).
    pub had_bom: bool,
    /// Invalid UTF-8 was replaced with U+FFFD while decoding.
    pub lossy: bool,
}

/// Decode bytes as UTF-8, stripping a BOM for parsing only.
pub fn decode_text(bytes: &[u8]) -> TextFile {
    let (body, had_bom) = match bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(rest) => (rest, true),
        None => (bytes, false),
    };
    match std::str::from_utf8(body) {
        Ok(s) => TextFile {
            text: s.to_owned(),
            had_bom,
            lossy: false,
        },
        Err(_) => TextFile {
            text: String::from_utf8_lossy(body).into_owned(),
            had_bom,
            lossy: true,
        },
    }
}

/// Read a file (read-only) and decode it as UTF-8, stripping a BOM for parsing.
pub fn read_text(path: &Path) -> std::io::Result<TextFile> {
    Ok(decode_text(&std::fs::read(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn cfg(src: &str) -> EffectiveConfig {
        EffectiveConfig::from_texts(None, Some(src))
    }

    fn touch(root: &Path, rel: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        fs::write(p, "- x\n").expect("write");
    }

    const FILES: &[&str] = &[
        "pages/a.md",
        "pages/b.markdown",
        "pages/sub/c.md",
        "pages/contents.md",
        "pages/archived.md",
        "pages/.hidden.md",
        "pages/.dir/x.md",
        "pages/img.png",
        "pages/UPPER.MD",
        "pages/foo/node_modules/x.js",
        "journals/2025_11_14.md",
        "journals/2025_11_15.md",
        "journals/2024_01_01.md",
        "archived/old.md",
        "test.md",
        "assets/pic.png",
        "assets/doc.pdf",
        "draws/d.excalidraw",
        "whiteboards/w.edn",
        "logseq/config.edn",
        "logseq/custom.css",
        "logseq/bak/pages/a/2025.md",
        "logseq/.recycle/old.md",
        "logseq/version-files/local/x.md",
        "logseq/graphs-txid.edn",
        "logseq/pages-metadata.edn",
        ".git/config",
        ".DS_Store",
        "pages/.DS_Store",
    ];

    fn build() -> tempfile::TempDir {
        let d = tempfile::tempdir().expect("tmp");
        for f in FILES {
            touch(d.path(), f);
        }
        d
    }

    fn paths(v: &[ScannedFile]) -> Vec<&str> {
        v.iter().map(|f| f.path.as_str()).collect()
    }

    const GOLDEN: &[&str] = &[
        "archived/old.md",
        "draws/d.excalidraw",
        "journals/2024_01_01.md",
        "journals/2025_11_14.md",
        "journals/2025_11_15.md",
        "logseq/config.edn",
        "logseq/custom.css",
        "pages/a.md",
        "pages/archived.md",
        "pages/b.markdown",
        "pages/contents.md",
        "pages/sub/c.md",
        "test.md",
        "whiteboards/w.edn",
    ];

    #[test]
    fn golden_scan() {
        let d = build();
        let files = scan_graph(d.path(), &cfg("{}")).expect("scan");
        assert_eq!(paths(&files), GOLDEN);
    }

    #[test]
    fn hidden_prefixes() {
        let d = build();
        let files =
            scan_graph(d.path(), &cfg(r#"{:hidden ["/archived" "test.md"]}"#)).expect("scan");
        let got = paths(&files);
        assert!(!got.contains(&"archived/old.md"));
        assert!(!got.contains(&"test.md"));
        assert!(got.contains(&"pages/archived.md"));
        assert_eq!(got.len(), GOLDEN.len() - 2);
    }

    #[test]
    fn ignore_rules() {
        for p in [
            ".git/config",
            ".x",
            "pages/.x.md",
            "a/.b/c.md",
            "logseq/bak/x.md",
            "logseq/.recycle/x.md",
            "logseq/version-files/x.md",
            "logseq/graphs-txid.edn",
            "logseq/pages-metadata.edn",
            "pages/node_modules/x.js",
            "pages/.DS_Store",
            ".DS_Store",
        ] {
            assert!(is_ignored_path(p), "{p}");
        }
        for p in [
            "pages/a.md",
            "logseq/config.edn",
            "pages/a..md",
            "node_modules/x.js",
            "pages/v1.0.md",
        ] {
            assert!(!is_ignored_path(p), "{p}");
        }
        assert!(has_allowed_extension("a/b.tldr"));
        assert!(!has_allowed_extension("a/b.png"));
        assert!(!has_allowed_extension("a/b.MD"));
        assert!(!has_allowed_extension("a/md"));
    }

    #[test]
    fn filter_files_order() {
        let d = build();
        let files = scan_graph(d.path(), &cfg("{}")).expect("scan");
        let ordered = parse_order(&files);
        assert_eq!(
            paths(&ordered),
            vec![
                "journals/2025_11_15.md",
                "journals/2025_11_14.md",
                "journals/2024_01_01.md",
                "logseq/config.edn",
                "logseq/custom.css",
                "pages/contents.md",
                "whiteboards/w.edn",
                "archived/old.md",
                "pages/a.md",
                "pages/archived.md",
                "pages/b.markdown",
                "pages/sub/c.md",
                "test.md",
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_skipped() {
        let d = build();
        let outside = tempfile::tempdir().expect("tmp");
        touch(outside.path(), "x.md");
        std::os::unix::fs::symlink(outside.path(), d.path().join("pages/link")).expect("ln");
        std::os::unix::fs::symlink(outside.path().join("x.md"), d.path().join("pages/l.md"))
            .expect("ln");
        let files = scan_graph(d.path(), &cfg("{}")).expect("scan");
        assert_eq!(paths(&files), GOLDEN);
    }

    #[test]
    fn nfd_names_are_normalised() {
        let d = tempfile::tempdir().expect("tmp");
        touch(d.path(), "pages/Cafe\u{301}.md");
        let files = scan_graph(d.path(), &cfg("{}")).expect("scan");
        assert_eq!(paths(&files), vec!["pages/Caf\u{e9}.md"]);
        assert!(files[0].abs.exists());
    }

    #[test]
    fn bom_reader() {
        let t = decode_text(b"\xEF\xBB\xBF- a\r\n");
        assert_eq!(
            t,
            TextFile {
                text: "- a\r\n".into(),
                had_bom: true,
                lossy: false
            }
        );
        let t = decode_text(b"- \xFF");
        assert!(t.lossy && !t.had_bom);
    }

    /// Scanning and reading never write into the graph (no sidecar files, no mtime changes).
    #[test]
    fn no_trace() {
        let d = build();
        let snapshot = |root: &Path| {
            let mut v: Vec<(String, u64, std::time::SystemTime)> = WalkDir::new(root)
                .into_iter()
                .filter_map(Result::ok)
                .map(|e| {
                    let m = e.metadata().expect("meta");
                    (
                        e.path().display().to_string(),
                        m.len(),
                        m.modified().expect("mtime"),
                    )
                })
                .collect();
            v.sort();
            v
        };
        let before = snapshot(d.path());
        let files = scan_graph(d.path(), &cfg("{}")).expect("scan");
        for f in parse_order(&files) {
            read_text(&f.abs).expect("read");
        }
        assert_eq!(snapshot(d.path()), before);
    }
}
