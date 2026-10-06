//! Simple query DSL (`{{query ...}}`): pre-transform, AST and parser (design §7.1).

use unicode_normalization::UnicodeNormalization;

use super::QueryError;
use super::edn::{Edn, read_all};

/// A property value argument, parsed as the DSL does: integers and booleans keep their type,
/// `#x` and `[[x]]` lose their decoration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropValue {
    /// Integer.
    Int(i64),
    /// Boolean.
    Bool(bool),
    /// Text (trimmed, decoration removed).
    Str(String),
}

/// The query AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    /// `(and ...)`
    And(Vec<Query>),
    /// `(or ...)`
    Or(Vec<Query>),
    /// `(not ...)`: negation of the conjunction of the arguments.
    Not(Vec<Query>),
    /// `[[x]]` / `#x`: page key (lower-cased).
    PageRef(String),
    /// `"text"`
    Text(String),
    /// `(task ...)`, `(todo ...)`: upper-cased markers.
    Task(Vec<String>),
    /// `(priority ...)`: upper-cased priorities.
    Priority(Vec<String>),
    /// `(property k [v])` on blocks.
    Property {
        /// Normalised key.
        key: String,
        /// Value, when given.
        value: Option<PropValue>,
    },
    /// `(page-property k [v])`.
    PageProperty {
        /// Normalised key.
        key: String,
        /// Value, when given.
        value: Option<PropValue>,
    },
    /// `(between a b)` over journal days; arguments are kept raw and resolved at compile time.
    Between(String, String),
    /// `(between created-at a b)` / `(between last-modified-at a b)`.
    BetweenProp {
        /// `created-at` or `last-modified-at`.
        key: String,
        /// Start.
        from: String,
        /// End.
        to: String,
    },
    /// `(page x)`: page key.
    Page(String),
    /// `(namespace x)`: page key of the direct parent.
    Namespace(String),
    /// `(page-tags a b)`: page keys.
    PageTags(Vec<String>),
    /// `(all-page-tags)`
    AllPageTags,
}

/// Post-processing sort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortBy {
    /// Normalised property key.
    pub key: String,
    /// Descending (the default).
    pub desc: bool,
}

/// A parsed simple query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleQuery {
    /// The filter tree.
    pub filter: Query,
    /// `(sort-by k [asc|desc])`.
    pub sort_by: Option<SortBy>,
    /// `(sample n)`.
    pub sample: Option<usize>,
}

/// `[[x]]` / `#x` / `#[[x]]` become string literals `"[[x]]"` (outside string literals).
#[must_use]
pub fn pre_transform(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len() + 8);
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == '"' {
            out.push(c);
            i += 1;
            while i < b.len() {
                out.push(b[i]);
                if b[i] == '\\' && i + 1 < b.len() {
                    out.push(b[i + 1]);
                    i += 2;
                    continue;
                }
                i += 1;
                if b[i - 1] == '"' {
                    break;
                }
            }
            continue;
        }
        let hash = c == '#';
        let ref_at = if hash { i + 1 } else { i };
        if b.get(ref_at) == Some(&'[')
            && b.get(ref_at + 1) == Some(&'[')
            && let Some(end) = find_ref_end(&b, ref_at + 2)
        {
            let inner: String = b[ref_at + 2..end].iter().collect();
            push_ref(&mut out, &inner);
            i = end + 2;
            continue;
        }
        if hash
            && b.get(i + 1)
                .is_some_and(|n| !n.is_whitespace() && !"{\"_()[]".contains(*n))
        {
            let mut j = i + 1;
            while j < b.len() && !b[j].is_whitespace() && !"()[]{}\",".contains(b[j]) {
                j += 1;
            }
            let tag: String = b[i + 1..j].iter().collect();
            push_ref(&mut out, &tag);
            i = j;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn find_ref_end(b: &[char], from: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = from;
    while i + 1 < b.len() {
        if b[i] == '[' && b[i + 1] == '[' {
            depth += 1;
            i += 2;
        } else if b[i] == ']' && b[i + 1] == ']' {
            if depth == 0 {
                return Some(i);
            }
            depth -= 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    None
}

fn push_ref(out: &mut String, inner: &str) {
    out.push_str("\"[[");
    out.push_str(&inner.replace('\\', "\\\\").replace('"', "\\\""));
    out.push_str("]]\"");
}

/// Lower-case + NFC (the value normalisation of the index).
#[must_use]
pub fn lower_nfc(s: &str) -> String {
    s.to_lowercase().nfc().collect()
}

fn page_key(s: &str) -> String {
    let s = s.trim();
    let s = s
        .strip_prefix("[[")
        .and_then(|r| r.strip_suffix("]]"))
        .unwrap_or(s);
    bitacora_core::naming::page_key(s.trim())
}

fn is_page_ref(s: &str) -> bool {
    s.starts_with("[[") && s.ends_with("]]") && s.len() >= 4
}

fn syntax(msg: impl Into<String>) -> QueryError {
    QueryError::Syntax(msg.into())
}

/// Strips an optional `{{query ...}}` wrapper.
fn strip_macro(src: &str) -> &str {
    let t = src.trim();
    if let Some(rest) = t.strip_prefix("{{") {
        let rest = rest.strip_suffix("}}").unwrap_or(rest).trim_start();
        if rest.len() >= 5 && rest[..5].eq_ignore_ascii_case("query") {
            return rest[5..].trim();
        }
    }
    t
}

/// Parses a simple query (with or without the `{{query ...}}` macro around it).
pub fn parse(src: &str) -> Result<SimpleQuery, QueryError> {
    let body = pre_transform(strip_macro(src));
    let forms = read_all(&body).map_err(|e| syntax(e.to_string()))?;
    if forms.is_empty() {
        return Err(syntax("empty query"));
    }
    let mut p = Parser {
        sort_by: None,
        sample: None,
    };
    let mut parts = Vec::new();
    for f in &forms {
        if let Some(q) = p.node(f)? {
            parts.push(q);
        }
    }
    let filter = match parts.len() {
        0 => return Err(syntax("the query has no filter (only sort-by / sample)")),
        1 => parts.remove(0),
        _ => Query::And(parts),
    };
    Ok(SimpleQuery {
        filter,
        sort_by: p.sort_by,
        sample: p.sample,
    })
}

struct Parser {
    sort_by: Option<SortBy>,
    sample: Option<usize>,
}

/// The name of a symbol, keyword or string argument.
fn name_of(e: &Edn) -> Option<String> {
    match e {
        Edn::Sym(s) | Edn::Str(s) => Some(s.clone()),
        Edn::Kw(s) => Some(s.clone()),
        Edn::Int(n) => Some(n.to_string()),
        Edn::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn norm_key(e: &Edn) -> Result<String, QueryError> {
    let n = name_of(e).ok_or_else(|| syntax(format!("expected a property name, got {e}")))?;
    Ok(n.trim().to_lowercase().replace(['_', ' '], "-"))
}

/// Arguments that may be given inline or as one collection.
fn flat_args(args: &[Edn]) -> Vec<&Edn> {
    match args.first() {
        Some(Edn::Vector(v) | Edn::List(v) | Edn::Set(v)) => v.iter().collect(),
        _ => args.iter().collect(),
    }
}

pub(super) fn parse_prop_value(e: &Edn) -> Result<PropValue, QueryError> {
    match e {
        Edn::Int(n) => Ok(PropValue::Int(*n)),
        Edn::Bool(b) => Ok(PropValue::Bool(*b)),
        _ => {
            let s = name_of(e)
                .or_else(|| match e {
                    Edn::Float(x) => Some(x.to_string()),
                    _ => None,
                })
                .ok_or_else(|| syntax(format!("unsupported property value {e}")))?;
            let t = s.trim();
            Ok(match t {
                "true" => PropValue::Bool(true),
                "false" => PropValue::Bool(false),
                _ => {
                    if let Ok(n) = t.parse::<i64>() {
                        PropValue::Int(n)
                    } else if is_page_ref(t) {
                        PropValue::Str(t[2..t.len() - 2].trim().to_owned())
                    } else if let Some(rest) = t.strip_prefix('#') {
                        PropValue::Str(rest.trim().to_owned())
                    } else {
                        PropValue::Str(t.to_owned())
                    }
                }
            })
        }
    }
}

impl Parser {
    /// `Ok(None)` for post-processing forms (`sort-by`, `sample`) that carry no filter.
    fn node(&mut self, e: &Edn) -> Result<Option<Query>, QueryError> {
        match e {
            Edn::Str(s) => Ok(Some(if is_page_ref(s) {
                Query::PageRef(page_key(s))
            } else {
                Query::Text(s.clone())
            })),
            Edn::List(items) | Edn::Vector(items) => self.form(items),
            other => Err(syntax(format!("unexpected form {other}"))),
        }
    }

    fn children(&mut self, args: &[Edn], op: &str) -> Result<Vec<Query>, QueryError> {
        let mut out = Vec::new();
        for a in args {
            if let Some(q) = self.node(a)? {
                out.push(q);
            }
        }
        if out.is_empty() {
            return Err(syntax(format!("`{op}` needs at least one filter")));
        }
        Ok(out)
    }

    fn form(&mut self, items: &[Edn]) -> Result<Option<Query>, QueryError> {
        let Some(head) = items.first() else {
            return Err(syntax("empty form `()`"));
        };
        let Some(op) = head.sym_lc().or_else(|| match head {
            Edn::Kw(k) => Some(k.to_lowercase()),
            _ => None,
        }) else {
            return Err(syntax(format!("expected an operator, got {head}")));
        };
        let args = &items[1..];
        let need = |n: usize| -> Result<(), QueryError> {
            if args.len() < n {
                Err(syntax(format!("`{op}` needs at least {n} argument(s)")))
            } else {
                Ok(())
            }
        };
        let q = match op.as_str() {
            "and" => Query::And(self.children(args, "and")?),
            "or" => Query::Or(self.children(args, "or")?),
            "not" => Query::Not(self.children(args, "not")?),
            "task" | "todo" => {
                let m: Vec<String> = flat_args(args)
                    .iter()
                    .filter_map(|e| name_of(e))
                    .map(|s| s.to_uppercase())
                    .collect();
                if m.is_empty() {
                    return Err(syntax("`task` needs at least one marker"));
                }
                Query::Task(m)
            }
            "priority" => {
                let m: Vec<String> = flat_args(args)
                    .iter()
                    .filter_map(|e| name_of(e))
                    .map(|s| s.to_uppercase())
                    .collect();
                if m.is_empty() {
                    return Err(syntax("`priority` needs at least one priority"));
                }
                Query::Priority(m)
            }
            "property" | "page-property" => {
                need(1)?;
                if args.len() > 2 {
                    return Err(syntax(format!("`{op}` takes a key and an optional value")));
                }
                let key = norm_key(&args[0])?;
                let value = args.get(1).map(parse_prop_value).transpose()?;
                if op == "property" {
                    Query::Property { key, value }
                } else {
                    Query::PageProperty { key, value }
                }
            }
            "between" => match args.len() {
                2 => Query::Between(date_arg(&args[0])?, date_arg(&args[1])?),
                3 => {
                    let key = norm_key(&args[0])?;
                    if key != "created-at" && key != "last-modified-at" {
                        return Err(QueryError::Unsupported(format!(
                            "between on property `{key}` (only created-at and last-modified-at)"
                        )));
                    }
                    Query::BetweenProp {
                        key,
                        from: date_arg(&args[1])?,
                        to: date_arg(&args[2])?,
                    }
                }
                _ => return Err(syntax("`between` takes 2 or 3 arguments")),
            },
            "page" => {
                need(1)?;
                Query::Page(page_key(
                    &name_of(&args[0]).ok_or_else(|| syntax("bad page name"))?,
                ))
            }
            "namespace" => {
                need(1)?;
                let k = page_key(&name_of(&args[0]).ok_or_else(|| syntax("bad namespace"))?);
                if k.is_empty() {
                    return Err(syntax("empty namespace"));
                }
                Query::Namespace(k)
            }
            "page-tags" => {
                let tags: Vec<String> = flat_args(args)
                    .iter()
                    .filter_map(|e| name_of(e))
                    .map(|s| page_key(&s))
                    .collect();
                if tags.is_empty() {
                    return Err(syntax("`page-tags` needs at least one tag"));
                }
                Query::PageTags(tags)
            }
            "all-page-tags" => Query::AllPageTags,
            "sort-by" => {
                need(1)?;
                let key = norm_key(&args[0])?;
                let desc = !matches!(
                    args.get(1).and_then(Edn::sym_lc_or_kw).as_deref(),
                    Some("asc")
                );
                self.sort_by = Some(SortBy { key, desc });
                return Ok(None);
            }
            "sample" => {
                need(1)?;
                match &args[0] {
                    Edn::Int(n) if *n >= 0 => {
                        self.sample = Some(usize::try_from(*n).unwrap_or(usize::MAX));
                    }
                    other => return Err(syntax(format!("`sample` needs a count, got {other}"))),
                }
                return Ok(None);
            }
            other => {
                return Err(QueryError::Unsupported(format!("query operator `{other}`")));
            }
        };
        Ok(Some(q))
    }
}

fn date_arg(e: &Edn) -> Result<String, QueryError> {
    match e {
        Edn::Sym(s) | Edn::Str(s) | Edn::Kw(s) => Ok(s.clone()),
        other => Err(syntax(format!("invalid date argument {other}"))),
    }
}

impl Edn {
    fn sym_lc_or_kw(&self) -> Option<String> {
        match self {
            Edn::Sym(s) | Edn::Kw(s) => Some(s.to_lowercase()),
            _ => None,
        }
    }
}

impl Query {
    /// Whether this leaf is block-level under the result-type rule (design §7.1.2).
    fn is_block_level(&self) -> bool {
        match self {
            Query::And(v) | Query::Or(v) | Query::Not(v) => v.iter().any(Query::is_block_level),
            Query::PageRef(_)
            | Query::Text(_)
            | Query::Task(_)
            | Query::Priority(_)
            | Query::Property { .. }
            | Query::Between(..)
            | Query::BetweenProp { .. }
            | Query::Page(_) => true,
            Query::PageProperty { .. }
            | Query::Namespace(_)
            | Query::PageTags(_)
            | Query::AllPageTags => false,
        }
    }

    /// Logseq's result-type rule: blocks if any block-level filter appears, else pages.
    #[must_use]
    pub fn returns_blocks(&self) -> bool {
        self.is_block_level()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn pre_transform_refs_and_tags() {
        assert_eq!(
            pre_transform("(and [[Foo Bar]] #tag #[[a b]] \"[[x]] #y\")"),
            "(and \"[[Foo Bar]]\" \"[[tag]]\" \"[[a b]]\" \"[[x]] #y\")"
        );
    }

    #[test]
    fn parses_operators() {
        let q = parse("{{query (and [[a]] (task TODO doing) (priority [a b]) (not (page X)))}}")
            .expect("parse");
        assert_eq!(
            q.filter,
            Query::And(vec![
                Query::PageRef("a".into()),
                Query::Task(vec!["TODO".into(), "DOING".into()]),
                Query::Priority(vec!["A".into(), "B".into()]),
                Query::Not(vec![Query::Page("x".into())]),
            ])
        );
    }

    #[test]
    fn property_values() {
        let q = parse("(property Some_Key #book)").expect("parse");
        assert_eq!(
            q.filter,
            Query::Property {
                key: "some-key".into(),
                value: Some(PropValue::Str("book".into()))
            }
        );
        let q = parse("(property n 3)").expect("parse");
        assert!(matches!(
            q.filter,
            Query::Property {
                value: Some(PropValue::Int(3)),
                ..
            }
        ));
        let q = parse("(page-property done true)").expect("parse");
        assert!(matches!(
            q.filter,
            Query::PageProperty {
                value: Some(PropValue::Bool(true)),
                ..
            }
        ));
    }

    #[test]
    fn post_processing_and_text() {
        let q = parse("(and \"hello\" (sort-by rating asc) (sample 3))").expect("parse");
        assert_eq!(q.filter, Query::And(vec![Query::Text("hello".into())]));
        assert_eq!(
            q.sort_by,
            Some(SortBy {
                key: "rating".into(),
                desc: false
            })
        );
        assert_eq!(q.sample, Some(3));
        assert_eq!(
            parse("\"foo\"").expect("parse").filter,
            Query::Text("foo".into())
        );
    }

    #[test]
    fn result_type_rule() {
        let rule = |s: &str| parse(s).expect("parse").filter.returns_blocks();
        assert!(rule("[[x]]"));
        assert!(rule("(and (page-tags a) (task TODO))"));
        assert!(!rule("(and (page-property type book) (page-tags fiction))"));
        assert!(!rule("(namespace foo)"));
        assert!(!rule("(all-page-tags)"));
    }

    #[test]
    fn errors() {
        assert!(matches!(
            parse("(frobnicate x)"),
            Err(QueryError::Unsupported(_))
        ));
        assert!(matches!(parse("(and"), Err(QueryError::Syntax(_))));
        assert!(matches!(parse(""), Err(QueryError::Syntax(_))));
        assert!(matches!(parse("(sample 3)"), Err(QueryError::Syntax(_))));
        assert!(matches!(
            parse("(between foo -1d today)"),
            Err(QueryError::Unsupported(_))
        ));
    }
}
