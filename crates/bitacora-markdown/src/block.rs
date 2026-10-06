//! Block-level analysis: ties the head parser, planning lines, drawers, property scanner and
//! inline scanner together for one block's *content* (see [`crate::outline::content_of`]).
//!
//! [`analyze`] answers "what does Logseq derive from this block": marker and priority, the lifted
//! `SCHEDULED` / `DEADLINE` dates, the logbook, the property group and the references
//! (`with-page-refs` / `with-block-refs`, `docs/analysis/logseq/03-parsing-indexing-search.md`).
//! Nothing is rewritten: every span points into the analysed text.

use crate::inline::{RefMode, RefSet, collect, scan_line};
use crate::lines::{Lines, ParserOptions, RegionKind, RegionPart, is_ws};
use crate::properties::{PropValue, PropertyConfig, PropertyScan, interpret, scan_properties};
use crate::span::Span;
use crate::tasks::drawer::{Drawer, LogEntry, find_drawers, parse_logbook};
use crate::tasks::head::{BlockHead, parse_head};
use crate::tasks::timestamp::{Planning, parse_planning_line};

/// The references of a block.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockRefs {
    /// The task marker word (`TODO`): Logseq makes the marker a page reference.
    pub marker: Option<String>,
    /// The priority (`A`): also a page reference.
    pub priority: Option<String>,
    /// References found in the title and body text.
    pub content: RefSet,
    /// Pages referenced by property values of the effective property group.
    pub property_pages: Vec<String>,
}

impl BlockRefs {
    /// Every referenced page in Logseq's order (marker, priority, text refs, property refs),
    /// followed by the namespace parents (`a/b/c` adds `a` and `a/b`).
    #[must_use]
    pub fn pages(&self) -> Vec<String> {
        let mut set = RefSet::default();
        for p in self.marker.iter().chain(self.priority.iter()) {
            set.pages.push(p.clone());
        }
        let mut all = set;
        for p in self.content.pages.iter().chain(self.property_pages.iter()) {
            if !all.pages.contains(p) {
                all.pages.push(p.clone());
            }
        }
        all.pages_with_namespace_parents()
    }

    /// Referenced block ids (valid UUIDs, text only), in order.
    #[must_use]
    pub fn block_refs(&self) -> &[String] {
        &self.content.block_refs
    }
}

/// Everything derived from one block's content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockAnalysis {
    /// Heading size, marker and priority.
    pub head: BlockHead,
    /// Planning entries of the paragraph that starts with a `SCHEDULED:` / `DEADLINE:` / `CLOSED:`
    /// keyword.
    pub planning: Vec<Planning>,
    /// `:block/scheduled` as `yyyymmdd`.
    pub scheduled: Option<u32>,
    /// `:block/deadline` as `yyyymmdd`.
    pub deadline: Option<u32>,
    /// `:block/repeated?`: the lifted scheduled or deadline timestamp has a repeater.
    pub repeated: bool,
    /// Drawers other than `:PROPERTIES:`.
    pub drawers: Vec<Drawer>,
    /// The lines of the `:LOGBOOK:` drawer.
    pub logbook: Vec<LogEntry>,
    /// Property groups.
    pub properties: PropertyScan,
    /// References.
    pub refs: BlockRefs,
}

/// Names of `#+BEGIN_X` blocks whose content is not inline-parsed (or is skipped by Logseq's
/// reference walk): source, example, export, comment and custom queries.
fn opaque_begin(name: &str) -> bool {
    ["SRC", "EXAMPLE", "EXPORT", "COMMENT", "QUERY"]
        .iter()
        .any(|n| name.eq_ignore_ascii_case(n))
}

fn begin_name(line: &[u8]) -> &str {
    let t = &line[line.iter().take_while(|&&b| is_ws(b)).count()..];
    let rest = t.get(8..).unwrap_or(&[]);
    let n = rest
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || **b == b'_')
        .count();
    std::str::from_utf8(&rest[..n]).unwrap_or("")
}

fn is_directive_line(line: &[u8]) -> bool {
    let t = &line[line.iter().take_while(|&&b| is_ws(b)).count()..];
    let Some(rest) = t.strip_prefix(b"#+") else {
        return false;
    };
    let n = rest.iter().take_while(|&&b| b != b':' && !is_ws(b)).count();
    n > 0 && rest.get(n) == Some(&b':')
}

fn overlaps(spans: &[Span], start: usize, end: usize) -> bool {
    spans.iter().any(|s| start < s.end && s.start < end)
}

/// Analyses the content of one block. `cfg` carries the property settings from `config.edn`.
#[must_use]
pub fn analyze(content: &str, cfg: &PropertyConfig, opts: ParserOptions) -> BlockAnalysis {
    let bytes = content.as_bytes();
    let head = parse_head(content);
    let properties = scan_properties(bytes, Span::new(0, bytes.len()), opts);
    let drawers = find_drawers(content, opts);
    let logbook = drawers
        .iter()
        .find(|d| d.is_logbook())
        .map(|d| parse_logbook(content, d))
        .unwrap_or_default();

    let mut skip: Vec<Span> = properties.groups.iter().map(|g| g.span).collect();
    skip.extend(drawers.iter().map(|d| d.span));

    // Walk the lines once: planning paragraphs and the text lines that carry inline syntax.
    let mut planning = Vec::new();
    let mut text_lines: Vec<(usize, usize)> = Vec::new();
    let mut in_paragraph = false;
    let mut paragraph_is_planning = false;
    let mut region_opaque = false;
    for (idx, line) in Lines::with_options(bytes, opts).enumerate() {
        let (start, end) = (line.start, line.content_end());
        if let Some((kind, part)) = line.region {
            in_paragraph = false;
            match (kind, part) {
                (RegionKind::Begin, RegionPart::Open) => {
                    region_opaque = opaque_begin(begin_name(line.content));
                }
                (RegionKind::Begin, RegionPart::Inside) if !region_opaque => {
                    text_lines.push((start, end));
                }
                _ => {}
            }
            continue;
        }
        if overlaps(&skip, line.start, line.end) || is_directive_line(line.content) {
            in_paragraph = false;
            continue;
        }
        let blank = line.content.iter().all(|&b| is_ws(b));
        if blank {
            in_paragraph = false;
            continue;
        }
        let from = if idx == 0 {
            head.title_start.min(end)
        } else {
            start
        };
        if from < end {
            text_lines.push((from, end));
        }
        if idx == 0 {
            // The title line is not a paragraph: `- a SCHEDULED: <...>` is not lifted.
            continue;
        }
        if !in_paragraph {
            in_paragraph = true;
            let found = parse_planning_line(content, start, end);
            paragraph_is_planning = !found.is_empty();
            planning.extend(found);
        } else if paragraph_is_planning {
            planning.extend(parse_planning_line(content, start, end));
        }
    }

    let mut scheduled = None;
    let mut deadline = None;
    let mut repeated = false;
    for p in &planning {
        if let Some(d) = p.lifted_date() {
            match p.kind {
                crate::tasks::timestamp::PlanningKind::Scheduled => scheduled = Some(d),
                _ => deadline = Some(d),
            }
            repeated |= p.timestamp.repeater.is_some();
        }
    }

    let mut text_refs = RefSet::default();
    for (lo, hi) in text_lines {
        let tokens = scan_line(content, lo, hi);
        text_refs.merge(collect(content, &tokens, RefMode::Content));
    }

    let mut property_pages: Vec<String> = Vec::new();
    for line in properties.valid_lines() {
        if let PropValue::Pages(pages) = interpret(&line.key_norm, &line.value_raw, cfg) {
            for p in pages {
                if !property_pages.contains(&p) {
                    property_pages.push(p);
                }
            }
        }
    }

    let refs = BlockRefs {
        marker: head.marker.map(|m| m.as_str().to_owned()),
        priority: head.priority.map(|c| c.to_string()),
        content: text_refs,
        property_pages,
    };
    BlockAnalysis {
        head,
        planning,
        scheduled,
        deadline,
        repeated,
        drawers,
        logbook,
        properties,
        refs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(content: &str) -> BlockAnalysis {
        analyze(
            content,
            &PropertyConfig::default(),
            ParserOptions::default(),
        )
    }

    #[test]
    fn marker_priority_and_refs() {
        let a = run(
            "TODO [#A] call [[Bob]] about #project and ((6500c1a4-0000-4000-8000-000000000001))",
        );
        assert_eq!(a.refs.pages(), ["TODO", "A", "Bob", "project"]);
        assert_eq!(
            a.refs.block_refs(),
            ["6500c1a4-0000-4000-8000-000000000001"]
        );
        assert_eq!(a.refs.content.tags, ["project"]);
    }

    #[test]
    fn priority_brackets_are_not_a_tag() {
        let a = run("[#A] x");
        assert_eq!(a.refs.pages(), ["A"]);
        assert!(a.refs.content.tags.is_empty());
    }

    #[test]
    fn planning_is_lifted_from_the_first_paragraph_only() {
        let a = run("TODO x\n  SCHEDULED: <2024-01-01 Mon .+1d>\n  DEADLINE: <2024-02-01 Thu>");
        assert_eq!(a.scheduled, Some(20_240_101));
        assert_eq!(a.deadline, Some(20_240_201));
        assert!(a.repeated);

        // After other text, or on the title line, nothing is lifted.
        assert_eq!(run("x\ntext\nSCHEDULED: <2024-01-01 Mon>").scheduled, None);
        assert_eq!(run("x SCHEDULED: <2024-01-01 Mon>").scheduled, None);
        // Properties before the planning line do not matter.
        let a = run("x\nkey:: v\nSCHEDULED: <2024-01-01 Mon>");
        assert_eq!(a.scheduled, Some(20_240_101));
        assert!(!a.repeated);
        // Ranges are not lifted.
        assert_eq!(
            run("x\nDEADLINE: <2024-01-01 Mon>--<2024-01-05 Fri>").deadline,
            None
        );
    }

    #[test]
    fn logbook_and_drawers_hide_their_text() {
        let a = run(
            "DOING x\n:LOGBOOK:\nCLOCK: [2024-01-01 Mon 10:00:00]\n[[hidden]]\n:END:\n[[shown]]",
        );
        assert_eq!(a.refs.content.pages, ["shown"]);
        assert_eq!(a.drawers.len(), 1);
        assert_eq!(a.logbook.len(), 2);
    }

    #[test]
    fn regions_properties_and_queries() {
        let a = run(
            "x\n```\n[[fence]]\n```\n#+BEGIN_SRC\n[[src]]\n#+END_SRC\n#+BEGIN_QUERY\n[[q]]\n#+END_QUERY\n#+BEGIN_QUOTE\n[[quote]] #qt\n#+END_QUOTE\nk:: [[prop]]\n[[after]]",
        );
        assert_eq!(a.refs.content.pages, ["quote", "qt", "after"]);
        assert_eq!(a.refs.property_pages, ["prop"]);
        assert_eq!(a.refs.pages(), ["quote", "qt", "after", "prop"]);
    }

    #[test]
    fn title_property_and_directives() {
        let a = run("k:: [[a/b]]\n#+title: [[no]]");
        assert!(a.refs.content.pages.is_empty());
        assert_eq!(a.refs.pages(), ["a/b", "a"]);
    }

    #[test]
    fn namespace_parents_come_last() {
        let a = run("[[a/b/c]] [[x]]");
        assert_eq!(a.refs.pages(), ["a/b/c", "x", "a", "a/b"]);
    }

    #[test]
    fn heading_marker_after_hashes() {
        let a = run("## NOW [#B] [[p]]");
        assert_eq!(a.head.heading, Some(2));
        assert_eq!(a.refs.pages(), ["NOW", "B", "p"]);
    }
}
