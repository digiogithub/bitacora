//! Turns debounced file-system events into [`WatchEvent`]s: inspects the file system, reads and
//! hashes content, applies ignore rules, the echo filter and the "same content as last seen"
//! no-op filter.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::echo::EchoFilter;
use crate::ignore::IgnoreRules;
use crate::{FileEvent, FileEventKind, WatchEvent, WatchNotice};

/// Receives watcher output. Called from watcher threads: keep it quick (e.g. send on a channel).
pub(crate) type Sink = Arc<dyn Fn(WatchEvent) + Send + Sync>;

/// A normalised unit of work derived from one or more raw events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Op {
    /// Something happened at this absolute path: inspect it.
    Touch(PathBuf),
    /// `from` was renamed to `to` (absolute paths).
    Rename { from: PathBuf, to: PathBuf },
}

pub(crate) struct Processor {
    root: PathBuf,
    ignore: IgnoreRules,
    echo: EchoFilter,
    sink: Sink,
    /// Last hash observed (or written by us) per relative path.
    known: HashMap<String, blake3::Hash>,
}

impl Processor {
    pub(crate) fn new(root: PathBuf, ignore: IgnoreRules, echo: EchoFilter, sink: Sink) -> Self {
        Self {
            root,
            ignore,
            echo,
            sink,
            known: HashMap::new(),
        }
    }

    fn rel(&self, abs: &Path) -> Option<String> {
        let rel = abs.strip_prefix(&self.root).ok()?;
        let parts: Vec<_> = rel.components().map(|c| c.as_os_str().to_str()).collect();
        if parts.is_empty() || parts.iter().any(Option::is_none) {
            return None;
        }
        Some(parts.into_iter().flatten().collect::<Vec<_>>().join("/"))
    }

    fn emit(&self, e: WatchEvent) {
        (self.sink)(e);
    }

    fn notice(&self, message: String) {
        self.emit(WatchEvent::Notice(WatchNotice {
            message,
            fallback_polling: false,
        }));
    }

    pub(crate) fn apply(&mut self, ops: Vec<Op>) {
        for op in ops {
            match op {
                Op::Touch(p) => self.touch(&p),
                Op::Rename { from, to } => self.rename(&from, &to),
            }
        }
    }

    /// Reports every file we already announced that no longer exists. Used by the periodic
    /// rescan: its own mtime baseline cannot know a file that appeared and vanished between two
    /// ticks (or whose removal the OS watcher lost), but `known` does.
    pub(crate) fn sweep_missing(&mut self) -> usize {
        let mut missing: Vec<String> = self
            .known
            .keys()
            .filter(|rel| {
                matches!(
                    fs::symlink_metadata(self.root.join(rel.as_str())),
                    Err(e) if e.kind() == io::ErrorKind::NotFound
                )
            })
            .cloned()
            .collect();
        missing.sort();
        let n = missing.len();
        for rel in missing {
            self.removed(&rel);
        }
        n
    }

    /// Number of files currently announced (diagnostics).
    pub(crate) fn known_len(&self) -> usize {
        self.known.len()
    }

    fn touch(&mut self, abs: &Path) {
        let Some(rel) = self.rel(abs) else { return };
        match fs::symlink_metadata(abs) {
            Ok(md) if md.is_dir() => {
                if !self.ignore.is_ignored(&rel) {
                    for f in walk_files(&self.root, abs, &self.ignore) {
                        self.touch_file(&f);
                    }
                }
            }
            Ok(md) if md.is_file() => {
                if self.ignore.accepts_file(&rel) {
                    self.touch_file(&rel);
                }
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => self.gone(&rel),
            Err(e) => self.notice(format!("cannot stat {rel}: {e}")),
        }
    }

    /// `rel` no longer exists: a file removal, or a removed directory.
    fn gone(&mut self, rel: &str) {
        if self.ignore.accepts_file(rel) {
            self.removed(rel);
        } else if !self.ignore.is_ignored(rel) {
            let prefix = format!("{rel}/");
            let mut files: Vec<String> = self
                .known
                .keys()
                .filter(|k| k.starts_with(&prefix))
                .cloned()
                .collect();
            files.sort();
            for f in files {
                self.removed(&f);
            }
        }
    }

    fn removed(&mut self, rel: &str) {
        self.known.remove(rel);
        if self.echo.is_removal_echo(rel) {
            return;
        }
        self.emit(WatchEvent::File(FileEvent {
            rel_path: rel.to_owned(),
            kind: FileEventKind::Removed,
            hash: None,
            bytes: None,
        }));
    }

    /// Reads `rel`; `None` when it disappeared (already reported) or could not be read.
    fn read(&mut self, rel: &str) -> Option<(Arc<[u8]>, blake3::Hash)> {
        match fs::read(self.root.join(rel)) {
            Ok(bytes) => {
                let hash = blake3::hash(&bytes);
                Some((Arc::from(bytes), hash))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.gone(rel);
                None
            }
            Err(e) => {
                self.notice(format!("cannot read {rel}: {e}"));
                None
            }
        }
    }

    fn touch_file(&mut self, rel: &str) {
        let Some((bytes, hash)) = self.read(rel) else {
            return;
        };
        let prev = self.known.insert(rel.to_owned(), hash);
        if self.echo.is_echo(rel, hash) || prev == Some(hash) {
            return;
        }
        // Logseq ignores a journal that holds only an empty bullet (an untouched day).
        if is_blank_journal(rel, &bytes) {
            return;
        }
        self.emit(WatchEvent::File(FileEvent {
            rel_path: rel.to_owned(),
            kind: FileEventKind::Upserted,
            hash: Some(hash),
            bytes: Some(bytes),
        }));
    }

    fn rename(&mut self, from: &Path, to: &Path) {
        let (Some(from_rel), Some(to_rel)) = (self.rel(from), self.rel(to)) else {
            self.touch(from);
            self.touch(to);
            return;
        };
        if fs::symlink_metadata(to).is_ok_and(|m| m.is_dir()) {
            self.touch(from);
            self.touch(to);
            return;
        }
        let from_ok = self.ignore.accepts_file(&from_rel);
        let to_ok = self.ignore.accepts_file(&to_rel);
        match (from_ok, to_ok) {
            (true, true) => {
                let Some((bytes, hash)) = self.read(&to_rel) else {
                    // Target vanished again: `read` reported its removal; report the source too.
                    self.removed(&from_rel);
                    return;
                };
                self.known.remove(&from_rel);
                self.known.insert(to_rel.clone(), hash);
                if self.echo.is_echo(&to_rel, hash) {
                    return;
                }
                self.emit(WatchEvent::File(FileEvent {
                    rel_path: to_rel,
                    kind: FileEventKind::Renamed { from: from_rel },
                    hash: Some(hash),
                    bytes: Some(bytes),
                }));
            }
            (false, true) => self.touch_file(&to_rel),
            (true, false) => self.removed(&from_rel),
            (false, false) => {}
        }
    }
}

/// Accepted files below `dir` (graph-relative paths), pruning ignored directories and skipping
/// symlinks.
pub(crate) fn walk_files(root: &Path, dir: &Path, ignore: &IgnoreRules) -> Vec<String> {
    let mut out = Vec::new();
    walk_into(root, dir, ignore, &mut |rel, _| out.push(rel));
    out.sort();
    out
}

/// Visits accepted files below `dir` with their metadata.
pub(crate) fn walk_into(
    root: &Path,
    dir: &Path,
    ignore: &IgnoreRules,
    visit: &mut dyn FnMut(String, &fs::Metadata),
) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let Some(rel) = rel.to_str().map(|s| s.replace('\\', "/")) else {
            continue;
        };
        let Ok(md) = fs::symlink_metadata(&path) else {
            continue;
        };
        if md.is_dir() {
            if !ignore.is_ignored(&rel) {
                walk_into(root, &path, ignore, visit);
            }
        } else if md.is_file() && ignore.accepts_file(&rel) {
            visit(rel, &md);
        }
    }
}

/// A journal file whose trimmed content is just `-`: an untouched day, not worth a re-parse.
/// (The "equals the default template" case needs the graph config and is not decided here.)
fn is_blank_journal(rel: &str, bytes: &[u8]) -> bool {
    rel.starts_with("journals/") && bytes.trim_ascii() == b"-"
}
