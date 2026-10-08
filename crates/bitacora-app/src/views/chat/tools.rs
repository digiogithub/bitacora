//! Pure helpers behind the tool-call cards and the agent header (BIT-US-0148): argument
//! summaries, JSON pretty-printing, status and duration texts, token counts.

use std::time::Duration;

use bitacora_runtime::ai::{CardState, CardView, ChatMessage, ToolCallView};
use serde_json::Value;

/// Longest one-line argument summary, in characters.
const SUMMARY_CHARS: usize = 80;
/// Longest JSON or result text a card expands to, in characters.
const DETAIL_CHARS: usize = 6_000;

/// Where a tool call stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallStatus {
    /// No result yet.
    Running,
    /// A result arrived.
    Done,
    /// A result arrived and it reports a failure.
    Failed,
}

/// The status of `call`.
#[must_use]
pub fn call_status(call: &ToolCallView) -> CallStatus {
    let Some(result) = &call.result else {
        return CallStatus::Running;
    };
    let head = result.trim_start();
    let failed = serde_json::from_str::<Value>(head)
        .ok()
        .is_some_and(|v| v.get("ok") == Some(&Value::Bool(false)) || v.get("error").is_some())
        || head.to_lowercase().starts_with("error");
    if failed {
        CallStatus::Failed
    } else {
        CallStatus::Done
    }
}

/// One piece of an assistant turn's tool activity (BIT-US-0179).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolSegment {
    /// A run of consecutive calls (with their resolved approval cards), folded into one row.
    /// Holds indices into `ChatMessage::tool_calls`.
    Group(Vec<usize>),
    /// A call whose approval card still needs the user; stays visible outside any group.
    Pending(usize),
}

/// Splits the tool calls of `msg` into collapsible groups and pending approvals, in order.
/// A call is pending while its approval card (same id) is `CardState::Pending`.
#[must_use]
pub fn group_tool_calls(msg: &ChatMessage, cards: &[CardView]) -> Vec<ToolSegment> {
    let mut out: Vec<ToolSegment> = Vec::new();
    for (i, call) in msg.tool_calls.iter().enumerate() {
        let pending = cards
            .iter()
            .any(|v| v.card.id == call.id && v.state == CardState::Pending);
        if pending {
            out.push(ToolSegment::Pending(i));
        } else if let Some(ToolSegment::Group(ix)) = out.last_mut() {
            ix.push(i);
        } else {
            out.push(ToolSegment::Group(vec![i]));
        }
    }
    out
}

/// Aggregate status of a group: `(running, failed)` call counts.
#[must_use]
pub fn group_counts(calls: &[ToolCallView], ix: &[usize]) -> (usize, usize) {
    let mut running = 0;
    let mut failed = 0;
    for c in ix.iter().filter_map(|i| calls.get(*i)) {
        match call_status(c) {
            CallStatus::Running => running += 1,
            CallStatus::Failed => failed += 1,
            CallStatus::Done => {}
        }
    }
    (running, failed)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut t: String = s.chars().take(max).collect();
        t.push('…');
        t
    }
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A one-line summary of a call's JSON arguments: `key: value` pairs of an object, or the raw
/// text squeezed onto one line. Streaming (half-received) arguments fall back to the raw text.
#[must_use]
pub fn summarize_args(args: &str) -> String {
    let raw = args.trim();
    let summary = match serde_json::from_str::<Value>(raw) {
        Ok(Value::Object(map)) => map
            .iter()
            .map(|(k, v)| match v {
                Value::String(s) => format!("{k}: {s}"),
                other => format!("{k}: {other}"),
            })
            .collect::<Vec<_>>()
            .join(", "),
        Ok(Value::Null) => String::new(),
        Ok(other) => other.to_string(),
        Err(_) => raw.to_owned(),
    };
    truncate(&one_line(&summary), SUMMARY_CHARS)
}

/// Pretty JSON when `text` is JSON, the text itself otherwise; capped for display.
#[must_use]
pub fn detail_text(text: &str) -> String {
    let pretty = serde_json::from_str::<Value>(text.trim())
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| text.to_owned());
    truncate(&pretty, DETAIL_CHARS)
}

/// `0.4 s`, `12 s`, `1 m 05 s`.
#[must_use]
pub fn duration_text(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1_000 {
        format!("{ms} ms")
    } else if ms < 10_000 {
        format!("{:.1} s", d.as_secs_f32())
    } else if ms < 60_000 {
        format!("{} s", d.as_secs())
    } else {
        format!("{} m {:02} s", d.as_secs() / 60, d.as_secs() % 60)
    }
}

/// `950`, `1.5k`, `200k`, `1.2M`.
#[must_use]
pub fn tokens_text(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => {
            let k = n as f64 / 1_000.0;
            if k >= 100.0 {
                format!("{k:.0}k")
            } else {
                format!("{k:.1}k").replace(".0k", "k")
            }
        }
        _ => format!("{:.1}M", n as f64 / 1_000_000.0).replace(".0M", "M"),
    }
}

/// Share of the context window in use, `0.0..=1.0`; `None` when the window is unknown.
#[must_use]
pub fn usage_fraction(used: u64, window: u64) -> Option<f32> {
    (window > 0).then(|| (used as f32 / window as f32).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(result: Option<&str>) -> ToolCallView {
        ToolCallView {
            id: "c".into(),
            name: "search".into(),
            args: "{}".into(),
            result: result.map(str::to_owned),
            ..ToolCallView::default()
        }
    }

    #[test]
    fn status_follows_the_result() {
        assert_eq!(call_status(&call(None)), CallStatus::Running);
        assert_eq!(call_status(&call(Some("3 pages"))), CallStatus::Done);
        assert_eq!(
            call_status(&call(Some(r#"{"ok":false,"error":"nope"}"#))),
            CallStatus::Failed
        );
        assert_eq!(call_status(&call(Some("Error: boom"))), CallStatus::Failed);
        assert_eq!(
            call_status(&call(Some(r#"{"ok":true,"result":{}}"#))),
            CallStatus::Done
        );
    }

    #[test]
    fn args_are_summarised_on_one_line() {
        assert_eq!(
            summarize_args(r#"{"query":"graph  notes","limit":5}"#),
            "query: graph notes, limit: 5"
        );
        assert_eq!(summarize_args("{\"query\": \"par"), "{\"query\": \"par");
        assert_eq!(summarize_args(""), "");
        let long = format!(r#"{{"q":"{}"}}"#, "x".repeat(300));
        assert!(summarize_args(&long).chars().count() <= SUMMARY_CHARS + 1);
    }

    #[test]
    fn details_are_pretty_json_or_plain() {
        assert!(detail_text(r#"{"a":1}"#).contains("\n  \"a\": 1"));
        assert_eq!(detail_text("plain result"), "plain result");
    }

    #[test]
    fn durations_and_tokens_read_well() {
        assert_eq!(duration_text(Duration::from_millis(420)), "420 ms");
        assert_eq!(duration_text(Duration::from_millis(1_500)), "1.5 s");
        assert_eq!(duration_text(Duration::from_secs(65)), "1 m 05 s");
        assert_eq!(tokens_text(950), "950");
        assert_eq!(tokens_text(1_500), "1.5k");
        assert_eq!(tokens_text(200_000), "200k");
        assert_eq!(tokens_text(2_000), "2k");
        assert_eq!(usage_fraction(50, 200), Some(0.25));
        assert_eq!(usage_fraction(50, 0), None);
        assert_eq!(usage_fraction(500, 200), Some(1.0));
    }

    #[test]
    fn tool_calls_group_around_pending_approvals() {
        use bitacora_runtime::ai::{ApprovalCard, CardKind};
        let mk = |id: &str, result: Option<&str>| ToolCallView {
            id: id.into(),
            name: "t".into(),
            result: result.map(str::to_owned),
            ..ToolCallView::default()
        };
        let msg = ChatMessage {
            tool_calls: vec![
                mk("a", Some("ok")),
                mk("b", None),
                mk("c", None),
                mk("d", Some("Error: x")),
            ],
            ..ChatMessage::default()
        };
        assert_eq!(
            group_tool_calls(&msg, &[]),
            vec![ToolSegment::Group(vec![0, 1, 2, 3])]
        );
        let card = |id: &str, state| CardView {
            card: ApprovalCard {
                id: id.into(),
                kind: CardKind::Question(Default::default()),
                timeout_secs: 0,
                remember_tool: None,
            },
            state,
        };
        let cards = [
            card("c", CardState::Pending),
            card("a", CardState::Approved),
        ];
        assert_eq!(
            group_tool_calls(&msg, &cards),
            vec![
                ToolSegment::Group(vec![0, 1]),
                ToolSegment::Pending(2),
                ToolSegment::Group(vec![3]),
            ]
        );
        assert_eq!(group_counts(&msg.tool_calls, &[0, 1, 2, 3]), (2, 1));
        assert!(group_tool_calls(&ChatMessage::default(), &[]).is_empty());
    }
}
