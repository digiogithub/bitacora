//! EDN value model, reader (CST to value) and `pr-str` style printer.

use crate::cst::{Cst, Kind, Node};
use crate::error::{Diagnostic, DiagnosticKind};

/// An EDN value. Maps keep source order; keywords are stored without the leading `:`.
#[derive(Debug, Clone, PartialEq)]
pub enum Edn {
    /// `nil`
    Nil,
    /// `true` / `false`
    Bool(bool),
    /// Integer that fits `i64`.
    Int(i64),
    /// Floating point number.
    Float(f64),
    /// Other numeric literal kept verbatim (`10N`, `1.5M`, integer overflow).
    Number(String),
    /// String.
    Str(String),
    /// Character.
    Char(char),
    /// Keyword, without `:` (`file/name-format`).
    Keyword(String),
    /// Symbol.
    Symbol(String),
    /// `( ... )`, e.g. `(fn [x] ...)`.
    List(Vec<Edn>),
    /// `[ ... ]`
    Vector(Vec<Edn>),
    /// `#{ ... }` in source order.
    Set(Vec<Edn>),
    /// `{ ... }` in source order.
    Map(Vec<(Edn, Edn)>),
    /// `#tag value`
    Tagged(String, Box<Edn>),
}

impl Edn {
    /// Builds a keyword value from its name (no colon).
    pub fn kw(name: &str) -> Edn {
        Edn::Keyword(name.to_owned())
    }

    /// Builds a string value.
    pub fn str(s: &str) -> Edn {
        Edn::Str(s.to_owned())
    }

    /// Builds a vector of strings.
    pub fn strs<I, S>(items: I) -> Edn
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Edn::Vector(items.into_iter().map(|s| Edn::str(s.as_ref())).collect())
    }

    /// Looks a keyword key up in a map value.
    pub fn get(&self, key: &str) -> Option<&Edn> {
        match self {
            Edn::Map(entries) => entries
                .iter()
                .find(|(k, _)| matches!(k, Edn::Keyword(n) if n == key))
                .map(|(_, v)| v),
            _ => None,
        }
    }

    /// String content, if this is a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Edn::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Bool content.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Edn::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Keyword name, if this is a keyword.
    pub fn as_keyword(&self) -> Option<&str> {
        match self {
            Edn::Keyword(s) => Some(s),
            _ => None,
        }
    }

    /// Integer content.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Edn::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Elements of a vector, list or set.
    pub fn as_items(&self) -> Option<&[Edn]> {
        match self {
            Edn::Vector(v) | Edn::List(v) | Edn::Set(v) => Some(v),
            _ => None,
        }
    }

    /// Map entries.
    pub fn as_map(&self) -> Option<&[(Edn, Edn)]> {
        match self {
            Edn::Map(m) => Some(m),
            _ => None,
        }
    }

    /// Prints the value like Clojure's `pr-str` (single spaces, no commas).
    pub fn pr_str(&self) -> String {
        let mut out = String::new();
        self.print_into(&mut out);
        out
    }

    fn print_into(&self, out: &mut String) {
        fn seq(out: &mut String, open: &str, close: char, items: &[Edn]) {
            out.push_str(open);
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                it.print_into(out);
            }
            out.push(close);
        }
        match self {
            Edn::Nil => out.push_str("nil"),
            Edn::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Edn::Int(i) => out.push_str(&i.to_string()),
            Edn::Float(f) => out.push_str(&format!("{f:?}")),
            Edn::Number(n) | Edn::Symbol(n) => out.push_str(n),
            Edn::Str(s) => print_string(s, out),
            Edn::Char(c) => print_char(*c, out),
            Edn::Keyword(k) => {
                out.push(':');
                out.push_str(k);
            }
            Edn::List(v) => seq(out, "(", ')', v),
            Edn::Vector(v) => seq(out, "[", ']', v),
            Edn::Set(v) => seq(out, "#{", '}', v),
            Edn::Map(m) => {
                out.push('{');
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    k.print_into(out);
                    out.push(' ');
                    v.print_into(out);
                }
                out.push('}');
            }
            Edn::Tagged(t, v) => {
                out.push('#');
                out.push_str(t);
                out.push(' ');
                v.print_into(out);
            }
        }
    }
}

fn print_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn print_char(c: char, out: &mut String) {
    match c {
        '\n' => out.push_str("\\newline"),
        ' ' => out.push_str("\\space"),
        '\t' => out.push_str("\\tab"),
        '\r' => out.push_str("\\return"),
        c => {
            out.push('\\');
            out.push(c);
        }
    }
}

/// Reads the first top-level form of `src` (like `clojure.edn/read-string`).
///
/// Returns `Ok(None)` for an empty document. Duplicate map keys and duplicate set elements are
/// errors, exactly as in Logseq's config validation.
pub fn read_str(src: &str) -> Result<Option<Edn>, Diagnostic> {
    let cst = Cst::parse(src)?;
    read_cst(&cst)
}

/// Converts the first form of an already parsed tree to a value.
pub fn read_cst(cst: &Cst) -> Result<Option<Edn>, Diagnostic> {
    cst.first_form()
        .map(|n| node_to_edn(cst.source(), n))
        .transpose()
}

/// Converts one CST form to a value.
pub fn node_to_edn(src: &str, n: &Node) -> Result<Edn, Diagnostic> {
    let text = n.text(src);
    Ok(match n.kind {
        Kind::Nil => Edn::Nil,
        Kind::Bool => Edn::Bool(text == "true"),
        Kind::Number => parse_number(text),
        Kind::Str => Edn::Str(unescape(src, n)?),
        Kind::Char => Edn::Char(parse_char(src, n)?),
        Kind::Keyword => Edn::Keyword(text[1..].to_owned()),
        Kind::Symbol => Edn::Symbol(text.to_owned()),
        Kind::List => Edn::List(forms(src, n)?),
        Kind::Vector => Edn::Vector(forms(src, n)?),
        Kind::Set => {
            let items = forms(src, n)?;
            for (i, it) in items.iter().enumerate() {
                if items[..i].contains(it) {
                    return Err(Diagnostic::at(
                        src,
                        n.span.start,
                        DiagnosticKind::DuplicateKey,
                        format!("duplicate set element {}", it.pr_str()),
                    ));
                }
            }
            Edn::Set(items)
        }
        Kind::Map => {
            let nodes: Vec<&Node> = n.forms().collect();
            if !nodes.len().is_multiple_of(2) {
                return Err(Diagnostic::at(
                    src,
                    n.span.start,
                    DiagnosticKind::OddMapEntries,
                    "map literal must contain an even number of forms",
                ));
            }
            let mut entries: Vec<(Edn, Edn)> = Vec::with_capacity(nodes.len() / 2);
            for pair in nodes.chunks(2) {
                let k = node_to_edn(src, pair[0])?;
                if entries.iter().any(|(ek, _)| *ek == k) {
                    return Err(Diagnostic::at(
                        src,
                        pair[0].span.start,
                        DiagnosticKind::DuplicateKey,
                        format!("duplicate key {}", k.pr_str()),
                    ));
                }
                entries.push((k, node_to_edn(src, pair[1])?));
            }
            Edn::Map(entries)
        }
        Kind::Tagged => {
            let tag = n.children[0].text(src).to_owned();
            let value = n
                .forms()
                .nth(1)
                .map(|v| node_to_edn(src, v))
                .transpose()?
                .unwrap_or(Edn::Nil);
            Edn::Tagged(tag, Box::new(value))
        }
        Kind::Whitespace | Kind::Comment | Kind::Discard => Edn::Nil,
    })
}

fn forms(src: &str, n: &Node) -> Result<Vec<Edn>, Diagnostic> {
    n.forms().map(|c| node_to_edn(src, c)).collect()
}

fn parse_number(t: &str) -> Edn {
    let plain = !t.ends_with(['N', 'M']);
    if plain {
        let body = t.strip_prefix('+').unwrap_or(t);
        if !body.contains(['.', 'e', 'E']) {
            return body
                .parse::<i64>()
                .map_or_else(|_| Edn::Number(t.to_owned()), Edn::Int);
        }
        if let Ok(f) = body.parse::<f64>() {
            return Edn::Float(f);
        }
    }
    Edn::Number(t.to_owned())
}

fn parse_char(src: &str, n: &Node) -> Result<char, Diagnostic> {
    let body = &n.text(src)[1..];
    let mut it = body.chars();
    let first = it.next();
    let bad = || {
        Diagnostic::at(
            src,
            n.span.start,
            DiagnosticKind::Syntax,
            format!("invalid character literal `\\{body}`"),
        )
    };
    match (first, it.next()) {
        (Some(c), None) => Ok(c),
        _ => match body {
            "newline" => Ok('\n'),
            "space" => Ok(' '),
            "tab" => Ok('\t'),
            "return" => Ok('\r'),
            b if b.starts_with('u') && b.len() == 5 => u32::from_str_radix(&b[1..], 16)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(bad),
            _ => Err(bad()),
        },
    }
}

fn unescape(src: &str, n: &Node) -> Result<String, Diagnostic> {
    let raw = n.text(src);
    let inner = &raw[1..raw.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut it = inner.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let bad = |what: String| Diagnostic::at(src, n.span.start, DiagnosticKind::Syntax, what);
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('u') => {
                let hex: String = it.by_ref().take(4).collect();
                let ch = u32::from_str_radix(&hex, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or_else(|| bad(format!("invalid unicode escape `\\u{hex}`")))?;
                out.push(ch);
            }
            other => {
                return Err(bad(format!(
                    "unsupported escape `\\{}` in string",
                    other.unwrap_or(' ')
                )));
            }
        }
    }
    Ok(out)
}
