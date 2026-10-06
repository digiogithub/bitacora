//! Block and page render models: what a block shows, derived from its Markdown content.
//!
//! The text is read through `bitacora_markdown::block::analyze` (marker, priority, planning,
//! properties, logbook) and the inline scanner; nothing is rewritten and nothing here knows
//! about GPUI.

use std::ops::Range;

use bitacora_markdown::block::analyze;
use bitacora_markdown::properties::{PropValue, PropertyConfig, interpret};
use bitacora_markdown::tasks::drawer::{LogEntry, format_duration, total_seconds};
use bitacora_markdown::tasks::head::Marker;
use bitacora_markdown::{ParserOptions, build_tree, content_of, pre_block_content, split};

use super::highlight::{TokenClass, highlight};
use super::inline::{BlockResolver, NavTarget, Role, TextLayout, layout_line, layout_lines};

/// How a task marker is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerView {
    /// The marker word as written (`TODO`).
    pub label: String,
    /// `Some(checked)` when a checkbox is drawn (TODO, LATER, NOW, DOING, DONE).
    pub checkbox: Option<bool>,
    /// The label is shown next to the checkbox (DOING, NOW, ...) or alone (WAITING, CANCELED).
    pub show_label: bool,
    /// Finished or cancelled: the title is dimmed (cancelled: struck through).
    pub finished: bool,
    /// Cancelled: the title is struck through.
    pub cancelled: bool,
}

impl MarkerView {
    fn new(marker: Marker) -> Self {
        let label = marker.as_str().to_owned();
        let (checkbox, show_label, finished, cancelled) = match marker {
            Marker::Todo | Marker::Later => (Some(false), false, false, false),
            Marker::Done => (Some(true), false, true, false),
            Marker::Doing | Marker::Now | Marker::Started | Marker::InProgress => {
                (Some(false), true, false, false)
            }
            Marker::Waiting | Marker::Wait => (None, true, false, false),
            Marker::Canceled | Marker::Cancelled => (None, true, true, true),
        };
        Self {
            label,
            checkbox,
            show_label,
            finished,
            cancelled,
        }
    }
}

/// A `SCHEDULED` / `DEADLINE` / `CLOSED` chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanningChip {
    /// Keyword (`SCHEDULED`).
    pub keyword: &'static str,
    /// Canonical timestamp text (`<2024-01-01 Mon>`).
    pub text: String,
}

/// One property row of the properties table.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyRow {
    /// Key as written.
    pub key: String,
    /// Rendered value (page-valued properties become links).
    pub value: TextLayout,
}

/// Summary of a collapsed `:LOGBOOK:` drawer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogbookSummary {
    /// Number of drawer lines.
    pub entries: usize,
    /// Total clocked time (`01:30:00`), when any entry is closed.
    pub total: Option<String>,
}

/// A code region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlock {
    /// Language label, when given.
    pub language: Option<String>,
    /// Code text.
    pub text: String,
    /// Highlighted ranges.
    pub tokens: Vec<(Range<usize>, TokenClass)>,
}

/// One item of a block body.
#[derive(Debug, Clone, PartialEq)]
pub enum BodyItem {
    /// A paragraph (consecutive lines).
    Text(TextLayout),
    /// A code fence or `#+BEGIN_SRC` region.
    Code(CodeBlock),
    /// A `#+BEGIN_QUOTE` region.
    Quote(TextLayout),
}

/// Everything shown for one block.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockModel {
    /// `id::` UUID (lower case), when present.
    pub id: Option<String>,
    /// Heading level (1-6).
    pub heading: Option<u8>,
    /// Task marker.
    pub marker: Option<MarkerView>,
    /// Priority letter (`A`).
    pub priority: Option<char>,
    /// First line, rendered.
    pub title: TextLayout,
    /// Body items after the title.
    pub body: Vec<BodyItem>,
    /// Planning chips.
    pub planning: Vec<PlanningChip>,
    /// Visible properties.
    pub properties: Vec<PropertyRow>,
    /// Logbook summary.
    pub logbook: Option<LogbookSummary>,
    /// `collapsed:: true`.
    pub collapsed: bool,
}

/// Properties Logseq hides in the rendered block (see `04-editor-outliner-operations.md` §8).
pub fn is_hidden_property(key_norm: &str) -> bool {
    const HIDDEN: &[&str] = &[
        "id",
        "custom-id",
        "background-color",
        "heading",
        "collapsed",
        "created-at",
        "updated-at",
        "last-modified-at",
        "query-table",
        "query-properties",
        "query-sort-by",
        "query-sort-desc",
        "ls-type",
        "todo",
        "doing",
        "now",
        "later",
        "done",
    ];
    HIDDEN.contains(&key_norm)
        || key_norm.starts_with("hl-")
        || key_norm.starts_with("logseq.macro-")
        || key_norm.starts_with("logseq.order-list-type")
        || key_norm.starts_with("logseq.tldraw.")
}

fn is_planning_line(line: &str) -> bool {
    let t = line.trim_start();
    ["SCHEDULED:", "DEADLINE:", "CLOSED:"]
        .iter()
        .any(|k| t.starts_with(k))
}

/// Case-insensitive ASCII `strip_prefix` that never slices inside a multi-byte character.
fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.as_bytes().get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix.as_bytes())
        .then(|| &text[prefix.len()..])
}

fn overlaps(spans: &[(usize, usize)], start: usize, end: usize) -> bool {
    spans
        .iter()
        .any(|&(s, e)| start < e && s < end.max(start + 1))
}

enum Region {
    Fence {
        language: String,
        text: String,
    },
    Begin {
        name: String,
        language: String,
        text: String,
    },
}

impl BlockModel {
    /// Builds the model of one block from its content (bullet removed, de-indented).
    pub fn from_content(content: &str, cfg: &PropertyConfig, resolver: &dyn BlockResolver) -> Self {
        let analysis = analyze(content, cfg, ParserOptions::default());
        let mut skip: Vec<(usize, usize)> = analysis
            .properties
            .groups
            .iter()
            .map(|g| (g.span.start, g.span.end))
            .collect();
        skip.extend(analysis.drawers.iter().map(|d| (d.span.start, d.span.end)));
        let planning_spans: Vec<(usize, usize)> = analysis
            .planning
            .iter()
            .map(|p| (p.span.start, p.span.end))
            .collect();

        let mut model = Self {
            heading: analysis.head.heading.map(|h| h.clamp(1, 6) as u8),
            marker: analysis.head.marker.map(MarkerView::new),
            priority: analysis.head.priority,
            ..Self::default()
        };

        // Properties.
        if let Some(group) = analysis.properties.effective() {
            for line in group.lines.iter().filter(|l| l.valid) {
                if line.key_norm == "id" {
                    model.id = Some(line.value_raw.trim().to_ascii_lowercase());
                }
                if line.key_norm == "collapsed" {
                    model.collapsed = line.value_raw.trim() == "true";
                }
                if is_hidden_property(&line.key_norm) {
                    continue;
                }
                model.properties.push(PropertyRow {
                    key: line.key_raw.clone(),
                    value: property_value(&line.key_norm, &line.value_raw, cfg, resolver),
                });
            }
        }

        model.planning = analysis
            .planning
            .iter()
            .map(|p| PlanningChip {
                keyword: p.kind.keyword().trim_end_matches(':'),
                text: match &p.range_end {
                    Some(end) => format!("{}--{}", p.timestamp.format(), end.format()),
                    None => p.timestamp.format(),
                },
            })
            .collect();

        if !analysis.logbook.is_empty() {
            let closed = analysis
                .logbook
                .iter()
                .any(|e| matches!(e, LogEntry::Clock(c) if c.end.is_some()));
            model.logbook = Some(LogbookSummary {
                entries: analysis.logbook.len(),
                total: closed.then(|| format_duration(total_seconds(&analysis.logbook))),
            });
        }

        // Walk the lines: the first free line is the title, the rest is the body.
        let mut offset = 0;
        let mut title_done = false;
        let mut paragraph: Vec<&str> = Vec::new();
        let mut region: Option<Region> = None;
        let mut quote: Option<Vec<&str>> = None;
        let flush_paragraph = |model: &mut Self, paragraph: &mut Vec<&str>| {
            if !paragraph.is_empty() {
                let layout = layout_lines(paragraph.iter().copied(), resolver);
                if !layout.is_empty() {
                    model.body.push(BodyItem::Text(layout));
                }
                paragraph.clear();
            }
        };
        for (index, raw) in content.split_inclusive('\n').enumerate() {
            let start = offset;
            offset += raw.len();
            let line = raw.trim_end_matches(['\n', '\r']);
            let end = start + line.len();
            if let Some(r) = region.as_mut() {
                let trimmed = line.trim();
                let closed = match r {
                    Region::Fence { .. } => trimmed.starts_with("```"),
                    Region::Begin { name, .. } => strip_prefix_ci(trimmed, "#+END_")
                        .is_some_and(|rest| rest.trim().eq_ignore_ascii_case(name)),
                };
                if closed {
                    if let Some(done) = region.take() {
                        model.push_region(done);
                    }
                } else {
                    let text = match r {
                        Region::Fence { text, .. } | Region::Begin { text, .. } => text,
                    };
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(line);
                }
                continue;
            }
            if let Some(q) = quote.as_mut() {
                if line.trim().eq_ignore_ascii_case("#+END_QUOTE") {
                    let lines = quote.take().unwrap_or_default();
                    let layout = layout_lines(lines, resolver);
                    model.body.push(BodyItem::Quote(layout));
                } else {
                    q.push(line);
                }
                continue;
            }
            if overlaps(&skip, start, end)
                || (!planning_spans.is_empty()
                    && is_planning_line(line)
                    && overlaps(&planning_spans, start, end))
            {
                if index == 0 {
                    title_done = true;
                }
                flush_paragraph(&mut model, &mut paragraph);
                continue;
            }
            if !title_done {
                title_done = true;
                let from = analysis.head.title_start.min(end).max(start);
                model.title = layout_line(&content[from..end], resolver);
                continue;
            }
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("```") {
                flush_paragraph(&mut model, &mut paragraph);
                region = Some(Region::Fence {
                    language: rest.trim().to_owned(),
                    text: String::new(),
                });
            } else if let Some(begin) = strip_prefix_ci(trimmed, "#+BEGIN_") {
                flush_paragraph(&mut model, &mut paragraph);
                let mut parts = begin.splitn(2, char::is_whitespace);
                let name = parts.next().unwrap_or("").to_owned();
                let language = parts.next().unwrap_or("").trim().to_owned();
                if name.eq_ignore_ascii_case("QUOTE") {
                    quote = Some(Vec::new());
                } else {
                    region = Some(Region::Begin {
                        name,
                        language,
                        text: String::new(),
                    });
                }
            } else if line.trim().is_empty() {
                flush_paragraph(&mut model, &mut paragraph);
            } else {
                paragraph.push(line);
            }
        }
        // Unclosed regions render what they collected.
        if let Some(r) = region.take() {
            model.push_region(r);
        }
        if let Some(lines) = quote.take() {
            model
                .body
                .push(BodyItem::Quote(layout_lines(lines, resolver)));
        }
        flush_paragraph(&mut model, &mut paragraph);
        model
    }

    fn push_region(&mut self, region: Region) {
        let (name, language, text) = match region {
            Region::Fence { language, text } => ("SRC".to_owned(), language, text),
            Region::Begin {
                name,
                language,
                text,
            } => (name, language, text),
        };
        if name.eq_ignore_ascii_case("COMMENT") {
            return;
        }
        let language = (!language.is_empty()).then_some(language);
        let tokens = highlight(&text, language.as_deref().unwrap_or(""));
        self.body.push(BodyItem::Code(CodeBlock {
            language,
            text,
            tokens,
        }));
    }

    /// Whether the title line is struck through (cancelled task).
    pub fn is_cancelled(&self) -> bool {
        self.marker.as_ref().is_some_and(|m| m.cancelled)
    }
}

fn property_value(
    key_norm: &str,
    value: &str,
    cfg: &PropertyConfig,
    resolver: &dyn BlockResolver,
) -> TextLayout {
    if let PropValue::Pages(pages) = interpret(key_norm, value, cfg)
        && !value.contains("[[")
        && !value.contains('#')
    {
        let mut out = TextLayout::default();
        for (i, page) in pages.iter().enumerate() {
            if i > 0 {
                out.text.push_str(", ");
            }
            let start = out.text.len();
            out.text.push_str(page);
            let range = start..out.text.len();
            out.styled.push(super::inline::StyledRange {
                range: range.clone(),
                role: Role::PageRef,
                emphasis: Default::default(),
            });
            out.links.push((range, NavTarget::Page(page.clone())));
        }
        return out;
    }
    layout_line(value, resolver)
}

/// A block that references another block (shown when the reference bubble is opened).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Referrer {
    /// UUID of the referencing block.
    pub uuid: String,
    /// Title of the page it lives on.
    pub page: String,
    /// Its first line.
    pub title: String,
}

/// One row of an outline: a block with its tree position and view-only state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Row {
    /// Index of the block in the page (the pre-block is not a row's index source: it is row 0
    /// with `block_index == None`).
    pub block_index: Option<usize>,
    /// Tree depth, 0 for top-level blocks and the pre-block.
    pub depth: usize,
    /// Whether the block has children (collapsed or not).
    pub has_children: bool,
    /// What to draw.
    pub block: BlockModel,
    /// Block UUID from the index (rows read from files have none).
    pub uuid: Option<String>,
    /// View-only collapse override (`None`: follow `collapsed::`). Never persisted.
    pub view_collapsed: Option<bool>,
    /// How many blocks reference this block.
    pub ref_count: usize,
    /// The referencing blocks once the bubble was opened; `None` while closed.
    pub referrers: Option<Vec<Referrer>>,
}

impl Row {
    /// Whether the children are hidden right now.
    pub fn is_collapsed(&self) -> bool {
        self.has_children && self.view_collapsed.unwrap_or(self.block.collapsed)
    }
}

/// Indexes of the rows that are visible: descendants of a collapsed row are skipped.
pub fn visible_rows(rows: &[Row]) -> Vec<usize> {
    let mut out = Vec::with_capacity(rows.len());
    let mut hidden_below: Option<usize> = None;
    for (i, row) in rows.iter().enumerate() {
        if let Some(limit) = hidden_below {
            if row.depth > limit {
                continue;
            }
            hidden_below = None;
        }
        if row.is_collapsed() {
            hidden_below = Some(row.depth);
        }
        out.push(i);
    }
    out
}

/// Flips the view-only collapse state of row `ix`; false when it has no children.
pub fn toggle_row(rows: &mut [Row], ix: usize) -> bool {
    let Some(row) = rows.get_mut(ix) else {
        return false;
    };
    if !row.has_children {
        return false;
    }
    row.view_collapsed = Some(!row.is_collapsed());
    true
}

/// The view-only collapse overrides keyed by block UUID (kept across a reload).
pub fn collapse_overrides(rows: &[Row]) -> std::collections::HashMap<String, bool> {
    rows.iter()
        .filter_map(|r| Some((r.uuid.clone()?, r.view_collapsed?)))
        .collect()
}

/// Re-applies [`collapse_overrides`] to freshly loaded rows.
pub fn apply_overrides(rows: &mut [Row], overrides: &std::collections::HashMap<String, bool>) {
    for row in rows {
        if let Some(c) = row.uuid.as_ref().and_then(|u| overrides.get(u)) {
            row.view_collapsed = Some(*c);
        }
    }
}

/// A page as a flat list of rows; collapsed subtrees stay in the list and are hidden by
/// [`visible_rows`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PageModel {
    /// Rows in document order.
    pub rows: Vec<Row>,
}

impl PageModel {
    /// Builds the model from the page bytes.
    pub fn from_source(source: &[u8], cfg: &PropertyConfig, resolver: &dyn BlockResolver) -> Self {
        let outline = split(source);
        let links = build_tree(&outline.blocks);
        let mut rows = Vec::with_capacity(outline.blocks.len() + 1);
        if let Some(span) = outline.pre_block {
            let content = pre_block_content(source, span);
            let block = BlockModel::from_content(&content, cfg, resolver);
            // A pre-block without anything to show (blank) is not a row.
            if !block.title.is_empty() || !block.properties.is_empty() || !block.body.is_empty() {
                rows.push(Row {
                    block,
                    ..Row::default()
                });
            }
        }
        for (i, raw) in outline.blocks.iter().enumerate() {
            let depth = links[i].depth.saturating_sub(1);
            let content = content_of(source, raw);
            let block = BlockModel::from_content(&content, cfg, resolver);
            let has_children = links
                .get(i + 1)
                .is_some_and(|next| next.depth.saturating_sub(1) > depth);
            rows.push(Row {
                block_index: Some(i),
                depth,
                has_children,
                block,
                ..Row::default()
            });
        }
        Self { rows }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::inline::NoBlocks;

    fn block(content: &str) -> BlockModel {
        BlockModel::from_content(content, &PropertyConfig::default(), &NoBlocks)
    }

    #[test]
    fn marker_priority_heading_and_title() {
        let b = block("TODO [#A] call [[Bob]]");
        let m = b.marker.expect("marker");
        assert_eq!(m.checkbox, Some(false));
        assert_eq!(b.priority, Some('A'));
        assert_eq!(b.title.text, "call [[Bob]]");
        let b = block("DONE ship");
        assert_eq!(b.marker.expect("m").checkbox, Some(true));
        let b = block("CANCELED nope");
        let m = b.marker.expect("m");
        assert!(m.cancelled && m.checkbox.is_none() && m.show_label);
        let b = block("## NOW heading");
        assert_eq!(b.heading, Some(2));
        assert_eq!(b.title.text, "heading");
    }

    #[test]
    fn properties_hide_builtins_and_link_tags() {
        let b = block(
            "Title\nid:: 6500c1a4-0000-4000-8000-000000000001\ncollapsed:: true\ntags:: a, b\nstatus:: open",
        );
        assert_eq!(
            b.id.as_deref(),
            Some("6500c1a4-0000-4000-8000-000000000001")
        );
        assert!(b.collapsed);
        let keys: Vec<_> = b.properties.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, ["tags", "status"]);
        assert_eq!(b.properties[0].value.links.len(), 2);
        assert_eq!(b.title.text, "Title");
        assert!(b.body.is_empty());
    }

    #[test]
    fn planning_and_logbook() {
        let b = block(
            "TODO x\nSCHEDULED: <2024-01-01 Mon>\n:LOGBOOK:\nCLOCK: [2024-01-01 Mon 10:00:00]--[2024-01-01 Mon 11:30:00] =>  01:30:00\n:END:",
        );
        assert_eq!(b.planning.len(), 1);
        assert_eq!(b.planning[0].keyword, "SCHEDULED");
        assert_eq!(b.planning[0].text, "<2024-01-01 Mon>");
        let log = b.logbook.expect("logbook");
        assert_eq!(log.entries, 1);
        assert_eq!(log.total.as_deref(), Some("01:30:00"));
        assert!(b.body.is_empty());
    }

    #[test]
    fn code_fences_and_src_blocks() {
        let b = block("t\n```rust\nfn x() {}\n```\nafter\n#+BEGIN_SRC python\nx = 1\n#+END_SRC");
        assert_eq!(b.body.len(), 3);
        let BodyItem::Code(c) = &b.body[0] else {
            panic!("code")
        };
        assert_eq!(c.language.as_deref(), Some("rust"));
        assert_eq!(c.text, "fn x() {}");
        assert!(!c.tokens.is_empty());
        assert!(matches!(&b.body[1], BodyItem::Text(t) if t.text == "after"));
        assert!(matches!(&b.body[2], BodyItem::Code(c) if c.language.as_deref() == Some("python")));
    }

    #[test]
    fn quotes_and_unclosed_regions() {
        let b = block("t\n#+BEGIN_QUOTE\nwise [[x]]\n#+END_QUOTE");
        assert!(matches!(&b.body[0], BodyItem::Quote(q) if q.text == "wise [[x]]"));
        let b = block("t\n```\nnever closed");
        assert!(matches!(&b.body[0], BodyItem::Code(c) if c.text == "never closed"));
    }

    #[test]
    fn multibyte_text_after_hash_plus_never_panics() {
        for body in [
            "#+\u{5e74}\u{5e74}x",
            "#+BEGIN_\u{5e74}\n#+END_\u{5e74}",
            "#+END_\u{5e74}",
        ] {
            let b = block(&format!("t\n{body}"));
            assert_eq!(b.title.text, "t");
        }
    }

    #[test]
    fn image_lines_produce_images() {
        let b = block("t\n![a](../assets/x.png)");
        let BodyItem::Text(t) = &b.body[0] else {
            panic!("text")
        };
        assert_eq!(t.images.len(), 1);
    }

    #[test]
    fn page_rows_follow_depth_and_collapse() {
        let src = "title:: Demo\n\n- a\n  collapsed:: true\n\t- hidden\n- b\n\t- c\n";
        let page = PageModel::from_source(src.as_bytes(), &PropertyConfig::default(), &NoBlocks);
        let shown: Vec<_> = visible_rows(&page.rows)
            .into_iter()
            .map(|i| (page.rows[i].depth, page.rows[i].block.title.text.clone()))
            .collect();
        assert_eq!(
            shown,
            [
                (0, String::new()),
                (0, "a".into()),
                (0, "b".into()),
                (1, "c".into())
            ]
        );
        assert_eq!(page.rows.len(), 5);
        assert!(page.rows[1].has_children && page.rows[1].block.collapsed);
        assert_eq!(page.rows[0].block.properties.len(), 1);
    }

    #[test]
    fn view_collapse_toggles_and_survives_a_reload() {
        let src = "- a\n\t- b\n\t\t- c\n- d\n";
        let mut page =
            PageModel::from_source(src.as_bytes(), &PropertyConfig::default(), &NoBlocks);
        assert_eq!(visible_rows(&page.rows).len(), 4);
        assert!(toggle_row(&mut page.rows, 0));
        assert_eq!(visible_rows(&page.rows), [0, 3]);
        assert!(toggle_row(&mut page.rows, 0));
        assert_eq!(visible_rows(&page.rows).len(), 4);
        assert!(!toggle_row(&mut page.rows, 2), "leaf rows do not toggle");
        page.rows[1].uuid = Some("u1".into());
        assert!(toggle_row(&mut page.rows, 1));
        let overrides = collapse_overrides(&page.rows);
        let mut fresh =
            PageModel::from_source(src.as_bytes(), &PropertyConfig::default(), &NoBlocks);
        fresh.rows[1].uuid = Some("u1".into());
        apply_overrides(&mut fresh.rows, &overrides);
        assert_eq!(visible_rows(&fresh.rows), [0, 1, 3]);
    }
}
