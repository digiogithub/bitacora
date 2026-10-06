//! Which graph paths the watcher reports.

use std::sync::Arc;

use bitacora_core::scan::is_ignored_path;

type HiddenFn = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Ignore rules shared by the watcher and the polling fallback.
///
/// Combines Logseq's walk rules (`bitacora_core::scan::is_ignored_path`: dot-paths, `.git`,
/// `logseq/bak`, `logseq/.recycle`, `logseq/version-files`, `graphs-txid.edn`,
/// `pages-metadata.edn`, `node_modules`, `.DS_Store`), our `*.bitacora-tmp` files and the
/// config `:hidden` prefixes (supplied as a predicate, e.g. `|p| cfg.is_hidden(p)`).
#[derive(Clone, Default)]
pub struct IgnoreRules {
    hidden: Option<HiddenFn>,
}

impl std::fmt::Debug for IgnoreRules {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IgnoreRules")
            .field("hidden", &self.hidden.is_some())
            .finish()
    }
}

impl IgnoreRules {
    /// Rules without `:hidden` prefixes.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the config `:hidden` predicate (receives graph-relative `/` paths).
    #[must_use]
    pub fn with_hidden(mut self, f: impl Fn(&str) -> bool + Send + Sync + 'static) -> Self {
        self.hidden = Some(Arc::new(f));
        self
    }

    /// Whether the path (file or directory, graph-relative) must be skipped entirely.
    #[must_use]
    pub fn is_ignored(&self, rel: &str) -> bool {
        let rel = rel.trim_start_matches('/');
        if rel.ends_with(".bitacora-tmp")
            || rel.split('/').any(|seg| seg == "node_modules")
            || is_ignored_path(rel)
        {
            return true;
        }
        // Directories are given without trailing slash; `node_modules` is matched with one.
        if is_ignored_path(&format!("{rel}/")) {
            return true;
        }
        self.hidden
            .as_ref()
            .is_some_and(|h| h(rel) || h(&format!("{rel}/")))
    }

    /// Whether a file at `rel` is reported: not ignored and `.md`/`.markdown`/`.org` or
    /// `logseq/config.edn`.
    #[must_use]
    pub fn accepts_file(&self, rel: &str) -> bool {
        let rel = rel.trim_start_matches('/');
        let ext_ok = rel == "logseq/config.edn"
            || matches!(
                rel.rsplit_once('.').map(|(_, e)| e),
                Some("md" | "markdown" | "org")
            );
        ext_ok && !self.is_ignored(rel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rules() {
        let r = IgnoreRules::new();
        for p in [
            ".git/HEAD",
            "logseq/bak/pages/a.md",
            "logseq/.recycle/a.md",
            "logseq/version-files/x.md",
            "logseq/pages-metadata.edn",
            "pages/.hidden.md",
            "pages/.a.md.bitacora-tmp",
            "pages/a.md.bitacora-tmp",
            "node_modules/x/readme.md",
            "pages/node_modules/x/readme.md",
            "pages/.DS_Store",
        ] {
            assert!(!r.accepts_file(p), "{p} must be ignored");
        }
        for p in [
            "pages/a.md",
            "journals/2026_10_06.md",
            "logseq/config.edn",
            "x.org",
        ] {
            assert!(r.accepts_file(p), "{p} must be accepted");
        }
        assert!(!r.accepts_file("assets/img.png"));
        assert!(!r.accepts_file("logseq/custom.css"));
        assert!(r.is_ignored("logseq/bak"));
        assert!(r.is_ignored("a/node_modules"));
    }

    #[test]
    fn hidden_predicate() {
        let r = IgnoreRules::new().with_hidden(|p| p.starts_with("archived"));
        assert!(!r.accepts_file("archived/a.md"));
        assert!(r.is_ignored("archived"));
        assert!(r.accepts_file("pages/a.md"));
    }
}
