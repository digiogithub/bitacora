//! Pending approvals that fail closed (BIT-T-0459, BIT-SP-0011.R2).
//!
//! Every interrupt the user must answer (a Pando permission prompt, a `propose_edit` card, an
//! `AskUserQuestion`) is registered in [`PendingApprovals`]. The only way to approve is an
//! explicit [`Decision::Approve`] for that exact call id while it is still pending. Anything else
//! resolves it as denied (or, for a question, cancelled): the deadline passing
//! ([`PendingApprovals::expire`]), the panel closing, a thread or graph switch, quit, a cancelled
//! run, or Pando having given up on its side. A resolved approval is final, so a late "Approve"
//! click after a timeout cannot write anything.
//!
//! Time comes from a [`Clock`] so tests can advance it without sleeping.

use std::collections::BTreeMap;
use std::time::Duration;

use pando::agui::hitl::{self, QuestionAnswer};

/// Default time a card waits for an answer.
pub const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);

/// A monotonic clock (time since an arbitrary origin).
pub trait Clock: Send + Sync {
    /// Time elapsed since the clock's origin.
    fn now(&self) -> Duration;
}

/// The real clock.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    origin: std::time::Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        Self {
            origin: std::time::Instant::now(),
        }
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

/// What kind of answer an interrupt takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalKind {
    /// A `pando_permission_request`.
    Permission,
    /// A `propose_edit` card.
    ProposeEdit,
    /// An `AskUserQuestion`.
    Question,
}

/// An answer from the user.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Allow (permission and edit cards only).
    Approve,
    /// Refuse.
    Deny,
    /// Answer a question.
    Answer(QuestionAnswer),
}

/// Why something was denied without the user clicking Deny.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyReason {
    /// The user pressed Deny.
    User,
    /// The deadline passed.
    Timeout,
    /// The chat panel was closed.
    PanelClosed,
    /// The user switched to another thread.
    ThreadSwitch,
    /// The graph was closed or switched.
    GraphSwitch,
    /// The app is quitting.
    Quit,
    /// The run was cancelled.
    Cancelled,
    /// Pando resolved it first (its own timeout) or the run is gone.
    ServerExpired,
}

impl DenyReason {
    /// Stable name for logs and tool results.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user_denied",
            Self::Timeout => "timeout",
            Self::PanelClosed => "panel_closed",
            Self::ThreadSwitch => "thread_switch",
            Self::GraphSwitch => "graph_switch",
            Self::Quit => "quit",
            Self::Cancelled => "cancelled",
            Self::ServerExpired => "server_expired",
        }
    }
}

/// The final state of one approval.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// The user approved.
    Approved,
    /// Denied (or cancelled, for a question).
    Denied(DenyReason),
    /// A question was answered.
    Answered(QuestionAnswer),
}

impl Resolution {
    /// Only an explicit approval allows the action.
    #[must_use]
    pub fn is_approved(&self) -> bool {
        matches!(self, Self::Approved)
    }

    /// The `tool` message payload for a permission or question prompt.
    #[must_use]
    pub fn hitl_payload(&self) -> String {
        match self {
            Self::Approved => hitl::approve(),
            Self::Denied(_) => hitl::deny(),
            Self::Answered(a) => hitl::answer_question(a),
        }
    }

    /// The payload for a question: a denial is a cancellation.
    #[must_use]
    pub fn question_payload(&self) -> String {
        match self {
            Self::Answered(a) => hitl::answer_question(a),
            _ => hitl::cancel_question(),
        }
    }
}

#[derive(Debug)]
struct Entry {
    kind: ApprovalKind,
    deadline: Duration,
    resolution: Option<Resolution>,
}

/// The approvals of one chat thread.
#[derive(Debug)]
pub struct PendingApprovals {
    timeout: Duration,
    entries: BTreeMap<String, Entry>,
}

impl Default for PendingApprovals {
    fn default() -> Self {
        Self::new(DEFAULT_APPROVAL_TIMEOUT)
    }
}

impl PendingApprovals {
    /// An empty book; each card waits `timeout` for its answer.
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            entries: BTreeMap::new(),
        }
    }

    /// Registers a pending approval whose deadline is `now + timeout`. Registering an id twice
    /// keeps the first entry.
    pub fn register(&mut self, id: &str, kind: ApprovalKind, now: Duration) {
        self.entries.entry(id.to_owned()).or_insert(Entry {
            kind,
            deadline: now.saturating_add(self.timeout),
            resolution: None,
        });
    }

    /// Applies the user's answer. Returns `false` (and changes nothing) when the id is unknown,
    /// already resolved, or the decision does not fit the kind (an approval of a question, an
    /// answer to a permission prompt).
    pub fn decide(&mut self, id: &str, decision: Decision) -> bool {
        let Some(e) = self.entries.get_mut(id) else {
            return false;
        };
        if e.resolution.is_some() {
            return false;
        }
        let resolution = match (e.kind, decision) {
            (ApprovalKind::Question, Decision::Answer(a)) => Resolution::Answered(a),
            (ApprovalKind::Permission | ApprovalKind::ProposeEdit, Decision::Approve) => {
                Resolution::Approved
            }
            (_, Decision::Deny) => Resolution::Denied(DenyReason::User),
            _ => return false,
        };
        e.resolution = Some(resolution);
        true
    }

    /// Resolves one pending approval as denied for `reason`. `false` when not pending.
    pub fn deny(&mut self, id: &str, reason: DenyReason) -> bool {
        match self.entries.get_mut(id) {
            Some(e) if e.resolution.is_none() => {
                e.resolution = Some(Resolution::Denied(reason));
                true
            }
            _ => false,
        }
    }

    /// Resolves every pending approval whose deadline passed as [`DenyReason::Timeout`].
    /// Returns the ids it resolved.
    pub fn expire(&mut self, now: Duration) -> Vec<String> {
        let mut out = Vec::new();
        for (id, e) in &mut self.entries {
            if e.resolution.is_none() && now >= e.deadline {
                e.resolution = Some(Resolution::Denied(DenyReason::Timeout));
                out.push(id.clone());
            }
        }
        out
    }

    /// Resolves every pending approval as denied for `reason` (panel close, thread or graph
    /// switch, quit, cancelled run). Returns the ids it resolved.
    pub fn deny_all(&mut self, reason: DenyReason) -> Vec<String> {
        let mut out = Vec::new();
        for (id, e) in &mut self.entries {
            if e.resolution.is_none() {
                e.resolution = Some(Resolution::Denied(reason));
                out.push(id.clone());
            }
        }
        out
    }

    /// The resolution of `id`, once there is one.
    #[must_use]
    pub fn resolution(&self, id: &str) -> Option<&Resolution> {
        self.entries.get(id)?.resolution.as_ref()
    }

    /// Whether `id` is registered and still waiting.
    #[must_use]
    pub fn is_pending(&self, id: &str) -> bool {
        self.entries.get(id).is_some_and(|e| e.resolution.is_none())
    }

    /// Ids still waiting.
    #[must_use]
    pub fn pending_ids(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(_, e)| e.resolution.is_none())
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// The earliest deadline among pending approvals.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Duration> {
        self.entries
            .values()
            .filter(|e| e.resolution.is_none())
            .map(|e| e.deadline)
            .min()
    }

    /// Forgets `id` (after its answer was delivered).
    pub fn remove(&mut self, id: &str) {
        self.entries.remove(id);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const T0: Duration = Duration::from_secs(10);

    fn book() -> PendingApprovals {
        let mut b = PendingApprovals::new(Duration::from_secs(60));
        b.register("p", ApprovalKind::Permission, T0);
        b.register("e", ApprovalKind::ProposeEdit, T0);
        b.register("q", ApprovalKind::Question, T0);
        b
    }

    #[test]
    fn only_an_explicit_approval_approves() {
        let mut b = book();
        assert!(b.decide("p", Decision::Approve));
        assert_eq!(b.resolution("p"), Some(&Resolution::Approved));
        assert!(b.resolution("p").unwrap().is_approved());
        assert_eq!(
            b.resolution("p").unwrap().hitl_payload(),
            r#"{"approved":true}"#
        );
        // Pending ones are not approved.
        assert!(b.resolution("e").is_none());
    }

    #[test]
    fn mismatched_and_repeated_decisions_are_ignored() {
        let mut b = book();
        assert!(!b.decide("q", Decision::Approve));
        assert!(!b.decide("p", Decision::Answer(QuestionAnswer::default())));
        assert!(!b.decide("nope", Decision::Approve));
        assert!(b.is_pending("q") && b.is_pending("p"));
        assert!(b.decide("e", Decision::Deny));
        assert!(!b.decide("e", Decision::Approve), "a denial is final");
        assert!(!b.resolution("e").unwrap().is_approved());
    }

    #[test]
    fn timeout_denies_and_a_late_approval_does_nothing() {
        let mut b = book();
        assert!(b.expire(T0 + Duration::from_secs(59)).is_empty());
        let gone = b.expire(T0 + Duration::from_secs(60));
        assert_eq!(gone, vec!["e", "p", "q"]);
        assert_eq!(
            b.resolution("e"),
            Some(&Resolution::Denied(DenyReason::Timeout))
        );
        assert!(!b.decide("e", Decision::Approve));
        assert!(!b.resolution("e").unwrap().is_approved());
        assert_eq!(
            b.resolution("q").unwrap().question_payload(),
            r#"{"cancelled":true,"answers":[]}"#
        );
    }

    #[test]
    fn every_trigger_resolves_pending_approvals_as_denied() {
        for reason in [
            DenyReason::PanelClosed,
            DenyReason::ThreadSwitch,
            DenyReason::GraphSwitch,
            DenyReason::Quit,
            DenyReason::Cancelled,
            DenyReason::ServerExpired,
        ] {
            let mut b = book();
            b.decide("p", Decision::Approve);
            let denied = b.deny_all(reason);
            assert_eq!(denied, vec!["e", "q"], "{reason:?}");
            assert_eq!(b.resolution("p"), Some(&Resolution::Approved));
            for id in ["e", "q"] {
                assert_eq!(b.resolution(id), Some(&Resolution::Denied(reason)));
                assert!(!b.resolution(id).unwrap().is_approved());
            }
            assert!(b.pending_ids().is_empty());
            assert!(!b.decide("e", Decision::Approve));
        }
    }

    #[test]
    fn next_deadline_tracks_pending_only() {
        let mut b = PendingApprovals::new(Duration::from_secs(5));
        assert!(b.next_deadline().is_none());
        b.register("a", ApprovalKind::Permission, Duration::from_secs(1));
        b.register("b", ApprovalKind::Permission, Duration::from_secs(3));
        assert_eq!(b.next_deadline(), Some(Duration::from_secs(6)));
        b.deny("a", DenyReason::User);
        assert_eq!(b.next_deadline(), Some(Duration::from_secs(8)));
    }
}
