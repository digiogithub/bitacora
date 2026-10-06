//! End-to-end flows of the sync UI against real temporary repositories (BIT-US-0043,
//! BIT-US-0047, BIT-US-0048, BIT-US-0054, BIT-US-0070): onboarding, status stream, conflict
//! resolution through the visual resolver, page history with restore and undo, and the
//! "page changed on disk" banner. They need the system `git`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use bitacora_core::editor::Cmd;
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;
use bitacora_core::queue::{Request, Source};
use bitacora_sync::detect_git;
use bitacora_sync::onboarding::{OnboardingConfig, clone_graph, enable_sync};
use bitacora_sync::repo_setup::Identity;
use bitacora_sync::state::SyncState;
use bitacora_testkit::git::{git, git_available, init_bare};

use crate::nav::Route;
use crate::settings::AppSettings;
use crate::sync_prefs::{SyncForm, SyncPrefs};
use crate::ui::Entity;
use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
use crate::views::conflicts::Action;
use crate::views::sync_dialog::{DialogMode, Phase, SyncDialogEvent};
use crate::views::workspace::{Workspace, WorkspaceConfig};
use crate::{keymap, theme};

const PAGE: &str = "- first block is a long line\n- second block is another long line\n- third\n";

fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::ui::init(cx);
        crate::views::panels::register_panels(cx);
        theme::install(cx, AppSettings::default(), None);
        keymap::load_with_user(cx, None).expect("default keymap");
    });
}

fn onboarding(name: &str) -> OnboardingConfig {
    let mut c = OnboardingConfig::new(detect_git(None));
    c.identity = Some(Identity {
        name: name.to_owned(),
        email: format!("{name}@example.com"),
    });
    c
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

/// Device A (a connected graph) and device B (a clone), sharing a bare remote.
fn two_devices(tmp: &Path) -> (PathBuf, PathBuf, String) {
    let remote = tmp.join("remote.git");
    init_bare(&remote);
    let url = remote.to_string_lossy().into_owned();
    let a_dir = tmp.join("a").join("graph");
    write(&a_dir, "pages/p.md", PAGE);
    write(&a_dir, "logseq/config.edn", "{}\n");
    enable_sync(&a_dir, &url, "main", &onboarding("alice")).unwrap();
    let b_dir = tmp.join("b").join("graph");
    std::fs::create_dir_all(b_dir.parent().unwrap()).unwrap();
    clone_graph(&url, &b_dir, &onboarding("bob")).unwrap();
    (a_dir, b_dir, url)
}

fn enable_prefs(state_dir: &Path, graph: &Path, device: &str) {
    let root = graph.canonicalize().unwrap();
    let prefs = SyncPrefs {
        enabled: true,
        branch: "main".into(),
        device: device.into(),
        ..SyncPrefs::default()
    };
    prefs.save(&SyncPrefs::file_for(state_dir, &root)).unwrap();
}

fn workspace<'a>(
    cx: &'a mut TestAppContext,
    data: &Path,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    setup(cx);
    let data = data.to_path_buf();
    cx.add_window_view(move |window, cx| {
        Workspace::new(
            WorkspaceConfig {
                index_data_dir: Some(data.clone()),
                state_dir: Some(data.join("state")),
                global_config: Some(data.join("no-global.edn")),
                ..WorkspaceConfig::default()
            },
            window,
            cx,
        )
    })
}

fn wait_until(
    cx: &mut VisualTestContext,
    what: &str,
    mut done: impl FnMut(&mut VisualTestContext) -> bool,
) {
    cx.executor().allow_parking();
    for _ in 0..3000 {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {what}");
}

fn sync_state(ws: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<(SyncState, usize)> {
    let status = ws.read_with(cx, |w, _| w.status_bar().clone());
    status.read_with(cx, |bar, _| {
        bar.sync_view()
            .map(|v| (v.status.state.clone(), v.status.conflicts))
    })
}

#[gpui_test]
fn the_form_validates_before_any_git_work_and_clone_opens_the_graph_with_sync_on(
    cx: &mut TestAppContext,
) {
    if !git_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let (_a, _b, url) = two_devices(tmp.path());
    let data = tmp.path().join("data");
    let (ws, cx) = workspace(cx, &data);
    let dialog = ws.read_with(cx, |w, _| w.sync_dialog().clone());
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    let _sub = cx.update(|_, cx| {
        cx.subscribe(&dialog, move |_, event: &SyncDialogEvent, _| {
            sink.borrow_mut().push(event.clone());
        })
    });

    ws.update_in(cx, |w, window, cx| w.open_clone_dialog(window, cx));
    assert_eq!(
        dialog.read_with(cx, |d, _| d.mode()),
        Some(DialogMode::Clone)
    );
    // An empty form shows the field errors and does not start anything.
    dialog.update_in(cx, |d, window, cx| d.submit(window, cx));
    assert!(dialog.read_with(cx, |d, _| d.shows_errors()));
    assert_eq!(
        dialog.read_with(cx, |d, _| d.phase().clone()),
        Phase::Editing
    );
    assert!(dialog.read_with(cx, |d, cx| !d.errors(cx).is_empty()));
    assert!(events.borrow().is_empty());

    // A valid form clones into the chosen folder.
    let dest = tmp.path().join("cloned");
    let form = SyncForm {
        remote_url: url,
        branch: "main".into(),
        name: "Carol".into(),
        email: "carol@example.com".into(),
        device: "carol-laptop".into(),
        destination: dest.display().to_string(),
    };
    dialog.update_in(cx, |d, window, cx| d.set_form(&form, window, cx));
    assert!(dialog.read_with(cx, |d, cx| d.errors(cx).is_empty()));
    dialog.update_in(cx, |d, window, cx| d.submit(window, cx));
    wait_until(cx, "the clone to finish", |_| {
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, SyncDialogEvent::Cloned { .. }))
    });
    assert!(dest.join("pages/p.md").is_file());
    assert_eq!(dialog.read_with(cx, |d, _| d.mode()), None);

    // The workspace opened the clone and started sync with the stored preferences.
    wait_until(cx, "sync to start on the cloned graph", |cx| {
        matches!(sync_state(&ws, cx), Some((SyncState::Idle, 0)))
    });
    assert_eq!(
        ws.read_with(cx, |w, _| w.sync_prefs().device.clone()),
        "carol-laptop"
    );
    // The identity is repo-local.
    assert_eq!(git(&dest, &["config", "--local", "user.name"]), "Carol");
}

#[gpui_test]
fn enable_sync_connects_a_plain_folder_and_the_status_bar_follows_the_engine(
    cx: &mut TestAppContext,
) {
    if !git_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let remote = tmp.path().join("remote.git");
    init_bare(&remote);
    let graph = tmp.path().join("graph");
    write(&graph, "pages/p.md", PAGE);
    write(&graph, "logseq/config.edn", "{}\n");
    let data = tmp.path().join("data");
    let (ws, cx) = workspace(cx, &data);
    let path = graph.clone();
    ws.update_in(cx, |w, window, cx| w.open_graph(path, window, cx));
    cx.executor().allow_parking();
    // Sync is off: the slot says so and "Sync now" points to the panel.
    wait_until(cx, "the session", |cx| {
        ws.read_with(cx, |w, _| w.queue().is_some())
    });
    assert!(sync_state(&ws, cx).is_none());

    let dialog = ws.read_with(cx, |w, _| w.sync_dialog().clone());
    ws.update_in(cx, |w, window, cx| w.open_enable_dialog(window, cx));
    let form = SyncForm {
        remote_url: remote.display().to_string(),
        branch: "main".into(),
        name: "Alice".into(),
        email: "alice@example.com".into(),
        device: "alice-laptop".into(),
        destination: String::new(),
    };
    dialog.update_in(cx, |d, window, cx| d.set_form(&form, window, cx));
    dialog.update_in(cx, |d, window, cx| d.submit(window, cx));
    wait_until(cx, "sync to be enabled and running", |cx| {
        matches!(sync_state(&ws, cx), Some((SyncState::Idle, 0)))
    });
    let message = ws
        .read_with(cx, |w, _| w.status_bar().clone())
        .read_with(cx, |bar, _| bar.sync_view().map(|v| v.message.clone()));
    assert_eq!(message.as_deref(), Some("Synced"));
    assert!(graph.join(".git").is_dir());
    assert_eq!(
        git(&remote, &["rev-parse", "main"]),
        git(&graph, &["rev-parse", "HEAD"]),
        "the first push reached the remote"
    );
    assert_eq!(
        git(&graph, &["config", "--local", "user.email"]),
        "alice@example.com"
    );
    // The preferences persisted for the next start.
    let file = SyncPrefs::file_for(&data.join("state"), &graph.canonicalize().unwrap());
    assert!(SyncPrefs::load(&file).enabled);

    // "Sync now" through the panel path: still idle afterwards, and the panel mirrors it.
    ws.update_in(cx, |w, window, cx| w.sync_now(window, cx));
    let panel = ws.read_with(cx, |w, _| w.sync_panel().clone());
    ws.update_in(cx, |w, _, cx| w.open_sync_panel(cx));
    wait_until(cx, "panel view", |cx| {
        panel.read_with(cx, |p, _| p.view().is_some())
    });
    assert!(panel.read_with(cx, |p, _| p.is_open()));
    let backend = panel.read_with(cx, |p, _| p.view().map(|v| v.backend.description.clone()));
    assert!(backend.is_some_and(|d| d.contains("git")));
}

#[gpui_test]
fn conflicts_are_resolved_block_by_block_in_the_visual_resolver_and_the_merge_is_pushed(
    cx: &mut TestAppContext,
) {
    if !git_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let (a_dir, b_dir, _url) = two_devices(tmp.path());
    let data = tmp.path().join("data");
    enable_prefs(&data.join("state"), &a_dir, "alice");
    let (ws, cx) = workspace(cx, &data);
    let path = a_dir.clone();
    ws.update_in(cx, |w, window, cx| w.open_graph(path, window, cx));
    wait_until(cx, "sync idle", |cx| {
        matches!(sync_state(&ws, cx), Some((SyncState::Idle, 0)))
    });
    ws.update_in(cx, |w, _, cx| w.navigate(Route::Page("p".into()), cx));
    let page = ws.read_with(cx, |w, cx| w.page_view(cx));
    wait_until(cx, "the page", |cx| {
        page.read_with(cx, |p, _| p.title() == Some("p"))
    });

    // Both devices edit the same block; B pushes first.
    write(
        &a_dir,
        "pages/p.md",
        &PAGE.replace("second block", "second block (alice)"),
    );
    write(
        &b_dir,
        "pages/p.md",
        &PAGE.replace("second block", "second block (bob)"),
    );
    git(&b_dir, &["add", "-A"]);
    git(&b_dir, &["commit", "-m", "bob edit"]);
    git(&b_dir, &["push", "origin", "HEAD:main"]);
    ws.update_in(cx, |w, window, cx| w.sync_now(window, cx));
    wait_until(cx, "the conflicted status", |cx| {
        matches!(sync_state(&ws, cx), Some((SyncState::Conflicted, 1)))
    });
    let message = ws
        .read_with(cx, |w, _| w.status_bar().clone())
        .read_with(cx, |bar, _| bar.sync_view().map(|v| v.message.clone()));
    assert_eq!(message.as_deref(), Some("Conflicts (1)"));
    // The page on screen carries the "sync conflicts" banner.
    let banner = ws.read_with(cx, |w, _| w.disk_banner().clone());
    assert_eq!(banner.read_with(cx, |b, _| b.sync_conflicts()), 1);

    // The resolver lists the block with both versions and the progress.
    let view = ws.read_with(cx, |w, _| w.conflicts().clone());
    ws.update_in(cx, |w, window, cx| w.open_conflicts(window, cx));
    assert!(view.read_with(cx, |v, _| v.is_open()));
    let cards = view.read_with(cx, |v, _| v.cards().to_vec());
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].path, "pages/p.md");
    assert!(cards[0].ours.as_deref().unwrap().contains("(alice)"));
    assert!(cards[0].theirs.as_deref().unwrap().contains("(bob)"));
    assert_eq!(view.read_with(cx, |v, _| v.progress().resolved), 0);

    // Keep theirs: the last resolution commits and pushes the merge.
    view.update_in(cx, |v, window, cx| v.apply(0, Action::Theirs, window, cx));
    wait_until(cx, "the merge to be pushed", |cx| {
        matches!(sync_state(&ws, cx), Some((SyncState::Idle, 0)))
    });
    assert!(
        std::fs::read_to_string(a_dir.join("pages/p.md"))
            .unwrap()
            .contains("(bob)")
    );
    assert!(
        !std::fs::read_to_string(a_dir.join("pages/p.md"))
            .unwrap()
            .contains("<<<<<<<")
    );
    assert_eq!(
        git(&a_dir, &["rev-parse", "HEAD"]),
        git(&a_dir, &["rev-parse", "origin/main"])
    );
    wait_until(cx, "the resolver to see the finished merge", |cx| {
        view.read_with(cx, |v, _| v.cards().is_empty())
    });
    assert_eq!(banner.read_with(cx, |b, _| b.sync_conflicts()), 0);
}

#[gpui_test]
fn page_history_shows_block_diffs_restores_selected_blocks_and_undoes(cx: &mut TestAppContext) {
    if !git_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let (a_dir, _b_dir, _url) = two_devices(tmp.path());
    // A second version of the page, committed after the connecting commit.
    let v2 = PAGE.replace(
        "second block is another",
        "second block was rewritten as another",
    );
    write(&a_dir, "pages/p.md", &v2);
    git(&a_dir, &["add", "-A"]);
    git(&a_dir, &["commit", "-m", "rewrite the second block"]);
    let data = tmp.path().join("data");
    enable_prefs(&data.join("state"), &a_dir, "alice");
    let (ws, cx) = workspace(cx, &data);
    let path = a_dir.clone();
    ws.update_in(cx, |w, window, cx| w.open_graph(path, window, cx));
    wait_until(cx, "the session", |cx| {
        ws.read_with(cx, |w, _| w.queue().is_some())
    });
    ws.update_in(cx, |w, _, cx| w.navigate(Route::Page("p".into()), cx));
    let page = ws.read_with(cx, |w, cx| w.page_view(cx));
    wait_until(cx, "the page", |cx| {
        page.read_with(cx, |p, _| p.title() == Some("p"))
    });

    ws.update_in(cx, |w, window, cx| w.open_history(window, cx));
    let history = ws.read_with(cx, |w, _| w.history().clone());
    wait_until(cx, "the history list", |cx| {
        history.read_with(cx, |h, _| h.entries().len() >= 2)
    });
    // Newest first: the rewrite, then the connecting commit.
    history.update(cx, |h, cx| h.select(1, cx));
    wait_until(cx, "the diff", |cx| {
        history.read_with(cx, |h, _| h.diff().is_some())
    });
    let blocks = history.read_with(cx, |h, _| h.diff().unwrap().blocks.len());
    assert_eq!(blocks, 1, "only the rewritten block differs");
    history.update(cx, |h, cx| h.toggle_block(0, cx));
    assert_eq!(history.read_with(cx, |h, _| h.checked()), vec![0]);

    // Restore that block: the file gets the old text, through core's writer.
    history.update(cx, |h, cx| h.restore_selected(cx));
    wait_until(cx, "the restore", |cx| {
        history.read_with(cx, |h, _| {
            h.message().is_some_and(|m| m.starts_with("Restored"))
        })
    });
    let restored = move || {
        std::fs::read_to_string(a_dir.join("pages/p.md"))
            .unwrap()
            .contains("second block is another")
    };
    wait_until(cx, "the file to be written", |_| restored());
    // Undo brings the rewrite back.
    history.update(cx, |h, cx| h.undo_restore(cx));
    wait_until(cx, "the undo", |cx| {
        history.read_with(cx, |h, _| h.message() == Some("Restore undone"))
    });
}

#[gpui_test]
fn a_page_changed_on_disk_shows_the_banner_and_keep_mine_resolves_it(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    let graph = tmp.path().join("graph");
    let base = "- alpha one two three\n- beta one two three\n";
    write(&graph, "pages/Home.md", base);
    write(&graph, "logseq/config.edn", "{}\n");
    let data = tmp.path().join("data");
    let (ws, cx) = workspace(cx, &data);
    let path = graph.clone();
    ws.update_in(cx, |w, window, cx| w.open_graph(path, window, cx));
    wait_until(cx, "the session", |cx| {
        ws.read_with(cx, |w, _| w.queue().is_some())
    });
    let queue = ws.read_with(cx, |w, _| w.queue().unwrap().clone());
    let key = PageKey::from_title("Home");
    queue
        .execute(
            Source::Ui,
            Request::LoadPage {
                key: key.clone(),
                title: "Home".into(),
                path: Some(GraphPath::new("pages/Home.md").unwrap()),
                bytes: base.as_bytes().to_vec(),
            },
        )
        .unwrap();
    let first = queue.snapshot(&key).unwrap().blocks[0].id;
    queue
        .execute(
            Source::Ui,
            Request::Run {
                label: "edit",
                cmd: Cmd::SetText {
                    id: first,
                    text: "alpha one two three MINE".into(),
                },
            },
        )
        .unwrap();
    let theirs = "- alpha one two three THEIRS\n- beta one two three\n";
    queue
        .execute(
            Source::External,
            Request::ExternalChange {
                key: key.clone(),
                bytes: theirs.as_bytes().to_vec(),
            },
        )
        .unwrap();

    // The notice reaches the banner; it shows for the page on screen only.
    let banner = ws.read_with(cx, |w, _| w.disk_banner().clone());
    wait_until(cx, "the conflict notice", |cx| {
        banner.read_with(cx, |b, _| b.count() == 1)
    });
    assert!(banner.read_with(cx, |b, _| b.active().is_none()));
    banner.update(cx, |b, cx| b.set_current(Some(key.clone()), cx));
    let notice = banner.read_with(cx, |b, _| b.active().cloned()).unwrap();
    assert_eq!(notice.diff.len(), 1);
    assert!(notice.diff[0].mine.as_deref().unwrap().contains("MINE"));
    assert!(notice.diff[0].disk.as_deref().unwrap().contains("THEIRS"));

    // Show diff opens the block-level overlay.
    let diff = ws.read_with(cx, |w, _| w.disk_diff().clone());
    banner.update(cx, |_, cx| {
        cx.emit(crate::views::disk_conflict::DiskConflictEvent::ShowDiff(
            key.clone(),
        ));
    });
    cx.run_until_parked();
    assert!(diff.read_with(cx, |d, _| d.is_open()));
    assert_eq!(diff.read_with(cx, |d, _| d.diffs().len()), 1);

    // Keep mine overwrites the disk version (backed up first) and clears the banner.
    ws.update_in(cx, |w, window, cx| {
        w.resolve_disk(key.clone(), bitacora_core::queue::Keep::Mine, window, cx);
    });
    wait_until(cx, "the banner to clear", |cx| {
        banner.read_with(cx, |b, _| b.count() == 0)
    });
    wait_until(cx, "the file to hold my version", |_| {
        std::fs::read_to_string(graph.join("pages/Home.md"))
            .unwrap()
            .contains("MINE")
    });
    assert!(!diff.read_with(cx, |d, _| d.is_open()));
}
