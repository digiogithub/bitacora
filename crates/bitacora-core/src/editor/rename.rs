//! Page rename and merge (BIT-US-0061, BIT-US-0082, BIT-US-0087;
//! `docs/analysis/logseq/01-file-graph-layout.md` section 10.2).
//!
//! [`Workspace::plan_rename`] compiles a [`RenameRequest`] into the ops of ONE transaction:
//!
//! * the page file is renamed in place (same directory and extension, name from the active
//!   `:file/name-format`), the page gets its new key and title, `title::` / front-matter `title:`
//!   is rewritten, namespace children (`a/x` for `a`) follow with the prefix replaced once;
//! * every block that refers to the page is rewritten surgically ([`crate::rename::rewrite_refs`]),
//!   in every loaded page and in every page the [`RefLookup`] (the index) says refers to it;
//! * `:favorites` and `[:default-home :page]` of `logseq/config.edn` are updated through
//!   [`ConfigEditor`] as an [`Op::EditFile`];
//! * when the target page exists the pages are **merged** (Logseq's `merge-pages!`): the source
//!   blocks move to the end of the target and the source page is deleted (its file is recycled at
//!   the next flush). The caller must opt in with [`MergeMode::Merge`]; by default the request is
//!   refused with [`RenameError::TargetExists`] so the UI can ask for confirmation first.
//!
//! The transaction is atomic and undoable like any other: undo restores titles, keys, file names,
//! block text and `config.edn` (the files themselves are written by the next flush).
//!
//! Journals are never renamed: their title is derived from the file name, which must not move
//! (Logseq does the same, `page.cljs:486`). Pages that do not exist as files (referenced-only
//! pages) can be renamed too: only references and config change.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bitacora_config::{ConfigEditor, EffectiveConfig};
use bitacora_markdown::edit::properties::{get_property, set_front_matter_property, set_property};
use bitacora_markdown::properties::PropertyConfig;

use super::flush::FileStore;
use super::lifecycle::{LifecycleError, new_page_path};
use super::model::Position;
use super::op::Op;
use super::tx::{CommitError, Transaction};
use super::workspace::Workspace;
use crate::graph::PageKey;
use crate::graph_path::GraphPath;
use crate::journal::detect_journal;
use crate::naming::{file_body_encode, needs_title_property};
use crate::rename::{renamed, rewrite_refs};

/// `logseq/config.edn`.
pub const CONFIG_PATH: &str = "logseq/config.edn";

/// A page file known to the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageFile {
    /// Page title.
    pub title: String,
    /// Graph-relative path of its file.
    pub path: GraphPath,
}

/// Answers "where is X" questions from the index (`bitacora-runtime` implements it over
/// `IndexReader`; the index is a cache, so answers may be stale: they are verified against the
/// files before anything is rewritten, and a reference the index misses is not rewritten).
pub trait RefLookup: Send + Sync {
    /// Files that may contain references to any of `names` (page names as written; matching is
    /// case-insensitive): `[[name]]`, `#name`, property values and namespace refs below `name`.
    /// Property keys (`name:: v`) should be included when the index knows them.
    ///
    /// # Errors
    /// A message when the index cannot be queried.
    fn files_referencing(&self, names: &[String]) -> Result<Vec<PageFile>, String>;

    /// Pages with a file whose title is below the namespace `title` (`title/...`).
    ///
    /// # Errors
    /// A message when the index cannot be queried.
    fn namespace_children(&self, title: &str) -> Result<Vec<PageFile>, String>;

    /// The file of the page `title`; `None` when it has none (virtual page) or is unknown.
    ///
    /// # Errors
    /// A message when the index cannot be queried.
    fn page_file(&self, title: &str) -> Result<Option<PageFile>, String>;
}

/// What to do when the target page already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MergeMode {
    /// Refuse with [`RenameError::TargetExists`] (the UI asks the user).
    #[default]
    Refuse,
    /// Merge the pages. `keep_aliases` appends the source's `alias::` values to the target's;
    /// otherwise they are dropped and listed in [`RenameReport::dropped_aliases`].
    Merge {
        /// Carry the source's aliases over.
        keep_aliases: bool,
    },
}

/// A rename request.
#[derive(Clone)]
pub struct RenameRequest {
    /// Current title.
    pub from: String,
    /// New title.
    pub to: String,
    /// Effective graph config (name format, directories, journal formats, property settings).
    pub config: Arc<EffectiveConfig>,
    /// Index lookups; `None` limits the cascade to the pages already loaded.
    pub lookup: Option<Arc<dyn RefLookup>>,
    /// Behaviour when `to` exists.
    pub merge: MergeMode,
}

impl std::fmt::Debug for RenameRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenameRequest")
            .field("from", &self.from)
            .field("to", &self.to)
            .field("merge", &self.merge)
            .finish_non_exhaustive()
    }
}

/// Why a rename was refused. Nothing changed.
#[derive(Debug, thiserror::Error)]
pub enum RenameError {
    /// A title is blank.
    #[error("page title is blank")]
    Blank,
    /// Old and new title are identical.
    #[error("the new title is the same as the old one")]
    Unchanged,
    /// Journals are never renamed.
    #[error("journal page `{0}` cannot be renamed")]
    Journal(String),
    /// The page is read-only (`.org`).
    #[error("page `{0}` is read-only")]
    ReadOnly(String),
    /// The target page exists; ask the user and retry with [`MergeMode::Merge`].
    #[error("page `{0}` already exists")]
    TargetExists(String),
    /// A new file name is already used by another file.
    #[error("file `{0}` already exists")]
    PathTaken(String),
    /// A namespace child would collide with an existing page.
    #[error("namespace page `{0}` already exists")]
    ChildCollision(String),
    /// The new title does not map to a usable file name.
    #[error("title does not map to a file path: {0}")]
    BadPath(String),
    /// The index lookup failed.
    #[error("index lookup failed: {0}")]
    Lookup(String),
    /// A file could not be read.
    #[error("cannot read `{path}`: {message}")]
    Read {
        /// File.
        path: GraphPath,
        /// Error text.
        message: String,
    },
    /// The planned ops did not commit (nothing changed).
    #[error(transparent)]
    Commit(#[from] CommitError),
}

impl From<LifecycleError> for RenameError {
    fn from(e: LifecycleError) -> Self {
        match e {
            LifecycleError::BlankTitle => Self::Blank,
            other => Self::BadPath(other.to_string()),
        }
    }
}

/// Outcome of a committed rename.
#[derive(Debug, Clone)]
pub struct RenameReport {
    /// The transaction (undo it to revert the whole rename).
    pub tx: Transaction,
    /// `(old title, new title)` of every renamed page: the page itself, then namespace children.
    pub renamed: Vec<(String, String)>,
    /// The pages were merged.
    pub merged: bool,
    /// Blocks and preambles whose text was rewritten.
    pub rewritten_blocks: usize,
    /// Pages with at least one rewritten reference.
    pub rewritten_pages: Vec<PageKey>,
    /// Read-only pages that refer to the page but were not rewritten.
    pub skipped_read_only: Vec<String>,
    /// Aliases of a merged page that were dropped (opt in with `keep_aliases` to keep them).
    pub dropped_aliases: Vec<String>,
    /// `config.edn` was changed.
    pub config_updated: bool,
    /// Non-fatal notices.
    pub warnings: Vec<String>,
}

/// The ops of a planned rename plus what the report needs.
#[derive(Debug)]
pub struct RenamePlan {
    /// The ops, in order.
    pub ops: Vec<Op>,
    /// Pages loaded into the workspace while planning (publish snapshots for them).
    pub loaded: Vec<PageKey>,
    /// See [`RenameReport`].
    pub renamed: Vec<(String, String)>,
    /// See [`RenameReport`].
    pub merged: bool,
    /// See [`RenameReport`].
    pub rewritten_blocks: usize,
    /// See [`RenameReport`].
    pub rewritten_pages: Vec<PageKey>,
    /// See [`RenameReport`].
    pub skipped_read_only: Vec<String>,
    /// See [`RenameReport`].
    pub dropped_aliases: Vec<String>,
    /// See [`RenameReport`].
    pub config_updated: bool,
    /// See [`RenameReport`].
    pub warnings: Vec<String>,
}

fn property_config(cfg: &EffectiveConfig) -> PropertyConfig {
    PropertyConfig {
        ignored_page_references_keywords: cfg
            .ignored_page_references_keywords()
            .iter()
            .map(|k| k.to_lowercase())
            .collect(),
        separated_by_commas: cfg
            .property_separated_by_commas()
            .iter()
            .map(|k| k.to_lowercase())
            .collect(),
    }
}

/// Reads `title: ...` of a YAML front matter.
fn front_matter_title(pre: &str) -> Option<String> {
    let mut lines = pre.split('\n');
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    for l in lines {
        let l = l.trim_end_matches('\r');
        if l.trim_end() == "---" {
            return None;
        }
        if let Some(rest) = l.strip_prefix("title:") {
            return Some(rest.trim().to_owned());
        }
    }
    None
}

/// The preamble with its title set to `title`: rewrites an existing `title::` / front matter
/// `title:`; adds `title::` only when `add` is set and the preamble has none.
fn preamble_with_title(pre: Option<&str>, title: &str, add: bool) -> Option<String> {
    match pre {
        Some(p) if p.trim_start().starts_with("---") => front_matter_title(p)
            .is_some()
            .then(|| set_front_matter_property(p, "title", title)),
        Some(p) if !p.trim().is_empty() => {
            (add || get_property(p, "title").is_some()).then(|| set_property(p, "title", title))
        }
        _ => add.then(|| set_property("", "title", title)),
    }
}

/// `dir/<encoded title>.<ext>` next to `old` (Logseq's `compute-new-file-path`).
fn sibling_path(
    old: &GraphPath,
    title: &str,
    cfg: &EffectiveConfig,
) -> Result<GraphPath, RenameError> {
    let dir = old.as_str().rfind('/').map_or("", |i| &old.as_str()[..i]);
    let ext = old.extension().unwrap_or("md");
    let body = file_body_encode(title.trim(), cfg.name_format());
    let rel = if dir.is_empty() {
        format!("{body}.{ext}")
    } else {
        format!("{dir}/{body}.{ext}")
    };
    GraphPath::new(&rel).map_err(|e| RenameError::BadPath(e.to_string()))
}

fn read(store: &dyn FileStore, path: &GraphPath) -> Result<Option<Vec<u8>>, RenameError> {
    store.read(path).map_err(|e| RenameError::Read {
        path: path.clone(),
        message: e.to_string(),
    })
}

/// A page to rename: its key now, old and new title, and where its file goes.
struct Move {
    key: PageKey,
    old_title: String,
    new_title: String,
    new_path: Option<GraphPath>,
}

impl Workspace {
    /// Loads the page `title` when it is not loaded yet: from the index's path, else from its
    /// planned path. Returns its key when the page exists (loaded or file-backed).
    fn load_for_rename(
        &mut self,
        title: &str,
        req: &RenameRequest,
        store: &dyn FileStore,
        loaded: &mut Vec<PageKey>,
    ) -> Result<Option<PageKey>, RenameError> {
        let key = PageKey::from_title(title);
        if self.page(&key).is_some() {
            return Ok(Some(key));
        }
        let mut candidates: Vec<(String, GraphPath)> = Vec::new();
        if let Some(l) = &req.lookup
            && let Some(f) = l.page_file(title).map_err(RenameError::Lookup)?
        {
            candidates.push((f.title, f.path));
        }
        if let Ok(p) = new_page_path(title, &req.config) {
            candidates.push((title.to_owned(), p));
        }
        for (t, path) in candidates {
            if let Some(bytes) = read(store, &path)? {
                let k = PageKey::from_title(&t);
                self.load_existing(&k, &t, &path, &bytes);
                loaded.push(k.clone());
                return Ok(Some(k));
            }
        }
        Ok(None)
    }

    fn load_page_file(
        &mut self,
        f: &PageFile,
        store: &dyn FileStore,
        loaded: &mut Vec<PageKey>,
    ) -> Result<(), RenameError> {
        let key = PageKey::from_title(&f.title);
        if self.page(&key).is_some() || self.page_for_path(&f.path).is_some() {
            return Ok(());
        }
        if let Some(bytes) = read(store, &f.path)? {
            self.load_existing(&key, &f.title, &f.path, &bytes);
            loaded.push(key);
        }
        Ok(())
    }

    /// Plans the rename (or merge) described by `req`. May load pages into the workspace (clean,
    /// not an edit); changes nothing else.
    ///
    /// # Errors
    /// [`RenameError`]; no op is returned then.
    #[allow(clippy::too_many_lines)]
    pub fn plan_rename(
        &mut self,
        store: &dyn FileStore,
        req: &RenameRequest,
    ) -> Result<RenamePlan, RenameError> {
        let cfg = &*req.config;
        let from = req.from.trim().to_owned();
        let to = req.to.trim().to_owned();
        if from.is_empty() || to.is_empty() {
            return Err(RenameError::Blank);
        }
        if from == to {
            return Err(RenameError::Unchanged);
        }
        for t in [&from, &to] {
            if detect_journal(t, cfg).is_some() {
                return Err(RenameError::Journal(t.clone()));
            }
        }
        let from_key = PageKey::from_title(&from);
        let to_key = PageKey::from_title(&to);
        let mut loaded = Vec::new();
        let source = self.load_for_rename(&from, req, store, &mut loaded)?;
        if let Some(k) = &source
            && self.page(k).is_some_and(|p| p.read_only)
        {
            return Err(RenameError::ReadOnly(from));
        }
        let case_only = from_key == to_key;
        let target = if case_only {
            None
        } else {
            self.load_for_rename(&to, req, store, &mut loaded)?
        };

        let mut plan = Plan::default();
        let mut moves: Vec<Move> = Vec::new();
        // An empty page that only exists in memory is replaced, not merged.
        let target = match target {
            Some(tk) if self.page(&tk).is_some_and(super::model::Page::is_virtual) => {
                plan.moves.push(Op::DeletePage {
                    page: tk.clone(),
                    captured: None,
                });
                plan.dropped.insert(tk);
                None
            }
            other => other,
        };
        let new_name: String;
        if let Some(tk) = target {
            // Merge into the existing page.
            let MergeMode::Merge { keep_aliases } = req.merge else {
                let title = self.page(&tk).map_or(to.clone(), |p| p.title.clone());
                return Err(RenameError::TargetExists(title));
            };
            let tgt = self.page(&tk).ok_or(RenameError::Blank)?;
            if tgt.read_only {
                return Err(RenameError::ReadOnly(tgt.title.clone()));
            }
            new_name = tgt.title.clone();
            plan.merged = true;
            if let Some(sk) = &source {
                self.plan_merge(sk, &tk, keep_aliases, &mut plan);
            }
            if let Some((k, before, after)) = plan.merge_preamble.take() {
                plan.preambles.insert(k, (before, Some(after)));
            }
            plan.renamed.push((from.clone(), new_name.clone()));
        } else {
            new_name = to.clone();
            if let Some(sk) = &source {
                let p = self.page(sk).ok_or(RenameError::Blank)?;
                moves.push(Move {
                    key: sk.clone(),
                    old_title: p.title.clone(),
                    new_title: to.clone(),
                    new_path: None,
                });
            }
            plan.renamed.push((from.clone(), to.clone()));
            self.collect_children(&from, &to, req, store, &mut loaded, &mut moves)?;
            for m in moves.iter().filter(|m| m.key != from_key) {
                plan.renamed
                    .push((m.old_title.clone(), m.new_title.clone()));
            }
        }

        // Cascade: which pages may refer to the old name (the page and its namespace children).
        let mut names = vec![from.clone()];
        for m in moves.iter().filter(|m| m.key != from_key) {
            names.push(m.old_title.clone());
        }
        if let Some(l) = &req.lookup {
            for f in l.files_referencing(&names).map_err(RenameError::Lookup)? {
                self.load_page_file(&f, store, &mut loaded)?;
            }
        }

        // Page moves: file, key/title, title property.
        let mut key_map: BTreeMap<PageKey, PageKey> = BTreeMap::new();
        for m in &mut moves {
            self.plan_page_move(m, cfg, store, &mut plan)?;
            let new_key = PageKey::from_title(&m.new_title);
            key_map.insert(m.key.clone(), new_key.clone());
            let page = self.page(&m.key).ok_or(RenameError::Blank)?;
            let legacy_needs =
                page.path.is_some() && needs_title_property(&m.new_title, cfg.name_format());
            let before = page.preamble.clone();
            let after = preamble_with_title(before.as_deref(), &m.new_title, legacy_needs);
            if after.is_some() && after != before {
                plan.preambles.insert(new_key, (before, after));
            }
        }

        // Rewrite references everywhere.
        let pcfg = property_config(cfg);
        let mut pages: Vec<PageKey> = self.pages().map(|p| p.key.clone()).collect();
        pages.sort();
        for key in pages {
            let Some(page) = self.page(&key) else {
                continue;
            };
            let final_key = key_map.get(&key).cloned().unwrap_or_else(|| key.clone());
            let mut changed = false;
            let mut block_ops = Vec::new();
            for id in page.dfs() {
                let Some(b) = page.block(id) else { continue };
                if let Some(after) = rewrite_refs(&b.text, &from, &new_name, &pcfg, !plan.merged) {
                    changed = true;
                    block_ops.push(Op::SetText {
                        id,
                        before: b.text.clone(),
                        after,
                    });
                }
            }
            let pre_before = plan
                .preambles
                .get(&final_key)
                .and_then(|(_, a)| a.clone())
                .or_else(|| page.preamble.clone());
            let pre_after = pre_before
                .as_deref()
                .filter(|p| !p.trim_start().starts_with("---"))
                .and_then(|p| rewrite_refs(p, &from, &new_name, &pcfg, !plan.merged));
            if let Some(a) = pre_after {
                changed = true;
                let orig = page.preamble.clone();
                plan.preambles.insert(final_key.clone(), (orig, Some(a)));
            }
            if !changed {
                continue;
            }
            if page.read_only {
                plan.skipped_read_only.push(page.title.clone());
                // Preamble edits of read-only pages are not planned either.
                plan.preambles.remove(&final_key);
                continue;
            }
            plan.rewritten_blocks += block_ops.len();
            plan.rewritten_pages.push(final_key);
            plan.cascade.extend(block_ops);
        }
        for (key, (before, after)) in std::mem::take(&mut plan.preambles) {
            if before != after {
                plan.preamble_ops.push(Op::SetPreamble {
                    page: key,
                    before,
                    after,
                });
            }
        }

        // config.edn.
        self.plan_config(store, cfg, &plan.renamed.clone(), &mut plan)?;

        let mut ops = plan.moves;
        ops.extend(plan.preamble_ops);
        ops.extend(plan.cascade);
        ops.extend(plan.tail);
        ops.extend(plan.config_ops);
        Ok(RenamePlan {
            ops,
            loaded,
            renamed: plan.renamed,
            merged: plan.merged,
            rewritten_blocks: plan.rewritten_blocks,
            rewritten_pages: plan.rewritten_pages,
            skipped_read_only: plan.skipped_read_only,
            dropped_aliases: plan.dropped_aliases,
            config_updated: plan.config_updated,
            warnings: plan.warnings,
        })
    }

    /// Namespace children of `from` (loaded pages and the index's), renamed with the prefix
    /// replaced once.
    fn collect_children(
        &mut self,
        from: &str,
        to: &str,
        req: &RenameRequest,
        store: &dyn FileStore,
        loaded: &mut Vec<PageKey>,
        moves: &mut Vec<Move>,
    ) -> Result<(), RenameError> {
        if let Some(l) = &req.lookup {
            for f in l.namespace_children(from).map_err(RenameError::Lookup)? {
                self.load_page_file(&f, store, loaded)?;
            }
        }
        let prefix = format!("{}/", PageKey::from_title(from).as_str());
        let mut children: Vec<(PageKey, String)> = self
            .pages()
            .filter(|p| p.key.as_str().starts_with(&prefix))
            .map(|p| (p.key.clone(), p.title.clone()))
            .collect();
        children.sort();
        let mut taken: BTreeSet<PageKey> = BTreeSet::new();
        for (key, title) in children {
            let Some(new_title) = renamed(&title, from, to, true) else {
                continue;
            };
            let new_key = PageKey::from_title(&new_title);
            if self.page(&new_key).is_some() || !taken.insert(new_key.clone()) {
                return Err(RenameError::ChildCollision(new_title));
            }
            if let Some(l) = &req.lookup
                && l.page_file(&new_title)
                    .map_err(RenameError::Lookup)?
                    .is_some()
            {
                return Err(RenameError::ChildCollision(new_title));
            }
            moves.push(Move {
                key,
                old_title: title,
                new_title,
                new_path: None,
            });
        }
        Ok(())
    }

    /// File rename and re-key of one page.
    fn plan_page_move(
        &self,
        m: &mut Move,
        cfg: &EffectiveConfig,
        store: &dyn FileStore,
        plan: &mut Plan,
    ) -> Result<(), RenameError> {
        let page = self.page(&m.key).ok_or(RenameError::Blank)?;
        let new_key = PageKey::from_title(&m.new_title);
        let new_path = match &page.path {
            Some(old) => sibling_path(old, &m.new_title, cfg)?,
            None => new_page_path(&m.new_title, cfg)?,
        };
        if page.path.as_ref() != Some(&new_path) {
            let case_only_file = page
                .path
                .as_ref()
                .is_some_and(|o| o.as_str().to_lowercase() == new_path.as_str().to_lowercase());
            let other_page = self
                .page_for_path(&new_path)
                .is_some_and(|k| *k != m.key && !plan.dropped.contains(k));
            let on_disk = if case_only_file {
                false
            } else {
                read(store, &new_path)?.is_some()
            };
            if other_page || on_disk {
                return Err(RenameError::PathTaken(new_path.to_string()));
            }
            plan.moves.push(Op::RenameFile {
                page: m.key.clone(),
                from: page.path.clone(),
                to: Some(new_path.clone()),
            });
        }
        m.new_path = Some(new_path);
        plan.moves.push(Op::RenamePage {
            from: m.key.clone(),
            to: new_key,
            title_from: page.title.clone(),
            title_to: m.new_title.clone(),
        });
        Ok(())
    }

    /// Moves the blocks of `src` to the end of `tgt` and deletes `src`.
    fn plan_merge(&self, src: &PageKey, tgt: &PageKey, keep_aliases: bool, plan: &mut Plan) {
        let (Some(s), Some(t)) = (self.page(src), self.page(tgt)) else {
            return;
        };
        if !s.is_blank() {
            for (k, id) in s.roots.iter().enumerate() {
                plan.moves.push(Op::Move {
                    id: *id,
                    from: None,
                    to: Position {
                        page: tgt.clone(),
                        parent: None,
                        index: t.roots.len() + k,
                    },
                });
            }
        }
        let aliases: Vec<String> = s
            .preamble
            .as_deref()
            .and_then(|p| get_property(p, "alias"))
            .map(|v| {
                v.split([',', '，'])
                    .map(|a| a.trim().to_owned())
                    .filter(|a| !a.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        if !aliases.is_empty() {
            if keep_aliases {
                let before = t.preamble.clone();
                let existing = before
                    .as_deref()
                    .and_then(|p| get_property(p, "alias"))
                    .unwrap_or_default();
                let mut all: Vec<String> = existing
                    .split([',', '，'])
                    .map(|a| a.trim().to_owned())
                    .filter(|a| !a.is_empty())
                    .collect();
                for a in aliases {
                    if !all.iter().any(|x| x.eq_ignore_ascii_case(&a)) {
                        all.push(a);
                    }
                }
                let after = set_property(before.as_deref().unwrap_or(""), "alias", &all.join(", "));
                if before.as_deref() != Some(after.as_str()) {
                    plan.merge_preamble = Some((tgt.clone(), before, after));
                }
            } else {
                plan.dropped_aliases = aliases;
                plan.warnings.push(format!(
                    "aliases of `{}` were not carried over: {}",
                    s.title,
                    plan.dropped_aliases.join(", ")
                ));
            }
        }
        plan.tail.push(Op::DeletePage {
            page: src.clone(),
            captured: None,
        });
    }

    /// `:favorites` and `[:default-home :page]` of `config.edn`.
    fn plan_config(
        &self,
        store: &dyn FileStore,
        _cfg: &EffectiveConfig,
        renamed: &[(String, String)],
        plan: &mut Plan,
    ) -> Result<(), RenameError> {
        let path = GraphPath::new(CONFIG_PATH).map_err(|e| RenameError::BadPath(e.to_string()))?;
        let Some(bytes) = read(store, &path)? else {
            return Ok(());
        };
        let Ok(text) = String::from_utf8(bytes.clone()) else {
            plan.warnings
                .push("config.edn is not valid UTF-8; favorites were not updated".into());
            return Ok(());
        };
        let mut ed = match ConfigEditor::parse(&text) {
            Ok(e) => e,
            Err(e) => {
                plan.warnings
                    .push(format!("config.edn was not updated: {e}"));
                return Ok(());
            }
        };
        let mut changed = false;
        for (old, new) in renamed {
            for r in [
                ed.favorites_rename(old, new),
                ed.default_home_rename(old, new),
            ] {
                match r {
                    Ok(c) => changed |= c,
                    Err(e) => plan
                        .warnings
                        .push(format!("config.edn was not fully updated: {e}")),
                }
            }
        }
        if changed && ed.text() != text {
            plan.config_ops.push(Op::EditFile {
                path,
                before: bytes,
                after: ed.into_text().into_bytes(),
            });
            plan.config_updated = true;
        }
        Ok(())
    }
}

#[derive(Default)]
struct Plan {
    moves: Vec<Op>,
    preamble_ops: Vec<Op>,
    cascade: Vec<Op>,
    tail: Vec<Op>,
    config_ops: Vec<Op>,
    renamed: Vec<(String, String)>,
    merged: bool,
    rewritten_blocks: usize,
    rewritten_pages: Vec<PageKey>,
    skipped_read_only: Vec<String>,
    dropped_aliases: Vec<String>,
    config_updated: bool,
    warnings: Vec<String>,
    merge_preamble: Option<(PageKey, Option<String>, String)>,
    preambles: BTreeMap<PageKey, (Option<String>, Option<String>)>,
    dropped: BTreeSet<PageKey>,
}
