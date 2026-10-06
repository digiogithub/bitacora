//! Block-level widgets found in a block's text: live queries and embeds (BIT-US-0102,
//! BIT-US-0104). Detection is pure; the views that draw them live in `views::widgets`.

use bitacora_markdown::inline::{InlineToken, scan_line};

/// How a query is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    /// `{{query (and [[a]] (task TODO))}}`.
    Simple,
    /// `#+BEGIN_QUERY ... #+END_QUERY` or a `{{query {:title ...}}}` EDN map.
    Advanced,
}

/// The `query-*` block properties that steer the result view.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QueryProps {
    /// `query-table:: true|false`: start in table view.
    pub table: Option<bool>,
    /// `query-properties:: [:block :page :priority]`: visible columns, in order.
    pub properties: Option<Vec<String>>,
    /// `query-sort-by:: priority`: sort column.
    pub sort_by: Option<String>,
    /// `query-sort-desc:: true|false`.
    pub sort_desc: Option<bool>,
}

impl QueryProps {
    /// Reads one `query-*` property (normalised key) into the struct; other keys are ignored.
    pub fn set(&mut self, key_norm: &str, value: &str) {
        let value = value.trim();
        match key_norm {
            "query-table" => self.table = parse_bool(value),
            "query-properties" => self.properties = Some(parse_name_list(value)),
            "query-sort-by" => {
                let name = value.trim_start_matches(':').trim();
                self.sort_by = (!name.is_empty()).then(|| name.to_owned());
            }
            "query-sort-desc" => self.sort_desc = parse_bool(value),
            _ => {}
        }
    }
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// `[:block :page :priority]` or `block, page` into names (leading colons removed).
pub fn parse_name_list(v: &str) -> Vec<String> {
    v.trim_matches(|c| c == '[' || c == ']')
        .split(|c: char| c.is_whitespace() || c == ',')
        .map(|s| s.trim().trim_matches('"').trim_start_matches(':'))
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A live query in a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuerySpec {
    /// Simple DSL or advanced.
    pub kind: QueryKind,
    /// The query text: the DSL expression, or the advanced query (with its markers).
    pub source: String,
    /// The block's `query-*` properties.
    pub props: QueryProps,
}

/// What an embed shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EmbedTarget {
    /// `{{embed ((uuid))}}` (lower-cased uuid).
    Block(String),
    /// `{{embed [[page]]}}`.
    Page(String),
}

impl EmbedTarget {
    /// Identity used by the cycle guard (page names are compared case-insensitively).
    pub fn ident(&self) -> String {
        match self {
            Self::Block(u) => format!("block:{u}"),
            Self::Page(p) => format!("page:{}", p.to_lowercase()),
        }
    }
}

/// A block-level widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Widget {
    /// Live query results.
    Query(QuerySpec),
    /// Embedded block or page.
    Embed(EmbedTarget),
}

/// Recognises a line that is exactly one `{{query ...}}` or `{{embed ...}}` macro.
pub fn detect_line(line: &str) -> Option<Widget> {
    let t = line.trim();
    if !t.starts_with("{{") || !t.ends_with("}}") {
        return None;
    }
    let tokens = scan_line(t, 0, t.len());
    let [InlineToken::Macro(m)] = tokens.as_slice() else {
        return None;
    };
    if m.span.start != 0 || m.span.end != t.len() {
        return None;
    }
    // `{{{embed ((id))}}}` is the same macro with the longer braces.
    let braces = if m.triple { 3 } else { 2 };
    let name = t[m.name.start..m.name.end].to_ascii_lowercase();
    let inner = t[braces..t.len() - braces].trim();
    let rest = inner.get(name.len()..).map(str::trim).unwrap_or_default();
    match name.as_str() {
        "query" => {
            let kind = if rest.starts_with('{') || rest.starts_with("[:find") {
                QueryKind::Advanced
            } else {
                QueryKind::Simple
            };
            (!rest.is_empty()).then(|| {
                Widget::Query(QuerySpec {
                    kind,
                    source: rest.to_owned(),
                    props: QueryProps::default(),
                })
            })
        }
        "embed" => parse_embed_target(rest).map(Widget::Embed),
        _ => None,
    }
}

/// `((uuid))` or `[[page]]`.
pub fn parse_embed_target(arg: &str) -> Option<EmbedTarget> {
    let a = arg.trim();
    if let Some(inner) = a.strip_prefix("((").and_then(|r| r.strip_suffix("))")) {
        let u = inner.trim().to_ascii_lowercase();
        return (u.len() == 36 && u.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
            .then_some(EmbedTarget::Block(u));
    }
    let inner = a.strip_prefix("[[")?.strip_suffix("]]")?.trim();
    (!inner.is_empty()).then(|| EmbedTarget::Page(inner.to_owned()))
}

/// A `#+BEGIN_QUERY` region as a widget.
pub fn advanced_region(text: &str) -> Widget {
    Widget::Query(QuerySpec {
        kind: QueryKind::Advanced,
        source: text.to_owned(),
        props: QueryProps::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_sole_macros_only() {
        let Some(Widget::Query(q)) = detect_line("{{query (and [[a]] (task TODO))}}") else {
            panic!("query");
        };
        assert_eq!(q.kind, QueryKind::Simple);
        assert_eq!(q.source, "(and [[a]] (task TODO))");
        let Some(Widget::Query(q)) =
            detect_line("  {{query [:find ?b :where [?b :block/marker \"TODO\"]]}} ")
        else {
            panic!("advanced");
        };
        assert_eq!(q.kind, QueryKind::Advanced);
        assert_eq!(
            detect_line("{{embed [[My Page]]}}"),
            Some(Widget::Embed(EmbedTarget::Page("My Page".into())))
        );
        assert_eq!(
            detect_line("{{embed ((6500C1A4-0000-4000-8000-000000000001))}}"),
            Some(Widget::Embed(EmbedTarget::Block(
                "6500c1a4-0000-4000-8000-000000000001".into()
            )))
        );
        assert_eq!(detect_line("text {{embed [[P]]}}"), None);
        assert_eq!(detect_line("{{embed [[P]]}} text"), None);
        assert_eq!(detect_line("{{embed ((not-a-uuid))}}"), None);
        assert_eq!(
            detect_line("{{{embed [[P]] }}}"),
            Some(Widget::Embed(EmbedTarget::Page("P".into())))
        );
        assert_eq!(detect_line("{{video https://x}}"), None);
        assert_eq!(detect_line("{{query }}"), None);
    }

    #[test]
    fn query_props_parse_lists_and_flags() {
        let mut p = QueryProps::default();
        p.set("query-table", "true");
        p.set("query-properties", "[:block :page, :priority]");
        p.set("query-sort-by", "priority");
        p.set("query-sort-desc", "false");
        assert_eq!(p.table, Some(true));
        assert_eq!(
            p.properties.as_deref(),
            Some(&["block".to_owned(), "page".into(), "priority".into()][..])
        );
        assert_eq!(p.sort_by.as_deref(), Some("priority"));
        assert_eq!(p.sort_desc, Some(false));
    }
}
