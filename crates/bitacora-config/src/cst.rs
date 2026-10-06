//! Lossless EDN concrete syntax tree.
//!
//! The tree keeps every byte of the input (whitespace, commas, comments, `#_` discards), so
//! printing it reproduces the source exactly. It is the foundation of the comment-preserving
//! editor in [`crate::edit`] and of the value reader in [`crate::edn`].

use std::ops::Range;

use crate::error::{Diagnostic, DiagnosticKind};

/// Kind of a CST node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Spaces, tabs, newlines and commas.
    Whitespace,
    /// `;` comment up to (not including) the line break.
    Comment,
    /// `#_` followed by trivia and the discarded form.
    Discard,
    /// `nil`.
    Nil,
    /// `true` / `false`.
    Bool,
    /// Integer or float literal (including `N` / `M` suffixes).
    Number,
    /// String literal, quotes included.
    Str,
    /// Character literal such as `\a` or `\newline`.
    Char,
    /// Keyword, leading `:` included.
    Keyword,
    /// Symbol.
    Symbol,
    /// `( ... )`
    List,
    /// `[ ... ]`
    Vector,
    /// `#{ ... }`
    Set,
    /// `{ ... }`
    Map,
    /// `#tag form`: children are the tag symbol, trivia and the tagged form.
    Tagged,
}

impl Kind {
    /// Whether the kind is trivia (not a form).
    pub fn is_trivia(self) -> bool {
        matches!(self, Kind::Whitespace | Kind::Comment | Kind::Discard)
    }

    /// Whether the node is a delimited collection.
    pub fn is_collection(self) -> bool {
        matches!(self, Kind::List | Kind::Vector | Kind::Set | Kind::Map)
    }

    fn open_len(self) -> usize {
        match self {
            Kind::Set => 2,
            _ => 1,
        }
    }

    fn close_char(self) -> Option<char> {
        match self {
            Kind::List => Some(')'),
            Kind::Vector => Some(']'),
            Kind::Set | Kind::Map => Some('}'),
            _ => None,
        }
    }
}

/// A CST node: a byte span and, for composite kinds, the children (trivia included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// What the node is.
    pub kind: Kind,
    /// Byte range in the source.
    pub span: Range<usize>,
    /// Children in source order; empty for atoms.
    pub children: Vec<Node>,
}

impl Node {
    /// Children that are forms (not trivia).
    pub fn forms(&self) -> impl Iterator<Item = &Node> {
        self.children.iter().filter(|c| !c.kind.is_trivia())
    }

    /// Source text of this node.
    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        &src[self.span.clone()]
    }

    /// Byte offset just after the opening delimiter of a collection.
    pub fn inner_start(&self) -> usize {
        self.span.start + self.kind.open_len()
    }

    /// Byte offset of the closing delimiter of a collection.
    pub fn close_pos(&self) -> usize {
        self.span.end.saturating_sub(1)
    }
}

/// A parsed EDN document: the source plus the tree of top-level nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cst {
    src: String,
    root: Vec<Node>,
}

impl Cst {
    /// Parses `src` into a lossless tree.
    pub fn parse(src: &str) -> Result<Cst, Diagnostic> {
        let mut p = Parser { src, pos: 0 };
        let root = p.parse_seq(None)?;
        Ok(Cst {
            src: src.to_owned(),
            root,
        })
    }

    /// The exact source text.
    pub fn source(&self) -> &str {
        &self.src
    }

    /// Top-level nodes (trivia and forms).
    pub fn root(&self) -> &[Node] {
        &self.root
    }

    /// First top-level form, if any (what `clojure.edn/read-string` reads).
    pub fn first_form(&self) -> Option<&Node> {
        self.root.iter().find(|n| !n.kind.is_trivia())
    }

    /// Prints the tree back to text by walking it; equals [`Cst::source`] for every input.
    pub fn print(&self) -> String {
        let mut out = String::with_capacity(self.src.len());
        for n in &self.root {
            print_node(&self.src, n, &mut out);
        }
        out
    }
}

fn print_node(src: &str, n: &Node, out: &mut String) {
    match n.kind {
        Kind::Discard => out.push_str("#_"),
        Kind::Tagged => out.push('#'),
        k if k.is_collection() => out.push_str(&src[n.span.start..n.inner_start()]),
        _ => {
            out.push_str(n.text(src));
            return;
        }
    }
    for c in &n.children {
        print_node(src, c, out);
    }
    if let Some(c) = n.kind.close_char() {
        out.push(c);
    }
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

fn is_ws(c: char) -> bool {
    c == ',' || c.is_whitespace()
}

fn is_token_end(c: char) -> bool {
    is_ws(c) || matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';')
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn bump(&mut self) {
        self.pos += self.peek().map_or(0, char::len_utf8);
    }

    fn err(&self, offset: usize, kind: DiagnosticKind, msg: impl Into<String>) -> Diagnostic {
        Diagnostic::at(self.src, offset, kind, msg)
    }

    /// Parses nodes until the matching `close` (or EOF when `None`).
    fn parse_seq(&mut self, close: Option<char>) -> Result<Vec<Node>, Diagnostic> {
        let mut out = Vec::new();
        loop {
            let Some(c) = self.peek() else {
                return match close {
                    None => Ok(out),
                    Some(_) => Err(self.err(
                        self.pos,
                        DiagnosticKind::Syntax,
                        "unexpected end of input, unclosed collection",
                    )),
                };
            };
            if Some(c) == close {
                return Ok(out);
            }
            if matches!(c, ')' | ']' | '}') {
                return Err(self.err(
                    self.pos,
                    DiagnosticKind::Syntax,
                    format!("unmatched delimiter `{c}`"),
                ));
            }
            out.push(self.parse_node()?);
        }
    }

    fn parse_node(&mut self) -> Result<Node, Diagnostic> {
        let start = self.pos;
        let Some(c) = self.peek() else {
            return Err(self.err(start, DiagnosticKind::Syntax, "unexpected end of input"));
        };
        if is_ws(c) {
            while self.peek().is_some_and(is_ws) {
                self.bump();
            }
            return Ok(leaf(Kind::Whitespace, start, self.pos));
        }
        match c {
            ';' => {
                while self.peek().is_some_and(|c| c != '\n' && c != '\r') {
                    self.bump();
                }
                Ok(leaf(Kind::Comment, start, self.pos))
            }
            '"' => self.parse_string(),
            '(' => self.parse_coll(Kind::List, 1, ')'),
            '[' => self.parse_coll(Kind::Vector, 1, ']'),
            '{' => self.parse_coll(Kind::Map, 1, '}'),
            '\\' => {
                self.pos += 1;
                // A char literal is at least one char, then a token run (`\newline`, `A`).
                if self.peek().is_none() {
                    return Err(self.err(start, DiagnosticKind::Syntax, "dangling `\\`"));
                }
                self.bump();
                self.take_token_rest();
                Ok(leaf(Kind::Char, start, self.pos))
            }
            '#' => self.parse_dispatch(),
            _ => {
                self.take_token_rest();
                let text = &self.src[start..self.pos];
                Ok(leaf(classify_token(text), start, self.pos))
            }
        }
    }

    fn take_token_rest(&mut self) {
        while self.peek().is_some_and(|c| !is_token_end(c)) {
            self.bump();
        }
    }

    fn parse_string(&mut self) -> Result<Node, Diagnostic> {
        let start = self.pos;
        self.pos += 1;
        loop {
            match self.peek() {
                None => {
                    return Err(self.err(start, DiagnosticKind::Syntax, "unterminated string"));
                }
                Some('"') => {
                    self.pos += 1;
                    return Ok(leaf(Kind::Str, start, self.pos));
                }
                Some('\\') => {
                    self.pos += 1;
                    self.bump();
                }
                Some(_) => self.bump(),
            }
        }
    }

    fn parse_coll(&mut self, kind: Kind, open: usize, close: char) -> Result<Node, Diagnostic> {
        let start = self.pos;
        self.pos += open;
        let children = self.parse_seq(Some(close))?;
        self.pos += 1; // closing delimiter
        Ok(Node {
            kind,
            span: start..self.pos,
            children,
        })
    }

    /// Reads trivia and then one form (after `#_` or a tag); `what` names the owner in errors.
    fn parse_prefixed(
        &mut self,
        start: usize,
        mut children: Vec<Node>,
        what: &str,
    ) -> Result<Vec<Node>, Diagnostic> {
        loop {
            if self.peek().is_none_or(|c| matches!(c, ')' | ']' | '}')) {
                return Err(self.err(start, DiagnosticKind::Syntax, what.to_owned()));
            }
            let n = self.parse_node()?;
            let done = !n.kind.is_trivia();
            children.push(n);
            if done {
                return Ok(children);
            }
        }
    }

    fn parse_dispatch(&mut self) -> Result<Node, Diagnostic> {
        let start = self.pos;
        match self.src[start + 1..].chars().next() {
            Some('{') => self.parse_coll(Kind::Set, 2, '}'),
            Some('_') => {
                self.pos += 2;
                let children =
                    self.parse_prefixed(start, Vec::new(), "`#_` without a form to discard")?;
                Ok(Node {
                    kind: Kind::Discard,
                    span: start..self.pos,
                    children,
                })
            }
            Some(c) if c.is_alphabetic() => {
                self.pos += 1;
                let tag_start = self.pos;
                self.take_token_rest();
                let tag = leaf(Kind::Symbol, tag_start, self.pos);
                let children =
                    self.parse_prefixed(start, vec![tag], "tagged literal without a value")?;
                Ok(Node {
                    kind: Kind::Tagged,
                    span: start..self.pos,
                    children,
                })
            }
            _ => Err(self.err(
                start,
                DiagnosticKind::Syntax,
                "unsupported dispatch form after `#`",
            )),
        }
    }
}

fn leaf(kind: Kind, start: usize, end: usize) -> Node {
    Node {
        kind,
        span: start..end,
        children: Vec::new(),
    }
}

fn classify_token(t: &str) -> Kind {
    match t {
        "nil" => Kind::Nil,
        "true" | "false" => Kind::Bool,
        _ if t.starts_with(':') => Kind::Keyword,
        _ if is_number(t) => Kind::Number,
        _ => Kind::Symbol,
    }
}

/// EDN number grammar: optional sign, digits, optional fraction/exponent, optional `N`/`M`.
pub(crate) fn is_number(t: &str) -> bool {
    let b = t.as_bytes();
    let mut i = 0;
    if matches!(b.first(), Some(b'+' | b'-')) {
        i += 1;
    }
    let digits = |i: &mut usize| {
        let s = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        *i > s
    };
    if !digits(&mut i) {
        return false;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        digits(&mut i);
    }
    if i < b.len() && matches!(b[i], b'e' | b'E') {
        i += 1;
        if i < b.len() && matches!(b[i], b'+' | b'-') {
            i += 1;
        }
        if !digits(&mut i) {
            return false;
        }
    }
    if i < b.len() && matches!(b[i], b'N' | b'M') {
        i += 1;
    }
    i == b.len()
}
