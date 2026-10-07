//! The markdown subset of assistant answers (BIT-T-0453): headings, paragraphs, bullet lists,
//! block quotes and fenced code. Inline constructs (`**bold**`, `` `code` ``, `[[Page]]`,
//! `((block))`, URLs) are left to the inline renderer, so they look and navigate exactly like in
//! the outline. Parsing is pure and cheap enough to rerun on every streamed delta; an unclosed
//! code fence is shown as code to the end of the text, so a half-streamed answer never flickers
//! into prose.

/// One block of an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdBlock {
    /// `#`..`###` heading.
    Heading { level: u8, text: String },
    /// Consecutive plain lines.
    Paragraph(String),
    /// One list item (`-`, `*`, `+` or `1.`), with its nesting depth.
    Bullet { depth: usize, text: String },
    /// Consecutive `>` lines.
    Quote(String),
    /// A fenced code block.
    Code { lang: String, text: String },
}

fn flush(out: &mut Vec<MdBlock>, para: &mut Vec<String>, quote: &mut Vec<String>) {
    if !para.is_empty() {
        out.push(MdBlock::Paragraph(para.join("\n")));
        para.clear();
    }
    if !quote.is_empty() {
        out.push(MdBlock::Quote(quote.join("\n")));
        quote.clear();
    }
}

/// Splits `text` into blocks.
#[must_use]
pub fn parse_blocks(text: &str) -> Vec<MdBlock> {
    let mut out: Vec<MdBlock> = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let mut quote: Vec<String> = Vec::new();
    let mut code: Option<(String, Vec<&str>)> = None;

    for line in text.lines() {
        if let Some((lang, lines)) = code.as_mut() {
            if line.trim_start().starts_with("```") {
                out.push(MdBlock::Code {
                    lang: std::mem::take(lang),
                    text: lines.join("\n"),
                });
                code = None;
            } else {
                lines.push(line);
            }
            continue;
        }
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("```") {
            flush(&mut out, &mut para, &mut quote);
            code = Some((rest.trim().to_owned(), Vec::new()));
        } else if trimmed.is_empty() {
            flush(&mut out, &mut para, &mut quote);
        } else if let Some((level, rest)) = heading(trimmed) {
            flush(&mut out, &mut para, &mut quote);
            out.push(MdBlock::Heading {
                level,
                text: rest.to_owned(),
            });
        } else if let Some(rest) = bullet(trimmed) {
            flush(&mut out, &mut para, &mut quote);
            let indent = line.len() - trimmed.len();
            out.push(MdBlock::Bullet {
                depth: indent / 2,
                text: rest.to_owned(),
            });
        } else if let Some(rest) = trimmed.strip_prefix('>') {
            if !para.is_empty() {
                flush(&mut out, &mut para, &mut quote);
            }
            quote.push(rest.strip_prefix(' ').unwrap_or(rest).to_owned());
        } else {
            if !quote.is_empty() {
                flush(&mut out, &mut para, &mut quote);
            }
            para.push(trimmed.to_owned());
        }
    }
    if let Some((lang, lines)) = code {
        out.push(MdBlock::Code {
            lang,
            text: lines.join("\n"),
        });
    }
    flush(&mut out, &mut para, &mut quote);
    out
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    if (1..=3).contains(&hashes) {
        let rest = line[hashes..].strip_prefix(' ')?;
        return Some((u8::try_from(hashes).ok()?, rest.trim()));
    }
    None
}

fn bullet(line: &str) -> Option<&str> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return Some(rest);
        }
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && digits <= 3 {
        return line[digits..].strip_prefix(". ");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_the_supported_blocks() {
        let blocks = parse_blocks(
            "# Title\nsome **bold** text\nnext line\n\n- one\n  - nested\n1. first\n> quoted\n\n```rust\nlet a = 1;\n```\nend",
        );
        assert_eq!(
            blocks,
            vec![
                MdBlock::Heading {
                    level: 1,
                    text: "Title".into()
                },
                MdBlock::Paragraph("some **bold** text\nnext line".into()),
                MdBlock::Bullet {
                    depth: 0,
                    text: "one".into()
                },
                MdBlock::Bullet {
                    depth: 1,
                    text: "nested".into()
                },
                MdBlock::Bullet {
                    depth: 0,
                    text: "first".into()
                },
                MdBlock::Quote("quoted".into()),
                MdBlock::Code {
                    lang: "rust".into(),
                    text: "let a = 1;".into()
                },
                MdBlock::Paragraph("end".into()),
            ]
        );
    }

    #[test]
    fn an_unclosed_fence_streams_as_code() {
        assert_eq!(
            parse_blocks("intro\n```\nhalf"),
            vec![
                MdBlock::Paragraph("intro".into()),
                MdBlock::Code {
                    lang: String::new(),
                    text: "half".into()
                }
            ]
        );
    }

    #[test]
    fn hash_without_space_and_deep_headings_are_plain() {
        assert_eq!(
            parse_blocks("#tag and #### four"),
            vec![MdBlock::Paragraph("#tag and #### four".into())]
        );
        assert!(parse_blocks("").is_empty());
    }
}
