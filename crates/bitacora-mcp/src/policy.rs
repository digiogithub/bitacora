//! Write policy: global toggles, per-token rate limit and protected pages (BIT-SP-0007.R6, R12,
//! R14; design `mcp-server.md` section 3).
//!
//! Writes and deletes are **off by default** (`mcp.allow_writes` / `mcp.allow_deletes`). The policy
//! is shared (`Arc`) between the server and its owner, so the app can flip a toggle without
//! restarting the endpoint.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};

/// Write operations allowed per token and minute (design section 3).
pub const DEFAULT_WRITES_PER_MINUTE: usize = 60;
/// Blocks accepted by one write call (design section 3).
pub const MAX_BLOCKS_PER_CALL: usize = 200;

/// Page property that makes a page read-only for agents.
pub const READONLY_PROPERTY: &str = "bitacora-agent-readonly";

/// Live-adjustable write policy.
#[derive(Debug)]
pub struct WritePolicy {
    allow_writes: AtomicBool,
    allow_deletes: AtomicBool,
    protected_namespaces: RwLock<Vec<String>>,
    limiter: RateLimiter,
}

impl WritePolicy {
    /// A policy with the given toggles and protected namespaces; the rate limit is
    /// [`DEFAULT_WRITES_PER_MINUTE`].
    #[must_use]
    pub fn new(allow_writes: bool, allow_deletes: bool, protected_namespaces: Vec<String>) -> Self {
        Self {
            allow_writes: AtomicBool::new(allow_writes),
            allow_deletes: AtomicBool::new(allow_deletes),
            protected_namespaces: RwLock::new(protected_namespaces),
            limiter: RateLimiter::new(DEFAULT_WRITES_PER_MINUTE, Duration::from_secs(60)),
        }
    }

    /// Replaces the rate limiter (tests use a tiny budget).
    #[must_use]
    pub fn with_rate_limit(mut self, max: usize, window: Duration) -> Self {
        self.limiter = RateLimiter::new(max, window);
        self
    }

    /// `mcp.allow_writes`.
    #[must_use]
    pub fn allow_writes(&self) -> bool {
        self.allow_writes.load(Ordering::SeqCst)
    }

    /// `mcp.allow_deletes`.
    #[must_use]
    pub fn allow_deletes(&self) -> bool {
        self.allow_deletes.load(Ordering::SeqCst)
    }

    /// Turns agent writes on or off.
    pub fn set_allow_writes(&self, on: bool) {
        self.allow_writes.store(on, Ordering::SeqCst);
    }

    /// Turns agent deletes (remove block, rename page, delete page) on or off.
    pub fn set_allow_deletes(&self, on: bool) {
        self.allow_deletes.store(on, Ordering::SeqCst);
    }

    /// Replaces `mcp.protected_namespaces`.
    pub fn set_protected_namespaces(&self, namespaces: Vec<String>) {
        *self.protected_namespaces.write() = namespaces;
    }

    /// Whether `page` is the root or a child of a protected namespace (case-insensitive).
    #[must_use]
    pub fn in_protected_namespace(&self, page: &str) -> bool {
        let page = page.trim().to_lowercase();
        self.protected_namespaces.read().iter().any(|ns| {
            let ns = ns.trim().trim_end_matches('/').to_lowercase();
            !ns.is_empty() && (page == ns || page.starts_with(&format!("{ns}/")))
        })
    }

    /// Takes one write op from `token`'s budget. `Err(retry_after)` when the budget is spent.
    ///
    /// # Errors
    /// The time after which the next op fits.
    pub fn take_write(&self, token: &str) -> Result<(), Duration> {
        self.limiter.take(token, Instant::now())
    }
}

impl Default for WritePolicy {
    fn default() -> Self {
        Self::new(false, false, Vec::new())
    }
}

/// Sliding-window counter per token.
#[derive(Debug)]
struct RateLimiter {
    max: usize,
    window: Duration,
    hits: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl RateLimiter {
    fn new(max: usize, window: Duration) -> Self {
        Self {
            max,
            window,
            hits: Mutex::new(HashMap::new()),
        }
    }

    fn take(&self, token: &str, now: Instant) -> Result<(), Duration> {
        let mut all = self.hits.lock();
        let q = all.entry(token.to_owned()).or_default();
        while q
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= self.window)
        {
            q.pop_front();
        }
        if q.len() >= self.max {
            let oldest = q.front().copied().unwrap_or(now);
            return Err((oldest + self.window).saturating_duration_since(now));
        }
        q.push_back(now);
        Ok(())
    }
}

/// Is the page property map marked agent-read-only (`bitacora-agent-readonly:: true`)?
#[must_use]
pub fn is_marked_readonly<'a>(
    mut properties: impl Iterator<Item = (&'a String, &'a String)>,
) -> bool {
    properties.any(|(k, v)| {
        k.trim().to_lowercase().replace('_', "-") == READONLY_PROPERTY
            && v.trim().eq_ignore_ascii_case("true")
    })
}

/// Editor state the app can report so agent writes never race a block being typed in
/// (design section 4). The default implementation reports nothing.
pub trait WriteGate: Send + Sync + 'static {
    /// `Some(retry_after_ms)` while the user is editing this block (`BLOCK_BUSY`).
    fn busy(&self, _page: &str, _block_uuid: &str) -> Option<u64> {
        None
    }
}

/// Gate used when no editor is attached.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenGate;

impl WriteGate for OpenGate {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_blocks_after_budget_and_recovers() {
        let l = RateLimiter::new(2, Duration::from_secs(60));
        let t0 = Instant::now();
        assert!(l.take("a", t0).is_ok());
        assert!(l.take("a", t0).is_ok());
        assert!(l.take("a", t0).is_err());
        // Another token has its own budget.
        assert!(l.take("b", t0).is_ok());
        assert!(l.take("a", t0 + Duration::from_secs(61)).is_ok());
    }

    #[test]
    fn namespaces_match_root_and_children_only() {
        let p = WritePolicy::new(true, true, vec!["Private/".into()]);
        assert!(p.in_protected_namespace("private"));
        assert!(p.in_protected_namespace("Private/Diary"));
        assert!(!p.in_protected_namespace("Privateer"));
        assert!(!p.in_protected_namespace("public/private"));
    }

    #[test]
    fn readonly_marker() {
        let k = "bitacora_agent_readonly".to_owned();
        let v = "TRUE".to_owned();
        assert!(is_marked_readonly([(&k, &v)].into_iter()));
        let f = "false".to_owned();
        assert!(!is_marked_readonly([(&k, &f)].into_iter()));
    }
}
