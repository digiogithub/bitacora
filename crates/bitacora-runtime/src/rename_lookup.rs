//! `RefLookup` over the SQLite index (BIT-US-0082): which files refer to a page, which pages live
//! below a namespace, where a page's file is. The cascade of a page rename verifies every answer
//! against the files, so a stale index can only cause a missed reference, never a wrong write.

use bitacora_core::editor::{PageFile, RefLookup};
use bitacora_core::graph_path::GraphPath;
use bitacora_index::{IndexReader, PageRow, RefFilters};

/// [`RefLookup`] backed by an [`IndexReader`].
#[derive(Debug, Clone)]
pub struct IndexRefLookup {
    reader: IndexReader,
}

impl IndexRefLookup {
    /// Wraps a reader (`Index::read_api()`).
    #[must_use]
    pub fn new(reader: IndexReader) -> Self {
        Self { reader }
    }
}

fn page_file(p: &PageRow) -> Option<PageFile> {
    let path = GraphPath::new(p.file_path.as_deref()?).ok()?;
    Some(PageFile {
        title: p.original_name.clone(),
        path,
    })
}

impl RefLookup for IndexRefLookup {
    fn files_referencing(&self, names: &[String]) -> Result<Vec<PageFile>, String> {
        let mut out: Vec<PageFile> = Vec::new();
        for name in names {
            let Some(page) = self.reader.page_by_name(name).map_err(|e| e.to_string())? else {
                continue;
            };
            // Unfiltered: the page's own `filters::` property must not hide references.
            let groups = self
                .reader
                .linked_references_with(page.id, &RefFilters::default())
                .map_err(|e| e.to_string())?;
            for g in groups {
                if let Some(f) = page_file(&g.page)
                    && !out.iter().any(|o| o.path == f.path)
                {
                    out.push(f);
                }
            }
        }
        Ok(out)
    }

    fn namespace_children(&self, title: &str) -> Result<Vec<PageFile>, String> {
        let Some(page) = self.reader.page_by_name(title).map_err(|e| e.to_string())? else {
            return Ok(Vec::new());
        };
        Ok(self
            .reader
            .namespace_tree(page.id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|n| n.depth > 0)
            .filter_map(|n| page_file(&n.page))
            .collect())
    }

    fn page_file(&self, title: &str) -> Result<Option<PageFile>, String> {
        Ok(self
            .reader
            .page_by_name(title)
            .map_err(|e| e.to_string())?
            .as_ref()
            .and_then(page_file))
    }
}
