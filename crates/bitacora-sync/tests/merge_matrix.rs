//! Merge matrix through temp bare repositories (AGENTS.md section 8): metadata-only, content
//! conflict, add/add journal, delete/modify, rename, special files, persisted conflict state,
//! resolution API, external markers and unmerged pulls. BIT-US-0052, BIT-US-0053. Every scenario
//! runs against both backends (system git hybrid and gix-only).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use bitacora_sync::backend::{CliConfig, select_backend};
use bitacora_sync::commit_msg::parse_message;
use bitacora_sync::engine::{EngineConfig, SyncEngine};
use bitacora_sync::merge::{BlockLocation, ConflictType, Resolution};
use bitacora_sync::resolve::ResolveError;
use bitacora_sync::state::SyncState;
use bitacora_sync::store::JsonMergeStore;
use bitacora_sync::writer::testing::DirGraphWriter;
use bitacora_testkit::git::git_raw;
use common::{Dev, KINDS, Kind, ManualTiming, World, assert_no_markers, onboarding_config};

const PAGE: &str = "- Alpha\n- Beta\n- Gamma\n";
const UUID: &str = "66500000-0000-4000-8000-0000000000aa";

fn synced_pair(kind: Kind, files: &[(&str, &str)]) -> (World, Dev, Dev) {
    let w = World::new(kind);
    let mut a = w.first_device(files);
    assert_eq!(a.engine.sync_now(), SyncState::Idle);
    let mut b = w.clone_device();
    assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
    (w, a, b)
}

fn parents(d: &Dev, rev: &str) -> Vec<String> {
    d.git(&["log", "-1", "--format=%P", rev])
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn message(d: &Dev, rev: &str) -> String {
    d.git(&["log", "-1", "--format=%B", rev])
}

/// Replaces `dev`'s engine by one that persists its merge state in `.git/bitacora`.
fn with_json_store(kind: Kind, dev: &mut Dev, device: &str) {
    let cfg = onboarding_config(kind, device);
    let backend = select_backend(&cfg.detection, &dev.dir, CliConfig::default()).unwrap();
    let store = JsonMergeStore::for_graph(&dev.dir).unwrap();
    dev.engine = SyncEngine::new(
        backend,
        Arc::clone(&dev.writer) as Arc<dyn bitacora_sync::writer::GraphWriter>,
        Box::new(store),
        EngineConfig::new(&dev.dir, device, "main"),
        ManualTiming::new(),
    );
}

#[test]
fn metadata_only_differences_merge_without_conflicts() {
    for kind in KINDS {
        let (w, mut a, mut b) = synced_pair(kind, &[("pages/P.md", PAGE)]);
        a.write(
            "pages/P.md",
            "- Alpha\n  collapsed:: true\n- Beta\n- Gamma\n",
        );
        b.write("pages/P.md", "- Alpha\n- Beta edited\n- Gamma\n");
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(
            b.read("pages/P.md"),
            "- Alpha\n  collapsed:: true\n- Beta edited\n- Gamma\n"
        );
        assert_eq!(parents(&b, "HEAD").len(), 2);
        assert_eq!(b.head(), w.remote_head());
        assert!(b.engine.pending_merge().is_none());
    }
}

#[test]
fn config_edn_merges_per_key_and_keeps_comments() {
    for kind in KINDS {
        let base = "{:journal/page-title-format \"MMM do, yyyy\"\n :preferred-format :markdown}\n";
        let (w, mut a, mut b) =
            synced_pair(kind, &[("logseq/config.edn", base), ("pages/P.md", PAGE)]);
        a.write(
            "logseq/config.edn",
            "{:journal/page-title-format \"MMM do, yyyy\"\n :preferred-format :markdown\n :default-templates {:journals \"daily\"}}\n",
        );
        b.write(
            "logseq/config.edn",
            ";; mine\n{:journal/page-title-format \"yyyy-MM-dd\" ; ISO\n :preferred-format :markdown}\n",
        );
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        let merged = b.read("logseq/config.edn");
        assert!(merged.starts_with(";; mine\n"), "{merged}");
        assert!(merged.contains("\"yyyy-MM-dd\" ; ISO"), "{merged}");
        assert!(
            merged.contains(":default-templates {:journals \"daily\"}"),
            "{merged}"
        );
        assert_eq!(b.head(), w.remote_head());
    }
}

#[test]
fn config_same_key_conflict_is_resolved_through_the_api() {
    for kind in KINDS {
        let (w, mut a, mut b) = synced_pair(
            kind,
            &[("logseq/config.edn", "{:a 1 :b 1}\n"), ("pages/P.md", PAGE)],
        );
        a.write("logseq/config.edn", "{:a 3 :b 1}\n");
        b.write("logseq/config.edn", "{:a 2 :b 2}\n");
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        assert_eq!(b.read("logseq/config.edn"), "{:a 2 :b 2}\n", "ours kept");
        let pending = b.engine.pending_merge().unwrap();
        assert_eq!(pending.conflicts.len(), 1);
        let c = &pending.conflicts[0];
        assert_eq!(c.kind, ConflictType::Config);
        assert_eq!(c.breadcrumb, vec!["a".to_owned()]);
        assert_eq!(
            (c.ours.as_deref(), c.theirs.as_deref()),
            (Some("2"), Some("3"))
        );
        let out = b
            .engine
            .resolve_conflict(&c.id, Resolution::Theirs)
            .unwrap();
        assert_eq!(out.remaining, 0);
        assert_eq!(out.state, SyncState::Idle, "{kind:?}");
        assert_eq!(b.read("logseq/config.edn"), "{:a 3 :b 2}\n");
        assert_eq!(b.head(), w.remote_head());
        assert_eq!(parents(&b, "HEAD").len(), 2);
        assert!(b.engine.pending_merge().is_none());
    }
}

#[test]
fn custom_css_line_merge_and_overlap() {
    for kind in KINDS {
        let (_w, mut a, mut b) = synced_pair(
            kind,
            &[
                ("logseq/custom.css", "a {}\nb {}\nc {}\n"),
                ("pages/P.md", PAGE),
            ],
        );
        a.write("logseq/custom.css", "a {}\nb {}\nc { color: red }\n");
        b.write("logseq/custom.css", "a { color: blue }\nb {}\nc {}\n");
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(
            b.read("logseq/custom.css"),
            "a { color: blue }\nb {}\nc { color: red }\n"
        );
        // Overlap on the same line conflicts and keeps ours.
        a.engine.sync_now();
        a.write(
            "logseq/custom.css",
            "a { color: green }\nb {}\nc { color: red }\n",
        );
        b.write(
            "logseq/custom.css",
            "a { color: black }\nb {}\nc { color: red }\n",
        );
        a.engine.sync_now();
        b.engine.sync_now();
        assert_eq!(
            b.read("logseq/custom.css"),
            "a { color: black }\nb {}\nc { color: red }\n"
        );
        let p = b.engine.pending_merge().unwrap();
        assert_eq!(p.conflicts[0].kind, ConflictType::Text);
        assert_no_markers(&b.dir);
    }
}

#[test]
fn add_add_journals_union_and_template_side_loses() {
    for kind in KINDS {
        let w = World::new(kind);
        let mut a = w.first_device(&[
            ("journals/2026_10_06.md", "- call Ana\n"),
            ("pages/H.md", "- h\n"),
        ]);
        a.engine.sync_now();
        // Bob created today's journal independently (no common base for that file).
        let dir = w.path("b");
        for (p, c) in [
            ("journals/2026_10_06.md", "- buy milk\n"),
            ("pages/H.md", "- h\n"),
        ] {
            let full = dir.join(p);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, c).unwrap();
        }
        bitacora_sync::onboarding::enable_sync(
            &dir,
            &w.url(),
            "main",
            &onboarding_config(kind, "bob"),
        )
        .unwrap();
        let mut b = common::make_dev(kind, &dir, "bob", |_| {});
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(
            b.read("journals/2026_10_06.md"),
            "- buy milk\n- call Ana\n",
            "{kind:?}: ours first, then theirs"
        );
        // Another day: bob's side is the empty `-` template, alice's has content: alice wins.
        let mut a2 = a;
        a2.write("journals/2026_10_07.md", "-\n");
        b.write("journals/2026_10_07.md", "- standup notes\n");
        a2.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(b.read("journals/2026_10_07.md"), "- standup notes\n");
    }
}

#[test]
fn delete_versus_modify_restores_by_default_and_both_directions_resolve() {
    for kind in KINDS {
        let (w, mut a, mut b) = synced_pair(kind, &[("pages/P.md", PAGE), ("pages/Q.md", "- q\n")]);
        // Alice deletes Q, Bob edits it: Bob's local side survives, conflict recorded.
        std::fs::remove_file(a.dir.join("pages/Q.md")).unwrap();
        b.write("pages/Q.md", "- q edited by bob\n");
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        let c = b.engine.pending_merge().unwrap().conflicts[0].clone();
        assert_eq!(c.kind, ConflictType::FileDeleteVsModify);
        assert_eq!(b.read("pages/Q.md"), "- q edited by bob\n");
        // "Keep deleted" is the resolution that follows their side.
        let out = b
            .engine
            .resolve_conflict(&c.id, Resolution::Theirs)
            .unwrap();
        assert_eq!(out.state, SyncState::Idle, "{kind:?}");
        assert!(!b.dir.join("pages/Q.md").exists());
        assert_eq!(b.head(), w.remote_head());
    }
    for kind in KINDS {
        let (w, mut a, mut b) = synced_pair(kind, &[("pages/P.md", PAGE), ("pages/Q.md", "- q\n")]);
        // Bob deletes Q locally, Alice edits it remotely: the default restores Alice's file.
        a.write("pages/Q.md", "- q edited by alice\n");
        std::fs::remove_file(b.dir.join("pages/Q.md")).unwrap();
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        assert_eq!(
            b.read("pages/Q.md"),
            "- q edited by alice\n",
            "{kind:?}: restored with their changes"
        );
        let c = b.engine.pending_merge().unwrap().conflicts[0].clone();
        assert_eq!(c.deleted_by, Some(bitacora_merge::Side::Ours));
        // Keeping the restored page confirms the default.
        let out = b
            .engine
            .resolve_conflict(&c.id, Resolution::Theirs)
            .unwrap();
        assert_eq!(out.state, SyncState::Idle, "{kind:?}");
        assert_eq!(b.read("pages/Q.md"), "- q edited by alice\n");
        assert_eq!(b.head(), w.remote_head());
    }
}

#[test]
fn rename_on_one_side_applies_the_other_sides_edit_at_the_new_path() {
    for kind in KINDS {
        let old = "- one is a fairly long first block\n- two is another reasonably long block\n- three closes the page nicely\n";
        let (w, mut a, mut b) = synced_pair(
            kind,
            &[("pages/Old.md", old), ("pages/Home.md", "- home\n")],
        );
        std::fs::rename(a.dir.join("pages/Old.md"), a.dir.join("pages/New.md")).unwrap();
        b.write(
            "pages/Old.md",
            "- one is a fairly long first block\n- two is another reasonably long block, edited\n- three closes the page nicely\n",
        );
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert!(!b.dir.join("pages/Old.md").exists(), "{kind:?}");
        assert_eq!(
            b.read("pages/New.md"),
            "- one is a fairly long first block\n- two is another reasonably long block, edited\n- three closes the page nicely\n"
        );
        assert_eq!(b.head(), w.remote_head());
    }
}

#[test]
fn links_to_the_old_name_added_by_the_other_side_are_rewritten() {
    for kind in KINDS {
        let old = "- one is a fairly long first block\n- two is another reasonably long block\n- three closes the page nicely\n";
        let x = "- intro\n- body\n";
        let (w, mut a, mut b) = synced_pair(
            kind,
            &[
                ("pages/Old.md", old),
                ("pages/X.md", x),
                ("pages/Y.md", "- y\n"),
            ],
        );
        // Alice (remote) renames Old -> New and rewrites nothing else; Bob adds a link to Old.
        std::fs::rename(a.dir.join("pages/Old.md"), a.dir.join("pages/New.md")).unwrap();
        b.write("pages/X.md", "- intro\n- body\n- see [[Old]] and #Old\n");
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(
            b.read("pages/X.md"),
            "- intro\n- body\n- see [[New]] and #New\n",
            "{kind:?}"
        );
        let msg = message(&b, "HEAD");
        assert!(
            msg.contains("Bitacora-Note") && msg.contains("[[Old]]"),
            "{msg}"
        );
        assert_eq!(b.head(), w.remote_head());
        // And the other direction: Bob renamed, Alice's new link (merged on Bob's side) follows.
        a.engine.sync_now();
        std::fs::rename(b.dir.join("pages/Y.md"), b.dir.join("pages/Why.md")).unwrap();
        a.write(
            "pages/X.md",
            "- intro\n- body\n- see [[New]] and #New\n- also [[Y]]\n",
        );
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(
            b.read("pages/X.md"),
            "- intro\n- body\n- see [[New]] and #New\n- also [[Why]]\n",
            "{kind:?}"
        );
    }
}

#[test]
fn rename_rename_conflict_offers_both_titles_with_alias() {
    for kind in KINDS {
        let old = "- one is a fairly long first block\n- two is another reasonably long block\n- three closes the page nicely\n";
        let (w, mut a, mut b) = synced_pair(
            kind,
            &[("pages/Old.md", old), ("pages/Home.md", "- home\n")],
        );
        std::fs::rename(a.dir.join("pages/Old.md"), a.dir.join("pages/B.md")).unwrap();
        std::fs::rename(b.dir.join("pages/Old.md"), b.dir.join("pages/A.md")).unwrap();
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        assert!(b.dir.join("pages/A.md").exists() && !b.dir.join("pages/B.md").exists());
        let c = b.engine.pending_merge().unwrap().conflicts[0].clone();
        assert_eq!(c.kind, ConflictType::RenameRename);
        assert_eq!(c.breadcrumb, vec!["A".to_owned(), "B".to_owned()]);
        assert_eq!(c.suggestion.as_deref(), Some("alias:: [[B]]"));
        let out = b.engine.resolve_conflict(&c.id, Resolution::Both).unwrap();
        assert_eq!(out.state, SyncState::Idle, "{kind:?}");
        assert!(
            b.read("pages/A.md").starts_with("alias:: [[B]]"),
            "{}",
            b.read("pages/A.md")
        );
        assert_eq!(b.head(), w.remote_head());
    }
}

#[test]
fn content_conflict_resolution_api_memo_and_restart() {
    for kind in KINDS {
        let (w, mut a, mut b) = synced_pair(
            kind,
            &[("pages/P.md", "- Alpha\n- Beta\n- Gamma\n- Delta\n")],
        );
        with_json_store(kind, &mut b, "bob");
        a.write(
            "pages/P.md",
            "- Alpha\n- Beta from alice\n- Gamma\n- Delta from alice\n",
        );
        b.write(
            "pages/P.md",
            "- Alpha\n- Beta from bob\n- Gamma\n- Delta from bob\n",
        );
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        let state = b.dir.join(".git/bitacora/merge-state.json");
        assert!(state.exists(), "persisted state");
        let pending = b.engine.pending_merge().unwrap();
        assert_eq!(pending.conflicts.len(), 2);
        let (c1, c2) = (pending.conflicts[0].clone(), pending.conflicts[1].clone());
        // Resolve c1 with "mine"; the choice is persisted before the app quits.
        let out = b.engine.resolve_conflict(&c1.id, Resolution::Ours).unwrap();
        assert_eq!(out.remaining, 1);
        assert!(matches!(
            b.engine.resolve_conflict(&c1.id, Resolution::Theirs),
            Err(ResolveError::AlreadyResolved(_))
        ));
        assert!(matches!(
            b.engine.resolve_conflict("nope", Resolution::Theirs),
            Err(ResolveError::UnknownConflict(_))
        ));
        drop(std::mem::replace(&mut b.engine, {
            // Restart: a fresh engine on the same repository reloads the state.
            let cfg = onboarding_config(kind, "bob");
            let backend = select_backend(&cfg.detection, &b.dir, CliConfig::default()).unwrap();
            SyncEngine::new(
                backend,
                Arc::new(DirGraphWriter::new(&b.dir)),
                Box::new(JsonMergeStore::for_graph(&b.dir).unwrap()),
                EngineConfig::new(&b.dir, "bob", "main"),
                ManualTiming::new(),
            )
        }));
        assert_eq!(b.engine.state(), &SyncState::Conflicted);
        let restored = b.engine.pending_merge().unwrap();
        assert_eq!(restored.conflicts.len(), 2);
        assert_eq!(restored.conflicts[0].resolution, Some(Resolution::Ours));
        assert!(restored.conflicts[1].is_unresolved());
        assert_eq!(b.engine.status().conflicts, 1);

        // The remote moves while conflicted: the recomputed merge re-applies c1 (same hashes)
        // and only c2 stays open.
        a.write("pages/Extra.md", "- extra\n");
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        let again = b.engine.pending_merge().unwrap();
        assert_eq!(again.theirs.as_hex(), w.remote_head());
        assert_eq!(again.conflicts.len(), 2, "{:?}", again.conflicts);
        let open: Vec<_> = again
            .conflicts
            .iter()
            .filter(|c| c.is_unresolved())
            .collect();
        assert_eq!(open.len(), 1, "{:?}", again.conflicts);
        assert_eq!(open[0].ours, c2.ours);
        assert_eq!(b.read("pages/Extra.md"), "- extra\n");

        // "Theirs" for the last one finishes the merge and removes the state file.
        let out = b
            .engine
            .resolve_conflict(&open[0].id.clone(), Resolution::Theirs)
            .unwrap();
        assert_eq!(out.state, SyncState::Idle, "{kind:?}");
        assert!(!state.exists());
        assert_eq!(
            b.read("pages/P.md"),
            "- Alpha\n- Beta from bob\n- Gamma\n- Delta from alice\n"
        );
        assert_eq!(b.head(), w.remote_head());
        let msg = parse_message(&message(&b, "HEAD"));
        assert_eq!(msg.kind, Some(bitacora_sync::CommitKind::Resolve));
        assert_no_markers(&b.dir);
    }
}

#[test]
fn bulk_resolution_per_page_and_keep_both() {
    for kind in KINDS {
        let (w, mut a, mut b) = synced_pair(
            kind,
            &[("pages/P.md", "- Alpha\n- Beta\n- Gamma\n- Delta\n")],
        );
        a.write(
            "pages/P.md",
            "- Alpha\n- Beta from alice\n- Gamma\n- Delta from alice\n",
        );
        b.write(
            "pages/P.md",
            "- Alpha\n- Beta from bob\n- Gamma\n- Delta from bob\n",
        );
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted);
        let out = b
            .engine
            .resolve_page("pages/P.md", &Resolution::Theirs)
            .unwrap();
        assert_eq!(
            (out.remaining, out.state.clone()),
            (0, SyncState::Idle),
            "{kind:?}"
        );
        assert_eq!(
            b.read("pages/P.md"),
            "- Alpha\n- Beta from alice\n- Gamma\n- Delta from alice\n"
        );
        assert_eq!(b.head(), w.remote_head());

        // Keep both: theirs becomes the next sibling.
        a.write(
            "pages/P.md",
            "- Alpha\n- Beta from alice\n- Gamma\n- Delta from alice 2\n",
        );
        b.write(
            "pages/P.md",
            "- Alpha\n- Beta from alice\n- Gamma\n- Delta from bob 2\n",
        );
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted);
        let id = b.engine.pending_merge().unwrap().conflicts[0].id.clone();
        let out = b.engine.resolve_conflict(&id, Resolution::Both).unwrap();
        assert_eq!(out.state, SyncState::Idle, "{kind:?}");
        assert_eq!(
            b.read("pages/P.md"),
            "- Alpha\n- Beta from alice\n- Gamma\n- Delta from bob 2\n- Delta from alice 2\n"
        );
        // An edit with markers is refused and nothing changes.
    }
}

#[test]
fn an_edit_with_markers_is_refused() {
    for kind in KINDS {
        let (_w, mut a, mut b) = synced_pair(kind, &[("pages/P.md", PAGE)]);
        a.write("pages/P.md", "- Alpha\n- Beta from alice\n- Gamma\n");
        b.write("pages/P.md", "- Alpha\n- Beta from bob\n- Gamma\n");
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted);
        let id = b.engine.pending_merge().unwrap().conflicts[0].id.clone();
        let err = b
            .engine
            .resolve_conflict(&id, Resolution::Edit("<<<<<<< x\nBeta\n=======\n".into()))
            .unwrap_err();
        assert!(matches!(err, ResolveError::InvalidEdit(_)), "{err:?}");
        assert_eq!(b.read("pages/P.md"), "- Alpha\n- Beta from bob\n- Gamma\n");
        assert_eq!(b.engine.status().conflicts, 1);
    }
}

#[test]
fn referenced_blocks_get_their_id_in_the_merge_commit() {
    for kind in KINDS {
        let (w, mut a, mut b) = synced_pair(
            kind,
            &[
                ("pages/A.md", "- Target\n- Other\n"),
                ("pages/B.md", "- b\n"),
                ("pages/C.md", "- c\n"),
            ],
        );
        b.engine.set_block_locator(Arc::new(|u: &str| {
            (u == UUID).then(|| BlockLocation {
                path: "pages/A.md".to_owned(),
                first_line: "Target".to_owned(),
            })
        }));
        // Alice (remote) references the block; Bob edits something else, so a merge is needed.
        a.write("pages/B.md", &format!("- b\n- see (({UUID}))\n"));
        b.write("pages/C.md", "- c edited\n");
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(
            b.read("pages/A.md"),
            format!("- Target\n  id:: {UUID}\n- Other\n"),
            "{kind:?}"
        );
        assert_eq!(
            b.git(&["status", "--porcelain"]),
            "",
            "no extra dirty state: {kind:?}"
        );
        assert_eq!(b.head(), w.remote_head());
        assert_eq!(
            parents(&b, "HEAD").len(),
            2,
            "written in the merge commit itself"
        );
    }
}

#[test]
fn external_marker_regions_are_merged_and_rewritten_clean() {
    for kind in KINDS {
        let (w, _a, mut b) = synced_pair(kind, &[("pages/P.md", "- shared\n")]);
        b.write(
            "pages/P.md",
            "- shared\n<<<<<<< HEAD\n- mine\n=======\n- theirs\n>>>>>>> origin/main\n",
        );
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(
            b.read("pages/P.md"),
            "- shared\n- mine\n- theirs\n",
            "{kind:?}"
        );
        assert_no_markers(&b.dir);
        assert_eq!(b.head(), w.remote_head());
        assert!(b.engine.pending_merge().is_none());
    }
}

#[test]
fn malformed_markers_hold_the_push_until_the_file_is_fixed() {
    for kind in KINDS {
        let (w, _a, mut b) = synced_pair(kind, &[("pages/P.md", "- shared\n")]);
        with_json_store(kind, &mut b, "bob");
        let before = w.remote_head();
        b.write("pages/P.md", "- shared\n<<<<<<< HEAD\n- mine\n");
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        assert_eq!(
            w.remote_head(),
            before,
            "nothing pushed with markers around"
        );
        let c = b.engine.pending_merge().unwrap().conflicts[0].clone();
        assert_eq!(c.kind, ConflictType::ExternalMarkers);
        assert!(c.id.starts_with("e-"));
        // Choosing a side is not possible for unbalanced markers; an edit is.
        assert!(matches!(
            b.engine.resolve_conflict(&c.id, Resolution::Theirs),
            Err(ResolveError::Unsupported(_))
        ));
        let out = b
            .engine
            .resolve_conflict(&c.id, Resolution::Edit("- shared\n- mine\n".into()))
            .unwrap();
        assert_eq!(out.state, SyncState::Idle, "{kind:?}");
        assert_eq!(b.read("pages/P.md"), "- shared\n- mine\n");
        assert_eq!(b.head(), w.remote_head());
        assert!(b.engine.pending_merge().is_none());
    }
}

#[test]
fn fixing_a_marker_file_by_hand_counts_as_the_edit_resolution() {
    for kind in KINDS {
        let (w, _a, mut b) = synced_pair(kind, &[("pages/P.md", "- shared\n")]);
        b.write("pages/P.md", ">>>>>>> stray\n- shared\n");
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        b.write("pages/P.md", "- shared\n- fixed\n");
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert!(b.engine.pending_merge().is_none());
        assert_eq!(b.head(), w.remote_head());
    }
}

#[test]
fn a_users_conflicted_git_pull_is_taken_over_and_merged_block_by_block() {
    for kind in KINDS {
        let (w, mut a, mut b) = synced_pair(kind, &[("pages/P.md", PAGE), ("pages/Q.md", "- q\n")]);
        a.write("pages/P.md", "- Alpha\n- Beta from alice\n- Gamma\n");
        a.write("pages/Q.md", "- q from alice\n");
        a.engine.sync_now();
        b.write("pages/P.md", "- Alpha\n- Beta from bob\n- Gamma\n");
        b.git(&["commit", "-qam", "bob edits"]);
        // The user runs `git pull`; the binary merge driver leaves P.md unmerged with ours.
        let out = git_raw(
            &b.dir,
            &["pull", "--no-rebase", "--no-edit", "origin", "main"],
        );
        assert!(!out.status.success(), "the pull must stop with a conflict");
        assert!(b.dir.join(".git/MERGE_HEAD").exists());
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        assert!(
            !b.dir.join(".git/MERGE_HEAD").exists(),
            "half-done merge was taken over"
        );
        assert_no_markers(&b.dir);
        assert_eq!(b.read("pages/P.md"), "- Alpha\n- Beta from bob\n- Gamma\n");
        assert_eq!(b.read("pages/Q.md"), "- q from alice\n");
        let p = b.engine.pending_merge().unwrap();
        assert_eq!(p.conflicts.len(), 1, "{:?}", p.conflicts);
        let out = b
            .engine
            .resolve_conflict(&p.conflicts[0].id, Resolution::Theirs)
            .unwrap();
        assert_eq!(out.state, SyncState::Idle, "{kind:?}");
        assert_eq!(
            b.read("pages/P.md"),
            "- Alpha\n- Beta from alice\n- Gamma\n"
        );
        assert_eq!(b.head(), w.remote_head());
    }
}

#[test]
fn markers_committed_by_another_tool_never_reach_the_work_tree() {
    for kind in KINDS {
        let (w, a, mut b) = synced_pair(kind, &[("pages/P.md", "- shared\n")]);
        // Alice (another tool) committed marker lines.
        a.write(
            "pages/P.md",
            "- shared\n<<<<<<< HEAD\n- x\n=======\n- y\n>>>>>>> topic\n",
        );
        a.git(&["commit", "-qam", "oops markers"]);
        a.git(&["push", "-q", "origin", "main"]);
        b.write("pages/Other.md", "- local\n");
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_no_markers(&b.dir);
        assert_eq!(b.read("pages/P.md"), "- shared\n- x\n- y\n", "{kind:?}");
        assert_eq!(b.head(), w.remote_head());
    }
}
