//! Read-side content exclusions for restricted tokens (BIT-US-0139, ADR-031).
//!
//! A token with an exclusion set (the dedicated `pando` token) sees the graph through
//! [`FilteredReader`]: pages that are `private:: true`, match an excluded page name, namespace,
//! graph-relative path prefix or `#tag` disappear from every read tool, resource and prompt, and
//! blocks of those pages are never returned (not even through backlinks, tasks or queries).
//!
//! The rule set mirrors `GraphConsent::exclusions` of `bitacora-config`: entries are graph-relative
//! path prefixes, page names (a name also hides its namespace children) or `#tag`s. The shared
//! `ContentPolicy` of the semantic-sync stories can feed [`ReadExclusions::new`] with the same
//! entries so every call site agrees.

use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::sync::broadcast;

use crate::reader::{
    BlockInfo, ChangeEvent, GraphInfo, GraphReader, ListPagesQuery, PageInfo, QueryOutcome,
    QueryRequest, ReaderError, ReaderResult, RefGroupInfo, SearchItem, SearchQuery, TaskQuery,
};

/// Page property that marks a page as private.
pub const PRIVATE_PROPERTY: &str = "private";

/// What a restricted token must not see.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadExclusions {
    /// Lower-case page names (a name also covers `name/child`).
    pages: Vec<String>,
    /// Lower-case graph-relative path prefixes.
    paths: Vec<String>,
    /// Lower-case tags without `#`.
    tags: Vec<String>,
    /// Hide `private:: true` pages (always on for restricted tokens).
    hide_private: bool,
    /// Hide everything (consent revoked).
    deny_all: bool,
}

impl ReadExclusions {
    /// Builds the rule set from consent exclusions (path prefixes, page names, `#tags`).
    /// `private:: true` pages are always hidden.
    #[must_use]
    pub fn new<I, S>(entries: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut out = Self {
            hide_private: true,
            ..Self::default()
        };
        for raw in entries {
            let e = raw.as_ref().trim().to_lowercase();
            if e.is_empty() {
                continue;
            }
            if let Some(tag) = e.strip_prefix('#') {
                out.tags.push(tag.trim().to_owned());
            } else {
                if e.contains('/') || e.ends_with(".md") {
                    out.paths.push(e.trim_start_matches("./").to_owned());
                }
                out.pages.push(e);
            }
        }
        out
    }

    /// A rule set that hides every page and block: what the `pando` token gets when the graph has
    /// no (or no longer has) consent for agent access.
    #[must_use]
    pub fn deny_all() -> Self {
        Self {
            hide_private: true,
            deny_all: true,
            ..Self::default()
        }
    }

    /// Whether `page` must stay invisible.
    #[must_use]
    pub fn hides_page(&self, page: &PageInfo) -> bool {
        if self.deny_all {
            return true;
        }
        if self.hide_private
            && page
                .properties
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case(PRIVATE_PROPERTY) && is_true(v))
        {
            return true;
        }
        if self.hides_name(&page.original_name) || self.hides_name(&page.name) {
            return true;
        }
        if let Some(file) = &page.file {
            let file = file.to_lowercase();
            if self.paths.iter().any(|p| file.starts_with(p.as_str())) {
                return true;
            }
        }
        page.tags.iter().any(|t| {
            let t = t.trim().trim_start_matches('#').to_lowercase();
            self.tags.contains(&t)
        })
    }

    fn hides_name(&self, name: &str) -> bool {
        if self.deny_all {
            return true;
        }
        let name = name.to_lowercase();
        self.pages.iter().any(|p| {
            name == *p
                || name
                    .strip_prefix(p.as_str())
                    .is_some_and(|r| r.starts_with('/'))
        })
    }
}

/// Whether the block itself carries `private:: true`.
fn block_is_private(b: &BlockInfo) -> bool {
    b.properties
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case(PRIVATE_PROPERTY) && is_true(v))
}

fn is_true(v: &str) -> bool {
    v.trim().eq_ignore_ascii_case("true")
}

/// A [`GraphReader`] that applies [`ReadExclusions`] to another reader.
pub struct FilteredReader {
    inner: Arc<dyn GraphReader>,
    rules: Arc<ReadExclusions>,
}

impl std::fmt::Debug for FilteredReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilteredReader").finish_non_exhaustive()
    }
}

impl FilteredReader {
    /// Wraps `inner`.
    #[must_use]
    pub fn new(inner: Arc<dyn GraphReader>, rules: Arc<ReadExclusions>) -> Self {
        Self { inner, rules }
    }

    /// Whether the page with this display name is hidden (unknown pages are visible).
    fn name_hidden(&self, name: &str) -> bool {
        if self.rules.hides_name(name) {
            return true;
        }
        match self.inner.page(name) {
            Ok(Some(p)) => self.rules.hides_page(&p),
            // A failed lookup must not leak: treat it as hidden.
            Err(_) => true,
            Ok(None) => false,
        }
    }

    /// A block is hidden with its page, with its own `private:: true` and with any private
    /// ancestor (a private block hides its whole subtree, and the breadcrumbs that would quote
    /// its title). A failed ancestor lookup hides the block.
    fn block_hidden(&self, b: &BlockInfo) -> bool {
        if self.name_hidden(&b.page) {
            return true;
        }
        if !self.rules.hide_private {
            return false;
        }
        if block_is_private(b) {
            return true;
        }
        match self.inner.ancestors(&b.uuid) {
            Ok(ancestors) => ancestors.iter().any(block_is_private),
            Err(_) => true,
        }
    }

    fn keep_blocks(&self, mut blocks: Vec<BlockInfo>) -> Vec<BlockInfo> {
        blocks.retain(|b| !self.block_hidden(b));
        blocks
    }

    fn keep_groups(&self, mut groups: Vec<RefGroupInfo>) -> Vec<RefGroupInfo> {
        groups.retain(|g| !self.name_hidden(&g.page));
        for g in &mut groups {
            g.blocks.retain(|i| !self.block_hidden(&i.block));
        }
        groups.retain(|g| !g.blocks.is_empty());
        groups
    }

    fn keep_pages(&self, mut pages: Vec<PageInfo>) -> Vec<PageInfo> {
        pages.retain(|p| !self.rules.hides_page(p));
        pages
    }
}

impl GraphReader for FilteredReader {
    fn graph_info(&self) -> ReaderResult<GraphInfo> {
        self.inner.graph_info()
    }

    fn search(&self, q: &SearchQuery) -> ReaderResult<Vec<SearchItem>> {
        let mut items = self.inner.search(q)?;
        let mut cache: BTreeMap<String, bool> = BTreeMap::new();
        items.retain(|i| {
            if *cache
                .entry(i.page.clone())
                .or_insert_with(|| self.name_hidden(&i.page))
            {
                return false;
            }
            // Block hits: a private block (or a block under one) is not searchable either.
            match (&i.uuid, self.rules.hide_private) {
                (Some(uuid), true) => match self.inner.block(uuid) {
                    Ok(Some(b)) => !self.block_hidden(&b),
                    Ok(None) => true,
                    Err(_) => false,
                },
                _ => true,
            }
        });
        Ok(items)
    }

    fn page(&self, name: &str) -> ReaderResult<Option<PageInfo>> {
        Ok(self.inner.page(name)?.filter(|p| !self.rules.hides_page(p)))
    }

    fn list_pages(&self, q: &ListPagesQuery) -> ReaderResult<Vec<PageInfo>> {
        Ok(self.keep_pages(self.inner.list_pages(q)?))
    }

    fn recent_pages(&self, offset: usize, limit: usize) -> ReaderResult<Vec<PageInfo>> {
        Ok(self.keep_pages(self.inner.recent_pages(offset, limit)?))
    }

    fn page_blocks(
        &self,
        page: &PageInfo,
        offset: usize,
        limit: usize,
        skip_collapsed: bool,
    ) -> ReaderResult<Vec<BlockInfo>> {
        if self.rules.hides_page(page) {
            return Ok(Vec::new());
        }
        let mut blocks = self
            .inner
            .page_blocks(page, offset, limit, skip_collapsed)?;
        blocks.retain(|b| !self.block_hidden(b));
        Ok(blocks)
    }

    fn block(&self, uuid: &str) -> ReaderResult<Option<BlockInfo>> {
        Ok(self.inner.block(uuid)?.filter(|b| !self.block_hidden(b)))
    }

    fn block_position(&self, uuid: &str) -> ReaderResult<Option<usize>> {
        match self.block(uuid)? {
            Some(_) => self.inner.block_position(uuid),
            None => Ok(None),
        }
    }

    fn subtree(&self, uuid: &str) -> ReaderResult<Vec<BlockInfo>> {
        match self.block(uuid)? {
            Some(_) => {
                let mut blocks = self.inner.subtree(uuid)?;
                blocks.retain(|b| !self.block_hidden(b));
                Ok(blocks)
            }
            None => Ok(Vec::new()),
        }
    }

    fn ancestors(&self, uuid: &str) -> ReaderResult<Vec<BlockInfo>> {
        match self.block(uuid)? {
            Some(_) => self.inner.ancestors(uuid),
            None => Ok(Vec::new()),
        }
    }

    fn journals(&self, before_day: Option<i64>, limit: usize) -> ReaderResult<Vec<PageInfo>> {
        Ok(self.keep_pages(self.inner.journals(before_day, limit)?))
    }

    fn journal(&self, day: i64) -> ReaderResult<Option<PageInfo>> {
        Ok(self
            .inner
            .journal(day)?
            .filter(|p| !self.rules.hides_page(p)))
    }

    fn today(&self) -> i64 {
        self.inner.today()
    }

    fn linked_references(&self, page: &PageInfo) -> ReaderResult<Vec<RefGroupInfo>> {
        if self.rules.hides_page(page) {
            return Ok(Vec::new());
        }
        Ok(self.keep_groups(self.inner.linked_references(page)?))
    }

    fn unlinked_references(&self, page: &PageInfo) -> ReaderResult<Vec<RefGroupInfo>> {
        if self.rules.hides_page(page) {
            return Ok(Vec::new());
        }
        Ok(self.keep_groups(self.inner.unlinked_references(page)?))
    }

    fn block_referrers(&self, uuid: &str) -> ReaderResult<Vec<BlockInfo>> {
        if self.block(uuid)?.is_none() {
            return Ok(Vec::new());
        }
        Ok(self.keep_blocks(self.inner.block_referrers(uuid)?))
    }

    fn tasks(&self, q: &TaskQuery) -> ReaderResult<Vec<BlockInfo>> {
        Ok(self.keep_blocks(self.inner.tasks(q)?))
    }

    fn run_query(&self, q: &QueryRequest) -> ReaderResult<QueryOutcome> {
        let mut out = self.inner.run_query(q)?;
        if out.kind == "rows" {
            // Generic tuples cannot be attributed to pages, so they could leak excluded content.
            return Err(ReaderError::invalid_query(
                "aggregate and tuple queries are not available to this token; \
                 use a query that returns blocks or pages",
            ));
        }
        out.blocks = self.keep_blocks(out.blocks);
        out.pages = self.keep_pages(out.pages);
        Ok(out)
    }

    fn page_file_text(&self, page: &PageInfo) -> ReaderResult<Option<String>> {
        if self.rules.hides_page(page) {
            return Ok(None);
        }
        // The raw file would carry the private blocks: serve it only when there are none.
        if self.rules.hide_private {
            let blocks = self.inner.page_blocks(page, 0, 1_000_000, false)?;
            if blocks.iter().any(|b| self.block_hidden(b)) {
                return Ok(None);
            }
        }
        self.inner.page_file_text(page)
    }

    fn config_text(&self) -> ReaderResult<Option<String>> {
        self.inner.config_text()
    }

    fn read_asset(&self, rel: &str, max_bytes: u64) -> ReaderResult<Option<Vec<u8>>> {
        self.inner.read_asset(rel, max_bytes)
    }

    /// Change events carry page names of excluded pages, so a restricted token gets none.
    fn changes(&self) -> Option<broadcast::Receiver<ChangeEvent>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(name: &str, file: Option<&str>) -> PageInfo {
        PageInfo {
            name: name.to_lowercase(),
            original_name: name.to_owned(),
            uuid: String::new(),
            properties: BTreeMap::new(),
            aliases: Vec::new(),
            tags: Vec::new(),
            journal_day: None,
            file: file.map(str::to_owned),
            updated_at: None,
            block_count: 0,
            etag: String::new(),
            id: 0,
        }
    }

    #[test]
    fn private_property_names_namespaces_paths_and_tags_are_hidden() {
        let r = ReadExclusions::new(["Secrets", "pages/hr", "#confidential"]);
        let mut p = page("Diary", None);
        assert!(!r.hides_page(&p));
        p.properties.insert("private".into(), "true".into());
        assert!(r.hides_page(&p));
        assert!(r.hides_page(&page("secrets", None)));
        assert!(r.hides_page(&page("Secrets/Bank", None)));
        assert!(!r.hides_page(&page("Secrets2", None)));
        assert!(r.hides_page(&page("Salaries", Some("pages/HR/salaries.md"))));
        let mut t = page("Plan", None);
        t.tags.push("Confidential".into());
        assert!(r.hides_page(&t));
    }

    #[test]
    fn deny_all_hides_every_page_and_name() {
        let r = ReadExclusions::deny_all();
        assert!(r.hides_page(&page("Anything", None)));
        assert!(r.hides_name("Anything"));
    }

    #[test]
    fn private_false_is_visible() {
        let r = ReadExclusions::new(Vec::<String>::new());
        let mut p = page("Open", None);
        p.properties.insert("private".into(), "false".into());
        assert!(!r.hides_page(&p));
    }
}
