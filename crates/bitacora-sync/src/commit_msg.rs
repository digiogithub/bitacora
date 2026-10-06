//! Structured commit messages and their parser (design `git-sync-merge` 2.4, BIT-SP-0006.R2).
//!
//! ```text
//! bitacora: edit 3 pages (journals/2026_10_06, Project X, Ideas)
//!
//! Bitacora-Device: laptop-jose
//! Bitacora-Kind: auto
//! Bitacora-Pages: journals/2026_10_06.md; pages/Project X.md; pages/Ideas.md
//! ```

use crate::backend::{CommitKind, CommitMessage};

/// Maximum subject length in characters.
pub const MAX_SUBJECT_CHARS: usize = 72;
/// Maximum number of paths listed in the `Bitacora-Pages` trailer.
pub const MAX_TRAILER_PAGES: usize = 200;

const DEVICE: &str = "Bitacora-Device";
const KIND: &str = "Bitacora-Kind";
const PAGES: &str = "Bitacora-Pages";
const AGENT: &str = "Bitacora-Agent";

/// Inputs of [`build_message`].
#[derive(Debug, Clone)]
pub struct MessageSpec<'a> {
    /// Commit kind.
    pub kind: CommitKind,
    /// Device name (`Bitacora-Device`).
    pub device: &'a str,
    /// Graph-relative paths touched by the commit, in a stable order.
    pub pages: &'a [String],
    /// MCP client name for [`CommitKind::Agent`] (`Bitacora-Agent`).
    pub agent: Option<&'a str>,
    /// Subject written by the user (manual commits) or the caller (merge, resolve, migrate).
    pub subject: Option<&'a str>,
}

/// Display title of a page path: `pages/` prefix and `.md` suffix dropped, `___` shown as `/`.
pub fn page_title(path: &str) -> String {
    let p = path.strip_prefix("pages/").unwrap_or(path);
    let p = p.strip_suffix(".md").unwrap_or(p);
    p.replace("___", "/")
}

fn one_line(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// `bitacora: edit N pages (a, b, c)`, shortened to fit [`MAX_SUBJECT_CHARS`].
fn edit_subject(pages: &[String]) -> String {
    let all_md = pages.iter().all(|p| p.ends_with(".md"));
    let noun = if all_md { "page" } else { "file" };
    let n = pages.len();
    let plural = if n == 1 { "" } else { "s" };
    let head = format!("bitacora: edit {n} {noun}{plural}");
    if n == 0 {
        return head;
    }
    let titles: Vec<String> = pages.iter().map(|p| one_line(&page_title(p))).collect();
    // Try the longest list of titles that fits; a trailing "+k more" keeps the count honest.
    for shown in (1..=titles.len()).rev() {
        let mut list = titles[..shown].join(", ");
        if shown < titles.len() {
            list.push_str(&format!(", +{} more", titles.len() - shown));
        }
        let candidate = format!("{head} ({list})");
        if candidate.chars().count() <= MAX_SUBJECT_CHARS {
            return candidate;
        }
    }
    // Even one title is too long: truncate the first one.
    let room = MAX_SUBJECT_CHARS.saturating_sub(head.chars().count() + 3);
    if room >= 4 {
        let suffix = if n > 1 {
            format!(", +{} more", n - 1)
        } else {
            String::new()
        };
        let room = room.saturating_sub(suffix.chars().count());
        if room >= 4 {
            return format!("{head} ({}{suffix})", truncate(&titles[0], room));
        }
    }
    head
}

/// Builds the message for `spec`.
pub fn build_message(spec: &MessageSpec<'_>) -> CommitMessage {
    let subject = match (spec.kind, spec.subject) {
        (CommitKind::Auto | CommitKind::Agent, None) => edit_subject(spec.pages),
        (CommitKind::Merge, None) => "bitacora: merge remote changes".to_string(),
        (CommitKind::Resolve, None) => "bitacora: resolve merge conflicts".to_string(),
        (CommitKind::Migrate, None) => "bitacora: connect graph to sync".to_string(),
        (CommitKind::Manual, None) => edit_subject(spec.pages),
        (_, Some(s)) => {
            let s = one_line(s);
            if s.is_empty() {
                edit_subject(spec.pages)
            } else {
                s
            }
        }
    };
    let mut msg = CommitMessage::new(subject)
        .trailer(DEVICE, one_line(spec.device))
        .kind(spec.kind);
    if !spec.pages.is_empty() {
        let mut listed: Vec<String> = spec
            .pages
            .iter()
            .take(MAX_TRAILER_PAGES)
            .map(|p| one_line(p))
            .collect();
        if spec.pages.len() > MAX_TRAILER_PAGES {
            listed.push(format!("(+{} more)", spec.pages.len() - MAX_TRAILER_PAGES));
        }
        msg = msg.trailer(PAGES, listed.join("; "));
    }
    if spec.kind == CommitKind::Agent
        && let Some(agent) = spec.agent
    {
        msg = msg.trailer(AGENT, one_line(agent));
    }
    msg
}

/// A parsed commit message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedMessage {
    /// First line.
    pub subject: String,
    /// `Bitacora-Kind`, when present and known.
    pub kind: Option<CommitKind>,
    /// `Bitacora-Device`.
    pub device: Option<String>,
    /// `Bitacora-Pages` entries (the "+N more" marker excluded).
    pub pages: Vec<String>,
    /// `Bitacora-Agent`.
    pub agent: Option<String>,
}

fn kind_from_str(s: &str) -> Option<CommitKind> {
    Some(match s {
        "auto" => CommitKind::Auto,
        "manual" => CommitKind::Manual,
        "merge" => CommitKind::Merge,
        "resolve" => CommitKind::Resolve,
        "agent" => CommitKind::Agent,
        "migrate" => CommitKind::Migrate,
        _ => return None,
    })
}

fn trailer_of(line: &str) -> Option<(&str, &str)> {
    let (k, v) = line.split_once(": ")?;
    let valid_key = !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid_key.then_some((k, v.trim()))
}

/// Parses the trailers of the last paragraph of `message`. Unknown input yields empty fields.
pub fn parse_message(message: &str) -> ParsedMessage {
    let normalized = message.replace("\r\n", "\n");
    let mut out = ParsedMessage {
        subject: normalized.lines().next().unwrap_or_default().to_string(),
        ..ParsedMessage::default()
    };
    let trimmed = normalized.trim_end();
    let Some(last) = trimmed.rsplit("\n\n").next() else {
        return out;
    };
    // A message that is only a subject has no trailer block.
    if !trimmed.contains("\n\n") {
        return out;
    }
    let lines: Vec<&str> = last.lines().collect();
    if lines.is_empty() || !lines.iter().all(|l| trailer_of(l).is_some()) {
        return out;
    }
    for line in lines {
        let Some((k, v)) = trailer_of(line) else {
            continue;
        };
        match k {
            DEVICE => out.device = Some(v.to_string()),
            KIND => out.kind = kind_from_str(v),
            AGENT => out.agent = Some(v.to_string()),
            PAGES => {
                out.pages = v
                    .split("; ")
                    .map(str::trim)
                    .filter(|p| !p.is_empty() && !(p.starts_with("(+") && p.ends_with(" more)")))
                    .map(str::to_string)
                    .collect();
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn spec_scenario_auto_message() {
        let p = pages(&[
            "journals/2026_10_06.md",
            "pages/Project X.md",
            "pages/Ideas.md",
        ]);
        let m = build_message(&MessageSpec {
            kind: CommitKind::Auto,
            device: "laptop-jose",
            pages: &p,
            agent: None,
            subject: None,
        });
        assert_eq!(
            m.render(),
            "bitacora: edit 3 pages (journals/2026_10_06, Project X, Ideas)\n\n\
             Bitacora-Device: laptop-jose\nBitacora-Kind: auto\n\
             Bitacora-Pages: journals/2026_10_06.md; pages/Project X.md; pages/Ideas.md\n"
        );
    }

    #[test]
    fn single_page_is_singular() {
        let p = pages(&["pages/Ideas.md"]);
        let m = build_message(&MessageSpec {
            kind: CommitKind::Auto,
            device: "d",
            pages: &p,
            agent: None,
            subject: None,
        });
        assert_eq!(m.subject, "bitacora: edit 1 page (Ideas)");
    }

    #[test]
    fn subject_never_exceeds_72_chars() {
        let many: Vec<String> = (0..40)
            .map(|i| format!("pages/Some Long Page {i}.md"))
            .collect();
        let long = pages(&[&format!("pages/{}.md", "x".repeat(200))]);
        for p in [many, long, pages(&["a.png", "b.png"])] {
            let m = build_message(&MessageSpec {
                kind: CommitKind::Auto,
                device: "d",
                pages: &p,
                agent: None,
                subject: None,
            });
            assert!(
                m.subject.chars().count() <= MAX_SUBJECT_CHARS,
                "{}",
                m.subject
            );
        }
    }

    #[test]
    fn agent_kind_adds_agent_trailer() {
        let p = pages(&["pages/A.md"]);
        let m = build_message(&MessageSpec {
            kind: CommitKind::Agent,
            device: "d",
            pages: &p,
            agent: Some("claude-desktop"),
            subject: None,
        });
        let parsed = parse_message(&m.render());
        assert_eq!(parsed.kind, Some(CommitKind::Agent));
        assert_eq!(parsed.agent.as_deref(), Some("claude-desktop"));
        // Non-agent kinds never carry the trailer.
        let m = build_message(&MessageSpec {
            kind: CommitKind::Auto,
            device: "d",
            pages: &p,
            agent: Some("ignored"),
            subject: None,
        });
        assert!(!m.render().contains("Bitacora-Agent"));
    }

    #[test]
    fn round_trip_parse() {
        let p = pages(&["journals/2026_10_06.md", "pages/Project X.md"]);
        let m = build_message(&MessageSpec {
            kind: CommitKind::Auto,
            device: "laptop-jose",
            pages: &p,
            agent: None,
            subject: None,
        });
        let parsed = parse_message(&m.render());
        assert_eq!(parsed.kind, Some(CommitKind::Auto));
        assert_eq!(parsed.device.as_deref(), Some("laptop-jose"));
        assert_eq!(parsed.pages, p);
        assert_eq!(parsed.subject, m.subject);
    }

    #[test]
    fn many_pages_are_capped_and_marker_ignored_on_parse() {
        let p: Vec<String> = (0..MAX_TRAILER_PAGES + 5)
            .map(|i| format!("pages/P{i}.md"))
            .collect();
        let m = build_message(&MessageSpec {
            kind: CommitKind::Auto,
            device: "d",
            pages: &p,
            agent: None,
            subject: None,
        });
        assert!(m.render().contains("(+5 more)"));
        assert_eq!(parse_message(&m.render()).pages.len(), MAX_TRAILER_PAGES);
    }

    #[test]
    fn foreign_messages_parse_to_nothing() {
        assert_eq!(parse_message("Auto saved by Logseq\n").kind, None);
        assert_eq!(parse_message("fix: x\n\nsome body text\n").device, None);
        assert_eq!(parse_message("").subject, "");
        // Trailers must be in the last paragraph.
        let parsed = parse_message("s\n\nBitacora-Kind: auto\n\nprose after\n");
        assert_eq!(parsed.kind, None);
    }

    #[test]
    fn user_subject_is_sanitised() {
        let p = pages(&["pages/A.md"]);
        let m = build_message(&MessageSpec {
            kind: CommitKind::Manual,
            device: "d",
            pages: &p,
            agent: None,
            subject: Some("my\nnote"),
        });
        assert_eq!(m.subject, "my note");
    }
}
