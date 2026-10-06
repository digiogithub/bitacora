//! The write pipeline (BIT-US-0063..0066): debounce, atomic writes, self-check, pre-write hash
//! check, external deletes, `logseq/bak` backups and write-failure recovery.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use bitacora_core::editor::fsio::{atomic_write, cleanup_stale_tmp};
use bitacora_core::editor::{
    Cmd, FileStat, FileStore, FsStore, MemStore, Op, OpError, Refusal, TakeDisk, Workspace,
};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;
use bitacora_core::queue::{
    CommandQueue, Keep, QueueConfig, QueueEvent, Request, Response, Source,
};
use bitacora_core::write_queue::DebounceConfig;

fn key() -> PageKey {
    PageKey::from_title("p")
}

fn gp(s: &str) -> GraphPath {
    GraphPath::new(s).expect("path")
}

fn path() -> GraphPath {
    gp("pages/p.md")
}

fn open(src: &str) -> Workspace {
    let mut ws = Workspace::new();
    ws.load_page(key(), "p", Some(path()), src.as_bytes());
    ws
}

fn first(ws: &Workspace) -> bitacora_core::editor::BlockId {
    ws.page(&key()).expect("page").dfs()[0]
}

fn set_first(ws: &mut Workspace, text: &str) {
    let id = first(ws);
    ws.run(
        "edit",
        &Cmd::SetText {
            id,
            text: text.to_owned(),
        },
    )
    .expect("edit");
}

fn fs_graph(src: &str) -> (tempfile::TempDir, FsStore, Workspace) {
    let dir = tempfile::tempdir().expect("tmp");
    std::fs::create_dir_all(dir.path().join("pages")).expect("mkdir");
    std::fs::write(dir.path().join("pages/p.md"), src).expect("seed");
    let store = FsStore::new(dir.path());
    (dir, store, open(src))
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).expect("read")
}

// ---- BIT-US-0064: atomic writer -----------------------------------------------------------

#[test]
fn atomic_write_creates_replaces_and_leaves_no_temp_file() {
    let dir = tempfile::tempdir().expect("tmp");
    let target = dir.path().join("a/b/c.md");
    atomic_write(&target, b"one").expect("create (parents too)");
    atomic_write(&target, b"two").expect("replace");
    assert_eq!(std::fs::read(&target).expect("r"), b"two");
    let names: Vec<_> = std::fs::read_dir(target.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["c.md"]);
}

#[cfg(unix)]
#[test]
fn atomic_write_keeps_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tmp");
    let target = dir.path().join("x.md");
    std::fs::write(&target, "old").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
    atomic_write(&target, b"new").unwrap();
    let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o640);
}

#[test]
fn stale_temp_files_are_cleaned_but_user_files_are_not() {
    let dir = tempfile::tempdir().expect("tmp");
    std::fs::create_dir_all(dir.path().join("pages")).unwrap();
    std::fs::write(dir.path().join("pages/.p.md.bitacora-tmp"), "partial").unwrap();
    std::fs::write(dir.path().join(".root.md.bitacora-tmp"), "partial").unwrap();
    std::fs::write(dir.path().join("pages/p.md"), "- keep\n").unwrap();
    std::fs::write(dir.path().join("pages/.hidden"), "keep").unwrap();
    assert_eq!(cleanup_stale_tmp(dir.path()), 2);
    assert!(dir.path().join("pages/p.md").exists());
    assert!(dir.path().join("pages/.hidden").exists());
    assert!(!dir.path().join("pages/.p.md.bitacora-tmp").exists());
}

#[test]
fn queue_start_cleans_stale_temp_files() {
    let dir = tempfile::tempdir().expect("tmp");
    std::fs::write(dir.path().join(".x.md.bitacora-tmp"), "partial").unwrap();
    let (q, join) = CommandQueue::spawn(
        Workspace::new(),
        Box::new(FsStore::new(dir.path())),
        QueueConfig::default(),
    );
    // A round trip proves the consumer finished its start-up work.
    q.flush(Source::Ui).expect("flush");
    assert!(!dir.path().join(".x.md.bitacora-tmp").exists());
    drop(join);
}

#[test]
fn text_that_would_become_a_bullet_is_refused() {
    let mut ws = open("- a\n- b\n");
    let id = first(&ws);
    let err = ws
        .run(
            "edit",
            &Cmd::SetText {
                id,
                text: "x\n- y".into(),
            },
        )
        .expect_err("refused");
    assert!(
        format!("{err:?}").contains("Unrepresentable"),
        "unexpected error {err:?}"
    );
    assert!(
        bitacora_core::editor::plan(
            &ws,
            &Cmd::InsertChild {
                page: key(),
                parent: None,
                text: "p\n  - q".into(),
            }
        )
        .is_err_and(|r| r == Refusal::Unrepresentable)
    );
    // Nothing changed.
    assert_eq!(ws.page(&key()).unwrap().serialize(), b"- a\n- b\n");
    // Raw ops are checked too.
    let mut op = Op::SetText {
        id,
        before: "a".into(),
        after: "z\n- y".into(),
    };
    assert_eq!(op.apply(&mut ws), Err(OpError::Unrepresentable));
}

#[test]
fn indented_continuation_lines_and_fences_stay_one_block() {
    let mut ws = open("- a\n");
    set_first(&mut ws, "a\nsecond line\n```\n- not a bullet\n```");
    let bytes = ws.page(&key()).unwrap().serialize();
    let reparsed = bitacora_markdown::Document::parse(bytes);
    assert_eq!(reparsed.blocks.len(), 1);
}

#[test]
fn self_check_refuses_to_write_a_page_that_reparses_differently() {
    // Build the impossible state by bypassing the op layer (a hypothetical serializer bug).
    let mut ws = open("- a\n- b\n");
    let id = first(&ws);
    let mut page = ws.remove_page(&key()).unwrap();
    page.blocks.get_mut(&id).unwrap().text = "x\n- y".into();
    page.dirty = true;
    ws.insert_page(page);
    let mut store = MemStore::default();
    store.files.insert(path(), b"- a\n- b\n".to_vec());
    let report = ws.flush(&mut store);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.unwritten, vec![path()]);
    assert_eq!(store.files[&path()], b"- a\n- b\n", "nothing written");
    assert!(ws.page(&key()).unwrap().needs_write(), "still dirty");
}

#[test]
fn edited_files_keep_bom_crlf_and_missing_final_newline() {
    let src = "\u{feff}title:: t\r\n\r\n- a\r\n  - b\r\n- c";
    let mut ws = open(src);
    set_first(&mut ws, "A");
    let out = String::from_utf8(ws.page(&key()).unwrap().serialize()).unwrap();
    assert_eq!(out, "\u{feff}title:: t\r\n\r\n- A\r\n  - b\r\n- c");
    // Editing the last block keeps the missing final newline too.
    let ids = ws.page(&key()).unwrap().dfs();
    ws.run(
        "edit",
        &Cmd::SetText {
            id: ids[2],
            text: "C".into(),
        },
    )
    .unwrap();
    let out = String::from_utf8(ws.page(&key()).unwrap().serialize()).unwrap();
    assert_eq!(out, "\u{feff}title:: t\r\n\r\n- A\r\n  - b\r\n- C");
}

#[test]
fn new_pages_are_utf8_lf_without_bom() {
    let mut ws = Workspace::new();
    let mut op = Op::CreatePage {
        page: key(),
        title: "p".into(),
        path: Some(path()),
        restored: None,
    };
    op.apply(&mut ws).expect("create");
    ws.run(
        "insert",
        &Cmd::InsertChild {
            page: key(),
            parent: None,
            text: "ä".into(),
        },
    )
    .expect("insert");
    let mut store = MemStore::default();
    let r = ws.flush(&mut store);
    assert!(r.is_complete());
    assert_eq!(store.files[&path()], "- ä\n".as_bytes());
}

// ---- BIT-US-0065: pre-write check ---------------------------------------------------------

#[test]
fn unchanged_file_is_written() {
    let (dir, mut store, mut ws) = fs_graph("- a\n");
    set_first(&mut ws, "b");
    let r = ws.flush(&mut store);
    assert!(r.is_complete());
    assert_eq!(read(dir.path(), "pages/p.md"), "- b\n");
    // The snapshot was rebased: the next edit writes again without conflict.
    set_first(&mut ws, "c");
    assert!(ws.flush(&mut store).is_complete());
    assert_eq!(read(dir.path(), "pages/p.md"), "- c\n");
}

#[test]
fn touched_only_file_is_still_written() {
    let (dir, mut store, mut ws) = fs_graph("- a\n");
    set_first(&mut ws, "b");
    assert!(ws.flush(&mut store).is_complete());
    // Same bytes, new mtime (`touch`).
    let p = dir.path().join("pages/p.md");
    let content = std::fs::read(&p).unwrap();
    std::fs::write(&p, &content).unwrap();
    let f = std::fs::File::options().write(true).open(&p).unwrap();
    f.set_modified(SystemTime::now() + Duration::from_secs(5))
        .unwrap();
    set_first(&mut ws, "c");
    let r = ws.flush(&mut store);
    assert!(r.conflicts.is_empty(), "{r:?}");
    assert_eq!(read(dir.path(), "pages/p.md"), "- c\n");
}

#[test]
fn externally_modified_file_is_never_overwritten() {
    let (dir, mut store, mut ws) = fs_graph("- a\n");
    set_first(&mut ws, "mine");
    // Same length, other content and a different mtime: must be caught by the hash.
    let p = dir.path().join("pages/p.md");
    std::fs::write(&p, "- z\n").unwrap();
    let f = std::fs::File::options().write(true).open(&p).unwrap();
    f.set_modified(SystemTime::now() + Duration::from_secs(5))
        .unwrap();
    let r = ws.flush(&mut store);
    assert_eq!(r.conflicts, vec![key()]);
    assert!(r.written.is_empty());
    assert_eq!(
        read(dir.path(), "pages/p.md"),
        "- z\n",
        "external bytes intact"
    );
    assert!(ws.page(&key()).unwrap().needs_write());
}

#[test]
fn deleted_file_of_a_dirty_page_is_recreated_with_parent_dirs() {
    let (dir, mut store, mut ws) = fs_graph("- a\n");
    std::fs::remove_dir_all(dir.path().join("pages")).unwrap();
    set_first(&mut ws, "mine");
    let r = ws.flush(&mut store);
    assert_eq!(r.recreated, vec![key()]);
    assert!(r.is_complete());
    assert_eq!(read(dir.path(), "pages/p.md"), "- mine\n");
}

#[test]
fn deleted_file_of_a_clean_page_drops_the_page() {
    let (dir, store, mut ws) = fs_graph("- a\n");
    std::fs::remove_file(dir.path().join("pages/p.md")).unwrap();
    let gone = ws.drop_missing(&store, &[path()]);
    assert_eq!(gone, vec![key()]);
    assert!(ws.page(&key()).is_none());
}

#[test]
fn drop_missing_keeps_dirty_pages_and_existing_files() {
    let (dir, store, mut ws) = fs_graph("- a\n");
    assert!(ws.drop_missing(&store, &[path()]).is_empty());
    set_first(&mut ws, "mine");
    std::fs::remove_file(dir.path().join("pages/p.md")).unwrap();
    assert!(ws.drop_missing(&store, &[path()]).is_empty());
    assert!(ws.page(&key()).is_some());
}

#[test]
fn file_replaced_by_a_directory_is_an_error_not_a_write() {
    let (dir, mut store, mut ws) = fs_graph("- a\n");
    std::fs::remove_file(dir.path().join("pages/p.md")).unwrap();
    std::fs::create_dir(dir.path().join("pages/p.md")).unwrap();
    set_first(&mut ws, "mine");
    let r = ws.flush(&mut store);
    assert_eq!(r.failed.len(), 1, "{r:?}");
    assert!(r.written.is_empty());
    assert!(dir.path().join("pages/p.md").is_dir());
    assert!(ws.page(&key()).unwrap().needs_write());
}

#[test]
fn page_created_externally_while_we_had_none_is_a_conflict() {
    let mut ws = Workspace::new();
    let mut op = Op::CreatePage {
        page: key(),
        title: "p".into(),
        path: Some(path()),
        restored: None,
    };
    op.apply(&mut ws).unwrap();
    ws.run(
        "insert",
        &Cmd::InsertChild {
            page: key(),
            parent: None,
            text: "x".into(),
        },
    )
    .unwrap();
    let mut store = MemStore::default();
    store.files.insert(path(), b"- theirs\n".to_vec());
    let r = ws.flush(&mut store);
    assert_eq!(r.conflicts, vec![key()]);
    assert_eq!(store.files[&path()], b"- theirs\n");
}

// ---- BIT-US-0066: backups and failures ---------------------------------------------------

fn bak_files(store: &MemStore) -> Vec<String> {
    store
        .files
        .keys()
        .filter(|p| p.as_str().starts_with("logseq/bak/"))
        .map(|p| p.as_str().to_owned())
        .collect()
}

#[test]
fn a_write_that_removes_text_backs_up_the_old_content() {
    let mut ws = open("- a\n- b\n");
    let mut store = MemStore::default();
    store.files.insert(path(), b"- a\n- b\n".to_vec());
    let ids = ws.page(&key()).unwrap().dfs();
    ws.run("del", &Cmd::DeleteBlocks { ids: vec![ids[1]] })
        .unwrap();
    let t = SystemTime::UNIX_EPOCH + Duration::from_millis(1_763_112_612_345);
    let r = ws.flush_at(&mut store, t, None);
    assert!(r.is_complete());
    let baks = bak_files(&store);
    assert_eq!(
        baks,
        vec!["logseq/bak/pages/p/2025-11-14T09_30_12.345Z.Desktop.md"]
    );
    assert_eq!(store.files[&gp(&baks[0])], b"- a\n- b\n");
    assert_eq!(r.backups.len(), 1);
}

#[test]
fn additive_writes_make_no_backup() {
    let mut ws = open("- a\n");
    let mut store = MemStore::default();
    store.files.insert(path(), b"- a\n".to_vec());
    set_first(&mut ws, "a\nmore");
    assert!(ws.flush(&mut store).is_complete());
    assert!(bak_files(&store).is_empty());
}

#[test]
fn backups_keep_only_the_newest_six() {
    let mut store = MemStore::default();
    let f = path();
    for i in 0..10u64 {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + i);
        bitacora_core::editor::backup::write_backup(&mut store, &f, format!("v{i}").as_bytes(), t)
            .unwrap();
    }
    let baks = bak_files(&store);
    assert_eq!(baks.len(), 6);
    let newest = &store.files[&gp(baks.last().unwrap())];
    assert_eq!(newest, b"v9");
    assert_eq!(store.files[&gp(&baks[0])], b"v4");
}

#[test]
fn keep_mine_backs_up_the_external_version_then_overwrites() {
    let mut ws = open("- a\n");
    let mut store = MemStore::default();
    store.files.insert(path(), b"- a\n".to_vec());
    set_first(&mut ws, "mine");
    store.files.insert(path(), b"- external\n".to_vec());
    assert_eq!(ws.flush(&mut store).conflicts, vec![key()]);
    let r = ws.resolve_keep_mine(&key(), &mut store, SystemTime::now());
    assert!(r.is_complete(), "{r:?}");
    assert_eq!(store.files[&path()], b"- mine\n");
    let baks = bak_files(&store);
    assert_eq!(baks.len(), 1);
    assert_eq!(store.files[&gp(&baks[0])], b"- external\n");
    assert!(!ws.page(&key()).unwrap().needs_write());
}

#[test]
fn take_disk_backs_up_unsaved_edits_then_reloads() {
    let mut ws = open("- a\n");
    let mut store = MemStore::default();
    store.files.insert(path(), b"- a\n".to_vec());
    set_first(&mut ws, "unsaved");
    store.files.insert(path(), b"- external\n".to_vec());
    let mut report = bitacora_core::editor::FlushReport::default();
    let t = ws.take_disk(&key(), &mut store, SystemTime::now(), &mut report);
    assert_eq!(t, TakeDisk::Reloaded);
    let baks = bak_files(&store);
    assert_eq!(baks.len(), 1);
    assert_eq!(store.files[&gp(&baks[0])], b"- unsaved\n");
    let p = ws.page(&key()).unwrap();
    assert!(!p.needs_write());
    assert_eq!(p.block(p.dfs()[0]).unwrap().text, "external");
}

/// A store whose writes (outside `logseq/bak`) fail while `broken` is set.
struct Flaky {
    inner: MemStore,
    broken: Arc<AtomicBool>,
}

impl FileStore for Flaky {
    fn read(&self, p: &GraphPath) -> io::Result<Option<Vec<u8>>> {
        self.inner.read(p)
    }
    fn stat(&self, p: &GraphPath) -> io::Result<Option<FileStat>> {
        self.inner.stat(p)
    }
    fn write(&mut self, p: &GraphPath, b: &[u8]) -> io::Result<()> {
        if self.broken.load(Ordering::SeqCst) && !p.as_str().starts_with("logseq/bak/") {
            return Err(io::Error::other("No space left on device"));
        }
        self.inner.write(p, b)
    }
    fn remove(&mut self, p: &GraphPath) -> io::Result<()> {
        self.inner.remove(p)
    }
    fn list(&self, d: &GraphPath) -> io::Result<Vec<String>> {
        self.inner.list(d)
    }
}

#[test]
fn write_failure_keeps_the_page_dirty_saves_a_backup_and_reports_the_file() {
    let broken = Arc::new(AtomicBool::new(true));
    let mut store = Flaky {
        inner: MemStore::default(),
        broken: broken.clone(),
    };
    store.inner.files.insert(path(), b"- a\n".to_vec());
    let mut ws = open("- a\n");
    set_first(&mut ws, "new");
    let r = ws.flush(&mut store);
    assert_eq!(r.failed.len(), 1);
    assert!(r.failed[0].1.contains("No space"));
    assert_eq!(r.unwritten, vec![path()]);
    assert!(ws.page(&key()).unwrap().needs_write());
    let baks = bak_files(&store.inner);
    assert_eq!(baks.len(), 1);
    assert_eq!(store.inner.files[&gp(&baks[0])], b"- new\n");
    assert_eq!(store.inner.files[&path()], b"- a\n", "old file untouched");
    // Recovery: the next flush succeeds.
    broken.store(false, Ordering::SeqCst);
    assert!(ws.flush(&mut store).is_complete());
    assert_eq!(store.inner.files[&path()], b"- new\n");
}

#[test]
fn multi_file_failures_list_every_unwritten_file() {
    let broken = Arc::new(AtomicBool::new(true));
    let mut store = Flaky {
        inner: MemStore::default(),
        broken,
    };
    let mut ws = open("- a\n");
    ws.load_page(
        PageKey::from_title("q"),
        "q",
        Some(gp("pages/q.md")),
        b"- q\n",
    );
    store.inner.files.insert(path(), b"- a\n".to_vec());
    store
        .inner
        .files
        .insert(gp("pages/q.md"), b"- q\n".to_vec());
    set_first(&mut ws, "x");
    let qid = ws.page(&PageKey::from_title("q")).unwrap().dfs()[0];
    ws.run(
        "edit",
        &Cmd::SetText {
            id: qid,
            text: "y".into(),
        },
    )
    .unwrap();
    let r = ws.flush(&mut store);
    assert_eq!(r.unwritten.len(), 2);
}

// ---- BIT-US-0063: debounced queue ---------------------------------------------------------

fn fast() -> QueueConfig {
    QueueConfig {
        debounce: Some(DebounceConfig {
            debounce: Duration::from_millis(40),
            max_delay: Duration::from_millis(200),
        }),
        ..QueueConfig::default()
    }
}

fn wait_for(mut f: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn edit(q: &CommandQueue, text: &str) {
    let snap = q.snapshot(&key()).expect("snap");
    q.run(
        Source::Ui,
        "edit",
        Cmd::SetText {
            id: snap.blocks[0].id,
            text: text.into(),
        },
    )
    .expect("run");
}

#[test]
fn edits_reach_disk_after_the_debounce_without_an_explicit_flush() {
    let (dir, store, ws) = fs_graph("- a\n");
    let events: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = events.clone();
    let cfg = QueueConfig {
        observers: vec![Box::new(move |e| {
            if let QueueEvent::Flushed(r) = e {
                sink.lock()
                    .unwrap()
                    .push(format!("flushed:{}", r.written.len()));
            }
        })],
        ..fast()
    };
    let (q, join) = CommandQueue::spawn(ws, Box::new(store), cfg);
    edit(&q, "one");
    assert!(
        q.snapshot(&key()).unwrap().dirty,
        "not written synchronously"
    );
    assert!(wait_for(|| read(dir.path(), "pages/p.md") == "- one\n"));
    assert!(wait_for(|| !q.snapshot(&key()).unwrap().dirty));
    // Rebased: a second edit is written without conflict.
    edit(&q, "two");
    assert!(wait_for(|| read(dir.path(), "pages/p.md") == "- two\n"));
    assert!(wait_for(|| events.lock().unwrap().len() == 2));
    assert_eq!(*events.lock().unwrap(), vec!["flushed:1", "flushed:1"]);
    drop(join);
}

#[test]
fn rapid_edits_coalesce_into_one_write() {
    let (dir, store, ws) = fs_graph("- a\n");
    let count = Arc::new(Mutex::new(0usize));
    let c = count.clone();
    let cfg = QueueConfig {
        observers: vec![Box::new(move |e| {
            if let QueueEvent::Flushed(r) = e {
                *c.lock().unwrap() += r.written.len();
            }
        })],
        debounce: Some(DebounceConfig {
            debounce: Duration::from_millis(150),
            max_delay: Duration::from_secs(2),
        }),
        ..QueueConfig::default()
    };
    let (q, join) = CommandQueue::spawn(ws, Box::new(store), cfg);
    for i in 0..10 {
        edit(&q, &format!("v{i}"));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(wait_for(|| read(dir.path(), "pages/p.md") == "- v9\n"));
    assert!(wait_for(|| *count.lock().unwrap() >= 1));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(*count.lock().unwrap(), 1);
    drop(join);
}

#[test]
fn shutdown_flushes_pending_edits() {
    let (dir, store, ws) = fs_graph("- a\n");
    let (q, join) = CommandQueue::spawn(ws, Box::new(store), QueueConfig::default());
    edit(&q, "last words");
    let (_ws, report) = join.shutdown_with_report().expect("shutdown");
    assert!(report.is_complete());
    assert_eq!(read(dir.path(), "pages/p.md"), "- last words\n");
}

#[test]
fn shutdown_reports_pages_that_could_not_be_saved() {
    let (dir, store, ws) = fs_graph("- a\n");
    let (q, join) = CommandQueue::spawn(ws, Box::new(store), QueueConfig::default());
    edit(&q, "mine");
    std::fs::write(dir.path().join("pages/p.md"), "- external, longer\n").unwrap();
    let (_ws, report) = join.shutdown_with_report().expect("shutdown");
    assert_eq!(report.conflicts, vec![key()]);
    assert_eq!(read(dir.path(), "pages/p.md"), "- external, longer\n");
}

#[test]
fn conflicts_are_reported_once_and_resolved_with_keep_mine() {
    let (dir, store, ws) = fs_graph("- a\n");
    let conflicts = Arc::new(Mutex::new(0usize));
    let c = conflicts.clone();
    let cfg = QueueConfig {
        observers: vec![Box::new(move |e| {
            if let QueueEvent::Conflict(_) = e {
                *c.lock().unwrap() += 1;
            }
        })],
        ..fast()
    };
    let (q, join) = CommandQueue::spawn(ws, Box::new(store), cfg);
    std::fs::write(dir.path().join("pages/p.md"), "- external!\n").unwrap();
    edit(&q, "mine");
    assert!(wait_for(|| *conflicts.lock().unwrap() == 1));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        *conflicts.lock().unwrap(),
        1,
        "parked, not retried in a loop"
    );
    assert_eq!(read(dir.path(), "pages/p.md"), "- external!\n");
    let r = q
        .execute(
            Source::Ui,
            Request::Resolve {
                key: key(),
                keep: Keep::Mine,
            },
        )
        .expect("resolve");
    assert!(matches!(r, Response::Resolved(rep, None) if rep.is_complete()));
    assert_eq!(read(dir.path(), "pages/p.md"), "- mine\n");
    let bak = dir.path().join("logseq/bak/pages/p");
    let files: Vec<_> = std::fs::read_dir(bak).unwrap().collect();
    assert_eq!(files.len(), 1);
    drop(join);
}

#[test]
fn failed_writes_retry_with_backoff_until_they_succeed() {
    let broken = Arc::new(AtomicBool::new(true));
    let mut store = Flaky {
        inner: MemStore::default(),
        broken: broken.clone(),
    };
    store.inner.files.insert(path(), b"- a\n".to_vec());
    let failures = Arc::new(Mutex::new(0usize));
    let f = failures.clone();
    let cfg = QueueConfig {
        observers: vec![Box::new(move |e| {
            if let QueueEvent::WriteFailed { .. } = e {
                *f.lock().unwrap() += 1;
            }
        })],
        ..fast()
    };
    let (q, join) = CommandQueue::spawn(open("- a\n"), Box::new(store), cfg);
    edit(&q, "retry me");
    assert!(wait_for(|| *failures.lock().unwrap() >= 1));
    assert!(q.snapshot(&key()).unwrap().dirty);
    broken.store(false, Ordering::SeqCst);
    // First retry fires after the 1 s backoff.
    for _ in 0..300 {
        if !q.snapshot(&key()).unwrap().dirty {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!q.snapshot(&key()).unwrap().dirty, "retry did not happen");
    drop(join);
}
