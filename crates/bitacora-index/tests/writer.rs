//! Single-writer transactional replace, page GC and UUID carry-over (BIT-US-0006).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use bitacora_index::{BUILTIN_PAGES, IndexEvent};
use common::{count, env, input, strings, writer};

const U1: &str = "6500c1a4-0000-4000-8000-000000000001";

#[test]
fn replace_inserts_page_blocks_refs_and_placeholders() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    let text = format!(
        "- first block #tag [[Linked Page]]\n  id:: {U1}\n- TODO second\n  - child ((6500c1a4-0000-4000-8000-0000000000ff))\n"
    );
    let out = w
        .replace_file(input("pages/Alpha.md", &text))
        .expect("replace");
    assert_eq!(out.status, "ok");
    assert_eq!(count(&index, "SELECT count(*) FROM blocks"), 3);
    assert_eq!(
        strings(
            &index,
            "SELECT name FROM pages WHERE file_id IS NULL AND is_builtin = 0 ORDER BY name"
        ),
        ["linked page", "tag"]
    );
    assert_eq!(
        strings(&index, "SELECT uuid FROM blocks WHERE uuid_source = 1"),
        [U1]
    );
    // parent_id resolved from parent_ord.
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM blocks c JOIN blocks p ON p.id = c.parent_id WHERE p.ord = 1 AND c.ord = 2"
        ),
        1
    );
    // FTS follows through the triggers.
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM blocks_fts WHERE blocks_fts MATCH 'second'"
        ),
        1
    );
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM pages_fts WHERE pages_fts MATCH 'alpha'"
        ),
        1
    );
    // Page time fallback comes from the file times.
    assert_eq!(
        count(&index, "SELECT created_at FROM pages WHERE name = 'alpha'"),
        1_690_000_000_000
    );
    assert_eq!(
        count(&index, "SELECT updated_at FROM pages WHERE name = 'alpha'"),
        1_700_000_000_000
    );
}

#[test]
fn builtin_pages_are_seeded_and_never_collected() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    let builtins = BUILTIN_PAGES.len() as i64;
    assert_eq!(
        count(&index, "SELECT count(*) FROM pages WHERE is_builtin = 1"),
        builtins
    );
    w.replace_file(input("pages/A.md", "- TODO a task\n"))
        .expect("replace");
    w.delete_file("pages/A.md").expect("delete");
    assert_eq!(
        count(&index, "SELECT count(*) FROM pages WHERE is_builtin = 1"),
        builtins
    );
    // Page "A" defined by a file keeps is_builtin and is demoted, not deleted.
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM pages WHERE name = 'a' AND file_id IS NULL"
        ),
        1
    );
    w.shutdown();
    // Seeding is idempotent across writer restarts.
    let _w = writer(&index);
    assert_eq!(
        count(&index, "SELECT count(*) FROM pages WHERE is_builtin = 1"),
        builtins
    );
    assert_eq!(count(&index, "SELECT count(*) FROM pages"), builtins);
}

#[test]
fn placeholder_gc_after_reference_is_removed() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    w.replace_file(input("pages/P.md", "- see [[Ghost/Child]] and #tg\n"))
        .expect("replace");
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM pages WHERE name IN ('ghost','ghost/child','tg')"
        ),
        3
    );
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM pages c JOIN pages p ON p.id = c.namespace_parent_id WHERE c.name = 'ghost/child' AND p.name = 'ghost'"
        ),
        1
    );
    w.replace_file(input("pages/P.md", "- nothing here\n"))
        .expect("replace");
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM pages WHERE is_builtin = 0 AND file_id IS NULL"
        ),
        0
    );
}

#[test]
fn delete_file_demotes_referenced_page_and_collects_the_rest() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    w.replace_file(input("pages/Target.md", "- target body\n"))
        .expect("t");
    w.replace_file(input("pages/Src.md", "- links [[Target]] and [[Orphan]]\n"))
        .expect("s");
    let rx = w.subscribe();
    let out = w
        .delete_file("pages/Target.md")
        .expect("delete")
        .expect("known");
    assert_eq!(out.block_uuids_removed.len(), 1);
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM files WHERE path = 'pages/Target.md'"
        ),
        0
    );
    // Still referenced: stays as a placeholder.
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM pages WHERE name = 'target' AND file_id IS NULL"
        ),
        1
    );
    // Deleting the referrer collects both placeholders.
    w.delete_file("pages/Src.md").expect("delete src");
    assert_eq!(
        count(&index, "SELECT count(*) FROM pages WHERE is_builtin = 0"),
        0
    );
    assert_eq!(count(&index, "SELECT count(*) FROM blocks"), 0);
    assert_eq!(count(&index, "SELECT count(*) FROM blocks_fts_docsize"), 0);
    assert!(w.delete_file("pages/none.md").expect("unknown").is_none());
    let events: Vec<_> = rx.try_iter().collect();
    assert!(
        matches!(&events[0], IndexEvent::FileDeleted { path, .. } if path == "pages/Target.md")
    );
}

#[test]
fn title_change_demotes_the_old_page() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    w.replace_file(input("pages/F.md", "title:: Old Name\n\n- body\n"))
        .expect("1");
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM pages WHERE name = 'old name' AND file_id IS NOT NULL"
        ),
        1
    );
    w.replace_file(input("pages/F.md", "title:: New Name\n\n- body\n"))
        .expect("2");
    assert_eq!(
        count(&index, "SELECT count(*) FROM pages WHERE name = 'old name'"),
        0,
        "unreferenced demoted page is collected"
    );
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM pages WHERE name = 'new name' AND file_id IS NOT NULL"
        ),
        1
    );
}

#[test]
fn duplicate_page_title_is_flagged_and_first_file_owns_the_page() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    w.replace_file(input("pages/a.md", "title:: Same\n\n- one\n"))
        .expect("a");
    let out = w
        .replace_file(input("pages/b.md", "title:: Same\n\n- two\n"))
        .expect("b");
    assert_eq!(out.status, "duplicate_page");
    assert_eq!(
        strings(
            &index,
            "SELECT f.path FROM pages p JOIN files f ON f.id = p.file_id WHERE p.name = 'same'"
        ),
        ["pages/a.md"]
    );
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM diagnostics WHERE kind = 'duplicate_page'"
        ),
        1
    );
    // Both files' blocks stay listed under the page.
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM blocks b JOIN pages p ON p.id = b.page_id WHERE p.name = 'same' AND b.is_pre_block = 0"
        ),
        2
    );
}

#[test]
fn explicit_id_owned_by_another_file_gets_a_fresh_uuid_and_a_diagnostic() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    let t = format!("- block\n  id:: {U1}\n");
    w.replace_file(input("pages/one.md", &t)).expect("one");
    w.replace_file(input("pages/two.md", &t)).expect("two");
    assert_eq!(
        count(
            &index,
            &format!("SELECT count(*) FROM blocks WHERE uuid = '{U1}'")
        ),
        1
    );
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM diagnostics d JOIN files f ON f.id = d.file_id WHERE d.kind = 'duplicate_block_id' AND f.path = 'pages/two.md'"
        ),
        1
    );
    assert_eq!(
        count(&index, "SELECT count(*) FROM blocks WHERE uuid_source = 0"),
        1
    );
}

#[test]
fn uuids_carry_over_across_edits_and_events_report_changes() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    let rx = w.subscribe();
    w.replace_file(input(
        "pages/C.md",
        "- alpha\n- the quick brown fox\n- gamma\n",
    ))
    .expect("1");
    let before = strings(&index, "SELECT uuid FROM blocks ORDER BY ord");
    assert_eq!(before.len(), 3);
    // Edit the middle block, insert one at the end, keep the others.
    w.replace_file(input(
        "pages/C.md",
        "- alpha\n- the quick brown fox jumps\n- gamma\n- brand new\n",
    ))
    .expect("2");
    let after = strings(&index, "SELECT uuid FROM blocks ORDER BY ord");
    assert_eq!(
        &after[..3],
        &before[..],
        "unchanged and edited blocks keep their UUID"
    );
    assert_eq!(
        strings(
            &index,
            "SELECT CAST(uuid_source AS TEXT) FROM blocks ORDER BY ord"
        ),
        // Unchanged blocks keep their row (and source); the edited one is re-inserted as carried.
        ["0", "2", "0", "0"]
    );
    let evs: Vec<_> = rx.try_iter().collect();
    assert_eq!(evs.len(), 2);
    let IndexEvent::FileReplaced {
        block_uuids_added,
        block_uuids_removed,
        ..
    } = &evs[1]
    else {
        panic!("expected FileReplaced");
    };
    assert_eq!(block_uuids_added, &[after[3].clone()]);
    assert!(block_uuids_removed.is_empty());
}

#[test]
fn rename_keeps_uuids_and_moves_the_page() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    w.replace_file(input("pages/Old.md", "- one\n- two\n"))
        .expect("1");
    let before = strings(&index, "SELECT uuid FROM blocks ORDER BY ord");
    let out = w
        .rename_file(
            "pages/Old.md",
            "pages/New.md",
            input("pages/New.md", "- one\n- two\n"),
        )
        .expect("rename")
        .expect("known");
    assert!(out.block_uuids_added.is_empty());
    assert_eq!(
        strings(&index, "SELECT uuid FROM blocks ORDER BY ord"),
        before
    );
    assert_eq!(strings(&index, "SELECT path FROM files"), ["pages/New.md"]);
    assert_eq!(
        count(&index, "SELECT count(*) FROM pages WHERE name = 'old'"),
        0
    );
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM pages WHERE name = 'new' AND file_id IS NOT NULL"
        ),
        1
    );
}

#[test]
fn aliases_and_tags_are_stored_and_search_title_includes_aliases() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    w.replace_file(input(
        "pages/Main.md",
        "alias:: Primary, Other\ntags:: t1\n\n- body\n",
    ))
    .expect("1");
    assert_eq!(count(&index, "SELECT count(*) FROM page_aliases"), 2);
    assert_eq!(count(&index, "SELECT count(*) FROM page_tags"), 1);
    assert_eq!(
        strings(&index, "SELECT search_title FROM pages WHERE name = 'main'"),
        ["main other primary"]
    );
    w.replace_file(input("pages/Main.md", "- body\n"))
        .expect("2");
    assert_eq!(count(&index, "SELECT count(*) FROM page_aliases"), 0);
    assert_eq!(
        strings(&index, "SELECT search_title FROM pages WHERE name = 'main'"),
        ["main"]
    );
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM pages WHERE is_builtin = 0 AND file_id IS NULL"
        ),
        0
    );
}

#[test]
fn writer_returns_the_connection_when_stopped() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    assert!(index.take_writer().is_err());
    w.shutdown();
    assert!(index.take_writer().is_ok());
}

#[test]
#[ignore = "benchmark: run with `cargo test -p bitacora-index --release -- --ignored --nocapture`"]
fn bench_replace_500_block_page() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    let mut text = String::new();
    for i in 0..500 {
        let indent = if i % 5 == 0 { "" } else { "  " };
        text.push_str(&format!(
            "{indent}- block {i} with some words about [[Topic {}]] and #tag{} lorem ipsum dolor sit amet\n",
            i % 20,
            i % 7
        ));
    }
    w.replace_file(input("pages/Big.md", &text)).expect("warm");
    let edited = text.replace("block 250 ", "block 250 edited ");
    let mut samples = Vec::new();
    for round in 0..20 {
        let src = if round % 2 == 0 { &edited } else { &text };
        let inp = input("pages/Big.md", src);
        let t = std::time::Instant::now();
        w.replace_file(inp).expect("replace");
        samples.push(t.elapsed());
    }
    samples.sort();
    let median = samples[samples.len() / 2];
    eprintln!(
        "replace 500-block page: median {median:?}, max {:?}",
        samples[samples.len() - 1]
    );
    assert!(median.as_millis() < 10, "median {median:?} exceeds 10 ms");
}
