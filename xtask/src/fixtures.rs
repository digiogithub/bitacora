//! `cargo xtask fixtures update|verify`: SHA-256 manifest of `fixtures/graphs/**`.
//!
//! The manifest (`fixtures/graphs/MANIFEST.sha256`) has one `<sha256>  <path>` line per
//! file, sorted by path, with `/` separators. `verify` fails on any changed, missing or
//! extra file, which proves that `.gitattributes` kept the fixture bytes exact.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

const MANIFEST_NAME: &str = "MANIFEST.sha256";

/// Mismatch between the manifest and the files on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    Changed(String),
    Missing(String),
    Extra(String),
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Problem::Changed(p) => write!(f, "changed: {p}"),
            Problem::Missing(p) => write!(f, "missing: {p}"),
            Problem::Extra(p) => write!(f, "extra (not in manifest): {p}"),
        }
    }
}

fn graphs_dir() -> PathBuf {
    // `xtask` lives at <root>/xtask.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/graphs")
}

pub fn run(sub: Option<&str>) -> Result<bool> {
    let root = graphs_dir();
    match sub {
        Some("update") => {
            let map = hash_tree(&root)?;
            std::fs::write(root.join(MANIFEST_NAME), render(&map)).context("writing manifest")?;
            println!("fixtures update: {} files recorded", map.len());
            Ok(true)
        }
        Some("verify") => {
            let manifest = std::fs::read_to_string(root.join(MANIFEST_NAME)).context(
                "reading fixtures/graphs/MANIFEST.sha256 (run `cargo xtask fixtures update`)",
            )?;
            let expected = parse(&manifest)?;
            let actual = hash_tree(&root)?;
            let problems = compare(&expected, &actual);
            if problems.is_empty() {
                println!("fixtures verify: OK ({} files)", actual.len());
                Ok(true)
            } else {
                for p in &problems {
                    eprintln!("fixtures verify: {p}");
                }
                eprintln!("fixtures verify: {} problem(s) found", problems.len());
                Ok(false)
            }
        }
        _ => bail!("usage: cargo xtask fixtures <update|verify>"),
    }
}

/// Hash every file under `root` (except the manifest) keyed by `/`-separated relative path.
pub fn hash_tree(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    for entry in WalkDir::new(root).sort_by_file_name() {
        let entry = entry.context("walking fixtures")?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(root).context("relative path")?;
        let rel = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        if rel == MANIFEST_NAME {
            continue;
        }
        let bytes = std::fs::read(entry.path()).with_context(|| format!("reading {rel}"))?;
        map.insert(rel, hex(&Sha256::digest(&bytes)));
    }
    Ok(map)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Render the manifest text (sorted because `BTreeMap` is).
pub fn render(map: &BTreeMap<String, String>) -> String {
    map.iter().map(|(p, h)| format!("{h}  {p}\n")).collect()
}

/// Parse manifest text into path -> hash.
pub fn parse(text: &str) -> Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let Some((hash, path)) = line.split_once("  ") else {
            bail!("manifest line {} is malformed: `{line}`", i + 1);
        };
        map.insert(path.to_string(), hash.to_string());
    }
    Ok(map)
}

/// Compare expected (manifest) against actual (disk).
pub fn compare(
    expected: &BTreeMap<String, String>,
    actual: &BTreeMap<String, String>,
) -> Vec<Problem> {
    let mut out = Vec::new();
    for (path, hash) in expected {
        match actual.get(path) {
            None => out.push(Problem::Missing(path.clone())),
            Some(h) if h != hash => out.push(Problem::Changed(path.clone())),
            Some(_) => {}
        }
    }
    for path in actual.keys() {
        if !expected.contains_key(path) {
            out.push(Problem::Extra(path.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(items: &[(&str, &str)]) -> BTreeMap<String, String> {
        items
            .iter()
            .map(|(p, h)| (p.to_string(), h.to_string()))
            .collect()
    }

    #[test]
    fn render_parse_round_trip() {
        let m = map(&[("a/b.md", "aa"), ("c d.md", "bb")]);
        assert_eq!(parse(&render(&m)).expect("parse"), m);
    }

    #[test]
    fn compare_reports_changed_missing_extra() {
        let expected = map(&[("a", "1"), ("b", "2"), ("c", "3")]);
        let actual = map(&[("a", "1"), ("b", "X"), ("d", "4")]);
        assert_eq!(
            compare(&expected, &actual),
            vec![
                Problem::Changed("b".into()),
                Problem::Missing("c".into()),
                Problem::Extra("d".into()),
            ]
        );
    }

    #[test]
    fn one_byte_change_is_detected_with_file_name() {
        let dir = std::env::temp_dir().join(format!("bitacora-xtask-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("g")).expect("mkdir");
        std::fs::write(dir.join("g/p.md"), b"- a\n").expect("write");
        let before = hash_tree(&dir).expect("hash");
        std::fs::write(dir.join("g/p.md"), b"- b\n").expect("write");
        let after = hash_tree(&dir).expect("hash");
        let problems = compare(&before, &after);
        std::fs::remove_dir_all(&dir).expect("cleanup");
        assert_eq!(problems, vec![Problem::Changed("g/p.md".into())]);
    }

    #[test]
    fn malformed_manifest_is_rejected() {
        assert!(parse("nospaces\n").is_err());
    }
}
