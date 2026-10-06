//! Small lexical syntax highlighter for code fences (keywords, strings, comments, numbers).
//!
//! Deliberately dependency-free: it colours the common languages of Logseq graphs well
//! enough for reading; it is not a parser.

use std::ops::Range;

/// Class of a highlighted token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenClass {
    /// Language keyword.
    Keyword,
    /// String literal.
    String,
    /// Comment.
    Comment,
    /// Numeric literal.
    Number,
}

struct Lang {
    keywords: &'static [&'static str],
    line_comment: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    quotes: &'static [char],
}

const C_LIKE: &[&str] = &[
    "fn",
    "let",
    "mut",
    "const",
    "static",
    "pub",
    "use",
    "mod",
    "struct",
    "enum",
    "impl",
    "trait",
    "match",
    "if",
    "else",
    "for",
    "while",
    "loop",
    "return",
    "break",
    "continue",
    "as",
    "in",
    "async",
    "await",
    "move",
    "where",
    "type",
    "self",
    "Self",
    "true",
    "false",
    "function",
    "var",
    "class",
    "new",
    "this",
    "import",
    "export",
    "from",
    "try",
    "catch",
    "throw",
    "interface",
    "extends",
    "implements",
    "public",
    "private",
    "protected",
    "void",
    "int",
    "null",
    "None",
    "package",
    "func",
    "go",
    "defer",
    "switch",
    "case",
    "default",
    "do",
    "final",
];
const PY_LIKE: &[&str] = &[
    "def", "class", "import", "from", "as", "if", "elif", "else", "for", "while", "return", "in",
    "is", "not", "and", "or", "None", "True", "False", "try", "except", "finally", "with",
    "lambda", "yield", "pass", "break", "continue", "raise", "async", "await", "echo", "fi",
    "then", "done", "esac", "function", "export", "local",
];
const LISP_LIKE: &[&str] = &[
    "def", "defn", "defn-", "let", "fn", "if", "when", "cond", "do", "loop", "recur", "ns",
    "require", "defmacro", "nil", "true", "false", "and", "or", "not",
];
const SQL_LIKE: &[&str] = &[
    "SELECT", "FROM", "WHERE", "INSERT", "INTO", "VALUES", "UPDATE", "SET", "DELETE", "CREATE",
    "TABLE", "JOIN", "ON", "AND", "OR", "NOT", "NULL", "ORDER", "BY", "GROUP", "LIMIT", "AS",
    "select", "from", "where", "insert", "into", "values", "update", "set", "delete", "create",
    "table", "join", "on", "and", "or", "not", "null", "order", "by", "group", "limit", "as",
];
const DATA_LIKE: &[&str] = &["true", "false", "null"];

fn lang(name: &str) -> Lang {
    let n = name.trim().to_ascii_lowercase();
    match n.as_str() {
        "python" | "py" | "sh" | "bash" | "shell" | "zsh" | "yaml" | "yml" | "toml" | "ruby"
        | "rb" | "r" | "conf" => Lang {
            keywords: PY_LIKE,
            line_comment: &["#"],
            block_comment: None,
            quotes: &['"', '\''],
        },
        "clojure" | "clj" | "cljs" | "edn" | "lisp" | "scheme" | "elisp" => Lang {
            keywords: LISP_LIKE,
            line_comment: &[";"],
            block_comment: None,
            quotes: &['"'],
        },
        "sql" => Lang {
            keywords: SQL_LIKE,
            line_comment: &["--"],
            block_comment: Some(("/*", "*/")),
            quotes: &['\''],
        },
        "json" | "jsonc" => Lang {
            keywords: DATA_LIKE,
            line_comment: &[],
            block_comment: None,
            quotes: &['"'],
        },
        "" | "text" | "txt" | "plain" => Lang {
            keywords: &[],
            line_comment: &[],
            block_comment: None,
            quotes: &[],
        },
        _ => Lang {
            keywords: C_LIKE,
            line_comment: &["//"],
            block_comment: Some(("/*", "*/")),
            quotes: &['"', '\'', '`'],
        },
    }
}

/// Highlights `code` for the language named `language` (empty or unknown: C-like rules for
/// unknown names, none for empty). Ranges are byte ranges, ordered and disjoint.
pub fn highlight(code: &str, language: &str) -> Vec<(Range<usize>, TokenClass)> {
    let lang = lang(language);
    let mut out = Vec::new();
    let mut i = 0;
    while i < code.len() {
        let rest = &code[i..];
        if let Some((open, close)) = lang.block_comment
            && rest.starts_with(open)
        {
            let end = rest[open.len()..]
                .find(close)
                .map_or(code.len(), |p| i + open.len() + p + close.len());
            out.push((i..end, TokenClass::Comment));
            i = end;
            continue;
        }
        if lang.line_comment.iter().any(|c| rest.starts_with(c)) {
            let end = rest.find('\n').map_or(code.len(), |p| i + p);
            out.push((i..end, TokenClass::Comment));
            i = end;
            continue;
        }
        let ch = rest.chars().next().unwrap_or(' ');
        if lang.quotes.contains(&ch) {
            let mut end = code.len();
            let mut escaped = false;
            for (off, c) in rest.char_indices().skip(1) {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == ch {
                    end = i + off + c.len_utf8();
                    break;
                } else if c == '\n' && ch != '`' {
                    end = i + off;
                    break;
                }
            }
            out.push((i..end, TokenClass::String));
            i = end;
            continue;
        }
        if ch.is_ascii_digit() {
            let len = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_'))
                .unwrap_or(rest.len());
            out.push((i..i + len, TokenClass::Number));
            i += len;
            continue;
        }
        if ch.is_alphabetic() || ch == '_' {
            let len = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
                .unwrap_or(rest.len())
                .max(ch.len_utf8());
            if lang.keywords.contains(&&rest[..len]) {
                out.push((i..i + len, TokenClass::Keyword));
            }
            i += len;
            continue;
        }
        i += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(code: &str, lang: &str) -> Vec<(String, TokenClass)> {
        highlight(code, lang)
            .into_iter()
            .map(|(r, c)| (code[r].to_owned(), c))
            .collect()
    }

    #[test]
    fn rust_tokens() {
        let got = classes("fn main() { let x = 42; // hi\n\"s\" }", "rust");
        assert!(got.contains(&("fn".into(), TokenClass::Keyword)));
        assert!(got.contains(&("let".into(), TokenClass::Keyword)));
        assert!(got.contains(&("42".into(), TokenClass::Number)));
        assert!(got.contains(&("// hi".into(), TokenClass::Comment)));
        assert!(got.contains(&("\"s\"".into(), TokenClass::String)));
    }

    #[test]
    fn hash_comments_and_plain() {
        let got = classes("x = 'a' # note", "python");
        assert!(got.contains(&("# note".into(), TokenClass::Comment)));
        assert!(highlight("fn let", "").is_empty());
    }

    #[test]
    fn unterminated_constructs_do_not_panic() {
        for code in ["\"open", "/* open", "'", "é\"é", "0x", "`multi\nline"] {
            for l in ["rust", "sql", "clj", "json", ""] {
                let tokens = highlight(code, l);
                for (r, _) in tokens {
                    assert!(code.is_char_boundary(r.start) && code.is_char_boundary(r.end));
                }
            }
        }
    }
}
