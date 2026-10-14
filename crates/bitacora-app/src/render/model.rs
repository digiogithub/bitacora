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
use super::inline::{
    BlockResolver, NavTarget, Role, TextLayout, layout_line, layout_line_at, layout_lines_at,
};
use super::widget::{self, QueryProps, Widget};

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
    /// Offset in the block text where each code line starts (click-to-caret).
    pub line_src: Vec<usize>,
    /// Set for `#+BEGIN_EXPORT` regions: the view shows a localized kind label instead of the
    /// language (BIT-T-0511).
    pub export: Option<ExportKind>,
}

/// The target of a `#+BEGIN_EXPORT <type>` region. Bitacora has no HTML or KaTeX engine, so the
/// source is shown as code (ADR-035).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    /// `#+BEGIN_EXPORT html`.
    Html,
    /// `#+BEGIN_EXPORT latex`.
    Latex,
    /// `#+BEGIN_EXPORT ascii`.
    Ascii,
    /// No type or another one.
    Other,
}

impl ExportKind {
    fn from_type(ty: &str) -> Self {
        match ty.trim().to_ascii_lowercase().as_str() {
            "html" => Self::Html,
            "latex" => Self::Latex,
            "ascii" => Self::Ascii,
            _ => Self::Other,
        }
    }

    /// The highlighter language of the exported source.
    fn highlight_language(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Latex => "latex",
            Self::Ascii | Self::Other => "",
        }
    }
}

impl CodeBlock {
    /// The block-text offset that matches display offset `at` of [`CodeBlock::text`].
    pub fn source_offset(&self, at: usize) -> Option<usize> {
        let at = at.min(self.text.len());
        let before = &self.text[..at];
        let line = before.matches('\n').count();
        let col = before.len() - before.rfind('\n').map_or(0, |i| i + 1);
        let start = *self.line_src.get(line).or_else(|| self.line_src.last())?;
        Some(start + col)
    }
}

/// The admonition kinds of `#+BEGIN_<KIND>` blocks (Logseq 0.10.x `<note`, `<tip`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalloutKind {
    /// `#+BEGIN_NOTE`.
    Note,
    /// `#+BEGIN_TIP`.
    Tip,
    /// `#+BEGIN_IMPORTANT`.
    Important,
    /// `#+BEGIN_CAUTION`.
    Caution,
    /// `#+BEGIN_WARNING`.
    Warning,
    /// `#+BEGIN_PINNED`.
    Pinned,
}

impl CalloutKind {
    /// The kind of `#+BEGIN_<name>` (case-insensitive), `None` for other regions.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name.to_ascii_uppercase().as_str() {
            "NOTE" => Self::Note,
            "TIP" => Self::Tip,
            "IMPORTANT" => Self::Important,
            "CAUTION" => Self::Caution,
            "WARNING" => Self::Warning,
            "PINNED" => Self::Pinned,
            _ => return None,
        })
    }
}

/// How a `#+BEGIN_<name>` region with structured content is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Special {
    Quote,
    Callout(CalloutKind),
    Center,
    Verse,
    /// Any other name (`#+BEGIN_FOO`): a plain container with a structured body.
    Container,
}

/// Whether a line opens a fence, a `#+BEGIN_<name>` region or a `> ` quote.
fn opens_region(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```")
        || trimmed.starts_with('>')
        || strip_prefix_ci(trimmed, "#+BEGIN_").is_some()
}

/// Region names whose body is verbatim text (code view, widget or hidden), never markup.
fn is_raw_region(name: &str) -> bool {
    ["SRC", "EXAMPLE", "EXPORT", "QUERY", "COMMENT"]
        .iter()
        .any(|n| name.eq_ignore_ascii_case(n))
}

impl Special {
    /// `None` for the raw regions (see [`is_raw_region`]).
    fn from_name(name: &str) -> Option<Self> {
        if is_raw_region(name) {
            return None;
        }
        if name.eq_ignore_ascii_case("QUOTE") {
            return Some(Self::Quote);
        }
        if name.eq_ignore_ascii_case("CENTER") {
            return Some(Self::Center);
        }
        if name.eq_ignore_ascii_case("VERSE") {
            return Some(Self::Verse);
        }
        Some(CalloutKind::from_name(name).map_or(Self::Container, Self::Callout))
    }

    /// Builds the item of a region whose body lines are `lines`.
    fn item(
        self,
        name: &str,
        lines: &[(&str, usize)],
        resolver: &dyn BlockResolver,
        depth: usize,
    ) -> BodyItem {
        if self == Self::Verse {
            return BodyItem::Verse(layout_lines_at(lines.iter().copied(), resolver));
        }
        let body = nested_items(lines, resolver, depth);
        match self {
            Self::Quote => BodyItem::Quote(body),
            Self::Callout(kind) => BodyItem::Callout { kind, body },
            Self::Center => BodyItem::Center(body),
            Self::Verse | Self::Container => BodyItem::Container {
                name: name.to_owned(),
                body,
            },
        }
    }
}

/// An open `#+BEGIN_<name>` region: its kind, name, nesting depth of same-name regions and the
/// `(line, offset)` pairs collected.
type OpenSpecial<'a> = (Special, String, RegionScan, Vec<(&'a str, usize)>);

/// Maximum nesting of quotes and special regions that is parsed structurally; deeper lines are
/// shown as plain text so hostile input cannot overflow the stack.
const MAX_NEST_DEPTH: usize = 32;

/// Tracks same-name nesting of an open special region, ignoring lines inside fences and raw
/// regions (a `#+END_QUOTE` inside a code body does not close the quote).
struct RegionScan {
    depth: usize,
    raw: Option<RawOpen>,
}

enum RawOpen {
    Fence,
    Begin(String),
}

impl RegionScan {
    fn new() -> Self {
        Self {
            depth: 1,
            raw: None,
        }
    }

    /// Feeds one body line of region `name`; true when the line closes the region.
    fn closes(&mut self, line: &str, name: &str) -> bool {
        match &self.raw {
            Some(RawOpen::Fence) => {
                if line.trim_start().starts_with("```") {
                    self.raw = None;
                }
                return false;
            }
            Some(RawOpen::Begin(raw)) => {
                if is_end_of(line, raw) {
                    self.raw = None;
                }
                return false;
            }
            None => {}
        }
        if line.trim_start().starts_with("```") {
            self.raw = Some(RawOpen::Fence);
        } else if is_end_of(line, name) {
            self.depth -= 1;
            return self.depth == 0;
        } else if is_begin_of(line, name) {
            self.depth += 1;
        } else if let Some(rest) = strip_prefix_ci(line.trim(), "#+BEGIN_") {
            let other = rest.split(char::is_whitespace).next().unwrap_or("");
            if is_raw_region(other) {
                self.raw = Some(RawOpen::Begin(other.to_owned()));
            }
        }
        false
    }
}

/// A line that closes `#+BEGIN_<name>`.
fn is_end_of(line: &str, name: &str) -> bool {
    strip_prefix_ci(line.trim(), "#+END_")
        .is_some_and(|rest| rest.trim().eq_ignore_ascii_case(name))
}

/// A line that opens a nested `#+BEGIN_<name>`.
fn is_begin_of(line: &str, name: &str) -> bool {
    strip_prefix_ci(line.trim(), "#+BEGIN_").is_some_and(|rest| {
        rest.split(char::is_whitespace)
            .next()
            .is_some_and(|n| n.eq_ignore_ascii_case(name))
    })
}

/// One item of a block body.
#[derive(Debug, Clone, PartialEq)]
pub enum BodyItem {
    /// A paragraph (consecutive lines).
    Text(TextLayout),
    /// A code fence or `#+BEGIN_SRC` region.
    Code(CodeBlock),
    /// A `#+BEGIN_QUOTE` region or consecutive `> ` lines; the body is structured markup.
    Quote(Vec<BodyItem>),
    /// An admonition (`#+BEGIN_NOTE` ... `#+END_NOTE` and its siblings).
    Callout {
        /// Which admonition.
        kind: CalloutKind,
        /// The content, as nested items.
        body: Vec<BodyItem>,
    },
    /// A `#+BEGIN_CENTER` region.
    Center(Vec<BodyItem>),
    /// A `#+BEGIN_VERSE` region (line breaks kept, so the body stays one inline layout).
    Verse(TextLayout),
    /// Any other `#+BEGIN_<name>` region: a plain container around structured content.
    Container {
        /// The region name as written.
        name: String,
        /// The content, as nested items.
        body: Vec<BodyItem>,
    },
    /// A Markdown heading line (`## Title`) inside a special region.
    Heading {
        /// Level 1-6.
        level: u8,
        /// The heading text.
        layout: TextLayout,
    },
    /// A run of list lines (`- a`, `1. b`) inside a special region.
    List(Vec<ListEntry>),
    /// A live query or an embed that fills its own line (BIT-US-0102, BIT-US-0104).
    Widget(Widget),
}

/// One line of a [`BodyItem::List`].
#[derive(Debug, Clone, PartialEq)]
pub struct ListEntry {
    /// Nesting level from the line indentation (two columns per level).
    pub depth: usize,
    /// The numeral as written (`1.`) for ordered lists; `None` for bullets.
    pub number: Option<String>,
    /// The item text.
    pub layout: TextLayout,
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
        starts: Vec<usize>,
    },
    Begin {
        name: String,
        language: String,
        text: String,
        starts: Vec<usize>,
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
        let mut query_props = QueryProps::default();
        if let Some(group) = analysis.properties.effective() {
            for line in group.lines.iter().filter(|l| l.valid) {
                query_props.set(&line.key_norm, &line.value_raw);
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
        let mut paragraph: Vec<(&str, usize)> = Vec::new();
        let mut region: Option<Region> = None;
        let mut quote: Option<OpenSpecial> = None;
        // Consecutive `> ` lines.
        let mut gt_quote: Vec<(&str, usize)> = Vec::new();
        let flush_paragraph = |model: &mut Self, paragraph: &mut Vec<(&str, usize)>| {
            if !paragraph.is_empty() {
                let layout = layout_lines_at(paragraph.iter().copied(), resolver);
                if !layout.is_empty() {
                    model.body.push(BodyItem::Text(layout));
                }
                paragraph.clear();
            }
        };
        for (index, raw) in content.split_inclusive('\n').enumerate() {
            let mut start = offset;
            offset += raw.len();
            let mut line = raw.trim_end_matches(['\n', '\r']);
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
                    let (text, starts) = match r {
                        Region::Fence { text, starts, .. } | Region::Begin { text, starts, .. } => {
                            (text, starts)
                        }
                    };
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    starts.push(start);
                    text.push_str(line);
                }
                continue;
            }
            if let Some((_, name, depth, q)) = quote.as_mut() {
                if depth.closes(line, name) {
                    if let Some((special, name, _, lines)) = quote.take() {
                        model.body.push(special.item(&name, &lines, resolver, 1));
                    }
                } else {
                    q.push((line, start));
                }
                continue;
            }
            if index > 0 && !gt_quote.is_empty() && !line.trim_start().starts_with('>') {
                let lines = std::mem::take(&mut gt_quote);
                model
                    .body
                    .push(BodyItem::Quote(nested_items(&lines, resolver, 1)));
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
                let head = &content[from..end];
                // A region opening the block (`- #+BEGIN_NOTE`, `- ```lang`, `- > quote`) is
                // rendered, not shown as a title: the block has no title line, as in Logseq.
                if !opens_region(head) {
                    if let Some(w) = widget::detect_line(head) {
                        model.body.push(BodyItem::Widget(w));
                    } else {
                        model.title = layout_line_at(head, from, resolver);
                    }
                    continue;
                }
                line = head;
                start = from;
            }
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("```") {
                flush_paragraph(&mut model, &mut paragraph);
                region = Some(Region::Fence {
                    language: rest.trim().to_owned(),
                    text: String::new(),
                    starts: Vec::new(),
                });
            } else if let Some(begin) = strip_prefix_ci(trimmed, "#+BEGIN_") {
                flush_paragraph(&mut model, &mut paragraph);
                let mut parts = begin.splitn(2, char::is_whitespace);
                let name = parts.next().unwrap_or("").to_owned();
                let language = parts.next().unwrap_or("").trim().to_owned();
                if let Some(special) = Special::from_name(&name) {
                    quote = Some((special, name, RegionScan::new(), Vec::new()));
                } else {
                    region = Some(Region::Begin {
                        name,
                        language,
                        text: String::new(),
                        starts: Vec::new(),
                    });
                }
            } else if let Some(rest) = trimmed.strip_prefix('>') {
                flush_paragraph(&mut model, &mut paragraph);
                let rest = rest.strip_prefix(' ').unwrap_or(rest);
                gt_quote.push((rest, start + (line.len() - rest.len())));
            } else if line.trim().is_empty() {
                flush_paragraph(&mut model, &mut paragraph);
            } else if let Some(w) = widget::detect_line(line) {
                flush_paragraph(&mut model, &mut paragraph);
                model.body.push(BodyItem::Widget(w));
            } else {
                paragraph.push((line, start));
            }
        }
        // Unclosed regions render what they collected.
        if let Some(r) = region.take() {
            model.push_region(r);
        }
        if let Some((special, name, _, lines)) = quote.take() {
            model.body.push(special.item(&name, &lines, resolver, 1));
        }
        if !gt_quote.is_empty() {
            model
                .body
                .push(BodyItem::Quote(nested_items(&gt_quote, resolver, 1)));
        }
        flush_paragraph(&mut model, &mut paragraph);
        for item in &mut model.body {
            if let BodyItem::Widget(Widget::Query(q)) = item {
                q.props = query_props.clone();
            }
        }
        model
    }

    fn push_region(&mut self, region: Region) {
        if let Some(item) = region_item(region) {
            self.body.push(item);
        }
    }

    /// Whether the title line is struck through (cancelled task).
    pub fn is_cancelled(&self) -> bool {
        self.marker.as_ref().is_some_and(|m| m.cancelled)
    }
}

/// The item of a verbatim region (`None` for hidden comments).
fn region_item(region: Region) -> Option<BodyItem> {
    let (name, language, text, line_src) = match region {
        Region::Fence {
            language,
            text,
            starts,
        } => ("SRC".to_owned(), language, text, starts),
        Region::Begin {
            name,
            language,
            text,
            starts,
        } => (name, language, text, starts),
    };
    if name.eq_ignore_ascii_case("COMMENT") {
        return None;
    }
    if name.eq_ignore_ascii_case("QUERY") {
        return Some(BodyItem::Widget(widget::advanced_region(&text)));
    }
    // `#+BEGIN_EXPORT <type>` has no HTML or KaTeX engine here: it is shown as highlighted
    // source under an explicit kind label (ADR-035).
    if name.eq_ignore_ascii_case("EXPORT") {
        let kind = ExportKind::from_type(&language);
        let tokens = highlight(&text, kind.highlight_language());
        return Some(BodyItem::Code(CodeBlock {
            language: None,
            text,
            tokens,
            line_src,
            export: Some(kind),
        }));
    }
    // `#+BEGIN_EXAMPLE` is verbatim text: monospace, no label, no highlighting.
    let example = name.eq_ignore_ascii_case("EXAMPLE");
    let language = (!language.is_empty() && !example).then_some(language);
    let tokens = if example {
        Vec::new()
    } else {
        highlight(&text, language.as_deref().unwrap_or(""))
    };
    Some(BodyItem::Code(CodeBlock {
        language,
        text,
        tokens,
        line_src,
        export: None,
    }))
}

/// The list marker at the start of `line`: `(indent columns, numeral, text offset)`.
fn list_marker(line: &str) -> Option<(usize, Option<String>, usize)> {
    let body = line.trim_start();
    let indent = line[..line.len() - body.len()]
        .chars()
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum();
    let bytes = body.as_bytes();
    let first = *bytes.first()?;
    if matches!(first, b'-' | b'*' | b'+')
        && bytes.get(1).is_some_and(|b| *b == b' ' || *b == b'\t')
    {
        return Some((indent, None, line.len() - body.len() + 2));
    }
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits > 0
        && digits <= 9
        && matches!(bytes.get(digits), Some(b'.' | b')'))
        && bytes
            .get(digits + 1)
            .is_some_and(|b| *b == b' ' || *b == b'\t')
    {
        return Some((
            indent,
            Some(body[..=digits].to_owned()),
            line.len() - body.len() + digits + 2,
        ));
    }
    None
}

/// A Markdown heading line: `(level, text offset)`.
fn heading_marker(line: &str) -> Option<(u8, usize)> {
    let body = line.trim_start();
    let hashes = body.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&hashes) && body[hashes..].starts_with(' ') {
        let lead = line.len() - body.len();
        return Some((hashes as u8, lead + hashes + 1));
    }
    None
}

/// Builds the structured items of a special region body (the same markup a block body has:
/// paragraphs, code, nested regions, quotes) plus lists and headings. Widgets are not
/// evaluated inside regions; a query region shows its source instead. Rendering only: the
/// saved text never changes.
fn nested_items(
    lines: &[(&str, usize)],
    resolver: &dyn BlockResolver,
    depth: usize,
) -> Vec<BodyItem> {
    if depth > MAX_NEST_DEPTH {
        let layout = layout_lines_at(lines.iter().copied(), resolver);
        return if layout.is_empty() {
            Vec::new()
        } else {
            vec![BodyItem::Text(layout)]
        };
    }
    let mut out: Vec<BodyItem> = Vec::new();
    let mut paragraph: Vec<(&str, usize)> = Vec::new();
    let mut quote: Vec<(&str, usize)> = Vec::new();
    let mut list: Vec<ListEntry> = Vec::new();
    fn flush(
        out: &mut Vec<BodyItem>,
        paragraph: &mut Vec<(&str, usize)>,
        quote: &mut Vec<(&str, usize)>,
        list: &mut Vec<ListEntry>,
        resolver: &dyn BlockResolver,
        depth: usize,
    ) {
        if !paragraph.is_empty() {
            let layout = layout_lines_at(paragraph.drain(..), resolver);
            if !layout.is_empty() {
                out.push(BodyItem::Text(layout));
            }
        }
        if !quote.is_empty() {
            let lines = std::mem::take(quote);
            out.push(BodyItem::Quote(nested_items(&lines, resolver, depth + 1)));
        }
        if !list.is_empty() {
            out.push(BodyItem::List(std::mem::take(list)));
        }
    }
    let mut i = 0;
    while i < lines.len() {
        let (line, start) = lines[i];
        i += 1;
        let trimmed = line.trim_start();
        if !trimmed.starts_with('>') && !quote.is_empty() {
            flush(
                &mut out,
                &mut paragraph,
                &mut quote,
                &mut list,
                resolver,
                depth,
            );
        }
        if let Some(rest) = trimmed.strip_prefix("```") {
            flush(
                &mut out,
                &mut paragraph,
                &mut quote,
                &mut list,
                resolver,
                depth,
            );
            let mut text = String::new();
            let mut starts = Vec::new();
            while i < lines.len() && !lines[i].0.trim().starts_with("```") {
                if !starts.is_empty() {
                    text.push('\n');
                }
                starts.push(lines[i].1);
                text.push_str(lines[i].0);
                i += 1;
            }
            i += 1;
            out.extend(region_item(Region::Fence {
                language: rest.trim().to_owned(),
                text,
                starts,
            }));
        } else if let Some(begin) = strip_prefix_ci(trimmed, "#+BEGIN_") {
            flush(
                &mut out,
                &mut paragraph,
                &mut quote,
                &mut list,
                resolver,
                depth,
            );
            let mut parts = begin.splitn(2, char::is_whitespace);
            let name = parts.next().unwrap_or("").to_owned();
            let language = parts.next().unwrap_or("").trim().to_owned();
            let special = Special::from_name(&name);
            let mut scan = RegionScan::new();
            let mut body: Vec<(&str, usize)> = Vec::new();
            while i < lines.len() {
                let (l, s) = lines[i];
                i += 1;
                let closed = if special.is_some() {
                    scan.closes(l, &name)
                } else {
                    is_end_of(l, &name)
                };
                if closed {
                    break;
                }
                body.push((l, s));
            }
            match special {
                Some(special) => out.push(special.item(&name, &body, resolver, depth + 1)),
                None => {
                    let text = body.iter().map(|(l, _)| *l).collect::<Vec<_>>().join("\n");
                    let item = region_item(Region::Begin {
                        name,
                        language,
                        text,
                        starts: body.iter().map(|(_, s)| *s).collect(),
                    });
                    // A query is not evaluated inside a region: show its source.
                    out.extend(item.map(|item| match item {
                        BodyItem::Widget(_) => BodyItem::Code(CodeBlock {
                            language: None,
                            text: body.iter().map(|(l, _)| *l).collect::<Vec<_>>().join("\n"),
                            tokens: Vec::new(),
                            line_src: body.iter().map(|(_, s)| *s).collect(),
                            export: None,
                        }),
                        other => other,
                    }));
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix('>') {
            if !paragraph.is_empty() || !list.is_empty() {
                flush(
                    &mut out,
                    &mut paragraph,
                    &mut quote,
                    &mut list,
                    resolver,
                    depth,
                );
            }
            let rest = rest.strip_prefix(' ').unwrap_or(rest);
            quote.push((rest, start + (line.len() - rest.len())));
        } else if line.trim().is_empty() {
            flush(
                &mut out,
                &mut paragraph,
                &mut quote,
                &mut list,
                resolver,
                depth,
            );
        } else if let Some((level, at)) = heading_marker(line) {
            flush(
                &mut out,
                &mut paragraph,
                &mut quote,
                &mut list,
                resolver,
                depth,
            );
            let layout = layout_line_at(&line[at..], start + at, resolver);
            out.push(BodyItem::Heading { level, layout });
        } else if let Some((indent, number, at)) = list_marker(line) {
            if !paragraph.is_empty() {
                flush(
                    &mut out,
                    &mut paragraph,
                    &mut quote,
                    &mut list,
                    resolver,
                    depth,
                );
            }
            list.push(ListEntry {
                depth: indent / 2,
                number,
                layout: layout_line_at(&line[at..], start + at, resolver),
            });
        } else {
            if !list.is_empty() {
                flush(
                    &mut out,
                    &mut paragraph,
                    &mut quote,
                    &mut list,
                    resolver,
                    depth,
                );
            }
            paragraph.push((line, start));
        }
    }
    flush(
        &mut out,
        &mut paragraph,
        &mut quote,
        &mut list,
        resolver,
        depth,
    );
    out
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
    /// Raw block text from the index (rows read from files or core have none). It identifies
    /// the block when the view edits it in place (`views::remote_edit`).
    pub content: Option<String>,
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
    fn fence_on_the_first_line_is_a_code_block_without_title() {
        // BIT-US-0185: block content of a nested `- ```` bullet with no language.
        let b = block("```\nfastcgi_param HTTPS on;\n```");
        assert!(b.title.text.is_empty());
        assert_eq!(b.body.len(), 1, "{:?}", b.body);
        let BodyItem::Code(c) = &b.body[0] else {
            panic!("code")
        };
        assert_eq!(c.language, None);
        assert_eq!(c.text, "fastcgi_param HTTPS on;");
        assert_eq!(c.source_offset(0), Some(4));
        let b = block("```nginx\nfastcgi_param HTTPS on;\nx 1;\n```");
        assert_eq!(b.body.len(), 1);
        assert!(
            matches!(&b.body[0], BodyItem::Code(c) if c.language.as_deref() == Some("nginx") && c.text.lines().count() == 2)
        );
    }

    /// The text of a nested body made of exactly one paragraph.
    fn only_text(items: &[BodyItem]) -> &str {
        match items {
            [BodyItem::Text(t)] => &t.text,
            other => panic!("expected one paragraph, got {other:?}"),
        }
    }

    #[test]
    fn note_holds_list_fence_and_heading() {
        let b = block(
            "t\n#+BEGIN_NOTE\n## Title [[x]]\n- one\n  - two\n1. three\n```rust\nlet a = 1;\n```\nclosing\n#+END_NOTE\nafter",
        );
        assert_eq!(b.body.len(), 2);
        let BodyItem::Callout { body, .. } = &b.body[0] else {
            panic!("callout: {:?}", b.body)
        };
        assert!(
            matches!(&body[0], BodyItem::Heading { level: 2, layout } if layout.text.starts_with("Title"))
        );
        let BodyItem::List(entries) = &body[1] else {
            panic!("list: {body:?}")
        };
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].layout.text, "one");
        assert_eq!(entries[1].depth, 1);
        assert_eq!(entries[2].number.as_deref(), Some("1."));
        assert!(
            matches!(&body[2], BodyItem::Code(c) if c.language.as_deref() == Some("rust") && c.text == "let a = 1;")
        );
        assert!(matches!(&body[3], BodyItem::Text(t) if t.text == "closing"));
        assert!(matches!(&b.body[1], BodyItem::Text(t) if t.text == "after"));
    }

    #[test]
    fn quote_holds_a_nested_note() {
        let content =
            "t\n#+BEGIN_QUOTE\nintro\n#+BEGIN_NOTE\ninner\n#+END_NOTE\noutro\n#+END_QUOTE";
        let b = block(content);
        assert_eq!(b.body.len(), 1);
        let BodyItem::Quote(q) = &b.body[0] else {
            panic!("quote")
        };
        assert_eq!(q.len(), 3);
        assert!(
            matches!(&q[1], BodyItem::Callout { kind: CalloutKind::Note, body } if only_text(body) == "inner")
        );
        // The same name nested closes at the matching end.
        let b =
            block("t\n#+BEGIN_QUOTE\n#+BEGIN_QUOTE\ndeep\n#+END_QUOTE\nmid\n#+END_QUOTE\nafter");
        let BodyItem::Quote(q) = &b.body[0] else {
            panic!("quote")
        };
        assert!(matches!(&q[0], BodyItem::Quote(d) if only_text(d) == "deep"));
        assert!(matches!(&q[1], BodyItem::Text(t) if t.text == "mid"));
        assert!(matches!(&b.body[1], BodyItem::Text(t) if t.text == "after"));
    }

    #[test]
    fn hostile_nesting_does_not_overflow_the_stack() {
        let deep = format!("t\n{}", "#+BEGIN_QUOTE\n".repeat(10_000));
        assert!(!block(&deep).body.is_empty());
        let gt = format!("t\n{}", ">".repeat(10_000));
        assert!(!block(&gt).body.is_empty());
        let gt_nested = format!("t\n#+BEGIN_QUOTE\n{} x\n#+END_QUOTE", ">".repeat(10_000));
        assert!(!block(&gt_nested).body.is_empty());
    }

    #[test]
    fn end_line_inside_raw_bodies_does_not_close_the_quote() {
        for raw in [
            "```\n#+END_QUOTE\n```",
            "#+BEGIN_SRC rust\n#+END_QUOTE\n#+END_SRC",
            "#+BEGIN_EXAMPLE\n#+BEGIN_QUOTE\n#+END_EXAMPLE",
        ] {
            // Top level, then nested inside another quote.
            let b = block(&format!("t\n#+BEGIN_QUOTE\n{raw}\n#+END_QUOTE\nafter"));
            assert_eq!(b.body.len(), 2, "{raw}: {:?}", b.body);
            let BodyItem::Quote(q) = &b.body[0] else {
                panic!("quote: {raw}")
            };
            assert!(matches!(q.as_slice(), [BodyItem::Code(_)]), "{raw}: {q:?}");
            let b = block(&format!(
                "t\n#+BEGIN_NOTE\n#+BEGIN_QUOTE\n{raw}\n#+END_QUOTE\n#+END_NOTE"
            ));
            let BodyItem::Callout { body, .. } = &b.body[0] else {
                panic!("callout: {raw}")
            };
            assert!(
                matches!(body.as_slice(), [BodyItem::Quote(q)] if matches!(q.as_slice(), [BodyItem::Code(_)])),
                "{raw}: {body:?}"
            );
        }
    }

    #[test]
    fn unknown_region_is_a_container() {
        let b = block("t\n#+BEGIN_FOO bar\n- a\ntext\n#+END_FOO");
        let BodyItem::Container { name, body } = &b.body[0] else {
            panic!("container: {:?}", b.body)
        };
        assert_eq!(name, "FOO");
        assert!(matches!(&body[0], BodyItem::List(l) if l.len() == 1));
        assert!(matches!(&body[1], BodyItem::Text(t) if t.text == "text"));
    }

    #[test]
    fn verse_keeps_line_breaks() {
        let b = block("t\n#+BEGIN_VERSE\nroses\n  are red\n#+END_VERSE");
        assert!(matches!(&b.body[0], BodyItem::Verse(l) if l.text.contains('\n')));
    }

    #[test]
    fn export_regions_are_labelled_code() {
        for (ty, kind, tokens) in [
            ("html", ExportKind::Html, true),
            ("LaTeX", ExportKind::Latex, true),
            ("ascii", ExportKind::Ascii, false),
            ("", ExportKind::Other, false),
        ] {
            let src = "<p class=\"a\">x</p> % c";
            let b = block(&format!("t\n#+BEGIN_EXPORT {ty}\n{src}\n#+END_EXPORT"));
            let BodyItem::Code(c) = &b.body[0] else {
                panic!("code: {:?}", b.body)
            };
            assert_eq!(c.export, Some(kind), "{ty}");
            assert_eq!(c.text, src);
            assert_eq!(!c.tokens.is_empty(), tokens, "{ty}");
        }
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
    fn admonitions_center_verse_and_example_regions() {
        for (name, kind) in [
            ("NOTE", CalloutKind::Note),
            ("tip", CalloutKind::Tip),
            ("IMPORTANT", CalloutKind::Important),
            ("CAUTION", CalloutKind::Caution),
            ("WARNING", CalloutKind::Warning),
            ("PINNED", CalloutKind::Pinned),
        ] {
            let b = block(&format!(
                "t\n#+BEGIN_{name}\nbody [[x]]\n#+END_{name}\nafter"
            ));
            assert!(
                matches!(&b.body[0], BodyItem::Callout { kind: k, body } if *k == kind && only_text(body) == "body [[x]]"),
                "{name}: {:?}",
                b.body
            );
            assert!(matches!(&b.body[1], BodyItem::Text(t) if t.text == "after"));
        }
        let b = block("t\n#+BEGIN_CENTER\nmid\n#+END_CENTER\n#+BEGIN_VERSE\na\nb\n#+END_VERSE");
        assert!(matches!(&b.body[0], BodyItem::Center(l) if only_text(l) == "mid"));
        assert!(matches!(&b.body[1], BodyItem::Verse(l) if l.text.contains('\n')));
        // Example is verbatim: monospace, no label, no highlighting; comments are hidden.
        let b = block(
            "t\n#+BEGIN_EXAMPLE\nlet x = 1;\n#+END_EXAMPLE\n#+BEGIN_COMMENT\nsecret\n#+END_COMMENT",
        );
        assert_eq!(b.body.len(), 1);
        assert!(
            matches!(&b.body[0], BodyItem::Code(c) if c.language.is_none() && c.tokens.is_empty() && c.text == "let x = 1;")
        );
        // An unclosed admonition renders what it collected.
        let b = block("t\n#+BEGIN_WARNING\nopen");
        assert!(matches!(
            &b.body[0],
            BodyItem::Callout {
                kind: CalloutKind::Warning,
                ..
            }
        ));
    }

    #[test]
    fn greater_than_lines_form_a_quote() {
        let b = block("t\n> first\n> second [[x]]\nplain");
        assert!(
            matches!(&b.body[0], BodyItem::Quote(q) if only_text(q).starts_with("first") && only_text(q).contains("second"))
        );
        assert!(matches!(&b.body[1], BodyItem::Text(t) if t.text == "plain"));
        let b = block("t\ntext\n>tight");
        assert!(matches!(&b.body[0], BodyItem::Text(_)));
        assert!(matches!(&b.body[1], BodyItem::Quote(q) if only_text(q) == "tight"));
        // A quote that ends the block is flushed.
        let b = block("t\n> last");
        assert!(matches!(&b.body[0], BodyItem::Quote(q) if only_text(q) == "last"));
    }

    #[test]
    fn quotes_and_unclosed_regions() {
        let b = block("t\n#+BEGIN_QUOTE\nwise [[x]]\n#+END_QUOTE");
        assert!(matches!(&b.body[0], BodyItem::Quote(q) if only_text(q) == "wise [[x]]"));
        let b = block("t\n```\nnever closed");
        assert!(matches!(&b.body[0], BodyItem::Code(c) if c.text == "never closed"));
    }

    #[test]
    fn rendered_text_maps_back_to_offsets_of_the_whole_block_text() {
        let content = "TODO the **title** [[x]]\nid:: 6f2c1b7a-0000-4000-8000-000000000001\nbody **two**\n```\nlet a = 1;\nlet b = 2;\n```";
        let b = block(content);
        // Title: the marker is removed from the title; display offsets still map into content.
        let at = b.title.text.find("title").expect("title");
        let src = b.title.source_offset(at).expect("mapped");
        assert_eq!(&content[src..src + 5], "title");
        let BodyItem::Text(t) = &b.body[0] else {
            panic!("paragraph first");
        };
        let at = t.text.find("two").expect("two");
        let src = t.source_offset(at).expect("mapped");
        assert_eq!(&content[src..src + 3], "two");
        let code = b
            .body
            .iter()
            .find_map(|i| match i {
                BodyItem::Code(c) => Some(c),
                _ => None,
            })
            .expect("code");
        let at = code.text.find("b = 2").expect("b");
        let src = code.source_offset(at).expect("mapped");
        assert_eq!(&content[src..src + 5], "b = 2");
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

    #[test]
    fn query_and_embed_macros_on_their_own_line_become_widgets() {
        use crate::render::widget::{EmbedTarget, QueryKind};
        let b = block(
            "{{query (task TODO)}}\nquery-table:: true\nquery-sort-by:: priority\nquery-sort-desc:: true\nquery-properties:: [:block :page]",
        );
        assert!(b.title.is_empty());
        assert!(b.properties.is_empty());
        let [BodyItem::Widget(Widget::Query(q))] = b.body.as_slice() else {
            panic!("one query widget: {:?}", b.body);
        };
        assert_eq!(q.kind, QueryKind::Simple);
        assert_eq!(q.source, "(task TODO)");
        assert_eq!(q.props.table, Some(true));
        assert_eq!(q.props.sort_by.as_deref(), Some("priority"));
        assert_eq!(q.props.sort_desc, Some(true));
        assert_eq!(q.props.properties.as_ref().map(Vec::len), Some(2));

        let b = block("Intro\n{{embed [[Other]]}}\nafter");
        assert_eq!(b.title.text, "Intro");
        assert!(
            matches!(&b.body[0], BodyItem::Widget(Widget::Embed(EmbedTarget::Page(p))) if p == "Other")
        );
        assert!(matches!(&b.body[1], BodyItem::Text(t) if t.text == "after"));

        // A macro inside a sentence stays inline text.
        let b = block("see {{embed [[Other]]}} here");
        assert!(b.body.is_empty());
    }

    #[test]
    fn regions_opening_the_block_render_without_a_title_line() {
        let b = block("#+BEGIN_QUOTE\nhola\n#+END_QUOTE");
        assert!(b.title.is_empty(), "title: {:?}", b.title.text);
        let [BodyItem::Quote(body)] = b.body.as_slice() else {
            panic!("one quote: {:?}", b.body);
        };
        assert!(matches!(body.as_slice(), [BodyItem::Text(t)] if t.text == "hola"));

        for (src, kind) in [
            ("#+BEGIN_NOTE\nNote\n#+END_NOTE", CalloutKind::Note),
            (
                "#+BEGIN_IMPORTANT\nImportant\n#+END_IMPORTANT",
                CalloutKind::Important,
            ),
            ("#+begin_tip\ntest tip\n#+end_tip", CalloutKind::Tip),
        ] {
            let b = block(src);
            assert!(b.title.is_empty(), "{src}: {:?}", b.title.text);
            assert!(
                matches!(b.body.as_slice(), [BodyItem::Callout { kind: k, .. }] if *k == kind),
                "{src}: {:?}",
                b.body
            );
        }

        let b = block(">  Blockquote");
        assert!(b.title.is_empty());
        let [BodyItem::Quote(body)] = b.body.as_slice() else {
            panic!("one quote: {:?}", b.body);
        };
        assert!(matches!(body.as_slice(), [BodyItem::Text(t)] if t.text.trim() == "Blockquote"));

        let b = block("#+BEGIN_CENTER\nmiddle\n#+END_CENTER\nafter");
        assert!(b.title.is_empty());
        assert!(
            matches!(b.body.as_slice(), [BodyItem::Center(_), BodyItem::Text(t)] if t.text == "after")
        );

        // A task marker stays on the row; the region still renders.
        let b = block("TODO #+BEGIN_NOTE\nbody\n#+END_NOTE");
        assert!(b.marker.is_some());
        assert!(b.title.is_empty());
        assert!(matches!(b.body.as_slice(), [BodyItem::Callout { .. }]));

        // Text before the region keeps its title.
        let b = block("Intro\n#+BEGIN_NOTE\nbody\n#+END_NOTE");
        assert_eq!(b.title.text, "Intro");
        assert!(matches!(b.body.as_slice(), [BodyItem::Callout { .. }]));
    }

    #[test]
    fn begin_query_regions_become_advanced_widgets() {
        use crate::render::widget::QueryKind;
        let b = block(
            "#+BEGIN_QUERY\n{:title \"T\" :query [:find ?b :where [?b :block/marker \"TODO\"]]}\n#+END_QUERY",
        );
        let [BodyItem::Widget(Widget::Query(q))] = b.body.as_slice() else {
            panic!("advanced widget: {:?}", b.body);
        };
        assert_eq!(q.kind, QueryKind::Advanced);
        assert!(q.source.starts_with("{:title"));
    }
}
