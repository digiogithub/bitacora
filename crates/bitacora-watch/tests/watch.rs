//! Integration tests with real temp directories. Waits use channel timeouts, never fixed sleeps
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! as synchronisation (sleeps appear only to separate writes in time).

use std::fs;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::thread::sleep;
use std::time::{Duration, Instant};

use bitacora_watch::{
    EchoFilter, FileEvent, FileEventKind, GraphWatcher, IgnoreRules, WatchConfig, WatchEvent,
};

const WAIT: Duration = Duration::from_secs(15);

struct Fixture {
    dir: tempfile::TempDir,
    rx: Receiver<WatchEvent>,
    echo: EchoFilter,
    _w: GraphWatcher,
}

fn start_with(cfg: WatchConfig, ignore: IgnoreRules, setup: impl FnOnce(&Path)) -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    for d in ["pages", "journals", "logseq"] {
        fs::create_dir_all(dir.path().join(d)).expect("mkdir");
    }
    setup(dir.path());
    let (tx, rx) = mpsc::channel();
    let echo = EchoFilter::new();
    let cfg = WatchConfig {
        trace_events: true,
        // Short safety rescan so a genuinely lost OS event cannot hang a test (the raw-event
        // trace still shows what the OS delivered).
        // `WatchConfig::default()` is already `Some(30 s)` on macOS, so a plain `.or(..)` would keep
        // 30 s there and outlast WAIT: cap whatever the test asked for at 2 s.
        safety_scan_interval: Some(
            cfg.safety_scan_interval
                .map_or(Duration::from_secs(2), |i| i.min(Duration::from_secs(2))),
        ),
        ..cfg
    };
    let w = GraphWatcher::start(dir.path(), ignore, echo.clone(), cfg, move |e| {
        eprintln!("[watch out] {e:?}");
        let _ = tx.send(e);
    })
    .expect("start");
    // Let the OS watch settle.
    sleep(Duration::from_millis(200));
    Fixture {
        dir,
        rx,
        echo,
        _w: w,
    }
}

fn start() -> Fixture {
    start_with(WatchConfig::default(), IgnoreRules::new(), |_| {})
}

impl Fixture {
    fn path(&self, rel: &str) -> std::path::PathBuf {
        self.dir.path().join(rel)
    }

    /// Next file event matching `pred`, skipping others, within `WAIT`.
    fn wait_file(&self, pred: impl Fn(&FileEvent) -> bool) -> FileEvent {
        let end = Instant::now() + WAIT;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(WatchEvent::File(f)) if pred(&f) => return f,
                Ok(_) => {}
                Err(_) => panic!(
                    "timed out waiting for event (see captured [watch raw]/[watch out] lines)"
                ),
            }
        }
    }

    /// Events received during the next `window`.
    fn drain_for(&self, window: Duration) -> Vec<WatchEvent> {
        let end = Instant::now() + window;
        let mut out = Vec::new();
        while let Ok(e) = self
            .rx
            .recv_timeout(end.saturating_duration_since(Instant::now()))
        {
            out.push(e);
        }
        out
    }

    /// Writes a sentinel and returns every file event seen before it (sentinel excluded).
    fn events_before_sentinel(&self) -> Vec<FileEvent> {
        sleep(Duration::from_millis(300));
        fs::write(self.path("pages/zz-sentinel.md"), "- s\n").expect("write");
        let mut out = Vec::new();
        let end = Instant::now() + WAIT;
        loop {
            match self
                .rx
                .recv_timeout(end.saturating_duration_since(Instant::now()))
            {
                Ok(WatchEvent::File(f)) if f.rel_path == "pages/zz-sentinel.md" => return out,
                Ok(WatchEvent::File(f)) => out.push(f),
                Ok(_) => {}
                Err(_) => panic!("sentinel never arrived"),
            }
        }
    }
}

fn own_write(fx: &Fixture, rel: &str, content: &str) {
    // Same protocol as core's writer: register, temp file, rename.
    fx.echo.record_bytes(rel, content.as_bytes());
    let dest = fx.path(rel);
    let name = dest.file_name().and_then(|n| n.to_str()).expect("name");
    let tmp = dest.with_file_name(format!(".{name}.bitacora-tmp"));
    fs::write(&tmp, content).expect("write tmp");
    fs::rename(&tmp, &dest).expect("rename");
}

#[test]
fn create_modify_delete() {
    let fx = start();
    fs::write(fx.path("pages/a.md"), "- one\n").expect("write");
    let ev = fx.wait_file(|f| f.rel_path == "pages/a.md");
    assert_eq!(ev.kind, FileEventKind::Upserted);
    assert_eq!(ev.hash, Some(blake3::hash(b"- one\n")));
    assert_eq!(ev.bytes.as_deref(), Some(&b"- one\n"[..]));

    fs::write(fx.path("pages/a.md"), "- two\n").expect("write");
    let ev = fx.wait_file(|f| f.rel_path == "pages/a.md");
    assert_eq!(ev.hash, Some(blake3::hash(b"- two\n")));

    fs::remove_file(fx.path("pages/a.md")).expect("rm");
    let ev = fx.wait_file(|f| f.rel_path == "pages/a.md");
    assert_eq!(ev.kind, FileEventKind::Removed);
    assert!(ev.hash.is_none());
}

#[test]
fn rename_is_detected() {
    let fx = start_with(WatchConfig::default(), IgnoreRules::new(), |root| {
        fs::write(root.join("pages/old.md"), "- x\n").expect("write");
    });
    fs::rename(fx.path("pages/old.md"), fx.path("pages/new.md")).expect("rename");
    let mut seen_new = false;
    let mut seen_old_removed = false;
    let end = Instant::now() + WAIT;
    while !seen_new {
        let left = end.saturating_duration_since(Instant::now());
        match fx.rx.recv_timeout(left) {
            Ok(WatchEvent::File(f)) => match (&f.kind, f.rel_path.as_str()) {
                (FileEventKind::Renamed { from }, "pages/new.md") => {
                    assert_eq!(from, "pages/old.md");
                    seen_new = true;
                }
                (FileEventKind::Upserted, "pages/new.md") => seen_new = true,
                (FileEventKind::Removed, "pages/old.md") => seen_old_removed = true,
                _ => {}
            },
            Ok(_) => {}
            Err(_) => panic!("rename not detected"),
        }
    }
    // Either a single Renamed event, or Removed(old) + Upserted(new) when the OS cannot pair them.
    let _ = seen_old_removed;
}

#[test]
fn ignored_paths_are_silent() {
    let fx = start_with(
        WatchConfig::default(),
        IgnoreRules::new().with_hidden(|p| p.starts_with("archived")),
        |root| {
            for d in [
                ".git",
                "logseq/bak/pages",
                "logseq/.recycle",
                "node_modules/x",
                "archived",
            ] {
                fs::create_dir_all(root.join(d)).expect("mkdir");
            }
        },
    );
    for rel in [
        ".git/a.md",
        "logseq/bak/pages/a.md",
        "logseq/.recycle/a.md",
        "node_modules/x/a.md",
        "archived/a.md",
        "pages/.hidden.md",
        "pages/a.md.bitacora-tmp",
        "pages/image.png",
    ] {
        fs::write(fx.path(rel), "- x\n").expect("write");
    }
    fs::write(fx.path("logseq/config.edn"), "{}\n").expect("write");
    let before = fx.events_before_sentinel();
    let paths: Vec<_> = before.iter().map(|e| e.rel_path.as_str()).collect();
    assert_eq!(paths, ["logseq/config.edn"], "got {paths:?}");
}

#[test]
fn burst_is_coalesced() {
    let fx = start();
    for i in 0..3 {
        fs::write(fx.path("pages/b.md"), format!("- v{i}\n")).expect("write");
        sleep(Duration::from_millis(15));
    }
    // FSEvents (and a loaded CI disk) may deliver the burst in pieces, so an intermediate state
    // can be reported first; the final content must always arrive, and nothing after it.
    let last_hash = blake3::hash(b"- v2\n");
    let last = fx.wait_file(|f| f.rel_path == "pages/b.md" && f.hash == Some(last_hash));
    assert_eq!(last.bytes.as_deref(), Some(&b"- v2\n"[..]));
    let rest: Vec<_> = fx
        .events_before_sentinel()
        .into_iter()
        .filter(|e| e.rel_path == "pages/b.md")
        .collect();
    assert!(rest.is_empty(), "extra events: {rest:?}");
}

#[test]
fn own_writes_are_suppressed_and_external_after_is_detected() {
    let fx = start();
    for i in 0..100 {
        own_write(&fx, "pages/c.md", &format!("- mine {i}\n"));
    }
    let leaked: Vec<_> = fx
        .events_before_sentinel()
        .into_iter()
        .filter(|e| e.rel_path.contains("c.md"))
        .collect();
    assert!(leaked.is_empty(), "own writes leaked: {leaked:?}");

    // External write right after our own write, different bytes: must be reported.
    fs::write(fx.path("pages/c.md"), "- external\n").expect("write");
    let ev = fx.wait_file(|f| f.rel_path == "pages/c.md");
    assert_eq!(ev.hash, Some(blake3::hash(b"- external\n")));
}

#[test]
fn external_write_within_300ms_of_own_write_is_detected() {
    let fx = start();
    own_write(&fx, "pages/d.md", "- mine\n");
    sleep(Duration::from_millis(50));
    fs::write(fx.path("pages/d.md"), "- theirs\n").expect("write");
    let ev = fx.wait_file(|f| f.rel_path == "pages/d.md");
    assert_eq!(ev.hash, Some(blake3::hash(b"- theirs\n")));
}

#[test]
fn own_delete_is_suppressed() {
    let fx = start_with(WatchConfig::default(), IgnoreRules::new(), |root| {
        fs::write(root.join("pages/e.md"), "- x\n").expect("write");
    });
    fx.echo.record_removal("pages/e.md");
    fs::remove_file(fx.path("pages/e.md")).expect("rm");
    let leaked: Vec<_> = fx
        .events_before_sentinel()
        .into_iter()
        // Only the removal must be suppressed; on Windows the setup file's (debounced) creation
        // event can still arrive after the watcher starts and is legitimate.
        .filter(|e| e.rel_path == "pages/e.md" && matches!(e.kind, FileEventKind::Removed))
        .collect();
    assert!(leaked.is_empty(), "{leaked:?}");
}

#[test]
fn same_content_touch_is_a_noop() {
    let fx = start();
    fs::write(fx.path("pages/f.md"), "- same\n").expect("write");
    fx.wait_file(|f| f.rel_path == "pages/f.md");
    fs::write(fx.path("pages/f.md"), "- same\n").expect("rewrite");
    let again: Vec<_> = fx
        .events_before_sentinel()
        .into_iter()
        .filter(|e| e.rel_path == "pages/f.md")
        .collect();
    assert!(again.is_empty(), "{again:?}");
}

#[test]
fn new_directory_with_files_is_scanned() {
    let fx = start();
    fs::create_dir_all(fx.path("pages/sub")).expect("mkdir");
    fs::write(fx.path("pages/sub/g.md"), "- g\n").expect("write");
    let ev = fx.wait_file(|f| f.rel_path == "pages/sub/g.md");
    assert_eq!(ev.kind, FileEventKind::Upserted);
    fs::remove_dir_all(fx.path("pages/sub")).expect("rm");
    let ev = fx.wait_file(|f| f.rel_path == "pages/sub/g.md");
    assert_eq!(ev.kind, FileEventKind::Removed);
}

#[test]
fn safety_scan_reports_new_directory_files() {
    let cfg = WatchConfig {
        safety_scan_interval: Some(Duration::from_millis(200)),
        ..WatchConfig::default()
    };
    let fx = start_with(cfg, IgnoreRules::new(), |_| {});
    fs::create_dir_all(fx.path("pages/deep/er")).expect("mkdir");
    fs::write(fx.path("pages/deep/er/s.md"), "- s\n").expect("write");
    let ev = fx.wait_file(|f| f.rel_path == "pages/deep/er/s.md");
    assert_eq!(ev.kind, FileEventKind::Upserted);
    fs::remove_dir_all(fx.path("pages/deep")).expect("rm");
    let ev = fx.wait_file(|f| f.rel_path == "pages/deep/er/s.md");
    assert_eq!(ev.kind, FileEventKind::Removed);
}

#[test]
fn safety_scan_alone_reports_create_and_remove_when_os_is_silent() {
    let cfg = WatchConfig {
        drop_os_events: true,
        safety_scan_interval: Some(Duration::from_millis(200)),
        ..WatchConfig::default()
    };
    let fx = start_with(cfg, IgnoreRules::new(), |_| {});
    fs::create_dir_all(fx.path("pages/sub")).expect("mkdir");
    fs::write(fx.path("pages/sub/g.md"), "- g\n").expect("write");
    let ev = fx.wait_file(|f| f.rel_path == "pages/sub/g.md");
    assert_eq!(ev.kind, FileEventKind::Upserted);
    fs::remove_dir_all(fx.path("pages/sub")).expect("rm");
    let ev = fx.wait_file(|f| f.rel_path == "pages/sub/g.md");
    assert_eq!(ev.kind, FileEventKind::Removed);
}

#[test]
fn polling_fallback_detects_changes() {
    let cfg = WatchConfig {
        force_polling: true,
        poll_interval: Duration::from_millis(150),
        ..WatchConfig::default()
    };
    let fx = start_with(cfg, IgnoreRules::new(), |root| {
        fs::write(root.join("pages/h.md"), "- 1\n").expect("write");
    });
    // First message is the fallback notice.
    let notices = fx.drain_for(Duration::from_millis(50));
    assert!(
        notices
            .iter()
            .any(|e| matches!(e, WatchEvent::Notice(n) if n.fallback_polling))
    );
    fs::write(fx.path("pages/h.md"), "- 22\n").expect("write");
    let ev = fx.wait_file(|f| f.rel_path == "pages/h.md");
    assert_eq!(ev.hash, Some(blake3::hash(b"- 22\n")));
    fs::write(fx.path("pages/new.md"), "- n\n").expect("write");
    fx.wait_file(|f| f.rel_path == "pages/new.md");
    fs::remove_file(fx.path("pages/h.md")).expect("rm");
    let ev = fx.wait_file(|f| f.rel_path == "pages/h.md");
    assert_eq!(ev.kind, FileEventKind::Removed);
}

#[test]
fn blank_journal_is_ignored_but_real_content_is_reported() {
    let fx = start_with(WatchConfig::default(), IgnoreRules::new(), |root| {
        fs::create_dir_all(root.join("journals")).expect("mkdir");
    });
    fs::write(fx.path("journals/2026_10_06.md"), "-\n").expect("write");
    fs::write(fx.path("pages/dash.md"), "-\n").expect("write");
    let before: Vec<_> = fx
        .events_before_sentinel()
        .into_iter()
        .map(|e| e.rel_path)
        .collect();
    assert_eq!(before, ["pages/dash.md"], "got {before:?}");

    fs::write(fx.path("journals/2026_10_06.md"), "- real\n").expect("write");
    let ev = fx.wait_file(|f| f.rel_path == "journals/2026_10_06.md");
    assert_eq!(ev.hash, Some(blake3::hash(b"- real\n")));
}

#[test]
fn missing_root_is_an_error() {
    let r = GraphWatcher::start(
        "/definitely/not/here",
        IgnoreRules::new(),
        EchoFilter::new(),
        WatchConfig::default(),
        |_| {},
    );
    assert!(r.is_err());
}
