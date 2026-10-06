//! Differential test over the WHOLE fixture corpus (`fixtures/graphs/**` and `fixtures/markdown/**`)
//! against the mldoc 1.5.7 oracle (`tools/mldoc-diff/corpus.js`).
//!
//! Per block it compares: start offset, raw level, marker, priority, property keys of the effective
//! property group, referenced pages/tags and referenced block ids. Known divergences live in
//! `fixtures/markdown/corpus-allowlist.txt` (one `path#block:field | reason` per line, see the
//! comments there); an allowlist entry that no longer diverges is itself a failure so the list
//! cannot rot.
//!
//! The test needs `node` and `npm install` in `tools/mldoc-diff`. It is skipped (with a note) when
//! they are missing, or when `BITACORA_SKIP_MLDOC=1`. Set `BITACORA_REQUIRE_MLDOC=1` (CI jobs that
//! install node) to turn "oracle unavailable" into a failure. `BITACORA_MLDOC_REPORT=1` prints
//! every divergence instead of failing, to refresh the allowlist.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use bitacora_markdown::block::analyze;
use bitacora_markdown::properties::PropertyConfig;
use bitacora_markdown::{ParserOptions, build_tree, content_of, split};
use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn md_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            md_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "md") {
            out.push(p);
        }
    }
}

fn oracle_available() -> Result<(), String> {
    if !root().join("tools/mldoc-diff/node_modules/mldoc").is_dir() {
        return Err("tools/mldoc-diff/node_modules/mldoc missing (run `npm install` there)".into());
    }
    match Command::new("node").arg("--version").output() {
        Ok(o) if o.status.success() => Ok(()),
        _ => Err("`node` not found".into()),
    }
}

fn oracle(file: &Path) -> Result<Value, String> {
    let out = Command::new("node")
        .arg(root().join("tools/mldoc-diff/corpus.js"))
        .arg(file)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn norm_key(k: &str) -> String {
    k.to_lowercase().replace('-', "_")
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v.dedup();
    v
}

/// All divergences of one file as `block:field -> detail` (`block` is `pre` or the block index).
fn compare(path: &Path, want: &Value) -> BTreeMap<String, String> {
    let mut diffs = BTreeMap::new();
    let input = std::fs::read(path).expect("read fixture");
    let outline = split(&input);
    let links = build_tree(&outline.blocks);
    let _ = links;
    let expected = want["blocks"].as_array().expect("blocks");
    if outline.blocks.len() != expected.len() {
        let ours: Vec<_> = outline.blocks.iter().map(|b| b.span.start).collect();
        let theirs: Vec<_> = expected.iter().map(|e| e["start"].as_u64()).collect();
        diffs.insert(
            "all:count".into(),
            format!(
                "ours {} {ours:?} vs mldoc {} {theirs:?}",
                ours.len(),
                theirs.len()
            ),
        );
        return diffs;
    }
    let cfg = PropertyConfig::default();
    for (i, (b, e)) in outline.blocks.iter().zip(expected).enumerate() {
        let mut check = |field: &str, ours: String, theirs: String| {
            if ours != theirs {
                diffs.insert(
                    format!("{i}:{field}"),
                    format!("ours {ours} vs mldoc {theirs}"),
                );
            }
        };
        let content = content_of(&input, b);
        let a = analyze(&content, &cfg, ParserOptions::default());
        check("start", b.span.start.to_string(), e["start"].to_string());
        check("level", b.raw_level.to_string(), e["level"].to_string());
        check(
            "marker",
            format!("{:?}", a.head.marker.map(|m| m.as_str())),
            format!("{:?}", e["marker"].as_str()),
        );
        check(
            "priority",
            format!("{:?}", a.head.priority.map(|c| c.to_string())),
            format!("{:?}", e["priority"].as_str()),
        );
        let ours_props: Vec<String> = a
            .properties
            .effective()
            .map(|g| {
                g.lines
                    .iter()
                    .filter(|l| l.valid)
                    .map(|l| norm_key(&l.key_raw))
                    .collect()
            })
            .unwrap_or_default();
        let their_props: Vec<String> = strings(&e["props"]).iter().map(|k| norm_key(k)).collect();
        check(
            "props",
            format!("{ours_props:?}"),
            format!("{their_props:?}"),
        );
        check(
            "pages",
            format!("{:?}", sorted(a.refs.content.pages.clone())),
            format!("{:?}", sorted(strings(&e["pages"]))),
        );
        check(
            "blockrefs",
            format!("{:?}", sorted(a.refs.content.block_refs.clone())),
            format!("{:?}", sorted(strings(&e["blocks"]))),
        );
    }
    diffs
}

type Allowlist = BTreeMap<String, String>;

fn load_allowlist() -> Allowlist {
    let text = std::fs::read_to_string(root().join("fixtures/markdown/corpus-allowlist.txt"))
        .unwrap_or_default();
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (key, reason) = l.split_once('|').unwrap_or((l, ""));
            (key.trim().to_owned(), reason.trim().to_owned())
        })
        .collect()
}

#[test]
fn corpus_matches_mldoc_oracle() {
    if std::env::var_os("BITACORA_SKIP_MLDOC").is_some_and(|v| v == "1") {
        eprintln!("skipped: BITACORA_SKIP_MLDOC=1");
        return;
    }
    if let Err(why) = oracle_available() {
        assert!(
            std::env::var_os("BITACORA_REQUIRE_MLDOC").is_none_or(|v| v != "1"),
            "mldoc oracle required but unavailable: {why}"
        );
        eprintln!("skipped: mldoc oracle unavailable: {why}");
        return;
    }

    let mut files = Vec::new();
    md_files(&root().join("fixtures/graphs"), &mut files);
    md_files(&root().join("fixtures/markdown"), &mut files);
    assert!(files.len() > 60, "corpus too small: {}", files.len());

    let results: Mutex<BTreeMap<String, BTreeMap<String, String>>> = Mutex::new(BTreeMap::new());
    let errors: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some(f) = files.get(i) else { break };
                    let rel = f
                        .strip_prefix(root().join("fixtures"))
                        .unwrap_or(f)
                        .to_string_lossy()
                        .replace('\\', "/");
                    match oracle(f) {
                        Ok(want) => {
                            let d = compare(f, &want);
                            if !d.is_empty() {
                                results.lock().expect("lock").insert(rel, d);
                            }
                        }
                        Err(e) => errors.lock().expect("lock").push(format!("{rel}: {e}")),
                    }
                }
            });
        }
    });
    let errors = errors.into_inner().expect("lock");
    assert!(errors.is_empty(), "oracle failed: {errors:#?}");
    let results = results.into_inner().expect("lock");

    if std::env::var_os("BITACORA_MLDOC_REPORT").is_some() {
        for (file, diffs) in &results {
            for (k, d) in diffs {
                eprintln!("{file}#{k} | {d}");
            }
        }
        return;
    }

    let allow = load_allowlist();
    let mut unexpected = Vec::new();
    let mut seen = BTreeSet::new();
    for (file, diffs) in &results {
        for (k, d) in diffs {
            let key = format!("{file}#{k}");
            if allow.contains_key(&key) {
                seen.insert(key);
            } else {
                unexpected.push(format!("{key} | {d}"));
            }
        }
    }
    let stale: Vec<_> = allow.keys().filter(|k| !seen.contains(*k)).collect();
    assert!(
        unexpected.is_empty(),
        "divergences from mldoc not in the allowlist:\n{}",
        unexpected.join("\n")
    );
    assert!(stale.is_empty(), "stale allowlist entries: {stale:#?}");
}
