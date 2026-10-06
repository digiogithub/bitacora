//! `parse(path, bytes, config) -> ParsedFile`: the pure derivation of index rows
//! (`docs/design/sqlite-index-schema.md` §4.3, `docs/analysis/logseq/03-parsing-indexing-search.md`
//! §3-§5). No DB, no I/O; deterministic.

use std::collections::{BTreeSet, HashMap, HashSet};

use bitacora_config::EffectiveConfig;
use bitacora_core::date::DateFormat;
use bitacora_core::graph_path::GraphPath;
use bitacora_core::journal::{detect_journal, journal_page};
use bitacora_core::naming::{derive_title, page_key, path_file_body};
use bitacora_core::scan::decode_text;
use bitacora_markdown::block::{BlockAnalysis, analyze};
use bitacora_markdown::inline::namespace_parents;
use bitacora_markdown::lines::ParserOptions;
use bitacora_markdown::outline::{content_of, pre_block_content, split_with};
use bitacora_markdown::page_props::{PageProperties, page_properties};
use bitacora_markdown::properties::{PropValue, PropertyConfig, interpret, normalize_key};
use bitacora_markdown::tasks::PlanningKind;
use bitacora_markdown::tree::build_tree;

use crate::normalize::{NormalizeOptions, search_text};
use crate::parsed::{
    BlockRefKind, Diagnostic, DiagnosticKind, FileFormat, PageDef, PageRefKind, PageRefName,
    ParsedBlock, ParsedBlockRef, ParsedFile, ParsedPageRef, ParsedProperty, ParsedPropertyValue,
    Severity, ValueType,
};

/// Bump when parsing output changes (`meta.parser_version`).
pub const PARSER_VERSION: u32 = 1;

/// Settings the parse depends on, derived from the effective `config.edn` (the "relevant config"
/// of the schema doc §4.1).
#[derive(Debug, Clone)]
pub struct ParseConfig<'a> {
    /// The effective config (naming, journals, directories).
    pub config: &'a EffectiveConfig,
    /// Search normalisation settings.
    pub normalize: NormalizeOptions,
}

impl<'a> ParseConfig<'a> {
    /// Defaults for everything but the config.
    #[must_use]
    pub fn new(config: &'a EffectiveConfig) -> Self {
        Self {
            config,
            normalize: NormalizeOptions::default(),
        }
    }
}

/// Hidden built-in property keys (Logseq `hidden-built-in-properties`).
const HIDDEN_BUILT_IN: &[&str] = &[
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
    "hl-type",
    "hl-page",
    "hl-stamp",
    "hl-color",
    "logseq.macro-name",
    "logseq.macro-arguments",
    "logseq.order-list-type",
    "logseq.tldraw.page",
    "logseq.tldraw.shape",
    "todo",
    "doing",
    "now",
    "later",
    "done",
];

/// Editable built-in property keys that do not carry page links.
const EDITABLE_BUILT_IN: &[&str] = &[
    "title",
    "icon",
    "template",
    "template-including-parent",
    "public",
    "filters",
    "exclude-from-graph-view",
    "logseq.query/nlp-date",
    "macro",
    "filetags",
    "logseq.color",
    "logseq.table.version",
    "logseq.table.compact",
    "logseq.table.headers",
    "logseq.table.hover",
    "logseq.table.borders",
    "logseq.table.stripes",
    "logseq.table.max-width",
];

/// Editable built-ins whose values are page links.
const LINKABLE_BUILT_IN: &[&str] = &["alias", "aliases", "tags"];

/// 0 user property, 1 editable built-in, 2 hidden built-in.
#[must_use]
pub fn builtin_level(key_norm: &str) -> u8 {
    if HIDDEN_BUILT_IN.contains(&key_norm) {
        2
    } else if EDITABLE_BUILT_IN.contains(&key_norm) || LINKABLE_BUILT_IN.contains(&key_norm) {
        1
    } else {
        0
    }
}

fn property_config(cfg: &EffectiveConfig) -> PropertyConfig {
    let norm = |v: Vec<String>| v.iter().map(|k| normalize_key(k)).collect::<BTreeSet<_>>();
    PropertyConfig {
        ignored_page_references_keywords: norm(cfg.ignored_page_references_keywords()),
        separated_by_commas: norm(cfg.property_separated_by_commas()),
    }
}

fn page_name(original: &str) -> PageRefName {
    PageRefName {
        name: page_key(original),
        original: original.to_owned(),
    }
}

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, &c)| match i {
            8 | 13 | 18 | 23 => c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

fn file_format(path: &GraphPath) -> FileFormat {
    match path.extension().map(str::to_ascii_lowercase).as_deref() {
        Some("md" | "markdown") => FileFormat::Markdown,
        Some("org") => FileFormat::Org,
        _ => FileFormat::Other,
    }
}

fn int_value(v: Option<PropValue>) -> Option<i64> {
    match v {
        Some(PropValue::Int(n)) => Some(n),
        _ => None,
    }
}

/// Derives the page a file defines (title precedence, journals, aliases, tags).
fn derive_page(
    path: &GraphPath,
    props: &PageProperties,
    format: FileFormat,
    pcfg: &PropertyConfig,
    cfg: &EffectiveConfig,
) -> PageDef {
    let title_prop = props.title().filter(|t| !t.is_empty());
    let mut title = derive_title(path.as_str(), title_prop, cfg);
    let mut key = page_key(&title);
    let mut journal_day = None;

    let journals_dir = format!("{}/", cfg.journals_directory());
    let in_journals = path.as_str().starts_with(&journals_dir);
    let from_name = (in_journals && title_prop.is_none())
        .then(|| {
            DateFormat::new(&cfg.journal_file_name_format())
                .ok()
                .and_then(|f| f.parse(path_file_body(path.as_str())))
        })
        .flatten();
    if let Some(date) = from_name {
        let j = journal_page(date, cfg);
        title = j.title;
        key = j.key;
        journal_day = Some(j.journal_day);
    } else if let Some(j) = detect_journal(&title, cfg) {
        title = j.title;
        key = j.key;
        journal_day = Some(j.journal_day);
    }

    let pages_of = |keys: &[&str]| -> Vec<PageRefName> {
        let mut out: Vec<PageRefName> = Vec::new();
        for k in keys {
            if let Some(PropValue::Pages(set)) = props.value(k, pcfg) {
                for p in set {
                    let n = page_name(&p);
                    if !out.iter().any(|o| o.name == n.name) {
                        out.push(n);
                    }
                }
            }
        }
        out
    };
    let parents = namespace_parents(&title)
        .into_iter()
        .filter(|p| page_key(p) != key)
        .map(|p| page_name(&p))
        .collect();
    PageDef {
        name: key,
        original_name: title,
        journal_day,
        namespace_parents: parents,
        aliases: pages_of(&["alias", "aliases"]),
        tags: pages_of(&["tags"]),
        format,
        created_at: int_value(props.value("created-at", pcfg)),
        updated_at: int_value(
            props
                .value("updated-at", pcfg)
                .or_else(|| props.value("last-modified-at", pcfg)),
        ),
    }
}

/// A raw property line, whatever scanner produced it.
struct RawProp {
    raw_key: String,
    key: String,
    value: String,
}

fn type_property(p: &RawProp, pos: u32, pcfg: &PropertyConfig) -> ParsedProperty {
    let lower_nfc = |s: &str| page_key(s);
    let (value_type, values) = match interpret(&p.key, &p.value, pcfg) {
        PropValue::Pages(set) => {
            let mut seen = HashSet::new();
            let values = set
                .into_iter()
                .map(|n| page_name(&n))
                .filter(|n| seen.insert(n.name.clone()))
                .map(|n| ParsedPropertyValue {
                    value_norm: n.name.clone(),
                    value_num: None,
                    ref_page: Some(n),
                })
                .collect();
            (ValueType::Refs, values)
        }
        PropValue::Bool(b) => (
            ValueType::Boolean,
            vec![ParsedPropertyValue {
                value_norm: b.to_string(),
                value_num: Some(f64::from(u8::from(b))),
                ref_page: None,
            }],
        ),
        PropValue::Int(n) => (
            ValueType::Integer,
            vec![ParsedPropertyValue {
                value_norm: n.to_string(),
                #[allow(clippy::cast_precision_loss)]
                value_num: Some(n as f64),
                ref_page: None,
            }],
        ),
        PropValue::Raw(s) | PropValue::Quoted(s) | PropValue::Str(s) => (
            ValueType::String,
            vec![ParsedPropertyValue {
                value_norm: lower_nfc(&s),
                value_num: None,
                ref_page: None,
            }],
        ),
    };
    ParsedProperty {
        key: p.key.clone(),
        pos,
        raw_key: p.raw_key.clone(),
        raw_value: p.value.trim().to_owned(),
        value_type,
        builtin: builtin_level(&p.key),
        values,
    }
}

/// Typed properties; a repeated key keeps its last value.
fn build_properties(raws: &[RawProp], pcfg: &PropertyConfig) -> Vec<ParsedProperty> {
    let mut last: HashMap<&str, usize> = HashMap::new();
    for (i, r) in raws.iter().enumerate() {
        last.insert(r.key.as_str(), i);
    }
    let mut out = Vec::new();
    for (i, r) in raws.iter().enumerate() {
        if last.get(r.key.as_str()) == Some(&i) {
            let pos = u32::try_from(out.len()).unwrap_or(u32::MAX);
            out.push(type_property(r, pos, pcfg));
        }
    }
    out
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut v = vec![0];
    v.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    v
}

fn line_of(starts: &[usize], offset: usize) -> u32 {
    let n = starts.partition_point(|&s| s <= offset);
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Ref accumulator that keeps first-seen order and drops duplicates.
#[derive(Default)]
struct PageRefs {
    refs: Vec<ParsedPageRef>,
    seen: HashSet<(String, PageRefKind)>,
}

impl PageRefs {
    fn add(&mut self, original: &str, kind: PageRefKind) {
        if original.trim().is_empty() {
            return;
        }
        let page = page_name(original);
        if self.seen.insert((page.name.clone(), kind)) {
            self.refs.push(ParsedPageRef { page, kind });
        }
    }
}

/// Names and ids an `{{embed ...}}` macro points at.
fn embed_targets(a: &BlockAnalysis) -> (HashSet<String>, HashSet<String>) {
    let mut pages = HashSet::new();
    let mut blocks = HashSet::new();
    for m in a.refs.content.macros.iter().filter(|m| m.name == "embed") {
        let joined = m.args.join(", ");
        let j = joined.trim();
        if let Some(inner) = j.strip_prefix("[[").and_then(|s| s.strip_suffix("]]")) {
            pages.insert(page_key(inner.trim()));
        } else if let Some(id) = j.strip_prefix("((").and_then(|s| s.strip_suffix("))")) {
            blocks.insert(id.trim().to_ascii_lowercase());
        } else {
            pages.insert(page_key(j));
        }
    }
    (pages, blocks)
}

fn block_refs_of(a: &BlockAnalysis, content: &str) -> Vec<ParsedBlockRef> {
    let (_, embedded) = embed_targets(a);
    let mut out = Vec::new();
    for id in a.refs.block_refs() {
        let id = id.to_ascii_lowercase();
        let n_embed = usize::from(embedded.contains(&id));
        let n_link = content
            .to_ascii_lowercase()
            .matches(&format!("]((({id})))"))
            .count();
        let n_total = content
            .to_ascii_lowercase()
            .matches(&format!("(({id}))"))
            .count();
        if n_embed > 0 {
            out.push(ParsedBlockRef {
                target_uuid: id.clone(),
                kind: BlockRefKind::Embed,
            });
        }
        if n_link > 0 {
            out.push(ParsedBlockRef {
                target_uuid: id.clone(),
                kind: BlockRefKind::Link,
            });
        }
        if n_total > n_embed + n_link || (n_total == 0 && n_embed == 0 && n_link == 0) {
            out.push(ParsedBlockRef {
                target_uuid: id,
                kind: BlockRefKind::Ref,
            });
        }
    }
    out
}

struct PageRefInputs<'a> {
    analysis: &'a BlockAnalysis,
    props: &'a [ParsedProperty],
    extra_property_pages: &'a [String],
    names_enabled: bool,
    exclude: &'a BTreeSet<String>,
}

fn page_refs_of(i: &PageRefInputs<'_>) -> Vec<ParsedPageRef> {
    let a = i.analysis;
    let mut r = PageRefs::default();
    if let Some(m) = &a.refs.marker {
        r.add(m, PageRefKind::Marker);
    }
    if let Some(p) = &a.refs.priority {
        r.add(p, PageRefKind::Priority);
    }
    let (embed_pages, _) = embed_targets(a);
    for p in &a.refs.content.pages {
        let kind = if embed_pages.contains(&page_key(p)) {
            PageRefKind::Embed
        } else if a.refs.content.tags.contains(p) {
            PageRefKind::Tag
        } else {
            PageRefKind::Link
        };
        r.add(p, kind);
    }
    for p in a.refs.property_pages.iter().chain(i.extra_property_pages) {
        r.add(p, PageRefKind::PropertyValue);
    }
    if i.names_enabled {
        for p in i.props {
            if p.builtin == 0 && !i.exclude.contains(&p.key) {
                r.add(&p.key, PageRefKind::PropertyName);
            }
        }
    }
    let direct: Vec<(String, String)> = r
        .refs
        .iter()
        .filter(|x| !matches!(x.kind, PageRefKind::Marker | PageRefKind::Priority))
        .map(|x| (x.page.original.clone(), x.page.name.clone()))
        .collect();
    for (orig, name) in direct {
        for parent in namespace_parents(&orig) {
            if page_key(&parent) != name {
                r.add(&parent, PageRefKind::NamespaceParent);
            }
        }
    }
    r.refs
}

fn first_line_title(content: &str, title_start: usize) -> String {
    let line_end = content.find(['\n', '\r']).unwrap_or(content.len());
    content
        .get(title_start.min(line_end)..line_end)
        .unwrap_or("")
        .trim()
        .to_owned()
}

fn hash16(s: &str) -> [u8; 16] {
    let h = blake3::hash(s.as_bytes());
    let mut out = [0u8; 16];
    out.copy_from_slice(&h.as_bytes()[..16]);
    out
}

fn push_unique_page(out: &mut Vec<PageRefName>, seen: &mut HashSet<String>, name: &PageRefName) {
    if seen.insert(name.name.clone()) {
        out.push(name.clone());
    }
}

/// Parses one graph file into index rows. `path` is graph-relative; `bytes` are the raw file
/// bytes (a UTF-8 BOM is tolerated).
#[must_use]
pub fn parse(path: &GraphPath, bytes: &[u8], cfg: &ParseConfig<'_>) -> ParsedFile {
    let format = file_format(path);
    let file_hash = *blake3::hash(bytes).as_bytes();
    let pcfg = property_config(cfg.config);
    let mut diagnostics = Vec::new();

    let text = decode_text(bytes);
    let bom = if text.had_bom { 3u64 } else { 0 };
    let opts = ParserOptions::default();
    let src = text.text.as_bytes();

    let (page_props, parse_blocks) = if format == FileFormat::Markdown && !text.lossy {
        (page_properties(src, opts), true)
    } else {
        (PageProperties::default(), false)
    };
    let page = derive_page(path, &page_props, format, &pcfg, cfg.config);

    match format {
        FileFormat::Markdown if text.lossy => diagnostics.push(Diagnostic {
            kind: DiagnosticKind::ParseError,
            severity: Severity::Error,
            line: None,
            message: "file is not valid UTF-8; blocks are not indexed".to_owned(),
        }),
        FileFormat::Org => diagnostics.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            severity: Severity::Info,
            line: None,
            message: "org-mode files are indexed as pages only".to_owned(),
        }),
        FileFormat::Other => diagnostics.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            severity: Severity::Info,
            line: None,
            message: "unsupported file type".to_owned(),
        }),
        FileFormat::Markdown => {}
    }

    let mut blocks: Vec<ParsedBlock> = Vec::new();
    if parse_blocks {
        let names_enabled = cfg.config.property_pages_enabled();
        let exclude: BTreeSet<String> = cfg
            .config
            .property_pages_excludelist()
            .iter()
            .map(|k| normalize_key(k))
            .collect();
        let starts = line_starts(&text.text);
        let outline = split_with(src, opts);
        let links = build_tree(&outline.blocks);
        let offset = usize::from(outline.pre_block.is_some());
        let mut sibling_counts: HashMap<Option<usize>, u32> = HashMap::new();
        let mut seen_ids: HashMap<String, u32> = HashMap::new();

        let mut make = |ord: usize,
                        pre: bool,
                        content: &str,
                        raws: Option<Vec<RawProp>>,
                        extra_pages: &[String],
                        span: (usize, usize),
                        head_offset: usize,
                        parent: Option<usize>,
                        depth: usize,
                        sib: u32|
         -> ParsedBlock {
            let a = analyze(content, &pcfg, opts);
            let raws = raws.unwrap_or_else(|| {
                a.properties
                    .effective()
                    .map(|g| {
                        g.lines
                            .iter()
                            .filter(|l| l.valid)
                            .map(|l| RawProp {
                                raw_key: l.key_raw.clone(),
                                key: l.key_norm.clone(),
                                value: l.value_raw.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            });
            let properties = build_properties(&raws, &pcfg);
            let prop = |k: &str| properties.iter().find(|p| p.key == k);
            let pint = |k: &str| match prop(k) {
                Some(p) if p.value_type == ValueType::Integer => {
                    p.values.first().and_then(|v| v.value_num).map(|n| {
                        #[allow(clippy::cast_possible_truncation)]
                        {
                            n as i64
                        }
                    })
                }
                _ => None,
            };
            let explicit = prop("id").map(|p| p.raw_value.to_ascii_lowercase());
            let line_start = line_of(&starts, span.0 + head_offset);
            let explicit_uuid = match explicit {
                Some(id) if is_uuid(&id) => Some(id),
                Some(id) => {
                    diagnostics.push(Diagnostic {
                        kind: DiagnosticKind::InvalidProperty,
                        severity: Severity::Warning,
                        line: Some(line_start),
                        message: format!("id:: value `{id}` is not a UUID"),
                    });
                    None
                }
                None => None,
            };
            if let Some(id) = &explicit_uuid
                && let Some(first) = seen_ids.insert(id.clone(), line_start)
            {
                diagnostics.push(Diagnostic {
                    kind: DiagnosticKind::DuplicateBlockId,
                    severity: Severity::Warning,
                    line: Some(line_start),
                    message: format!("id:: {id} already used at line {first}"),
                });
            }
            for l in a.properties.invalid_lines() {
                diagnostics.push(Diagnostic {
                    kind: DiagnosticKind::InvalidProperty,
                    severity: Severity::Info,
                    line: Some(line_start),
                    message: format!("invalid property name `{}`", l.key_raw),
                });
            }
            let collapsed = matches!(
                prop("collapsed").map(|p| p.values[0].value_norm.as_str()),
                Some("true")
            );
            let heading = a
                .head
                .heading
                .or_else(|| pint("heading").and_then(|n| usize::try_from(n).ok()))
                .filter(|n| (1..=6).contains(n))
                .and_then(|n| u8::try_from(n).ok());
            let raw_of = |kind: PlanningKind| {
                a.planning
                    .iter()
                    .rev()
                    .find(|p| p.kind == kind && p.lifted_date().is_some())
                    .and_then(|p| content.get(p.timestamp.span.start..p.timestamp.span.end))
                    .map(str::to_owned)
            };
            let (search, truncated) = search_text(content, cfg.normalize);
            if truncated {
                diagnostics.push(Diagnostic {
                    kind: DiagnosticKind::TooLarge,
                    severity: Severity::Warning,
                    line: Some(line_start),
                    message: format!(
                        "block longer than {} characters; search text truncated",
                        cfg.normalize.max_len
                    ),
                });
            }
            let page_refs = page_refs_of(&PageRefInputs {
                analysis: &a,
                props: &properties,
                extra_property_pages: extra_pages,
                names_enabled,
                exclude: &exclude,
            });
            let ord_u = u32::try_from(ord).unwrap_or(u32::MAX);
            ParsedBlock {
                ord: ord_u,
                subtree_end: ord_u,
                depth: u32::try_from(depth).unwrap_or(u32::MAX),
                sibling_idx: sib,
                parent_ord: parent.map(|p| u32::try_from(p).unwrap_or(u32::MAX)),
                is_pre_block: pre,
                title: first_line_title(content, a.head.title_start),
                search_text: search,
                marker: a.refs.marker.clone(),
                priority: a.refs.priority.clone(),
                scheduled: a.scheduled,
                scheduled_raw: raw_of(PlanningKind::Scheduled),
                deadline: a.deadline,
                deadline_raw: raw_of(PlanningKind::Deadline),
                repeated: a.repeated,
                collapsed,
                heading,
                explicit_uuid,
                created_at: pint("created-at"),
                updated_at: pint("updated-at").or_else(|| pint("last-modified-at")),
                byte_start: span.0 as u64 + bom,
                byte_end: span.1 as u64 + bom,
                line_start,
                content_hash: hash16(content),
                block_refs: block_refs_of(&a, content),
                page_refs,
                properties,
                content: content.to_owned(),
            }
        };

        if let Some(span) = outline.pre_block {
            let content = pre_block_content(src, span).into_owned();
            let raws: Vec<RawProp> = page_props
                .properties
                .iter()
                .filter(|p| p.valid)
                .map(|p| RawProp {
                    raw_key: p.key_raw.clone(),
                    key: p.key_norm.clone(),
                    value: p.value_raw.clone(),
                })
                .collect();
            // Page properties from `#+key:` / front matter are not property lines of the text, so
            // their page values are added explicitly.
            let extra: Vec<String> = raws
                .iter()
                .filter_map(|r| match interpret(&r.key, &r.value, &pcfg) {
                    PropValue::Pages(s) => Some(s),
                    _ => None,
                })
                .flatten()
                .collect();
            let mut b = make(
                0,
                page_props.is_pre_block,
                &content,
                Some(raws),
                &extra,
                (span.start, span.end),
                0,
                None,
                1,
                0,
            );
            // The pre-block's own text is properties; it has no task, title or planning.
            b.title = first_line_title(&content, 0);
            blocks.push(b);
            sibling_counts.insert(None, 1);
        }

        for (i, rb) in outline.blocks.iter().enumerate() {
            let content = content_of(src, rb).into_owned();
            let link = links[i];
            let parent = link.parent.map(|p| p + offset);
            let counter = sibling_counts.entry(link.parent).or_insert(0);
            let sib = *counter;
            *counter += 1;
            let b = make(
                i + offset,
                false,
                &content,
                None,
                &[],
                (rb.span.start, rb.span.end),
                rb.head_line.start - rb.span.start,
                parent,
                link.depth,
                sib,
            );
            blocks.push(b);
        }

        // Pre-order intervals: the end of a subtree is the largest descendant ord.
        for i in (0..blocks.len()).rev() {
            if let Some(p) = blocks[i].parent_ord {
                let end = blocks[i].subtree_end;
                let parent = &mut blocks[p as usize];
                parent.subtree_end = parent.subtree_end.max(end);
            }
        }
    }

    let mut referenced_pages = Vec::new();
    let mut seen = HashSet::new();
    for n in page
        .namespace_parents
        .iter()
        .chain(&page.aliases)
        .chain(&page.tags)
    {
        push_unique_page(&mut referenced_pages, &mut seen, n);
    }
    for b in &blocks {
        for r in &b.page_refs {
            push_unique_page(&mut referenced_pages, &mut seen, &r.page);
        }
        for p in &b.properties {
            for v in &p.values {
                if let Some(n) = &v.ref_page {
                    push_unique_page(&mut referenced_pages, &mut seen, n);
                }
            }
        }
    }

    ParsedFile {
        page,
        blocks,
        referenced_pages,
        diagnostics,
        file_hash,
    }
}
