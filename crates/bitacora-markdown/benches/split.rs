//! Criterion benchmarks for parsing and serializing a large page. Run with
//! `cargo bench -p bitacora-markdown`; CI only compiles them (`cargo bench --no-run`).

use bitacora_markdown::block::analyze;
use bitacora_markdown::properties::PropertyConfig;
use bitacora_markdown::{Document, ParserOptions, WriteOptions, content_of, serialize, split};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

/// About 10k blocks (~700 KiB): nested bullets, properties, refs, tasks, a logbook and code.
fn large_page() -> String {
    let mut s = String::from("title:: Bench page\ntags:: bench\n\n");
    for i in 0..1000 {
        s.push_str(&format!(
            "- TODO [#A] Block {i} with [[Page {i}]] and #tag{i} ((6500c1a4-0000-4000-8000-{i:012}))\n  id:: 6500c1a4-0000-4000-8000-{i:012}\n  SCHEDULED: <2024-01-01 Mon .+1d>\n  :LOGBOOK:\n  CLOCK: [2024-01-01 Mon 08:00:00]--[2024-01-01 Mon 08:30:00] =>  00:30:00\n  :END:\n"
        ));
        s.push_str("\t- child **bold** `code` https://example.com/a\n");
        s.push_str("\t\t- grandchild\n\t\t  with continuation\n");
        s.push_str("\t- ```rust\n\t  fn main() {}\n\t  ```\n");
        for j in 0..6 {
            s.push_str(&format!("\t- leaf {j} key:: value {j}\n"));
        }
    }
    s
}

fn bench_parse_serialize(c: &mut Criterion) {
    let text = large_page();
    let bytes = text.as_bytes();
    let mut g = c.benchmark_group("large_page");
    g.throughput(Throughput::Bytes(bytes.len() as u64));
    g.bench_function("split", |b| b.iter(|| split(black_box(bytes))));
    g.bench_function("document_parse", |b| {
        b.iter(|| Document::parse(black_box(bytes.to_vec())));
    });
    let doc = Document::parse(bytes.to_vec());
    g.bench_function("serialize_clean", |b| {
        b.iter(|| serialize(black_box(&doc), &WriteOptions::default()));
    });
    g.bench_function("serialize_one_edit", |b| {
        b.iter(|| {
            let mut d = doc.clone();
            d.set_block_content(5, "edited\nbody");
            serialize(black_box(&d), &WriteOptions::default())
        });
    });
    let outline = split(bytes);
    let cfg = PropertyConfig::default();
    g.bench_function("analyze_all_blocks", |b| {
        b.iter(|| {
            for blk in &outline.blocks {
                black_box(analyze(
                    &content_of(bytes, blk),
                    &cfg,
                    ParserOptions::default(),
                ));
            }
        });
    });
    g.finish();
}

criterion_group!(benches, bench_parse_serialize);
criterion_main!(benches);
