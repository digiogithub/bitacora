//! Property test: an index kept up to date by incremental watcher events equals an index built
//! from scratch on the final graph (AGENTS.md §8, BIT-T-0046).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use bitacora_config::EffectiveConfig;
use bitacora_core::graph_path::GraphPath;
use bitacora_index::dump::canonical_dump;
use bitacora_index::{FsChange, Indexer, IndexerOptions};
use common::{copy_dir, env_for, write};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

const LINES: [&str; 14] = [
    "- plain block",
    "- block with [[Page A]] and #tagx",
    "- TODO a task [[Page B]]",
    "  - nested child ((6500c1a4-0000-4000-8000-0000000000aa))",
    "- explicit\n  id:: {ID}",
    "- other explicit\n  id:: {ID}",
    "- namespaced [[Ns/Child/Leaf]]",
    "- éclair CAFÉ",
    "- DONE [#A] done thing [[Page A]]",
    "- prop block\n  color:: red\n  tags:: [[Page C]], other",
    "- SCHEDULED: <2026-10-06 Tue>",
    "  - deeper\n    - deepest [[Page B]]",
    "- a longer block that can be edited a little: the quick brown fox jumps over the lazy dog",
    "- embed {{embed [[Page A]]}}",
];

const HEADERS: [&str; 4] = [
    "",
    "alias:: Alias One, Alias Two\ntags:: Page C\n\n",
    "title:: Dup Title\n\n",
    "title:: Page A\n\n",
];

#[derive(Debug, Clone)]
enum Op {
    Edit {
        file: usize,
        line: usize,
        with: usize,
    },
    Insert {
        file: usize,
        line: usize,
        with: usize,
    },
    DropLine {
        file: usize,
        line: usize,
    },
    Create {
        name: usize,
        header: usize,
        lines: Vec<usize>,
    },
    Delete {
        file: usize,
    },
    Rename {
        file: usize,
        name: usize,
    },
    Touch {
        file: usize,
    },
}

fn op_strategy() -> impl Strategy<Value = Op> {
    let n = LINES.len();
    prop_oneof![
        (any::<usize>(), any::<usize>(), 0..n).prop_map(|(file, line, with)| Op::Edit {
            file,
            line,
            with
        }),
        (any::<usize>(), any::<usize>(), 0..n).prop_map(|(file, line, with)| Op::Insert {
            file,
            line,
            with
        }),
        (any::<usize>(), any::<usize>()).prop_map(|(file, line)| Op::DropLine { file, line }),
        (
            0usize..6,
            0..HEADERS.len(),
            prop::collection::vec(0..n, 0..6)
        )
            .prop_map(|(name, header, lines)| Op::Create {
                name,
                header,
                lines
            }),
        any::<usize>().prop_map(|file| Op::Delete { file }),
        (any::<usize>(), 0usize..6).prop_map(|(file, name)| Op::Rename { file, name }),
        any::<usize>().prop_map(|file| Op::Touch { file }),
    ]
}

/// `PROPTEST_CASES` overrides the default so the property can be stress-run (the explicit
/// `cases` in the config would otherwise shadow the environment variable).
fn cases_from_env(default: u32) -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn md_files(root: &std::path::Path) -> Vec<String> {
    let mut v = Vec::new();
    for dir in ["pages", "journals"] {
        let Ok(rd) = std::fs::read_dir(root.join(dir)) else {
            continue;
        };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.ends_with(".md") {
                v.push(format!("{dir}/{n}"));
            }
        }
    }
    v.sort();
    v
}

/// Explicit ids are unique per inserted line: which of two files holding the same `id::` keeps it
/// depends on indexing order (design §2.2), so the property is checked on graphs without clashes.
fn line_text(i: usize) -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0x1_0000);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    LINES[i].replace("{ID}", &format!("6500c1a4-0000-4000-8000-{n:012x}"))
}

fn gp(p: &str) -> GraphPath {
    GraphPath::new(p).expect("path")
}

fn apply(root: &std::path::Path, ix: &Indexer, op: &Op) {
    let files = md_files(root);
    let pick = |i: usize| (!files.is_empty()).then(|| files[i % files.len()].clone());
    match op {
        Op::Edit { file, line, with } | Op::Insert { file, line, with } => {
            let Some(f) = pick(*file) else { return };
            let text = std::fs::read_to_string(root.join(&f)).unwrap_or_default();
            let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
            let at = if lines.is_empty() {
                0
            } else {
                line % lines.len()
            };
            if matches!(op, Op::Edit { .. }) && !lines.is_empty() {
                lines[at] = line_text(*with);
            } else {
                lines.insert(at.min(lines.len()), line_text(*with));
            }
            let mut out = lines.join("\n");
            out.push('\n');
            write(root, &f, &out);
            ix.handle(&FsChange::Modified(gp(&f))).expect("modify");
        }
        Op::DropLine { file, line } => {
            let Some(f) = pick(*file) else { return };
            let text = std::fs::read_to_string(root.join(&f)).unwrap_or_default();
            let mut lines: Vec<&str> = text.lines().collect();
            if lines.is_empty() {
                return;
            }
            lines.remove(line % lines.len());
            let mut out = lines.join("\n");
            out.push('\n');
            write(root, &f, &out);
            ix.handle(&FsChange::Modified(gp(&f))).expect("modify");
        }
        Op::Create {
            name,
            header,
            lines,
        } => {
            let f = format!("pages/gen{name}.md");
            let mut out = HEADERS[*header].to_owned();
            for l in lines {
                out.push_str(&line_text(*l));
                out.push('\n');
            }
            write(root, &f, &out);
            ix.handle(&FsChange::Modified(gp(&f))).expect("create");
        }
        Op::Delete { file } => {
            let Some(f) = pick(*file) else { return };
            std::fs::remove_file(root.join(&f)).expect("rm");
            ix.handle(&FsChange::Deleted(gp(&f))).expect("delete");
        }
        Op::Rename { file, name } => {
            let Some(from) = pick(*file) else { return };
            let to = format!("pages/ren{name}.md");
            if from == to || root.join(&to).exists() {
                return;
            }
            std::fs::rename(root.join(&from), root.join(&to)).expect("mv");
            ix.handle(&FsChange::Renamed {
                from: gp(&from),
                to: gp(&to),
            })
            .expect("rename");
        }
        Op::Touch { file } => {
            let Some(f) = pick(*file) else { return };
            let bytes = std::fs::read(root.join(&f)).expect("read");
            std::fs::write(root.join(&f), bytes).expect("rewrite");
            ix.handle(&FsChange::Modified(gp(&f))).expect("touch");
        }
    }
}

fn dump_of(root: &std::path::Path, incremental: &[Op]) -> (String, String) {
    // Incremental: cold build the starting graph, then apply the ops through events.
    let work = tempfile::tempdir().expect("tmp");
    let g = work.path().join("g");
    copy_dir(root, &g);
    let env_a = env_for(g.clone());
    let index_a = env_a.open();
    let ix_a = Indexer::start(
        &index_a,
        IndexerOptions::new(&g, EffectiveConfig::default()),
    )
    .expect("a");
    ix_a.reconcile().expect("cold a");
    for op in incremental {
        apply(&g, &ix_a, op);
    }
    let a = canonical_dump(&index_a.reader().expect("r")).expect("dump a");

    // From scratch on the final graph.
    let env_b = env_for(g.clone());
    let index_b = env_b.open();
    let ix_b = Indexer::start(
        &index_b,
        IndexerOptions::new(&g, EffectiveConfig::default()),
    )
    .expect("b");
    ix_b.reconcile().expect("cold b");
    let b = canonical_dump(&index_b.reader().expect("r")).expect("dump b");
    (a, b)
}

fn first_diff(a: &str, b: &str) -> String {
    let (la, lb): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    let i = la
        .iter()
        .zip(&lb)
        .position(|(x, y)| x != y)
        .unwrap_or(la.len().min(lb.len()));
    format!(
        "line {i}\n incremental: {:?}\n scratch:     {:?}",
        la.get(i),
        lb.get(i)
    )
}

fn base_graph() -> tempfile::TempDir {
    let t = tempfile::tempdir().expect("tmp");
    let r = t.path();
    write(
        r,
        "pages/Page A.md",
        "alias:: PA\n\n- alpha [[Page B]]\n  id:: 6500c1a4-0000-4000-8000-0000000000aa\n- child\n  - nested #tagx\n",
    );
    write(
        r,
        "pages/Page B.md",
        "- beta\n- ref ((6500c1a4-0000-4000-8000-0000000000aa))\n",
    );
    write(r, "pages/Ns___Child.md", "- namespaced page\n");
    write(r, "pages/dup1.md", "title:: Dup Title\n\n- first\n");
    write(r, "pages/dup2.md", "title:: Dup Title\n\n- second\n");
    write(r, "journals/2026_10_05.md", "- yesterday [[Page A]]\n");
    write(
        r,
        "journals/2026_10_06.md",
        "- today\n  - TODO [#B] thing\n",
    );
    t
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases_from_env(40), failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("property"))), ..ProptestConfig::default() })]

    #[test]
    fn incremental_equals_rebuild_on_a_small_graph(ops in prop::collection::vec(op_strategy(), 1..12)) {
        let base = base_graph();
        let (a, b) = dump_of(base.path(), &ops);
        prop_assert!(a == b, "{}\nops: {ops:?}", first_diff(&a, &b));
    }
}

/// Regression: a generated `id::` once collided with the base graph's explicit id (the counter
/// reached `0xaa`), which makes "who keeps the id" order-dependent (design §2.2) and flaked the
/// property. The id counter now starts far from the fixtures' ids; this test pins the underlying
/// behaviour so a clash shows up as a deterministic result here, not as a flake.
#[test]
fn explicit_id_clash_between_files_is_reported_deterministically() {
    let base = base_graph();
    let (a, b) = dump_of(
        base.path(),
        &[Op::Create {
            name: 0,
            header: 0,
            lines: vec![],
        }],
    );
    assert!(a == b, "{}", first_diff(&a, &b));
    let work = tempfile::tempdir().expect("tmp");
    let g = work.path().join("g");
    copy_dir(base.path(), &g);
    write(
        &g,
        "journals/2026_10_06.md",
        "- clash\n  id:: 6500c1a4-0000-4000-8000-0000000000aa\n",
    );
    let env_a = env_for(g.clone());
    let index_a = env_a.open();
    let ix_a = Indexer::start(
        &index_a,
        IndexerOptions::new(&g, EffectiveConfig::default()),
    )
    .expect("a");
    ix_a.reconcile().expect("cold a");
    // Incremental path: re-handle the clashing file after Page A already owns the id.
    ix_a.handle(&FsChange::Modified(gp("journals/2026_10_06.md")))
        .expect("modify");
    let inc = canonical_dump(&index_a.reader().expect("r")).expect("dump");
    let env_b = env_for(g.clone());
    let index_b = env_b.open();
    let ix_b = Indexer::start(
        &index_b,
        IndexerOptions::new(&g, EffectiveConfig::default()),
    )
    .expect("b");
    ix_b.reconcile().expect("cold b");
    let scratch = canonical_dump(&index_b.reader().expect("r")).expect("dump");
    assert!(inc == scratch, "{}", first_diff(&inc, &scratch));
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases_from_env(12), failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("property"))), ..ProptestConfig::default() })]

    #[test]
    fn incremental_equals_rebuild_on_the_edge_cases_fixture(ops in prop::collection::vec(op_strategy(), 1..8)) {
        let (a, b) = dump_of(&bitacora_testkit::graph("edge-cases"), &ops);
        prop_assert!(a == b, "{}\nops: {ops:?}", first_diff(&a, &b));
    }
}
