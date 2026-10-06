//! Graph mutations the app triggers outside the (future) block editor: today's journal, page
//! and asset deletion and the favorites update that goes with a deleted page.
//!
//! Everything here is blocking, GPUI-free and goes through the command queue of the session
//! (AGENTS.md rule 3: single writer): pages and assets are *recycled* to `logseq/.recycle/`
//! (BIT-SP-0002.R14/R15), never unlinked, and `logseq/config.edn` is edited as text with the
//! comment-preserving [`ConfigEditor`] and written by the queue's atomic, hash-checked writer.

use std::time::Duration;

use bitacora_config::{ConfigEditor, EffectiveConfig};
use bitacora_core::date::Date;
use bitacora_core::editor::{Cmd, Opened};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;
use bitacora_core::queue::{CommandQueue, FileEdit, QueueError, Request, Response, Source};
use bitacora_core::recycle::asset_path_from_link;

use crate::data::GraphHandle;

/// How long a config edit waits for exclusive access to the graph.
const LOCK_TIMEOUT: Duration = Duration::from_secs(5);

/// The config file, relative to the graph.
const CONFIG_PATH: &str = "logseq/config.edn";

/// Why a graph operation failed (the message is user-presentable).
#[derive(Debug, thiserror::Error)]
pub enum OpsError {
    /// The command queue refused or failed.
    #[error(transparent)]
    Queue(#[from] QueueError),
    /// The index could not be read.
    #[error("index: {0}")]
    Index(#[from] bitacora_index::Error),
    /// The page has no file to delete (it only exists as a placeholder or a virtual page).
    #[error("`{0}` has no file")]
    NoFile(String),
    /// The path is not an asset link.
    #[error("`{0}` is not an asset")]
    NotAnAsset(String),
    /// A file could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// File.
        path: String,
        /// Cause.
        source: std::io::Error,
    },
    /// An invalid graph-relative path.
    #[error("invalid graph path `{0}`")]
    BadPath(String),
    /// The config file could not be edited.
    #[error("config: {0}")]
    Config(#[from] bitacora_config::Error),
}

/// What [`delete_page`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageDeleted {
    /// Where the file went (`logseq/.recycle/...`).
    pub recycled_to: Option<String>,
    /// The page was in `:favorites` and was removed from there.
    pub favorite_removed: bool,
}

/// What [`delete_asset`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetOutcome {
    /// The file went to `logseq/.recycle/`.
    Recycled(String),
    /// Other blocks still reference the file, so it was kept.
    Kept {
        /// Blocks that still mention it.
        references: usize,
    },
}

/// Makes today's journal available for editing (loaded from its file, or a virtual page that is
/// only written once it has content). `None` when journals are disabled.
///
/// # Errors
/// [`OpsError::Queue`] when the queue is closed or refuses.
pub fn ensure_today(
    queue: &CommandQueue,
    config: &EffectiveConfig,
    today: Date,
) -> Result<Option<Opened>, OpsError> {
    let response = queue.execute(
        Source::Ui,
        Request::EnsureToday {
            today,
            cfg: Box::new(config.clone()),
        },
    )?;
    match response {
        Response::Journal(opened) => Ok(opened),
        _ => Err(QueueError::Invalid("unexpected response".into()).into()),
    }
}

/// Deletes the page titled `title`: its file moves to `logseq/.recycle/` (undoable through the
/// recycle folder), and a `:favorites` entry for it is removed from `logseq/config.edn`.
///
/// # Errors
/// [`OpsError::NoFile`] for pages without a file, plus queue, index and I/O errors.
pub fn delete_page(
    queue: &CommandQueue,
    handle: &GraphHandle,
    title: &str,
) -> Result<PageDeleted, OpsError> {
    let page = handle
        .reader
        .page_by_name(title)?
        .filter(|p| p.file_path.is_some())
        .ok_or_else(|| OpsError::NoFile(title.to_owned()))?;
    let rel = page.file_path.clone().unwrap_or_default();
    let key = PageKey::from_title(&page.original_name);
    if queue.snapshot(&key).is_none() {
        let path = GraphPath::new(&rel).map_err(|_| OpsError::BadPath(rel.clone()))?;
        let bytes =
            std::fs::read(path.to_fs_path(&handle.root)).map_err(|source| OpsError::Read {
                path: rel.clone(),
                source,
            })?;
        queue.execute(
            Source::Ui,
            Request::LoadPage {
                key: key.clone(),
                title: page.original_name.clone(),
                path: Some(path),
                bytes,
            },
        )?;
    }
    queue.run(Source::Ui, "Delete page", Cmd::DeletePage { page: key })?;
    // The recycle move happens on flush; do it now so the file is gone when we return.
    let report = queue.flush(Source::Ui)?;
    let recycled_to = report
        .recycled
        .iter()
        .find(|(from, _)| from.as_str() == rel)
        .map(|(_, to)| to.as_str().to_owned());
    let favorite_removed = favorites_remove(queue, handle, &page.original_name)?;
    Ok(PageDeleted {
        recycled_to,
        favorite_removed,
    })
}

/// Removes `title` from `:favorites` in `logseq/config.edn` (comments and layout untouched).
/// Returns whether the entry was there.
///
/// # Errors
/// Config parse errors, I/O errors and queue errors (including a stale config file).
pub fn favorites_remove(
    queue: &CommandQueue,
    handle: &GraphHandle,
    title: &str,
) -> Result<bool, OpsError> {
    let file = handle.root.join(CONFIG_PATH);
    let old = match std::fs::read(&file) {
        Ok(bytes) => bytes,
        // No config, no favorites.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(source) => {
            return Err(OpsError::Read {
                path: CONFIG_PATH.to_owned(),
                source,
            });
        }
    };
    let text = String::from_utf8_lossy(&old).into_owned();
    let mut editor = ConfigEditor::parse(&text)?;
    if !editor.favorites_remove(title)? {
        return Ok(false);
    }
    let mut lock = queue.acquire(LOCK_TIMEOUT)?;
    lock.apply(vec![FileEdit::Write {
        path: CONFIG_PATH.to_owned(),
        content: editor.into_text().into_bytes(),
        expected: Some(old),
    }])?;
    Ok(true)
}

/// Blocks (other than `except_block`) whose text mentions the asset `path`.
///
/// # Errors
/// [`OpsError::Index`].
pub fn asset_references(
    handle: &GraphHandle,
    path: &GraphPath,
    except_block: Option<&str>,
) -> Result<usize, OpsError> {
    let hits = handle.reader.blocks_mentioning(path.as_str(), 100)?;
    Ok(hits
        .iter()
        .filter(|b| except_block != Some(b.uuid.as_str()))
        .count())
}

/// The "delete asset" action: recycles the file behind the asset link `link` (`../assets/x.png`
/// or `assets/x.png`) unless a block other than `except_block` still references it. Deleting
/// *text* never reaches this function: files are only removed by an explicit action.
///
/// # Errors
/// [`OpsError::NotAnAsset`] for links outside `assets/`, plus queue and index errors.
pub fn delete_asset(
    queue: &CommandQueue,
    handle: &GraphHandle,
    link: &str,
    except_block: Option<&str>,
) -> Result<AssetOutcome, OpsError> {
    let path = asset_path_from_link(link).ok_or_else(|| OpsError::NotAnAsset(link.to_owned()))?;
    let references = asset_references(handle, &path, except_block)?;
    if references > 0 {
        return Ok(AssetOutcome::Kept { references });
    }
    queue.run(
        Source::Ui,
        "Delete asset",
        Cmd::DeleteAsset { path: path.clone() },
    )?;
    let report = queue.flush(Source::Ui)?;
    let to = report
        .recycled
        .iter()
        .find(|(from, _)| *from == path)
        .map_or_else(
            || {
                bitacora_core::recycle::recycle_path(&path)
                    .as_str()
                    .to_owned()
            },
            |(_, to)| to.as_str().to_owned(),
        );
    Ok(AssetOutcome::Recycled(to))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ViewSettings;
    use bitacora_runtime::{RuntimeConfig, Session};
    use std::sync::Arc;

    struct Fixture {
        graph: tempfile::TempDir,
        _data: tempfile::TempDir,
        session: Option<Session>,
        handle: GraphHandle,
    }

    fn fixture(files: &[(&str, &str)], config: &str) -> Fixture {
        let graph = tempfile::tempdir().expect("graph");
        let data = tempfile::tempdir().expect("data");
        let root = graph.path();
        std::fs::create_dir_all(root.join("logseq")).expect("logseq");
        std::fs::write(root.join("logseq/config.edn"), config).expect("config");
        for (path, content) in files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().expect("parent")).expect("dirs");
            std::fs::write(file, content).expect("write");
        }
        let mut cfg = RuntimeConfig::new(root);
        cfg.data_dir = Some(data.path().to_path_buf());
        cfg.global_config = Some(data.path().join("no-global.edn"));
        cfg.watch = None;
        cfg.debounce = None;
        let session = Session::open(cfg).expect("session");
        let handle = GraphHandle {
            reader: session.read_api(),
            root: session.root().to_path_buf(),
            settings: Arc::new(ViewSettings::from_config(session.config())),
        };
        Fixture {
            graph,
            _data: data,
            session: Some(session),
            handle,
        }
    }

    impl Fixture {
        fn queue(&self) -> CommandQueue {
            self.session.as_ref().expect("session").queue().clone()
        }

        fn root(&self) -> std::path::PathBuf {
            self.graph.path().canonicalize().expect("root")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Some(s) = self.session.take() {
                let _ = s.shutdown(Duration::from_secs(10));
            }
        }
    }

    #[test]
    fn deleting_a_page_recycles_the_file_and_drops_the_favorite() {
        let config =
            "{;; keep me\n :favorites [\"Doomed\" \"Keep\"]\n :feature/enable-journals? true}\n";
        let f = fixture(
            &[
                ("pages/Doomed.md", "- goodbye\n"),
                ("pages/Keep.md", "- stay\n"),
            ],
            config,
        );
        let done = delete_page(&f.queue(), &f.handle, "doomed").expect("delete");
        assert!(done.favorite_removed);
        assert_eq!(
            done.recycled_to.as_deref(),
            Some("logseq/.recycle/pages_Doomed.md")
        );
        let root = f.root();
        assert!(!root.join("pages/Doomed.md").exists());
        assert_eq!(
            std::fs::read_to_string(root.join("logseq/.recycle/pages_Doomed.md"))
                .expect("recycled"),
            "- goodbye\n"
        );
        assert!(root.join("pages/Keep.md").exists());
        // The config edit is surgical: comment and other entries survive.
        let cfg = std::fs::read_to_string(root.join("logseq/config.edn")).expect("config");
        assert!(cfg.contains(";; keep me"), "{cfg}");
        assert!(cfg.contains("\"Keep\"") && !cfg.contains("Doomed"), "{cfg}");
    }

    #[test]
    fn pages_without_a_file_cannot_be_deleted() {
        let f = fixture(&[("pages/A1.md", "- see [[Ghost]]\n")], "{}");
        assert!(matches!(
            delete_page(&f.queue(), &f.handle, "Ghost"),
            Err(OpsError::NoFile(_))
        ));
        assert!(matches!(
            delete_page(&f.queue(), &f.handle, "Nowhere"),
            Err(OpsError::NoFile(_))
        ));
    }

    #[test]
    fn an_unreferenced_asset_is_recycled_and_a_referenced_one_is_kept() {
        let f = fixture(
            &[
                ("assets/x.png", "png-bytes"),
                ("assets/y.png", "other"),
                ("pages/P.md", "- ![y](../assets/y.png)\n- text\n"),
            ],
            "{}",
        );
        let kept = delete_asset(&f.queue(), &f.handle, "../assets/y.png", None).expect("kept");
        assert_eq!(kept, AssetOutcome::Kept { references: 1 });
        assert!(f.root().join("assets/y.png").exists());
        let gone = delete_asset(&f.queue(), &f.handle, "../assets/x.png", None).expect("recycled");
        assert_eq!(
            gone,
            AssetOutcome::Recycled("logseq/.recycle/assets_x.png".into())
        );
        assert!(!f.root().join("assets/x.png").exists());
        assert!(f.root().join("logseq/.recycle/assets_x.png").exists());
        // The only referencing block does not count against itself.
        let block = f
            .handle
            .reader
            .blocks_mentioning("assets/y.png", 5)
            .expect("blocks")
            .pop()
            .expect("block");
        let again =
            delete_asset(&f.queue(), &f.handle, "assets/y.png", Some(&block.uuid)).expect("ok");
        assert!(matches!(again, AssetOutcome::Recycled(_)));
        assert!(matches!(
            delete_asset(&f.queue(), &f.handle, "https://x/y.png", None),
            Err(OpsError::NotAnAsset(_))
        ));
    }

    #[test]
    fn todays_journal_is_virtual_until_it_has_content() {
        let f = fixture(&[("pages/A1.md", "- a\n")], "{}");
        let today = Date::new(2025, 3, 9).expect("date");
        let config = f.session.as_ref().expect("session").config().clone();
        let opened = ensure_today(&f.queue(), &config, today)
            .expect("ensure")
            .expect("journals enabled");
        assert!(matches!(opened, Opened::Virtual(_)), "{opened:?}");
        assert!(f.queue().snapshot(opened.key()).is_some());
        let _ = f.queue().flush(Source::Ui).expect("flush");
        let journals = f.root().join("journals");
        let written = std::fs::read_dir(&journals).map_or(0, Iterator::count);
        assert_eq!(written, 0, "an untouched journal is never written");
        // Journals switched off: nothing to ensure.
        let off = EffectiveConfig::from_texts(None, Some("{:feature/enable-journals? false}"));
        assert!(
            ensure_today(&f.queue(), &off, today)
                .expect("ensure")
                .is_none()
        );
    }
}
