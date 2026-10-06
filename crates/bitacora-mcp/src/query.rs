//! Minimal Logseq simple-query subset for the `query` tool, evaluated over [`GraphReader`].
//!
//! The full DSL to SQL compiler is BIT-US-0101; until it lands only conjunctions of
//! `(task M...)`, `(priority P...)`, `[[page]]` and `"text"` are evaluated here. Anything else
//! yields `NOT_SUPPORTED`; datalog yields `INVALID_QUERY`.

use std::collections::HashSet;

use crate::reader::{BlockInfo, GraphReader, SearchKind, SearchQuery, TaskQuery};
use crate::render::{Code, ToolError};

/// Maximum blocks returned.
pub(crate) const MAX_RESULTS: usize = 200;

/// A parsed query term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Expr {
    /// `(and a b ...)`; also the implicit top level.
    And(Vec<Expr>),
    /// `(task TODO DOING)`.
    Task(Vec<String>),
    /// `(priority a b)`.
    Priority(Vec<String>),
    /// `[[Page]]` (blocks referencing the page).
    Page(String),
    /// `"text"` (full-text).
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Open,
    Close,
    Page(String),
    Str(String),
    Atom(String),
}

fn unsupported(what: &str) -> ToolError {
    ToolError::new(
        Code::NotSupported,
        format!(
            "query term `{what}` is not supported yet; supported: and, (task ...), (priority ...), [[page]], \"text\""
        ),
    )
}

fn lex(src: &str) -> Result<Vec<Tok>, ToolError> {
    let mut out = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            _ if c.is_whitespace() => i += 1,
            '(' => {
                out.push(Tok::Open);
                i += 1;
            }
            ')' => {
                out.push(Tok::Close);
                i += 1;
            }
            '"' => {
                let mut s = String::new();
                i += 1;
                loop {
                    match chars.get(i) {
                        None => {
                            return Err(ToolError::new(Code::InvalidQuery, "unterminated string"));
                        }
                        Some('"') => {
                            i += 1;
                            break;
                        }
                        Some('\\') if i + 1 < chars.len() => {
                            s.push(chars[i + 1]);
                            i += 2;
                        }
                        Some(&ch) => {
                            s.push(ch);
                            i += 1;
                        }
                    }
                }
                out.push(Tok::Str(s));
            }
            '[' if chars.get(i + 1) == Some(&'[') => {
                let rest: String = chars[i + 2..].iter().collect();
                let end = rest
                    .find("]]")
                    .ok_or_else(|| ToolError::new(Code::InvalidQuery, "unterminated [[page]]"))?;
                out.push(Tok::Page(rest[..end].trim().to_owned()));
                i += 2 + rest[..end].chars().count() + 2;
            }
            '[' | ']' | '{' | '}' => return Err(unsupported(&c.to_string())),
            _ => {
                let mut s = String::new();
                while let Some(&ch) = chars.get(i) {
                    if ch.is_whitespace() || matches!(ch, '(' | ')' | '"') {
                        break;
                    }
                    s.push(ch);
                    i += 1;
                }
                out.push(Tok::Atom(s));
            }
        }
    }
    Ok(out)
}

/// Parse a query string.
pub(crate) fn parse(src: &str) -> Result<Expr, ToolError> {
    let trimmed = src.trim();
    if trimmed.is_empty() {
        return Err(ToolError::new(Code::InvalidQuery, "empty query"));
    }
    if trimmed.starts_with("[:find") || trimmed.contains(":find ") || trimmed.contains(":where") {
        return Err(ToolError::new(
            Code::InvalidQuery,
            "datalog queries are not supported; use the Logseq simple-query DSL",
        ));
    }
    let toks = lex(trimmed)?;
    let mut pos = 0;
    let mut terms = Vec::new();
    while pos < toks.len() {
        terms.push(parse_term(&toks, &mut pos)?);
    }
    Ok(match terms.len() {
        1 => terms.remove(0),
        _ => Expr::And(terms),
    })
}

fn parse_term(toks: &[Tok], pos: &mut usize) -> Result<Expr, ToolError> {
    let t = toks
        .get(*pos)
        .ok_or_else(|| ToolError::new(Code::InvalidQuery, "unexpected end of query"))?
        .clone();
    *pos += 1;
    match t {
        Tok::Page(p) => Ok(Expr::Page(p)),
        Tok::Str(s) => Ok(Expr::Text(s)),
        Tok::Close => Err(ToolError::new(Code::InvalidQuery, "unbalanced `)`")),
        Tok::Atom(a) => Err(unsupported(&a)),
        Tok::Open => {
            let Some(Tok::Atom(head)) = toks.get(*pos).cloned() else {
                return Err(ToolError::new(Code::InvalidQuery, "expected an operator"));
            };
            *pos += 1;
            let head_lc = head.to_ascii_lowercase();
            match head_lc.as_str() {
                "and" => {
                    let mut items = Vec::new();
                    loop {
                        match toks.get(*pos) {
                            Some(Tok::Close) => {
                                *pos += 1;
                                break;
                            }
                            None => {
                                return Err(ToolError::new(Code::InvalidQuery, "missing `)`"));
                            }
                            Some(_) => items.push(parse_term(toks, pos)?),
                        }
                    }
                    Ok(Expr::And(items))
                }
                "task" | "priority" => {
                    let mut args = Vec::new();
                    loop {
                        match toks.get(*pos) {
                            Some(Tok::Close) => {
                                *pos += 1;
                                break;
                            }
                            Some(Tok::Atom(a)) => {
                                args.push(a.to_ascii_uppercase());
                                *pos += 1;
                            }
                            _ => {
                                return Err(ToolError::new(
                                    Code::InvalidQuery,
                                    format!("bad arguments to `{head_lc}`"),
                                ));
                            }
                        }
                    }
                    if args.is_empty() {
                        return Err(ToolError::new(
                            Code::InvalidQuery,
                            format!("`{head_lc}` needs at least one argument"),
                        ));
                    }
                    Ok(if head_lc == "task" {
                        Expr::Task(args)
                    } else {
                        Expr::Priority(args)
                    })
                }
                _ => Err(unsupported(&head)),
            }
        }
    }
}

fn leaf(r: &dyn GraphReader, e: &Expr) -> Result<Vec<BlockInfo>, ToolError> {
    match e {
        Expr::And(_) => Ok(Vec::new()),
        Expr::Task(markers) => {
            let mut m = Vec::new();
            for s in markers {
                m.push(s.clone());
                if s == "CANCELED" {
                    m.push("CANCELLED".to_owned());
                }
                if s == "WAITING" {
                    m.push("WAIT".to_owned());
                }
            }
            Ok(r.tasks(&TaskQuery {
                markers: m,
                ..TaskQuery::default()
            })?)
        }
        Expr::Priority(ps) => {
            let mut out = Vec::new();
            for p in ps {
                out.extend(r.tasks(&TaskQuery {
                    priority: Some(p.clone()),
                    ..TaskQuery::default()
                })?);
            }
            Ok(out)
        }
        Expr::Page(name) => {
            let page = r
                .page(name)?
                .ok_or_else(|| ToolError::not_found(format!("page `{name}`")))?;
            Ok(r.linked_references(&page)?
                .into_iter()
                .flat_map(|g| g.blocks.into_iter().map(|i| i.block))
                .collect())
        }
        Expr::Text(text) => {
            let hits = r.search(&SearchQuery {
                query: text.clone(),
                limit: 100,
                kind: SearchKind::Block,
                page: None,
            })?;
            let mut out = Vec::new();
            for h in hits {
                if let Some(b) = h.uuid.as_deref().map(|u| r.block(u)).transpose()?.flatten() {
                    out.push(b);
                }
            }
            Ok(out)
        }
    }
}

fn flatten<'a>(e: &'a Expr, out: &mut Vec<&'a Expr>) {
    match e {
        Expr::And(items) => items.iter().for_each(|i| flatten(i, out)),
        other => out.push(other),
    }
}

/// Evaluate: the intersection of every leaf's blocks, in the first leaf's order.
pub(crate) fn run(r: &dyn GraphReader, e: &Expr) -> Result<Vec<BlockInfo>, ToolError> {
    let mut leaves = Vec::new();
    flatten(e, &mut leaves);
    let mut result: Option<Vec<BlockInfo>> = None;
    for l in leaves {
        let blocks = leaf(r, l)?;
        result = Some(match result {
            None => blocks,
            Some(prev) => {
                let keep: HashSet<&str> = blocks.iter().map(|b| b.uuid.as_str()).collect();
                prev.into_iter()
                    .filter(|b| keep.contains(b.uuid.as_str()))
                    .collect()
            }
        });
    }
    Ok(result.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_forms() {
        assert_eq!(
            parse("(and (task todo doing) [[Project X]])").ok(),
            Some(Expr::And(vec![
                Expr::Task(vec!["TODO".into(), "DOING".into()]),
                Expr::Page("Project X".into())
            ]))
        );
        assert_eq!(
            parse("\"hello world\"").ok(),
            Some(Expr::Text("hello world".into()))
        );
        assert_eq!(
            parse("(task TODO) [[a]]").ok(),
            Some(Expr::And(vec![
                Expr::Task(vec!["TODO".into()]),
                Expr::Page("a".into())
            ]))
        );
    }

    #[test]
    fn rejects_datalog_and_unsupported() {
        assert_eq!(
            parse("[:find ?b :where [?b :block/marker]]")
                .err()
                .map(|e| e.code),
            Some(Code::InvalidQuery)
        );
        assert_eq!(
            parse("(or (task TODO))").err().map(|e| e.code),
            Some(Code::NotSupported)
        );
        assert_eq!(
            parse("(and (between -7d today))").err().map(|e| e.code),
            Some(Code::NotSupported)
        );
        assert_eq!(
            parse("(and (task TODO)").err().map(|e| e.code),
            Some(Code::InvalidQuery)
        );
        assert_eq!(parse("  ").err().map(|e| e.code), Some(Code::InvalidQuery));
    }
}
