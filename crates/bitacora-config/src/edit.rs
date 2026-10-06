//! Comment-preserving `config.edn` editor (rewrite-edn style).
//!
//! Every edit is a splice of the original text: bytes outside the touched value, entry or element
//! are never rewritten, so comments, commas, discards, indentation and line endings survive. The
//! editor works on text only and returns bytes; persisting them is the core writer's job
//! (atomic write, hash check).

use std::ops::Range;

use crate::cst::{Cst, Kind, Node};
use crate::edn::Edn;
use crate::error::Error;

/// A parsed `config.edn` that can be edited surgically.
#[derive(Debug, Clone)]
pub struct ConfigEditor {
    cst: Cst,
}

/// Result of resolving a key path against the tree.
enum Located {
    Found {
        key: Node,
        value: Node,
    },
    /// `map` is the deepest existing map; `idx` is the first missing path segment.
    Missing {
        map: Node,
        idx: usize,
    },
}

fn edit_err(msg: impl Into<String>) -> Error {
    Error::Edit(msg.into())
}

impl ConfigEditor {
    /// Parses `src`; the first form must be a map, or the document empty.
    pub fn parse(src: &str) -> Result<Self, Error> {
        let cst = Cst::parse(src)?;
        if let Some(n) = cst.first_form()
            && n.kind != Kind::Map
        {
            return Err(edit_err("config root is not a map"));
        }
        Ok(ConfigEditor { cst })
    }

    /// The current text.
    pub fn text(&self) -> &str {
        self.cst.source()
    }

    /// Consumes the editor, returning the edited text.
    pub fn into_text(self) -> String {
        self.cst.source().to_owned()
    }

    /// Sets the value at `path` (keyword names without `:`), creating missing keys and nested
    /// maps. An existing value is replaced in place; a new key is inserted before the closing `}`.
    pub fn assoc(&mut self, path: &[&str], value: &Edn) -> Result<(), Error> {
        if path.is_empty() {
            return Err(edit_err("empty path"));
        }
        self.ensure_root()?;
        match self.locate(path)? {
            Located::Found { value: old, .. } => self.splice(old.span, &value.pr_str()),
            Located::Missing { map, idx } => {
                let mut nested = value.clone();
                for seg in path[idx + 1..].iter().rev() {
                    nested = Edn::Map(vec![(Edn::kw(seg), nested)]);
                }
                let entry = format!(":{} {}", path[idx], nested.pr_str());
                self.insert_entry(&map, &entry)
            }
        }
    }

    /// Replaces the value at `path` with `f(current)`; `f` gets `None` when the key is missing.
    pub fn update_in<F>(&mut self, path: &[&str], f: F) -> Result<(), Error>
    where
        F: FnOnce(Option<&Edn>) -> Edn,
    {
        self.ensure_root()?;
        let current = match self.locate(path)? {
            Located::Found { value, .. } => {
                Some(crate::edn::node_to_edn(self.cst.source(), &value)?)
            }
            Located::Missing { .. } => None,
        };
        let new = f(current.as_ref());
        self.assoc(path, &new)
    }

    /// Removes the key at `path` with its value; returns whether it existed.
    pub fn dissoc(&mut self, path: &[&str]) -> Result<bool, Error> {
        if !self.has_root() {
            return Ok(false);
        }
        match self.locate(path)? {
            Located::Missing { .. } => Ok(false),
            Located::Found { key, value } => {
                self.remove_form(key.span.start, value.span.end)?;
                Ok(true)
            }
        }
    }

    /// Appends `value` to the vector at `path`, creating `[value]` when the key is missing.
    /// Existing elements and comments in the vector are untouched.
    pub fn vec_push(&mut self, path: &[&str], value: &Edn) -> Result<(), Error> {
        self.ensure_root()?;
        match self.locate(path)? {
            Located::Missing { .. } => self.assoc(path, &Edn::Vector(vec![value.clone()])),
            Located::Found { value: vec, .. } => {
                if vec.kind != Kind::Vector {
                    return Err(edit_err("value is not a vector"));
                }
                let text = value.pr_str();
                match vec.forms().last() {
                    None => self.splice(vec.inner_start()..vec.inner_start(), &text),
                    Some(last) => {
                        let (start, end) = (last.span.start, last.span.end);
                        self.insert_after(start, end, &text)
                    }
                }
            }
        }
    }

    /// Removes the string element `item` (case-insensitive) from the vector at `path`.
    pub fn vec_remove_str(&mut self, path: &[&str], item: &str) -> Result<bool, Error> {
        match self.find_str_element(path, item)? {
            Some(n) => {
                self.remove_form(n.span.start, n.span.end)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Adds a page to `:favorites` unless already present (case-insensitive).
    pub fn favorites_add(&mut self, page: &str) -> Result<(), Error> {
        if self.find_str_element(&["favorites"], page)?.is_some() {
            return Ok(());
        }
        self.vec_push(&["favorites"], &Edn::str(page))
    }

    /// Removes a page from `:favorites`.
    pub fn favorites_remove(&mut self, page: &str) -> Result<bool, Error> {
        self.vec_remove_str(&["favorites"], page)
    }

    /// Renames a page in `:favorites` in place.
    pub fn favorites_rename(&mut self, old: &str, new: &str) -> Result<bool, Error> {
        match self.find_str_element(&["favorites"], old)? {
            Some(n) => {
                self.splice(n.span, &Edn::str(new).pr_str())?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Renames `[:default-home :page]` when it names `old` (case-insensitive).
    pub fn default_home_rename(&mut self, old: &str, new: &str) -> Result<bool, Error> {
        let path = ["default-home", "page"];
        if !self.has_root() {
            return Ok(false);
        }
        match self.locate(&path)? {
            Located::Found { value, .. } => {
                let cur = crate::edn::node_to_edn(self.cst.source(), &value)?;
                if cur.as_str().is_some_and(|s| s.eq_ignore_ascii_case(old)) {
                    self.splice(value.span, &Edn::str(new).pr_str())?;
                    return Ok(true);
                }
                Ok(false)
            }
            Located::Missing { .. } => Ok(false),
        }
    }

    // ---- internals -----------------------------------------------------------------------

    fn find_str_element(&self, path: &[&str], item: &str) -> Result<Option<Node>, Error> {
        if !self.has_root() {
            return Ok(None);
        }
        let Located::Found { value, .. } = self.locate(path)? else {
            return Ok(None);
        };
        if value.kind != Kind::Vector {
            return Err(edit_err("value is not a vector"));
        }
        for n in value.forms() {
            if n.kind == Kind::Str
                && let Edn::Str(s) = crate::edn::node_to_edn(self.cst.source(), n)?
                && s.eq_ignore_ascii_case(item)
            {
                return Ok(Some(n.clone()));
            }
        }
        Ok(None)
    }

    fn locate(&self, path: &[&str]) -> Result<Located, Error> {
        let mut map = self
            .cst
            .first_form()
            .cloned()
            .ok_or_else(|| edit_err("document has no map yet"));
        for (i, seg) in path.iter().enumerate() {
            let cur = map?;
            let want = format!(":{seg}");
            let pairs: Vec<&Node> = cur.forms().collect();
            let found = pairs
                .chunks(2)
                .find(|p| p[0].kind == Kind::Keyword && p[0].text(self.cst.source()) == want);
            match found {
                None => return Ok(Located::Missing { map: cur, idx: i }),
                Some(p) if p.len() < 2 => return Err(edit_err("map has a key without a value")),
                Some(p) => {
                    if i + 1 == path.len() {
                        return Ok(Located::Found {
                            key: p[0].clone(),
                            value: p[1].clone(),
                        });
                    }
                    if p[1].kind != Kind::Map {
                        return Err(edit_err(format!("`:{seg}` is not a map")));
                    }
                    map = Ok(p[1].clone());
                }
            }
        }
        Err(edit_err("empty path"))
    }

    /// Makes sure the document holds a map, appending `{}` to an empty one.
    fn ensure_root(&mut self) -> Result<(), Error> {
        if self.cst.first_form().is_some() {
            return Ok(());
        }
        let src = self.cst.source();
        let sep = if src.is_empty() || src.ends_with('\n') {
            ""
        } else {
            self.eol()
        };
        let at = src.len();
        self.splice(at..at, &format!("{sep}{{}}"))
    }

    fn has_root(&self) -> bool {
        self.cst.first_form().is_some()
    }

    fn eol(&self) -> &'static str {
        if self.cst.source().contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        }
    }

    /// Replaces `range` with `text` and re-parses; the result must still be valid EDN.
    fn splice(&mut self, range: Range<usize>, text: &str) -> Result<(), Error> {
        let src = self.cst.source();
        let mut out = String::with_capacity(src.len() + text.len());
        out.push_str(&src[..range.start]);
        out.push_str(text);
        out.push_str(&src[range.end..]);
        self.cst = Cst::parse(&out)?;
        Ok(())
    }

    /// Inserts a map entry (`:k v`) into `map`, after its last entry.
    fn insert_entry(&mut self, map: &Node, entry: &str) -> Result<(), Error> {
        let forms: Vec<&Node> = map.forms().collect();
        if !forms.len().is_multiple_of(2) {
            return Err(edit_err("map has a key without a value"));
        }
        match forms.chunks(2).last() {
            None => self.splice(map.inner_start()..map.inner_start(), entry),
            Some(p) => {
                let (anchor, end) = (p[0].span.start, p[1].span.end);
                self.insert_after(anchor, end, entry)
            }
        }
    }

    /// Inserts `text` after the form(s) spanning `anchor..end`: on its own line with the same
    /// indentation when `anchor` starts a line, else after a single space.
    fn insert_after(&mut self, anchor: usize, end: usize, text: &str) -> Result<(), Error> {
        let src = self.cst.source();
        let line_start = src[..anchor].rfind('\n').map_or(0, |i| i + 1);
        let prefix = &src[line_start..anchor];
        if prefix.chars().all(|c| c == ' ' || c == '\t') && (line_start > 0 || !prefix.is_empty()) {
            let mut pos = skip_blanks(src, end);
            if src[pos..].starts_with(';') {
                pos += src[pos..].find(['\n', '\r']).unwrap_or(src.len() - pos);
            }
            let ins = format!("{}{}{}", self.eol(), prefix, text);
            self.splice(pos..pos, &ins)
        } else {
            self.splice(end..end, &format!(" {text}"))
        }
    }

    /// Removes the form(s) spanning `start..end` (a map entry or a vector element) together with
    /// the whitespace that would otherwise be left dangling. Comments are kept.
    fn remove_form(&mut self, start: usize, end: usize) -> Result<(), Error> {
        let src = self.cst.source();
        let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
        let own_line = src[line_start..start]
            .chars()
            .all(|c| c == ' ' || c == '\t')
            && (line_start > 0 || start > 0);
        let after = skip_blanks(src, end);
        let at_eol = after >= src.len() || src[after..].starts_with(['\n', '\r']);
        let range = if own_line && at_eol {
            // Whole line goes, including its line break.
            let mut e = after;
            if src[e..].starts_with("\r\n") {
                e += 2;
            } else if src[e..].starts_with(['\n', '\r']) {
                e += 1;
            }
            line_start..e
        } else if own_line && src[after..].starts_with(';') {
            start..after
        } else if at_eol || src[after..].starts_with(['}', ']', ')']) {
            // Last on its line: drop the blanks before it instead of after it.
            let pre = src[..start].trim_end_matches([' ', '\t', ',']).len();
            pre..end
        } else {
            start..after
        };
        self.splice(range, "")
    }
}

/// Offset after any spaces, tabs and commas starting at `pos`.
fn skip_blanks(src: &str, pos: usize) -> usize {
    pos + src[pos..]
        .find(|c| !matches!(c, ' ' | '\t' | ','))
        .unwrap_or(src.len() - pos)
}
