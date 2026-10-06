//! Criterion benchmark for `merge_page` on large pages (BIT-US-0055). Run with
//! `cargo bench -p bitacora-merge`; CI only compiles it (`cargo bench --no-run`).
//!
//! Scenarios: both devices edit disjoint blocks (the common case), each side appends, and one side
//! reorders a quarter of the top-level subtrees while the other edits (exercises the matcher's
//! global fuzzy pass).

use bitacora_merge::{MergeEnv, merge_page};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

/// A page with `n` top-level blocks, each with two children, properties and a logbook on some.
fn page(n: usize) -> String {
    let mut s = String::from("title:: Bench page\ntags:: bench\n\n");
    for i in 0..n {
        s.push_str(&format!(
            "- Block number {i} talks about subject {} and mentions [[Page {}]]\n",
            i * 7 % 101,
            i % 50
        ));
        if i % 5 == 0 {
            s.push_str(&format!(
                "  id:: 6500c1a4-0000-4000-8000-{i:012}\n  :LOGBOOK:\n  CLOCK: [2024-01-01 Mon 08:00]--[2024-01-01 Mon 08:30] =>  00:30:00\n  :END:\n"
            ));
        }
        s.push_str(&format!("\t- child a of {i} with some words to fuzz on\n"));
        s.push_str(&format!("\t- child b of {i}\n\t  continuation line {i}\n"));
    }
    s
}

fn edit(base: &str, every: usize, offset: usize, suffix: &str) -> String {
    let mut out = String::with_capacity(base.len() + 1024);
    let mut block = 0usize;
    for line in base.split_inclusive('\n') {
        if line.starts_with("- Block number") {
            block += 1;
        }
        if line.starts_with("- Block number") && block % every == offset {
            out.push_str(line.trim_end_matches('\n'));
            out.push_str(suffix);
            out.push('\n');
        } else {
            out.push_str(line);
        }
    }
    out
}

fn reorder(base: &str, every: usize) -> String {
    let mut blocks: Vec<String> = Vec::new();
    let mut head = String::new();
    for line in base.split_inclusive('\n') {
        if line.starts_with("- ") {
            blocks.push(String::new());
        }
        match blocks.last_mut() {
            Some(b) => b.push_str(line),
            None => head.push_str(line),
        }
    }
    let n = blocks.len();
    for i in (0..n).step_by(every) {
        let j = n - 1 - i;
        if i < j {
            blocks.swap(i, j);
        }
    }
    head + &blocks.concat()
}

fn bench_merge(c: &mut Criterion) {
    let env = MergeEnv::new();
    let mut g = c.benchmark_group("merge_page");
    g.sample_size(10);
    for n in [1_000usize, 5_000] {
        let base = page(n);
        let ours = edit(&base, 20, 3, " (ours)");
        let theirs = edit(&base, 20, 11, " (theirs)");
        g.bench_with_input(BenchmarkId::new("disjoint_edits", n), &n, |b, _| {
            b.iter(|| merge_page(black_box(&base), black_box(&ours), black_box(&theirs), &env));
        });
        let appended_o = format!("{base}- appended by ours\n");
        let appended_t = format!("{base}- appended by theirs\n");
        g.bench_with_input(BenchmarkId::new("both_append", n), &n, |b, _| {
            b.iter(|| {
                merge_page(
                    black_box(&base),
                    black_box(&appended_o),
                    black_box(&appended_t),
                    &env,
                )
            });
        });
        let moved = reorder(&base, 4);
        g.bench_with_input(BenchmarkId::new("reorder_vs_edit", n), &n, |b, _| {
            b.iter(|| {
                merge_page(
                    black_box(&base),
                    black_box(&moved),
                    black_box(&theirs),
                    &env,
                )
            });
        });
    }
    g.finish();
}

criterion_group!(benches, bench_merge);
criterion_main!(benches);
