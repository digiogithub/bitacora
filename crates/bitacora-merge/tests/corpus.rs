//! Real-graph corpus test (BIT-T-0371). For every page of `fixtures/graphs/**` it simulates two
//! devices editing the page concurrently (random block-level edits from a seeded generator) and
//! checks the merge invariants:
//!
//! * no conflict markers are introduced and the output round-trips through the parser;
//! * `merge(b, x, x) == x`, `merge(b, b, x) == x`, `merge(b, x, b) == x`;
//! * every token one device wrote is in the output or reported in a conflict;
//! * lines of blocks neither device touched are byte-identical in the output;
//! * swapping ours/theirs yields the same conflict set (and the same lines when conflict free
//!   and no competing structural note was recorded);
//! * re-merging the result with the side that already contributed it changes nothing.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use bitacora_markdown::{Document, WriteOptions, serialize};
use bitacora_merge::{MergeEnv, MergeResult, NoteKind, merge_page};
use proptest::prelude::*;

fn pages() -> &'static Vec<(PathBuf, String)> {
    static PAGES: OnceLock<Vec<(PathBuf, String)>> = OnceLock::new();
    PAGES.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/graphs");
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "md")
                    && let Ok(t) = std::fs::read_to_string(&p)
                {
                    out.push((p, t));
                }
            }
        }
        out.sort();
        out
    })
}

/// Tiny deterministic generator (splitmix64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// A line of a device's copy: its text (with terminator) and the base line it came from.
#[derive(Clone)]
struct L {
    text: String,
    base: Option<usize>,
    /// Inside a fenced code block or `#+BEGIN_` region: never a block boundary.
    inert: bool,
}

/// Marks the lines strictly inside fenced code / `#+BEGIN_` regions of the base page.
fn inert_lines(base: &[String]) -> Vec<bool> {
    let mut out = vec![false; base.len()];
    let mut open: Option<&str> = None;
    for (i, l) in base.iter().enumerate() {
        let t = l.trim().trim_start_matches("- ").trim_start();
        match open {
            None => {
                if t.starts_with("```") {
                    open = Some("```");
                } else if t.to_ascii_uppercase().starts_with("#+BEGIN_") {
                    open = Some("#+END_");
                }
            }
            Some(close) => {
                out[i] = true;
                let closes = if close == "```" {
                    t.starts_with("```")
                } else {
                    t.to_ascii_uppercase().starts_with(close)
                };
                if closes {
                    open = None;
                }
            }
        }
    }
    out
}

struct Device {
    lines: Vec<L>,
    /// Base lines this device touched (edited blocks, deleted subtrees).
    touched: BTreeSet<usize>,
    tokens: Vec<String>,
    /// A delete or move happened: fuzzy matching may legitimately pair look-alike blocks.
    destructive: bool,
}

fn indent_of(s: &str) -> usize {
    s.len() - s.trim_start_matches([' ', '\t']).len()
}

/// A line that starts a block: a bullet, or an unindented `## Heading`.
fn starts_block(s: &str) -> bool {
    is_bullet(s) || (indent_of(s) == 0 && s.starts_with('#') && !s.starts_with("#+"))
}

fn starts_block_l(l: &L) -> bool {
    !l.inert && starts_block(&l.text)
}

fn is_bullet(s: &str) -> bool {
    s.trim_start_matches([' ', '\t']).starts_with("- ")
}

/// Indices of bullet lines that are safe to edit (outside fenced code and drawers), per base.
fn bullets(base: &[&str], inert: &[bool]) -> Vec<usize> {
    let mut fence = false;
    let mut v = Vec::new();
    for (i, l) in base.iter().enumerate() {
        let t = l.trim_start_matches([' ', '\t']);
        if t.trim_start_matches("- ").starts_with("```") {
            fence = !fence;
            continue;
        }
        // Property-only blocks (`- key:: v`) are skipped (volatile keys merge last-writer-wins), and
        // so are quote blocks (`- > text`), whose property placement is ambiguous.
        let first = t.trim_start_matches("- ");
        let prop_only = first.contains(":: ") || first.starts_with('>');
        if !fence && !inert[i] && is_bullet(l) && l.trim().len() > 1 && !prop_only {
            v.push(i);
        }
    }
    v
}

impl Device {
    fn new(base: &[String]) -> Self {
        let inert = inert_lines(base);
        Self {
            lines: base
                .iter()
                .enumerate()
                .map(|(i, t)| L {
                    text: t.clone(),
                    base: Some(i),
                    inert: inert[i],
                })
                .collect(),
            touched: BTreeSet::new(),
            tokens: Vec::new(),
            destructive: false,
        }
    }

    fn pos(&self, base_idx: usize) -> Option<usize> {
        self.lines.iter().position(|l| l.base == Some(base_idx))
    }

    /// End (exclusive) of the block starting at `p`: until the next bullet line.
    fn block_end(&self, p: usize) -> usize {
        let mut e = p + 1;
        while e < self.lines.len() && !starts_block_l(&self.lines[e]) {
            e += 1;
        }
        e
    }

    /// End (exclusive) of the subtree starting at `p`.
    fn subtree_end(&self, p: usize) -> usize {
        let ind = indent_of(&self.lines[p].text);
        let mut e = p + 1;
        while e < self.lines.len()
            && (!starts_block_l(&self.lines[e])
                || (!self.lines[e].inert
                    && is_bullet(&self.lines[e].text)
                    && indent_of(&self.lines[e].text) > ind))
        {
            e += 1;
        }
        e
    }

    fn touch_block(&mut self, p: usize, e: usize) {
        for l in &self.lines[p..e] {
            if let Some(b) = l.base {
                self.touched.insert(b);
            }
        }
    }

    fn apply(&mut self, rng: &mut Rng, cand: &[usize], dev: &str, n: usize) {
        if cand.is_empty() {
            return;
        }
        let bi = cand[rng.below(cand.len())];
        let Some(p) = self.pos(bi) else {
            return; // already deleted by this device
        };
        let token = format!("zq{dev}{n}zq");
        let eol = if self.lines[p].text.ends_with("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        if !self.lines[p].text.ends_with('\n') {
            self.lines[p].text.push('\n');
        }
        let ind: String = self.lines[p]
            .text
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        match rng.below(7) {
            0 | 1 => {
                // Edit the first line.
                let line = &mut self.lines[p].text;
                let body = line.trim_end_matches(['\r', '\n']).to_owned();
                *line = format!("{body} {token}{eol}");
                let e = self.block_end(p);
                self.touch_block(p, e);
                self.tokens.push(token);
            }
            2 => {
                // Insert a sibling after the subtree.
                let e = self.subtree_end(p);
                if e == self.lines.len() && !self.lines[e - 1].text.ends_with('\n') {
                    self.lines[e - 1].text.push('\n');
                }
                self.lines.insert(
                    e,
                    L {
                        text: format!("{ind}- new block {token} written here{eol}"),
                        base: None,
                        inert: false,
                    },
                );
                self.tokens.push(token);
            }
            3 => {
                // Delete the subtree.
                self.destructive = true;
                let e = self.subtree_end(p);
                self.touch_block(p, e);
                self.lines.drain(p..e);
            }
            4 => {
                // Add a user property.
                let next = self.lines.get(p + 1).map_or("", |l| l.text.as_str());
                if is_bullet(next) || next.is_empty() || next.trim_start().starts_with(':') {
                    let e = self.block_end(p);
                    self.touch_block(p, e);
                    self.lines.insert(
                        p + 1,
                        L {
                            text: format!("{ind}  {token}:: v{eol}"),
                            base: None,
                            inert: false,
                        },
                    );
                    self.tokens.push(token);
                }
            }
            5 => {
                // Collapse (metadata only).
                let e = self.block_end(p);
                let has = self.lines[p..e]
                    .iter()
                    .any(|l| l.text.contains("collapsed::"));
                if !has && e == p + 1 {
                    self.touch_block(p, e);
                    self.lines.insert(
                        p + 1,
                        L {
                            text: format!("{ind}  collapsed:: true{eol}"),
                            base: None,
                            inert: false,
                        },
                    );
                }
            }
            _ => {
                // Move a top-level subtree to the end of the page.
                if indent_of(&self.lines[p].text) == 0 {
                    self.destructive = true;
                    let e = self.subtree_end(p);
                    let mut moved: Vec<L> = self.lines.drain(p..e).collect();
                    if let Some(last) = self.lines.last_mut()
                        && !last.text.ends_with('\n')
                    {
                        last.text.push('\n');
                    }
                    if let Some(l) = moved.last_mut()
                        && !l.text.ends_with('\n')
                    {
                        l.text.push('\n');
                    }
                    self.lines.extend(moved);
                }
            }
        }
    }

    fn text(&self) -> String {
        self.lines.iter().map(|l| l.text.as_str()).collect()
    }
}

fn conflict_text(r: &MergeResult) -> String {
    r.conflicts
        .iter()
        .flat_map(|c| {
            [
                c.conflict.base.clone(),
                c.conflict.ours.clone(),
                c.conflict.theirs.clone(),
            ]
        })
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
}

fn kinds(r: &MergeResult) -> Vec<String> {
    let mut v: Vec<String> = r
        .conflicts
        .iter()
        .map(|c| format!("{:?}/{}", c.conflict.kind, c.conflict.field))
        .collect();
    v.sort();
    v
}

/// Lines compared modulo surrounding whitespace, ignoring blank ones (canonical rewriting of
/// blocks taken from the other side may normalise trailing spaces and indentation style).
fn trimmed_multiset(s: &str) -> HashMap<&str, usize> {
    let mut m = HashMap::new();
    for l in s.lines().map(str::trim).filter(|l| !l.is_empty()) {
        *m.entry(l).or_insert(0) += 1;
    }
    m
}

fn multiset(s: &str) -> HashMap<&str, usize> {
    let mut m = HashMap::new();
    for l in s.split_inclusive('\n') {
        *m.entry(l.strip_suffix('\n').unwrap_or(l)).or_insert(0) += 1;
    }
    m
}

fn check(name: &str, base: &str, seed: u64) -> Result<(), String> {
    let base_lines: Vec<String> = base.split_inclusive('\n').map(str::to_owned).collect();
    let refs: Vec<&str> = base_lines.iter().map(String::as_str).collect();
    let cand = bullets(&refs, &inert_lines(&base_lines));
    let mut rng = Rng(seed);
    let mut o = Device::new(&base_lines);
    let mut t = Device::new(&base_lines);
    for n in 0..1 + rng.below(4) {
        o.apply(&mut rng, &cand, "o", n);
    }
    for n in 0..1 + rng.below(4) {
        t.apply(&mut rng, &cand, "t", n);
    }
    let (ours, theirs) = (o.text(), t.text());
    let env = MergeEnv::new();
    let ctx = |m: &str| format!("{name} seed={seed}: {m}");
    let r = merge_page(base, &ours, &theirs, &env);
    if std::env::var("BITACORA_CORPUS_DEBUG").is_ok_and(|v| v == format!("{name}:{seed}")) {
        eprintln!(
            "--- {name} seed={seed}\nBASE:\n{base}\nOURS:\n{ours}\nTHEIRS:\n{theirs}\nOUT:\n{}\nCONFLICTS: {:?}",
            r.output, r.conflicts
        );
    }

    // Identity laws.
    if merge_page(base, &ours, &ours, &env).output != ours {
        return Err(ctx("merge(b,x,x) != x"));
    }
    if merge_page(base, base, &theirs, &env).output != theirs {
        return Err(ctx("merge(b,b,x) != x"));
    }
    if merge_page(base, &ours, base, &env).output != ours {
        return Err(ctx("merge(b,x,b) != x"));
    }
    // Markers.
    let marks = |s: &str| s.matches("<<<<<<<").count() + s.matches(">>>>>>>").count();
    if marks(&r.output) > marks(base).max(marks(&ours)).max(marks(&theirs)) {
        return Err(ctx("markers introduced"));
    }
    // Round trip.
    let rt = serialize(&Document::parse(r.output.clone()), &WriteOptions::default());
    if rt != r.output.as_bytes() {
        return Err(ctx("output does not round-trip"));
    }
    // Content preservation.
    let ct = conflict_text(&r);
    for tok in o
        .tokens
        .iter()
        .filter(|k| ours.contains(k.as_str()))
        .chain(t.tokens.iter().filter(|k| theirs.contains(k.as_str())))
    {
        if !r.output.contains(tok.as_str()) && !ct.contains(tok.as_str()) {
            return Err(ctx(&format!("token {tok} lost")));
        }
    }
    // Untouched blocks stay byte-identical.
    let untouched: String = base_lines
        .iter()
        .enumerate()
        .filter(|(i, l)| !o.touched.contains(i) && !t.touched.contains(i) && l.trim().len() > 1)
        .map(|(_, l)| l.as_str())
        .collect::<Vec<_>>()
        .join("");
    let have = multiset(&r.output);
    for (line, n) in multiset(&untouched) {
        if have.get(line).copied().unwrap_or(0) < n {
            return Err(ctx(&format!("untouched line lost or rewritten: {line:?}")));
        }
    }
    // Symmetry.
    let sw = merge_page(base, &theirs, &ours, &env);
    // Pages the block merge declined (irregular indentation, runs of empty bullets) fall back to a
    // line merge, which is not symmetric by nature; everything above still applies.
    let fallback = |x: &MergeResult| x.conflicts.iter().any(|c| c.conflict.field == "page");
    if fallback(&r) || fallback(&sw) {
        return Ok(());
    }
    if kinds(&r) != kinds(&sw) {
        return Err(ctx(&format!(
            "conflict set differs: {:?} vs {:?}",
            kinds(&r),
            kinds(&sw)
        )));
    }
    let structural = |x: &MergeResult| {
        x.notes
            .iter()
            .any(|n| matches!(n.kind, NoteKind::CompetingMove | NoteKind::CompetingReorder))
    };
    if r.conflicts.is_empty()
        && !structural(&r)
        && !structural(&sw)
        && trimmed_multiset(&r.output) != trimmed_multiset(&sw.output)
    {
        let (a, b) = (trimmed_multiset(&r.output), trimmed_multiset(&sw.output));
        let diff: Vec<String> = a
            .iter()
            .filter(|(k, v)| b.get(*k) != Some(v))
            .chain(b.iter().filter(|(k, v)| a.get(*k) != Some(v)))
            .take(4)
            .map(|(k, _)| format!("{:.60?}", k))
            .collect();
        return Err(ctx(&format!("swapped merge has different lines: {diff:?}")));
    }
    // Idempotence: the result already contains theirs' changes (only checked without deletes and
    // moves: look-alike blocks such as `Chapter 4` / `Chapter 5` may legitimately be paired by the
    // fuzzy matcher when one of them disappears).
    if r.conflicts.is_empty() && !o.destructive && !t.destructive {
        let again = merge_page(base, &r.output, &theirs, &env);
        if again.output != r.output && !fallback(&again) {
            if std::env::var("BITACORA_CORPUS_DEBUG").is_ok_and(|v| v == format!("{name}:{seed}")) {
                eprintln!(
                    "AGAIN:\n{}\nNOTES: {:?} / {:?}",
                    again.output, r.notes, again.notes
                );
            }
            return Err(ctx("not idempotent: merge(b, out, theirs) != out"));
        }
    }
    Ok(())
}

#[test]
fn corpus_invariants() {
    let seeds: u64 = std::env::var("BITACORA_CORPUS_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6);
    let mut failures = Vec::new();
    let mut total = 0;
    for (p, text) in pages() {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
        for seed in 0..seeds {
            total += 1;
            if let Err(e) = check(name.as_deref().unwrap_or("?"), text, seed) {
                failures.push(e);
            }
        }
    }
    assert!(total > 300, "corpus too small: {total}");
    assert!(
        failures.is_empty(),
        "{} of {total} cases failed; first 400:\n{}",
        failures.len(),
        failures
            .iter()
            .take(400)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Replays one case: `BITACORA_CORPUS_DEBUG="<page file name>:<seed>" cargo test --test corpus replay`.
#[test]
fn replay() {
    let Ok(spec) = std::env::var("BITACORA_CORPUS_DEBUG") else {
        return;
    };
    let (name, seed) = spec.rsplit_once(':').expect("name:seed");
    let seed: u64 = seed.parse().expect("numeric seed");
    let (_, text) = pages()
        .iter()
        .find(|(p, _)| p.file_name().is_some_and(|n| n.to_string_lossy() == name))
        .expect("page exists");
    if let Err(e) = check(name, text, seed) {
        eprintln!("FAIL: {e}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: std::env::var("PROPTEST_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(400),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// Fuzz over the corpus: a random page and a random seed.
    #[test]
    fn corpus_fuzz(idx in 0usize..10_000, seed in any::<u64>()) {
        let pages = pages();
        let (p, text) = &pages[idx % pages.len()];
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Err(e) = check(&name, text, seed) {
            return Err(TestCaseError::fail(e));
        }
    }
}

/// Coarse regression guard for the bench (`benches/merge_large.rs`): a page of 3000 blocks with
/// disjoint edits on both sides merges in a few seconds even in an unoptimised build.
#[test]
fn large_page_merge_stays_fast() {
    let mut base = String::new();
    for i in 0..1000 {
        base.push_str(&format!(
            "- Block number {i} talks about subject {} today\n",
            i * 7 % 101
        ));
        base.push_str(&format!("\t- child a of {i} with some words to fuzz on\n"));
        base.push_str(&format!("\t- child b of {i}\n"));
    }
    let edit = |off: usize, suffix: &str| -> String {
        let mut n = 0;
        base.split_inclusive('\n')
            .map(|l| {
                if l.starts_with("- Block") {
                    n += 1;
                    if n % 20 == off {
                        return format!("{} {suffix}\n", l.trim_end());
                    }
                }
                l.to_owned()
            })
            .collect()
    };
    let (o, t) = (edit(3, "ours"), edit(11, "theirs"));
    let start = std::time::Instant::now();
    let r = merge_page(&base, &o, &t, &MergeEnv::new());
    let took = start.elapsed();
    assert!(r.conflicts.is_empty());
    assert_eq!(r.output.matches("ours").count(), 50);
    assert_eq!(r.output.matches("theirs").count(), 50);
    assert!(took.as_secs() < 8, "merge took {took:?}");
}
