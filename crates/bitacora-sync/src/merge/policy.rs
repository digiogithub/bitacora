//! Path policy dispatch (design `git-sync-merge` 5.1, BIT-SP-0006.R17).

use crate::autocommit::is_ignored_path;

/// How a file is merged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Volatile or generated files: take ours.
    Ignore,
    /// Pages and journals: the block-aware merge.
    Markdown,
    /// `logseq/config.edn`: EDN-aware map merge.
    Config,
    /// css, js, `.gitignore` and other text: line diff3.
    TextLine,
    /// Whiteboards and drawings: take the changed side, keep both when both changed.
    WholeFile,
    /// Assets and non-UTF-8 files: take the changed side, keep both as a conflict copy.
    Binary,
}

/// Chooses the policy for `path`. `contents` are the base, ours and theirs bytes: a non-UTF-8
/// `.md` or `config.edn` falls back to [`Policy::Binary`].
pub fn policy_for(path: &str, contents: [Option<&[u8]>; 3]) -> Policy {
    if is_ignored_path(path) {
        return Policy::Ignore;
    }
    let all_utf8 = contents
        .iter()
        .flatten()
        .all(|b| std::str::from_utf8(b).is_ok());
    if path.ends_with(".md") {
        return if all_utf8 {
            Policy::Markdown
        } else {
            Policy::Binary
        };
    }
    if path == "logseq/config.edn" {
        return if all_utf8 {
            Policy::Config
        } else {
            Policy::Binary
        };
    }
    if (path.starts_with("whiteboards/") && path.ends_with(".edn"))
        || path.starts_with("draws/")
        || path.ends_with(".tldr")
        || path.ends_with(".excalidraw")
    {
        return Policy::WholeFile;
    }
    if path.starts_with("assets/") || !all_utf8 {
        return Policy::Binary;
    }
    Policy::TextLine
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policies_by_path() {
        let t = Some(b"x".as_slice());
        let all = |p| policy_for(p, [t, t, t]);
        assert_eq!(all("pages/A.md"), Policy::Markdown);
        assert_eq!(all("journals/2026_10_06.md"), Policy::Markdown);
        assert_eq!(all("logseq/config.edn"), Policy::Config);
        assert_eq!(all("logseq/custom.css"), Policy::TextLine);
        assert_eq!(all("logseq/custom.js"), Policy::TextLine);
        assert_eq!(all(".gitignore"), Policy::TextLine);
        assert_eq!(all("whiteboards/a.edn"), Policy::WholeFile);
        assert_eq!(all("draws/a.excalidraw"), Policy::WholeFile);
        assert_eq!(all("x/y.tldr"), Policy::WholeFile);
        assert_eq!(all("assets/a.png"), Policy::Binary);
        for ignored in [
            "logseq/bak/a.md",
            "logseq/.recycle/a.md",
            "logseq/version-files/x",
            "logseq/graphs-txid.edn",
            "logseq/pages-metadata.edn",
        ] {
            assert_eq!(all(ignored), Policy::Ignore, "{ignored}");
        }
    }

    #[test]
    fn non_utf8_text_falls_back_to_binary() {
        let t = Some(b"x".as_slice());
        let bad = Some([0xff, 0xfe].as_slice());
        assert_eq!(policy_for("pages/A.md", [t, bad, t]), Policy::Binary);
        assert_eq!(policy_for("logseq/config.edn", [t, t, bad]), Policy::Binary);
        assert_eq!(
            policy_for("logseq/custom.css", [None, bad, t]),
            Policy::Binary
        );
    }
}
