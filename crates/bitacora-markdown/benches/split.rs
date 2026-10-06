//! Template: `criterion` benchmark. Run with `cargo bench -p bitacora-markdown`;
//! CI only compiles it (`cargo bench --no-run`).

#[path = "../tests/common/mod.rs"]
mod common;

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn bench_split(c: &mut Criterion) {
    let text = "- block\n  continuation\r\n\t- child\n".repeat(1000);
    c.bench_function("split_lines_30k", |b| {
        b.iter(|| common::split_lines(black_box(&text)));
    });
}

criterion_group!(benches, bench_split);
criterion_main!(benches);
