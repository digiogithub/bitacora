//! A small EDN reader: the subset the query DSL and advanced query maps use.

use std::fmt;

/// An EDN value.
#[derive(Debug, Clone, PartialEq)]
pub enum Edn {
    /// `nil`
    Nil,
    /// `true` / `false`
    Bool(bool),
    /// Integer.
    Int(i64),
    /// Floating point number.
    Float(f64),
    /// String.
    Str(String),
    /// Symbol (`?x`, `and`, `clojure.string/includes?`).
    Sym(String),
    /// Keyword without the leading colon (`block/marker`).
    Kw(String),
    /// `( ... )`
    List(Vec<Edn>),
    /// `[ ... ]`
    Vector(Vec<Edn>),
    /// `{ k v ... }` in source order.
    Map(Vec<(Edn, Edn)>),
    /// `#{ ... }`
    Set(Vec<Edn>),
    /// `#"..."`
    Regex(String),
}

/// A reader error with the byte offset of the problem.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message} (at byte {offset})")]
pub struct EdnError {
    /// What went wrong.
    pub message: String,
    /// Byte offset in the source.
    pub offset: usize,
}

impl fmt::Display for Edn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn seq(f: &mut fmt::Formatter<'_>, open: &str, items: &[Edn], close: &str) -> fmt::Result {
            f.write_str(open)?;
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    f.write_str(" ")?;
                }
                write!(f, "{it}")?;
            }
            f.write_str(close)
        }
        match self {
            Edn::Nil => f.write_str("nil"),
            Edn::Bool(b) => write!(f, "{b}"),
            Edn::Int(n) => write!(f, "{n}"),
            Edn::Float(x) => write!(f, "{x}"),
            Edn::Str(s) => write!(f, "{s:?}"),
            Edn::Sym(s) => f.write_str(s),
            Edn::Kw(s) => write!(f, ":{s}"),
            Edn::List(v) => seq(f, "(", v, ")"),
            Edn::Vector(v) => seq(f, "[", v, "]"),
            Edn::Set(v) => seq(f, "#{", v, "}"),
            Edn::Regex(s) => write!(f, "#\"{s}\""),
            Edn::Map(m) => {
                f.write_str("{")?;
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ")?;
                    }
                    write!(f, "{k} {v}")?;
                }
                f.write_str("}")
            }
        }
    }
}

impl Edn {
    /// The name of a symbol, lower-cased.
    #[must_use]
    pub fn sym_lc(&self) -> Option<String> {
        match self {
            Edn::Sym(s) => Some(s.to_lowercase()),
            _ => None,
        }
    }

    /// Map lookup by keyword name.
    #[must_use]
    pub fn get_kw(&self, name: &str) -> Option<&Edn> {
        match self {
            Edn::Map(m) => m
                .iter()
                .find(|(k, _)| matches!(k, Edn::Kw(n) if n == name))
                .map(|(_, v)| v),
            _ => None,
        }
    }
}

/// Reads every top-level form of `src`.
pub fn read_all(src: &str) -> Result<Vec<Edn>, EdnError> {
    let mut r = Reader { s: src, pos: 0 };
    let mut out = Vec::new();
    loop {
        r.skip_ws();
        if r.pos >= src.len() {
            return Ok(out);
        }
        out.push(r.read()?);
    }
}

struct Reader<'a> {
    s: &'a str,
    pos: usize,
}

fn is_delim(c: char) -> bool {
    c.is_whitespace() || matches!(c, ',' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';')
}

impl Reader<'_> {
    fn err<T>(&self, m: &str) -> Result<T, EdnError> {
        Err(EdnError {
            message: m.to_owned(),
            offset: self.pos,
        })
    }

    fn peek(&self) -> Option<char> {
        self.s[self.pos..].chars().next()
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() || c == ',' {
                self.pos += c.len_utf8();
            } else if c == ';' {
                while let Some(c) = self.peek() {
                    if c == '\n' {
                        break;
                    }
                    self.pos += c.len_utf8();
                }
            } else {
                break;
            }
        }
    }

    fn read(&mut self) -> Result<Edn, EdnError> {
        self.skip_ws();
        let Some(c) = self.peek() else {
            return self.err("unexpected end of input");
        };
        match c {
            '(' => self.coll(')').map(Edn::List),
            '[' => self.coll(']').map(Edn::Vector),
            '{' => {
                let items = self.coll('}')?;
                if items.len() % 2 != 0 {
                    return self.err("map literal needs an even number of forms");
                }
                let mut it = items.into_iter();
                let mut m = Vec::new();
                while let (Some(k), Some(v)) = (it.next(), it.next()) {
                    m.push((k, v));
                }
                Ok(Edn::Map(m))
            }
            ')' | ']' | '}' => self.err("unbalanced closing delimiter"),
            '"' => self.string().map(Edn::Str),
            '#' => {
                self.pos += 1;
                match self.peek() {
                    Some('{') => {
                        self.pos += 1;
                        self.coll_after('}').map(Edn::Set)
                    }
                    Some('"') => self.string().map(Edn::Regex),
                    Some('_') => {
                        self.pos += 1;
                        self.read()?;
                        self.read()
                    }
                    _ => self.err("unsupported dispatch macro"),
                }
            }
            '\\' => {
                self.pos += 1;
                let tok = self.token();
                Ok(Edn::Str(tok))
            }
            ':' => {
                self.pos += 1;
                let t = self.token();
                if t.is_empty() {
                    return self.err("empty keyword");
                }
                Ok(Edn::Kw(t))
            }
            _ => {
                let t = self.token();
                if t.is_empty() {
                    return self.err("unexpected character");
                }
                Ok(atom(&t))
            }
        }
    }

    fn token(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if is_delim(c) {
                break;
            }
            self.pos += c.len_utf8();
        }
        self.s[start..self.pos].to_owned()
    }

    fn coll(&mut self, close: char) -> Result<Vec<Edn>, EdnError> {
        self.pos += 1;
        self.coll_after(close)
    }

    fn coll_after(&mut self, close: char) -> Result<Vec<Edn>, EdnError> {
        let mut out = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None => return self.err("unterminated collection"),
                Some(c) if c == close => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(_) => out.push(self.read()?),
            }
        }
    }

    fn string(&mut self) -> Result<String, EdnError> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            let Some(c) = self.peek() else {
                return self.err("unterminated string");
            };
            self.pos += c.len_utf8();
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let Some(e) = self.peek() else {
                        return self.err("unterminated string");
                    };
                    self.pos += e.len_utf8();
                    match e {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        '"' | '\\' => out.push(e),
                        // Regex escapes (`\d`) are kept verbatim.
                        other => {
                            out.push('\\');
                            out.push(other);
                        }
                    }
                }
                _ => out.push(c),
            }
        }
    }
}

fn atom(t: &str) -> Edn {
    match t {
        "nil" => return Edn::Nil,
        "true" => return Edn::Bool(true),
        "false" => return Edn::Bool(false),
        _ => {}
    }
    let digits = t.strip_prefix(['-', '+']).unwrap_or(t);
    if digits.starts_with(|c: char| c.is_ascii_digit()) {
        if let Ok(n) = t.trim_start_matches('+').parse::<i64>() {
            return Edn::Int(n);
        }
        if let Ok(x) = t.parse::<f64>() {
            return Edn::Float(x);
        }
    }
    Edn::Sym(t.to_owned())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn reads_basic_forms() {
        let v = read_all(r#"(and [1 -2 +3] {:a "x\"y"} #{b} nil true 1.5 ?x :block/name) ; c"#)
            .expect("read");
        assert_eq!(v.len(), 1);
        let Edn::List(items) = &v[0] else { panic!() };
        assert_eq!(items[0], Edn::Sym("and".into()));
        assert_eq!(
            items[1],
            Edn::Vector(vec![Edn::Int(1), Edn::Int(-2), Edn::Int(3)])
        );
        assert_eq!(
            items[2],
            Edn::Map(vec![(Edn::Kw("a".into()), Edn::Str("x\"y".into()))])
        );
        assert_eq!(items[8], Edn::Kw("block/name".into()));
    }

    #[test]
    fn errors_and_discard() {
        assert!(read_all("(a").is_err());
        assert!(read_all("{:a}").is_err());
        assert_eq!(
            read_all("#_ x y").expect("read"),
            vec![Edn::Sym("y".into())]
        );
        assert_eq!(
            read_all("#\"a\\d\"").expect("r"),
            vec![Edn::Regex("a\\d".into())]
        );
    }
}
