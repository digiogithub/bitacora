//! Page lifecycle: lazy file creation, today's journal, templates (BIT-US-0028, BIT-US-0057;
//! `docs/analysis/logseq/01-file-graph-layout.md` sections 3.3, 4 and 8).
//!
//! A page that is only navigated to, referenced, aliased or the namespace parent of another page
//! has no file. [`Workspace::open_page`] puts such a page in the workspace as a *virtual* page:
//! it has a planned path and one empty block to type into, but [`Page::needs_write`] stays false
//! until it holds real content, so no empty file is ever created. Today's journal works the same
//! way ([`Workspace::ensure_today`]), with the configured template as its starting content: a
//! journal that still equals its template is pristine and is not written either.
//!
//! None of this is an edit: opening a virtual page does not create a transaction, and the first
//! real edit is an ordinary [`Op`](super::op::Op) on the empty first block.

use bitacora_config::{EffectiveConfig, NameFormat, PreferredFormat};
use bitacora_markdown::edit::properties::{get_property, remove_property};

use super::flush::FileStore;
use super::model::{BlockId, Page, Subtree};
use super::workspace::Workspace;
use crate::date::Date;
use crate::graph::{Graph, PageFormat, PageKey};
use crate::graph_path::{GraphPath, GraphPathError};
use crate::journal::{detect_journal, journal_file_path, journal_page};
use crate::naming::{file_body_encode, needs_title_property};

/// Why a page could not be opened or created.
#[derive(Debug, thiserror::Error)]
pub enum LifecycleError {
    /// The title is blank.
    #[error("page title is blank")]
    BlankTitle,
    /// The title does not map to a valid graph path.
    #[error("title does not map to a file path: {0}")]
    BadPath(#[from] GraphPathError),
    /// Reading an existing file failed.
    #[error("cannot read `{path}`: {message}")]
    Read {
        /// File.
        path: GraphPath,
        /// Error text.
        message: String,
    },
}

/// Outcome of [`Workspace::open_page`] and [`Workspace::ensure_journal`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    /// The page already existed in the workspace (or was loaded from its file); the key may
    /// differ from the requested title when an alias resolved to its owner.
    Existing(PageKey),
    /// A new virtual page without a file.
    Virtual(PageKey),
}

impl Opened {
    /// Key of the page to show.
    #[must_use]
    pub fn key(&self) -> &PageKey {
        match self {
            Self::Existing(k) | Self::Virtual(k) => k,
        }
    }
}

fn extension(cfg: &EffectiveConfig) -> &'static str {
    match cfg.preferred_format() {
        PreferredFormat::Org => "org",
        PreferredFormat::Markdown => "md",
    }
}

/// Where a new page lives: `<:pages-directory>/<encoded title>.<ext>`, encoded with the active
/// `:file/name-format`, keeping the case of the title. Journal titles map to
/// `<:journals-directory>/<:journal/file-name-format>.<ext>`; `Contents` to `pages/contents`.
///
/// # Errors
/// [`LifecycleError::BlankTitle`] or [`LifecycleError::BadPath`].
pub fn new_page_path(title: &str, cfg: &EffectiveConfig) -> Result<GraphPath, LifecycleError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(LifecycleError::BlankTitle);
    }
    if let Some(j) = detect_journal(title, cfg)
        && let Some(p) = journal_file_path(cfg, j.date)
    {
        return Ok(p);
    }
    let ext = extension(cfg);
    let body = if title.eq_ignore_ascii_case("contents") {
        "contents".to_owned()
    } else {
        file_body_encode(title, cfg.name_format())
    };
    Ok(GraphPath::new(&format!(
        "{}/{body}.{ext}",
        cfg.pages_directory()
    ))?)
}

/// The text of an automatically added `title::` pre-block: written (once the page has content)
/// only in legacy graphs, for titles that do not survive an encode -> decode round trip.
#[must_use]
pub fn auto_title_preamble(title: &str, name_format: NameFormat) -> Option<String> {
    needs_title_property(title, name_format).then(|| format!("title:: {title}"))
}

fn read_file(store: &dyn FileStore, path: &GraphPath) -> Result<Option<Vec<u8>>, LifecycleError> {
    store.read(path).map_err(|e| LifecycleError::Read {
        path: path.clone(),
        message: e.to_string(),
    })
}

fn is_org(path: &GraphPath) -> bool {
    path.extension()
        .and_then(PageFormat::from_extension)
        .is_some_and(|f| f == PageFormat::Org)
}

impl Workspace {
    /// Loads `path` from the store as a normal (clean) page.
    fn load_existing(&mut self, key: &PageKey, title: &str, path: &GraphPath, bytes: &[u8]) {
        self.load_page(key.clone(), title, Some(path.clone()), bytes);
        if is_org(path)
            && let Some(p) = self.pages_mut().get_mut(key)
        {
            p.read_only = true;
        }
    }

    /// Opens a page for display or typing without creating a file.
    ///
    /// * a page already in the workspace is returned as is;
    /// * a page the `graph` knows with a file (an alias resolves to its owner) is loaded from the
    ///   store, so a `.org` page never gets a `.md` twin;
    /// * a file at the planned path that the workspace has not loaded yet is loaded;
    /// * otherwise a virtual page is added: planned path, one empty first block, the legacy
    ///   `title::` pre-block when needed. It is written once it holds content.
    ///
    /// # Errors
    /// [`LifecycleError`] for a blank title, an unusable path or an unreadable file.
    pub fn open_page(
        &mut self,
        title: &str,
        cfg: &EffectiveConfig,
        graph: Option<&Graph>,
        store: &dyn FileStore,
    ) -> Result<Opened, LifecycleError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(LifecycleError::BlankTitle);
        }
        if let Some(j) = detect_journal(title, cfg) {
            return self.ensure_journal(j.date, cfg, store, false);
        }
        let key = PageKey::from_title(title);
        if self.page(&key).is_some() {
            return Ok(Opened::Existing(key));
        }
        if let Some(g) = graph
            && let Some(gp) = g.resolve(title)
            && let Some(file) = &gp.file
        {
            let gkey = gp.key.clone();
            if self.page(&gkey).is_none() {
                if let Some(bytes) = read_file(store, file)? {
                    self.load_existing(&gkey, &gp.original_name, file, &bytes);
                } else {
                    return self.new_virtual(title, cfg, store);
                }
            }
            return Ok(Opened::Existing(gkey));
        }
        self.new_virtual(title, cfg, store)
    }

    fn new_virtual(
        &mut self,
        title: &str,
        cfg: &EffectiveConfig,
        store: &dyn FileStore,
    ) -> Result<Opened, LifecycleError> {
        let key = PageKey::from_title(title);
        let path = new_page_path(title, cfg)?;
        if let Some(bytes) = read_file(store, &path)? {
            self.load_existing(&key, title, &path, &bytes);
            return Ok(Opened::Existing(key));
        }
        let mut page = Page::empty(key.clone(), title, Some(path));
        page.lazy.auto_preamble = auto_title_preamble(title, cfg.name_format());
        page.preamble.clone_from(&page.lazy.auto_preamble);
        self.insert_page(page);
        let first = Subtree::new(self.alloc_id(), "");
        self.attach_clean(&key, vec![first]);
        Ok(Opened::Virtual(key))
    }

    /// Adds root subtrees to a page that is being set up, without leaving it dirty.
    fn attach_clean(&mut self, key: &PageKey, roots: Vec<Subtree>) {
        for (i, st) in roots.into_iter().enumerate() {
            if self.attach(key, None, i, st).is_err() {
                break;
            }
        }
        if let Some(p) = self.pages_mut().get_mut(key) {
            p.dirty = false;
        }
        self.touched.clear();
    }

    /// Makes sure the journal page of `date` exists in the workspace, without writing a file.
    ///
    /// An existing file is loaded (never replaced). Otherwise a virtual journal page is added at
    /// `<:journals-directory>/<:journal/file-name-format>.<ext>` holding either the configured
    /// template (`with_template`, `:default-templates {:journals "name"}`) or one empty block.
    /// A journal that still equals its template is pristine: it is not written until the content
    /// differs.
    ///
    /// # Errors
    /// [`LifecycleError`] when the journal path is unusable or the file cannot be read.
    pub fn ensure_journal(
        &mut self,
        date: Date,
        cfg: &EffectiveConfig,
        store: &dyn FileStore,
        with_template: bool,
    ) -> Result<Opened, LifecycleError> {
        let jp = journal_page(date, cfg);
        let key = PageKey::from_title(&jp.title);
        if self.page(&key).is_some() {
            return Ok(Opened::Existing(key));
        }
        let path = journal_file_path(cfg, date).ok_or(LifecycleError::BlankTitle)?;
        if let Some(bytes) = read_file(store, &path)? {
            self.load_existing(&key, &jp.title, &path, &bytes);
            return Ok(Opened::Existing(key));
        }
        self.insert_page(Page::empty(key.clone(), jp.title.clone(), Some(path)));
        let template = if with_template {
            let name = cfg.default_journal_template();
            (!name.trim().is_empty())
                .then(|| self.find_template(name.trim()))
                .flatten()
        } else {
            None
        };
        let roots = match template {
            Some(blocks) if !blocks.is_empty() => {
                let vars = TemplateVars::new(date, cfg, &jp.title);
                blocks
                    .into_iter()
                    .map(|b| expand_subtree(b, &vars))
                    .collect()
            }
            _ => Vec::new(),
        };
        let templated = !roots.is_empty();
        let roots = if templated {
            roots
        } else {
            vec![Subtree::new(self.alloc_id(), "")]
        };
        self.attach_clean(&key, roots);
        if templated && let Some(p) = self.pages_mut().get_mut(&key) {
            p.set_pristine_from_content();
        }
        Ok(Opened::Virtual(key))
    }

    /// Today's journal (`ensure_journal` with the template), `None` when
    /// `:feature/enable-journals?` is false. `today` is the local calendar date, supplied by the
    /// caller (see [`Date::from_unix_secs`]).
    ///
    /// # Errors
    /// As [`Workspace::ensure_journal`].
    pub fn ensure_today(
        &mut self,
        today: Date,
        cfg: &EffectiveConfig,
        store: &dyn FileStore,
    ) -> Result<Option<Opened>, LifecycleError> {
        if !cfg.journals_enabled() {
            return Ok(None);
        }
        self.ensure_journal(today, cfg, store, true).map(Some)
    }

    /// The template named `name`: the children of the block that carries `template:: name`
    /// (case-insensitive) on any loaded page, as fresh detached subtrees. When the template block
    /// has no children, or `template-including-parent:: true` is set, the block itself is part
    /// of the template (without its `template` properties).
    #[must_use]
    pub fn find_template(&self, name: &str) -> Option<Vec<Subtree>> {
        let wanted = name.trim().to_lowercase();
        let (page, id) = self.pages().find_map(|p| {
            p.dfs().into_iter().find_map(|id| {
                let b = p.block(id)?;
                let t = get_property(&b.text, "template")?;
                (t.trim().to_lowercase() == wanted).then(|| (p.key.clone(), id))
            })
        })?;
        let root = self.page(&page)?.block(id)?;
        let including_parent = get_property(&root.text, "template-including-parent")
            .map(|v| v.trim().eq_ignore_ascii_case("true"));
        let children = root.children.clone();
        let include_root = including_parent.unwrap_or(children.is_empty());
        let mut out = Vec::new();
        if include_root {
            let mut st = self.copy_subtree(&page, id)?;
            st.text = remove_property(
                &remove_property(&st.text, "template"),
                "template-including-parent",
            );
            out.push(st);
        } else {
            for c in children {
                out.push(self.copy_subtree(&page, c)?);
            }
        }
        Some(out)
    }

    fn copy_subtree(&self, page: &PageKey, id: BlockId) -> Option<Subtree> {
        let b = self.page(page)?.block(id)?;
        let children = b
            .children
            .iter()
            .map(|c| self.copy_subtree(page, *c))
            .collect::<Option<Vec<_>>>()?;
        Some(Subtree::new(self.alloc_id(), b.text.clone()).with_children(children))
    }
}

/// Dynamic template variables, expanded when a template is inserted.
struct TemplateVars {
    today: String,
    yesterday: String,
    tomorrow: String,
    current: String,
}

impl TemplateVars {
    fn new(date: Date, cfg: &EffectiveConfig, title: &str) -> Self {
        let name = |d: Option<Date>| {
            d.map_or_else(String::new, |d| {
                format!("[[{}]]", journal_page(d, cfg).title)
            })
        };
        Self {
            today: name(Some(date)),
            yesterday: name(date.add_days(-1)),
            tomorrow: name(date.add_days(1)),
            current: format!("[[{title}]]"),
        }
    }

    fn expand(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(i) = rest.find("<%") {
            let Some(j) = rest[i..].find("%>") else { break };
            out.push_str(&rest[..i]);
            let var = rest[i + 2..i + j].trim().to_lowercase();
            let value = match var.as_str() {
                "today" => Some(&self.today),
                "yesterday" => Some(&self.yesterday),
                "tomorrow" => Some(&self.tomorrow),
                "current page" => Some(&self.current),
                _ => None,
            };
            match value {
                Some(v) if !v.is_empty() => out.push_str(v),
                _ => out.push_str(&rest[i..i + j + 2]),
            }
            rest = &rest[i + j + 2..];
        }
        out.push_str(rest);
        out
    }
}

fn expand_subtree(mut st: Subtree, vars: &TemplateVars) -> Subtree {
    st.text = vars.expand(&st.text);
    st.children = st
        .children
        .into_iter()
        .map(|c| expand_subtree(c, vars))
        .collect();
    st
}

/// Detects the local date changing while the app runs (BIT-US-0057 midnight rollover). The app
/// keeps one tracker, polls [`DayRollover::observe`] on a timer that fires at the next midnight
/// (see [`Date::secs_until_midnight`]) and after resume from sleep, and calls
/// [`Workspace::ensure_today`] whenever it returns a date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayRollover {
    last: Date,
}

impl DayRollover {
    /// Starts at `today` (the date `ensure_today` was last called for).
    #[must_use]
    pub fn new(today: Date) -> Self {
        Self { last: today }
    }

    /// The new date when `now` is a different day than the last observed one.
    pub fn observe(&mut self, now: Date) -> Option<Date> {
        (now != self.last).then(|| {
            self.last = now;
            now
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(src: &str) -> EffectiveConfig {
        EffectiveConfig::from_texts(None, Some(src))
    }

    #[test]
    fn rollover_fires_once_per_day() {
        let d1 = Date::new(2025, 11, 14).expect("d");
        let d2 = Date::new(2025, 11, 15).expect("d");
        let mut r = DayRollover::new(d1);
        assert_eq!(r.observe(d1), None);
        assert_eq!(r.observe(d2), Some(d2));
        assert_eq!(r.observe(d2), None);
    }

    #[test]
    fn variables_expand() {
        let c = cfg("{}");
        let v = TemplateVars::new(Date::new(2025, 11, 14).expect("d"), &c, "Nov 14th, 2025");
        assert_eq!(
            v.expand("<% today %> and <% Yesterday %> <% unknown %> <%"),
            "[[Nov 14th, 2025]] and [[Nov 13th, 2025]] <% unknown %> <%"
        );
    }

    #[test]
    fn paths() {
        let tlb = cfg("{:file/name-format :triple-lowbar}");
        assert_eq!(
            new_page_path("Projects/Bitacora", &tlb)
                .expect("p")
                .as_str(),
            "pages/Projects___Bitacora.md"
        );
        assert_eq!(
            new_page_path("Contents", &tlb).expect("p").as_str(),
            "pages/contents.md"
        );
        assert!(matches!(
            new_page_path("  ", &tlb),
            Err(LifecycleError::BlankTitle)
        ));
    }
}
