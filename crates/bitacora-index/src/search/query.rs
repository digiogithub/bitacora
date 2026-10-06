//! Search query parser (BIT-T-0068): user text to safe FTS5 `MATCH` expressions.
//!
//! `and` / `&`, `or` / `|` and `not` become FTS5 operators. Every other token is folded with
//! the index normalizer and emitted as a double-quoted FTS5 string (inner quotes doubled), so
//! punctuation, FTS5 syntax characters (`* ^ : - ( ) "`) and bare keywords such as `NEAR` can
//! never produce a syntax error or inject operators. Double-quoted input is a phrase.

use crate::normalize::fold;

/// A boolean operator between two terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Both sides must match (also the implicit operator).
    And,
    /// Either side matches.
    Or,
    /// Left matches, right must not.
    Not,
}

/// One search term: a folded word or phrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    /// The folded text (a phrase keeps its inner spaces).
    pub text: String,
    /// Written as a quoted phrase.
    pub phrase: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    Term(Term),
    Op(Op),
}

/// A parsed query. Operators are normalized: no leading, trailing or doubled operators, and
/// terms are never adjacent without an operator (the implicit one is [`Op::And`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedQuery {
    items: Vec<Item>,
}

impl ParsedQuery {
    /// Parses `input`, folding terms with `remove_accents`.
    #[must_use]
    pub fn parse(input: &str, remove_accents: bool) -> Self {
        let mut raw: Vec<Item> = Vec::new();
        let mut chars = input.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
            } else if c == '"' {
                chars.next();
                let mut phrase = String::new();
                for ch in chars.by_ref() {
                    if ch == '"' {
                        break;
                    }
                    phrase.push(ch);
                }
                push_term(&mut raw, &phrase, true, remove_accents);
            } else {
                let mut word = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_whitespace() || ch == '"' {
                        break;
                    }
                    word.push(ch);
                    chars.next();
                }
                match word.to_lowercase().as_str() {
                    "and" | "&" => raw.push(Item::Op(Op::And)),
                    "or" | "|" => raw.push(Item::Op(Op::Or)),
                    "not" => raw.push(Item::Op(Op::Not)),
                    _ => push_term(&mut raw, &word, false, remove_accents),
                }
            }
        }
        Self {
            items: normalize_items(raw),
        }
    }

    /// No searchable term (empty input, only operators or punctuation).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.positive_terms().is_empty()
    }

    /// Terms that are not on the right of a `not`: the ones a hit must (or may) contain.
    #[must_use]
    pub fn positive_terms(&self) -> Vec<&Term> {
        let mut out = Vec::new();
        let mut negated = false;
        for it in &self.items {
            match it {
                Item::Op(Op::Not) => negated = true,
                Item::Op(_) => negated = false,
                Item::Term(t) => {
                    if !negated {
                        out.push(t);
                    }
                    negated = false;
                }
            }
        }
        out
    }

    /// The positive terms joined by a space: the text for title and fuzzy matching.
    #[must_use]
    pub fn plain(&self) -> String {
        self.positive_terms()
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// FTS5 expression for the word index (`blocks_fts`, unicode61). The last plain word gets
    /// a prefix `*` so results appear while typing. `None` when there is nothing to match.
    #[must_use]
    pub fn fts_words(&self) -> Option<String> {
        self.render(|t, last| {
            let q = quote(&t.text);
            if last && !t.phrase {
                format!("{q}*")
            } else {
                q
            }
        })
    }

    /// FTS5 expression for the trigram index (`blocks_fts_tri`, `pages_fts`). Trigram needs
    /// every term to have at least three characters; `None` when a positive term is shorter
    /// (use [`ParsedQuery::like_patterns`] then) or when nothing is searchable.
    #[must_use]
    pub fn fts_trigram(&self) -> Option<String> {
        if self.is_empty()
            || self
                .positive_terms()
                .iter()
                .any(|t| t.text.chars().count() < 3)
        {
            return None;
        }
        // Negated terms shorter than three characters cannot be expressed: drop them
        // together with the operator that led to them.
        let mut kept: Vec<Item> = Vec::new();
        for it in &self.items {
            match it {
                Item::Term(t) if t.text.chars().count() < 3 => {
                    if matches!(kept.last(), Some(Item::Op(_))) {
                        kept.pop();
                    }
                }
                other => kept.push(other.clone()),
            }
        }
        Self {
            items: normalize_items(kept),
        }
        .render(|t, _| quote(&t.text))
    }

    /// `(positive, negative)` lowercase `LIKE` patterns (`%term%`, `%`, `_` and `\` escaped,
    /// use `ESCAPE '\'`) for the substring fallback. Only AND semantics are honoured.
    #[must_use]
    pub fn like_patterns(&self) -> (Vec<String>, Vec<String>) {
        let mut pos = Vec::new();
        let mut neg = Vec::new();
        let mut negated = false;
        for it in &self.items {
            match it {
                Item::Op(Op::Not) => negated = true,
                Item::Op(_) => negated = false,
                Item::Term(t) => {
                    let p = format!("%{}%", escape_like(&t.text));
                    if negated {
                        neg.push(p);
                    } else {
                        pos.push(p);
                    }
                    negated = false;
                }
            }
        }
        (pos, neg)
    }

    fn render(&self, mut term: impl FnMut(&Term, bool) -> String) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let last_term = self
            .items
            .iter()
            .rposition(|i| matches!(i, Item::Term(_)))
            .unwrap_or(0);
        let mut out = String::new();
        for (idx, it) in self.items.iter().enumerate() {
            if !out.is_empty() {
                out.push(' ');
            }
            match it {
                Item::Op(Op::And) => out.push_str("AND"),
                Item::Op(Op::Or) => out.push_str("OR"),
                Item::Op(Op::Not) => out.push_str("NOT"),
                Item::Term(t) => out.push_str(&term(t, idx == last_term)),
            }
        }
        Some(out)
    }
}

fn push_term(raw: &mut Vec<Item>, text: &str, phrase: bool, remove_accents: bool) {
    let folded = fold(text, remove_accents);
    let folded = folded.trim();
    // A token without a single letter or digit (`---`, `*`) cannot match anything useful.
    if !folded.chars().any(char::is_alphanumeric) {
        return;
    }
    raw.push(Item::Term(Term {
        text: folded.to_owned(),
        phrase,
    }));
}

/// Drops leading/trailing operators, collapses runs (the later operator wins, `and not` is
/// `not`) and inserts the implicit `and` between adjacent terms.
fn normalize_items(raw: Vec<Item>) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::new();
    for it in raw {
        match it {
            Item::Op(op) => match out.last() {
                None => {}
                Some(Item::Op(_)) => {
                    out.pop();
                    out.push(Item::Op(op));
                }
                Some(Item::Term(_)) => out.push(Item::Op(op)),
            },
            Item::Term(t) => {
                if matches!(out.last(), Some(Item::Term(_))) {
                    out.push(Item::Op(Op::And));
                }
                out.push(Item::Term(t));
            }
        }
    }
    while matches!(out.last(), Some(Item::Op(_))) {
        out.pop();
    }
    out
}

/// Double-quotes `s` for FTS5 (`"` doubled).
#[must_use]
pub fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(q: &str) -> Option<String> {
        ParsedQuery::parse(q, true).fts_words()
    }

    #[test]
    fn plain_words_are_quoted_and_the_last_gets_a_prefix() {
        assert_eq!(words("foo bar").as_deref(), Some("\"foo\" AND \"bar\"*"));
    }

    #[test]
    fn operators_map_to_fts5() {
        assert_eq!(
            words("a and b or c not d").as_deref(),
            Some("\"a\" AND \"b\" OR \"c\" NOT \"d\"*")
        );
        assert_eq!(
            words("a & b | c").as_deref(),
            Some("\"a\" AND \"b\" OR \"c\"*")
        );
    }

    #[test]
    fn operator_edge_cases_are_dropped() {
        assert_eq!(words("and foo or").as_deref(), Some("\"foo\"*"));
        assert_eq!(words("a and not b").as_deref(), Some("\"a\" NOT \"b\"*"));
        assert_eq!(words("a or or b").as_deref(), Some("\"a\" OR \"b\"*"));
        assert_eq!(words("and or not"), None);
    }

    #[test]
    fn syntax_characters_stay_inside_quotes() {
        let m = words("NEAR(a b) col:x ^y -z \"un\"closed*").unwrap_or_default();
        assert_eq!(m.matches('"').count() % 2, 0, "{m}");
        assert!(m.starts_with("\"near(a\" AND \"b)\""), "{m}");
    }

    #[test]
    fn embedded_quotes_are_doubled() {
        assert_eq!(quote("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn phrases_and_unterminated_phrases() {
        assert_eq!(
            words("\"foo bar\" baz").as_deref(),
            Some("\"foo bar\" AND \"baz\"*")
        );
        assert_eq!(words("\"foo bar").as_deref(), Some("\"foo bar\""));
    }

    #[test]
    fn punctuation_only_input_is_empty() {
        assert!(ParsedQuery::parse("*** --- ()", true).is_empty());
        assert!(ParsedQuery::parse("   ", true).is_empty());
    }

    #[test]
    fn terms_are_folded_like_the_index() {
        assert_eq!(words("Café").as_deref(), Some("\"cafe\"*"));
        assert_eq!(ParsedQuery::parse("Café", false).plain(), "café");
    }

    #[test]
    fn trigram_requires_three_chars() {
        assert_eq!(
            ParsedQuery::parse("abc def", true).fts_trigram().as_deref(),
            Some("\"abc\" AND \"def\"")
        );
        assert_eq!(ParsedQuery::parse("ab cde", true).fts_trigram(), None);
        // A short negated term is dropped instead of disabling trigram.
        assert_eq!(
            ParsedQuery::parse("abc not de", true)
                .fts_trigram()
                .as_deref(),
            Some("\"abc\"")
        );
    }

    #[test]
    fn like_patterns_escape_wildcards() {
        let (p, n) = ParsedQuery::parse("50%_x not b", true).like_patterns();
        assert_eq!(p, vec!["%50\\%\\_x%".to_owned()]);
        assert_eq!(n, vec!["%b%".to_owned()]);
    }

    #[test]
    fn negated_terms_are_not_positive() {
        let q = ParsedQuery::parse("a not b c", true);
        assert_eq!(q.plain(), "a c");
    }
}
