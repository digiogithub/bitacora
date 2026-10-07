#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]
//! Shared helpers for the writer/reconcile tests.

use std::path::{Path, PathBuf};

use bitacora_config::EffectiveConfig;
use bitacora_core::graph_path::GraphPath;
use bitacora_index::{
    FileInput, FileKind, Index, IndexLocation, IndexWriter, OpenOptions, ParseConfig, WriteOptions,
    parse,
};

pub struct Env {
    pub tmp: tempfile::TempDir,
    pub graph: PathBuf,
    pub data: PathBuf,
}

pub fn env() -> Env {
    let tmp = tempfile::tempdir().expect("tempdir");
    let graph = tmp.path().join("graph");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&graph).expect("graph dir");
    Env { tmp, graph, data }
}

/// An environment whose graph is an existing directory (data dir in a fresh temp dir).
pub fn env_for(graph: PathBuf) -> Env {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data = tmp.path().join("data");
    Env { tmp, graph, data }
}

/// Recursively copy a directory.
pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for e in std::fs::read_dir(from).expect("read_dir") {
        let e = e.expect("entry");
        let dest = to.join(e.file_name());
        if e.file_type().expect("type").is_dir() {
            copy_dir(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), dest).expect("copy");
        }
    }
}

impl Env {
    pub fn open(&self) -> Index {
        self.open_with(&EffectiveConfig::default())
    }

    pub fn open_with(&self, cfg: &EffectiveConfig) -> Index {
        let loc = IndexLocation::in_data_dir(&self.data, &self.graph).expect("location");
        Index::open(loc, OpenOptions::for_config(&self.graph, cfg)).expect("open")
    }
}

pub fn writer(index: &Index) -> IndexWriter {
    IndexWriter::spawn(
        index.take_writer().expect("writer"),
        WriteOptions::default(),
    )
    .expect("spawn")
}

pub fn input(path: &str, text: &str) -> FileInput {
    let cfg = EffectiveConfig::default();
    let gp = GraphPath::new(path).expect("path");
    let parsed = parse(&gp, text.as_bytes(), &ParseConfig::new(&cfg));
    FileInput {
        kind: FileKind::classify(&gp, &cfg),
        path: gp,
        size: text.len() as u64,
        mtime_ns: 1_700_000_000_000_000_000,
        birth_ns: Some(1_690_000_000_000_000_000),
        parsed,
    }
}

pub fn count(index: &Index, sql: &str) -> i64 {
    index
        .reader()
        .expect("reader")
        .query_row(sql, [], |r| r.get(0))
        .expect("count")
}

pub fn strings(index: &Index, sql: &str) -> Vec<String> {
    let r = index.reader().expect("reader");
    let mut st = r.prepare(sql).expect("prepare");
    st.query_map([], |row| row.get::<_, String>(0))
        .expect("query")
        .map(|x| x.expect("row"))
        .collect()
}

pub fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
    std::fs::write(p, text).expect("write");
}

pub mod synth {
    //! Deterministic synthetic Logseq graph generator (BIT-T-0047).

    use std::fmt::Write as _;
    use std::path::Path;

    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 33
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    const WORDS: [&str; 24] = [
        "project", "meeting", "idea", "review", "budget", "design", "release", "bug", "plan",
        "research", "draft", "notes", "customer", "roadmap", "sprint", "backlog", "metric",
        "decision", "risk", "launch", "travel", "reading", "health", "garden",
    ];

    /// Shape of the generated graph.
    #[derive(Clone, Copy)]
    pub struct Spec {
        pub pages: usize,
        pub journals: usize,
        pub blocks_per_page: usize,
        pub seed: u64,
    }

    impl Spec {
        /// About 540 000 blocks (the "large" preset of BIT-T-0336).
        pub const fn large() -> Self {
            Self {
                pages: 8000,
                journals: 2000,
                blocks_per_page: 60,
                seed: 13,
            }
        }

        /// About 5 000 blocks (CI smoke tests).
        pub const fn small() -> Self {
            Self {
                pages: 150,
                journals: 100,
                blocks_per_page: 30,
                seed: 5,
            }
        }

        /// About 50 000 blocks.
        pub const fn fifty_k() -> Self {
            Self {
                pages: 800,
                journals: 400,
                blocks_per_page: 50,
                seed: 7,
            }
        }
    }

    fn block_text(rng: &mut Lcg, spec: &Spec, out: &mut String, indent: usize, n: usize) {
        let pad = "  ".repeat(indent);
        let mut line = format!("{pad}- ");
        if rng.below(12) == 0 {
            line.push_str("TODO ");
        }
        for _ in 0..(4 + rng.below(8)) {
            line.push_str(WORDS[rng.below(WORDS.len() as u64) as usize]);
            line.push(' ');
        }
        if rng.below(3) == 0 {
            let _ = write!(line, "[[Page {}]] ", rng.below(spec.pages as u64));
        }
        if rng.below(5) == 0 {
            let _ = write!(line, "#{} ", WORDS[rng.below(WORDS.len() as u64) as usize]);
        }
        let _ = write!(line, "item {n}");
        out.push_str(line.trim_end());
        out.push('\n');
        if rng.below(15) == 0 {
            let _ = writeln!(out, "{pad}  note:: value {}", rng.below(100));
        }
    }

    fn body(rng: &mut Lcg, spec: &Spec, blocks: usize) -> String {
        let mut s = String::new();
        let mut depth = 0usize;
        for n in 0..blocks {
            match rng.below(6) {
                0 if depth < 3 => depth += 1,
                1 if depth > 0 => depth -= 1,
                _ => {}
            }
            block_text(rng, spec, &mut s, depth, n);
        }
        s
    }

    /// Write one page with `blocks` blocks (the "5,000-block page" of the UI benchmarks).
    pub fn generate_big_page(root: &Path, title: &str, blocks: usize, seed: u64) {
        let spec = Spec {
            pages: 5000,
            journals: 0,
            blocks_per_page: blocks,
            seed,
        };
        let text = body(&mut Lcg(seed), &spec, blocks);
        super::write(root, &format!("pages/{title}.md"), &text);
    }

    /// Write one journal file per day for the last `days` days (today included) with `blocks`
    /// blocks each, so the journals feed of the app has recent content.
    pub fn generate_recent_journals(root: &Path, days: i64, blocks: usize, seed: u64) {
        let spec = Spec {
            pages: 5000,
            journals: 0,
            blocks_per_page: blocks,
            seed,
        };
        let today = i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() / 86_400),
        )
        .unwrap_or(0);
        for back in 0..days {
            let (y, m, d) = civil_from_days(today - back);
            let text = body(&mut Lcg(seed + back as u64), &spec, blocks);
            super::write(root, &format!("journals/{y}_{m:02}_{d:02}.md"), &text);
        }
    }

    /// Days since 1970-01-01 to (year, month, day) in the proleptic Gregorian calendar.
    fn civil_from_days(z: i64) -> (i64, i64, i64) {
        let z = z + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        (yoe + era * 400 + i64::from(m <= 2), m, d)
    }

    /// Write the graph under `root`; returns the number of blocks written.
    pub fn generate(root: &Path, spec: &Spec) -> usize {
        let mut rng = Lcg(spec.seed);
        let mut total = 0;
        for i in 0..spec.pages {
            let mut text = String::new();
            if i % 10 == 0 {
                let _ = writeln!(
                    text,
                    "tags:: {}\nalias:: Alias {i}\n",
                    WORDS[i % WORDS.len()]
                );
            }
            text.push_str(&body(&mut rng, spec, spec.blocks_per_page));
            total += spec.blocks_per_page;
            super::write(root, &format!("pages/Page {i}.md"), &text);
        }
        for d in 0..spec.journals {
            let (y, m, day) = (2000 + d / 336, 1 + (d / 28) % 12, 1 + d % 28);
            let text = body(&mut rng, spec, spec.blocks_per_page / 2);
            total += spec.blocks_per_page / 2;
            super::write(root, &format!("journals/{y}_{m:02}_{day:02}.md"), &text);
        }
        total
    }
}
