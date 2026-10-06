//! Matcher accuracy on edit pairs synthesized from the fixture graphs (BIT-T-0352, BIT-SP-0006.R9).
//!
//! Each fixture page is the base; "ours" is derived from it by known edits (word appended to long
//! first lines, leaf deletions, inserted blocks, adjacent sibling swaps). The ground truth maps every
//! block of ours back to its base block, so precision and recall of the base-to-ours matching can
//! be measured. Short blocks (fewer than four tokens) are never edited: by design they are
//! matched exactly or not at all (design doc open question 3).

use std::fs;
use std::path::{Path, PathBuf};

use bitacora_merge::{MergePage, match_pages};

fn md_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.filter_map(Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::path);
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            md_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "md") {
            out.push(p);
        }
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

/// Appends `suffix` to the first line of a raw block text.
fn edit_first_line(raw: &str, suffix: &str) -> String {
    match raw.find('\n') {
        Some(n) => {
            let cut = if raw[..n].ends_with('\r') { n - 1 } else { n };
            format!("{}{}{}", &raw[..cut], suffix, &raw[cut..])
        }
        None => format!("{raw}{suffix}"),
    }
}

#[derive(Clone, Copy)]
enum Origin {
    Base(usize),
    Inserted,
}

#[test]
fn matcher_precision_and_recall_on_fixture_edit_pairs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/graphs");
    let mut files = Vec::new();
    md_files(&root, &mut files);
    assert!(!files.is_empty(), "no fixture pages found");

    let (mut predicted, mut correct_pred, mut truth_total, mut truth_found) =
        (0u64, 0u64, 0u64, 0u64);
    let mut mispairs: Vec<String> = Vec::new();
    let mut pages = 0usize;

    for (n, path) in files.iter().enumerate() {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let base = MergePage::parse(&text);
        if base.blocks.len() < 3 || base.blocks.len() > 600 {
            continue;
        }
        pages += 1;
        let mut rng = Rng(0x9e37_79b9 ^ (n as u64 + 1));

        // Build ours from raw spans.
        let mut out = String::new();
        if let Some(pre) = &base.pre_block {
            out.push_str(&base.source[pre.raw.start..pre.raw.end]);
        }
        let mut origin: Vec<Origin> = Vec::new();
        let mut i = 0;
        while i < base.blocks.len() {
            let b = &base.blocks[i];
            let raw = base.raw_text(i);
            let roll = rng.next() % 100;
            let long = b.first_line().split_whitespace().count() >= 4;
            // Swap two adjacent leaf siblings.
            if roll < 5
                && i + 1 < base.blocks.len()
                && b.children.is_empty()
                && base.blocks[i + 1].children.is_empty()
                && base.blocks[i + 1].parent == b.parent
                && base.blocks[i + 1].depth == b.depth
                && !base.raw_text(i).is_empty()
                && base.raw_text(i).ends_with('\n')
                && base.raw_text(i + 1).ends_with('\n')
            {
                out.push_str(base.raw_text(i + 1));
                out.push_str(raw);
                origin.push(Origin::Base(i + 1));
                origin.push(Origin::Base(i));
                i += 2;
                continue;
            }
            if roll < 12
                && b.children.is_empty()
                && raw.ends_with('\n')
                && i + 1 < base.blocks.len()
            {
                // Delete a leaf.
                i += 1;
                continue;
            }
            if roll < 40 && long {
                out.push_str(&edit_first_line(raw, " edited today"));
            } else {
                out.push_str(raw);
            }
            origin.push(Origin::Base(i));
            if roll >= 90 && raw.ends_with('\n') && b.children.is_empty() {
                // Insert a brand new sibling after a leaf, with the same indentation.
                let indent: String = raw
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                out.push_str(&format!(
                    "{indent}- freshly inserted block number {} about nothing in particular\n",
                    rng.next() % 1000
                ));
                origin.push(Origin::Inserted);
            }
            i += 1;
        }

        let ours = MergePage::parse(&out);
        if ours.blocks.len() != origin.len() {
            continue; // an edit changed the block structure (e.g. a swap near code fences)
        }
        let m = match_pages(&base, &ours);

        // truth: base index -> ours index.
        let mut truth: Vec<Option<usize>> = vec![None; base.blocks.len()];
        for (j, o) in origin.iter().enumerate() {
            if let Origin::Base(k) = o {
                truth[*k] = Some(j);
            }
        }
        let equivalent = |bi: usize, oj: usize| match origin[oj] {
            Origin::Base(k) => {
                k == bi
                    || (base.blocks[k].normalized_hash() == base.blocks[bi].normalized_hash()
                        && base.blocks[k].depth == base.blocks[bi].depth)
            }
            Origin::Inserted => false,
        };
        for (bi, pred) in m.iter().enumerate() {
            if let Some(oj) = pred {
                predicted += 1;
                if equivalent(bi, *oj) {
                    correct_pred += 1;
                } else {
                    mispairs.push(format!(
                        "{}: base#{bi} {:?} -> ours#{oj} {:?}",
                        path.display(),
                        base.blocks[bi].first_line(),
                        ours.blocks[*oj].first_line()
                    ));
                }
            }
            // Recall counts only blocks that exist on both sides.
            if let Some(tj) = truth[bi] {
                // Short blocks that were not edited must still be found; edited long ones too.
                truth_total += 1;
                if pred.is_some_and(|p| p == tj || equivalent(bi, p)) {
                    truth_found += 1;
                }
            }
        }
    }

    let precision = correct_pred as f64 / predicted.max(1) as f64;
    let recall = truth_found as f64 / truth_total.max(1) as f64;
    for m in mispairs.iter().take(15) {
        eprintln!("mispair: {m}");
    }
    eprintln!(
        "pages={pages} predicted={predicted} correct={correct_pred} truth={truth_total} found={truth_found} precision={precision:.4} recall={recall:.4}"
    );
    assert!(pages > 0);
    assert!(precision >= 0.99, "precision {precision:.4} < 0.99");
    assert!(recall >= 0.95, "recall {recall:.4} < 0.95");
}
