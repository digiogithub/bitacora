//! Single-writer guard (BIT-T-0268, AGENTS.md rule 3): graph files are written only by
//! `bitacora-core`'s writer (`editor/fsio.rs`, driven by `editor/flush.rs`).
//!
//! A clippy `disallowed-methods` entry would also hit tests and fixtures in every crate, so the
//! rule is enforced here instead: this test scans the non-test source of the crates that must
//! never touch graph files for file-system mutations. Crates that legitimately write their own
//! non-graph files (app settings, SQLite index, git working copy set-up) are not scanned; a
//! file listed in `ALLOWED` writes something that is not a graph file.

use std::path::{Path, PathBuf};

const FORBIDDEN: &[&str] = &[
    "fs::write(",
    "fs::rename(",
    "fs::copy(",
    "fs::remove_file(",
    "fs::remove_dir",
    "fs::create_dir",
    "File::create(",
    "File::create_new(",
    "OpenOptions",
    "set_permissions(",
];

/// (crate, path relative to the crate's `src`) pairs that may mutate the file system.
const ALLOWED: &[(&str, &str)] = &[
    ("bitacora-core", "editor/fsio.rs"),
    // `reindex --force` deletes the SQLite cache files, never graph files.
    ("bitacora-cli", "cmd/reindex.rs"),
    // API token store under the app config dir, not the graph.
    ("bitacora-mcp", "tokens.rs"),
];

const SCANNED: &[&str] = &[
    "bitacora-core",
    "bitacora-markdown",
    "bitacora-merge",
    "bitacora-watch",
    "bitacora-mcp",
    "bitacora-cli",
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Non-test, non-comment lines of a source file (stops at the first `#[cfg(test)]`).
fn production_lines(text: &str) -> Vec<(usize, &str)> {
    text.lines()
        .enumerate()
        .take_while(|(_, l)| !l.trim_start().starts_with("#[cfg(test)]"))
        .filter(|(_, l)| {
            let t = l.trim_start();
            !t.starts_with("//")
        })
        .map(|(i, l)| (i + 1, l))
        .collect()
}

#[test]
fn only_the_core_writer_mutates_graph_files() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut violations = Vec::new();
    let mut scanned = 0;
    for krate in SCANNED {
        let src = crates_dir.join(krate).join("src");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        for f in files {
            let rel = f
                .strip_prefix(&src)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if ALLOWED.contains(&(*krate, rel.as_str())) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&f) else {
                continue;
            };
            scanned += 1;
            for (n, line) in production_lines(&text) {
                if FORBIDDEN.iter().any(|p| line.contains(p)) {
                    violations.push(format!("{krate}/src/{rel}:{n}: {}", line.trim()));
                }
            }
        }
    }
    assert!(scanned > 20, "guard scanned too few files ({scanned})");
    assert!(
        violations.is_empty(),
        "file-system mutation outside the core writer (AGENTS.md rule 3). Route it through the \
         command queue, or justify it in ALLOWED:\n{}",
        violations.join("\n")
    );
}

#[test]
fn guard_detects_a_violation() {
    let sample = "fn f() {\n    std::fs::write(p, b).ok();\n}\n#[cfg(test)]\nmod t { fn g() { std::fs::write(a, b).ok(); } }\n";
    let hits: Vec<_> = production_lines(sample)
        .into_iter()
        .filter(|(_, l)| FORBIDDEN.iter().any(|p| l.contains(p)))
        .collect();
    assert_eq!(hits.len(), 1);
}
