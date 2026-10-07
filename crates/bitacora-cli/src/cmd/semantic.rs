//! `bitacora-cli semantic`: status, resync, purge and search of the Pando-backed semantic index
//! (BIT-SP-0010.R5/R6). Settings come from the machine-local `pando.json`; nothing is sent
//! unless the integration is active and the graph has consent.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use bitacora_config::{PandoFeature, PandoSettings};
use bitacora_runtime::{
    HybridOptions, PandoOptions, PandoStatus, RuntimeConfig, SemanticState, Session,
};
use clap::{Args, Subcommand};
use serde::Serialize;

use super::GraphArgs;

/// Exit code when semantic search is not enabled or not reachable.
pub const EXIT_UNAVAILABLE: u8 = 3;

/// Options of `semantic`.
#[derive(Debug, Args)]
pub struct SemanticArgs {
    #[command(subcommand)]
    pub action: Action,
}

/// What to do.
#[derive(Debug, Subcommand)]
pub enum Action {
    /// Show whether semantic search is enabled and how much is synced.
    Status(CommonArgs),
    /// Re-diff the graph and send everything that is missing or changed.
    Resync(CommonArgs),
    /// Remove every document of this graph from Pando (found through the local ledger).
    Purge(CommonArgs),
    /// Hybrid (lexical + semantic) search; falls back to lexical when Pando is unavailable.
    Search(SearchArgs),
}

/// Options shared by the subcommands.
#[derive(Debug, Args)]
pub struct CommonArgs {
    #[command(flatten)]
    pub graph: GraphArgs,
    /// Pando settings file (default: `pando.json` in the platform config dir).
    #[arg(long)]
    pub pando_settings: Option<PathBuf>,
    /// Seconds to wait for Pando and for the sync to settle.
    #[arg(long, default_value_t = 60)]
    pub wait: u64,
}

/// Options of `semantic search`.
#[derive(Debug, Args)]
pub struct SearchArgs {
    #[command(flatten)]
    pub common: CommonArgs,
    /// Query text.
    pub query: String,
    /// Maximum hits.
    #[arg(long, default_value_t = 10)]
    pub limit: usize,
}

/// Whether semantic search can run for a graph, and why not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Eligibility {
    /// Settings file used.
    pub settings_file: Option<PathBuf>,
    /// `enabled` and mode is not `off`.
    pub active: bool,
    /// Mode (`managed`, `external`, `off`).
    pub mode: String,
    /// The graph has consent.
    pub consent: bool,
    /// The `semantic_search` feature is on.
    pub feature: bool,
    /// Path exclusions / page names configured for the graph.
    pub exclusions: usize,
    /// First blocker, `None` when everything is in place.
    pub blocker: Option<String>,
}

impl Eligibility {
    /// Whether nothing blocks semantic search.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.blocker.is_none()
    }
}

/// Evaluate `settings` for the graph at `graph` (canonical path).
#[must_use]
pub fn eligibility(
    settings: &PandoSettings,
    settings_file: Option<PathBuf>,
    graph: &Path,
) -> Eligibility {
    let key = graph.to_string_lossy().into_owned();
    let active = settings.is_active();
    let consent = settings.has_consent(&key);
    let feature = settings.feature_enabled(PandoFeature::SemanticSearch);
    let blocker = if !active {
        Some("the Pando integration is off (enable it in pando.json)".to_owned())
    } else if !consent {
        Some("this graph has no consent to send blocks to Pando".to_owned())
    } else if !feature {
        Some("the semantic_search feature is switched off".to_owned())
    } else {
        None
    };
    Eligibility {
        settings_file,
        active,
        mode: format!("{:?}", settings.mode).to_lowercase(),
        consent,
        feature,
        exclusions: settings.consent(&key).exclusions.len(),
        blocker,
    }
}

/// Load the settings at `explicit` or the default location (missing file = defaults).
pub fn load_settings(explicit: Option<&Path>) -> anyhow::Result<(PandoSettings, Option<PathBuf>)> {
    let path = explicit
        .map(Path::to_path_buf)
        .or_else(bitacora_runtime::default_pando_settings_path);
    let Some(p) = path else {
        return Ok((PandoSettings::default(), None));
    };
    let s = bitacora_runtime::load_pando_settings(&p)
        .with_context(|| format!("reading {}", p.display()))?;
    Ok((s, Some(p)))
}

/// Pando options for a headless session (`None` when the integration is inactive).
pub fn pando_options(
    explicit: Option<&Path>,
    graph: &Path,
) -> anyhow::Result<Option<PandoOptions>> {
    let (settings, _) = load_settings(explicit)?;
    Ok(settings
        .is_active()
        .then(|| PandoOptions::new(settings, graph)))
}

#[derive(Debug, Serialize)]
struct StatusOut {
    #[serde(flatten)]
    eligibility: Eligibility,
    pando: String,
    synced: Option<u64>,
    pending: Option<u64>,
}

fn describe(status: &PandoStatus) -> String {
    match status {
        PandoStatus::Off => "off".into(),
        PandoStatus::ConsentRequired => "consent required".into(),
        PandoStatus::Starting => "starting".into(),
        PandoStatus::Connected { version } => format!("connected (Pando {version})"),
        PandoStatus::Unauthorized => "unauthorized (the token was rejected)".into(),
        PandoStatus::TooOld { version, min } => {
            format!("too old (Pando {version}, minimum {min})")
        }
        PandoStatus::Unavailable { reason } => format!("unavailable: {reason}"),
    }
}

fn open(common: &CommonArgs) -> anyhow::Result<(Session, Eligibility)> {
    let resolved = common.graph.resolve()?;
    let (settings, file) = load_settings(common.pando_settings.as_deref())?;
    let elig = eligibility(&settings, file, &resolved.graph);
    let mut cfg = RuntimeConfig::new(&common.graph.graph);
    cfg.data_dir = common.graph.data_dir.clone();
    if settings.is_active() {
        cfg.pando = Some(PandoOptions::new(settings, &resolved.graph));
    }
    let session = Session::open(cfg).context("opening the graph session")?;
    Ok((session, elig))
}

fn wait_connected(session: &Session, budget: Duration) -> PandoStatus {
    let end = Instant::now() + budget;
    loop {
        let st = session.pando_status();
        let settled = matches!(
            st,
            PandoStatus::Connected { .. }
                | PandoStatus::Off
                | PandoStatus::ConsentRequired
                | PandoStatus::Unauthorized
                | PandoStatus::TooOld { .. }
                | PandoStatus::Unavailable { .. }
        );
        if settled || Instant::now() >= end {
            return st;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Runs the subcommand and returns the process exit code.
pub fn run(args: &SemanticArgs) -> anyhow::Result<u8> {
    let dir = bitacora_runtime::instance::default_dir()
        .context("cannot determine the data directory for the instance lock")?;
    let _instance = super::serve::acquire_instance(&dir)?;
    match &args.action {
        Action::Status(c) => status(c),
        Action::Resync(c) => resync(c, false),
        Action::Purge(c) => resync(c, true),
        Action::Search(s) => search(s),
    }
}

fn status(c: &CommonArgs) -> anyhow::Result<u8> {
    let (session, elig) = open(c)?;
    let st = wait_connected(&session, Duration::from_secs(c.wait.min(15)));
    let counts = session.semantic_status();
    let out = StatusOut {
        eligibility: elig,
        pando: describe(&st),
        synced: counts.as_ref().map(|s| s.synced),
        pending: counts.as_ref().map(|s| s.pending),
    };
    if c.graph.json {
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("Pando:      {}", out.pando);
        println!("Mode:       {}", out.eligibility.mode);
        println!("Consent:    {}", yes(out.eligibility.consent));
        println!("Feature:    {}", yes(out.eligibility.feature));
        println!("Exclusions: {}", out.eligibility.exclusions);
        match (out.synced, out.pending) {
            (Some(s), Some(p)) => println!("Documents:  {s} synced, {p} pending"),
            _ => println!("Documents:  semantic sync is not running"),
        }
        if let Some(b) = &out.eligibility.blocker {
            println!("Blocked:    {b}");
        }
    }
    session.shutdown(Duration::from_secs(10));
    Ok(if out.eligibility.ready() {
        0
    } else {
        EXIT_UNAVAILABLE
    })
}

fn yes(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

fn resync(c: &CommonArgs, purge: bool) -> anyhow::Result<u8> {
    let (session, elig) = open(c)?;
    let verb = if purge { "purge" } else { "resync" };
    if let Some(b) = &elig.blocker {
        eprintln!("bitacora-cli semantic {verb}: {b}");
        session.shutdown(Duration::from_secs(10));
        return Ok(EXIT_UNAVAILABLE);
    }
    let budget = Duration::from_secs(c.wait);
    let st = wait_connected(&session, budget);
    let Some(worker) = session.semantic() else {
        eprintln!("bitacora-cli semantic {verb}: semantic sync is not running");
        session.shutdown(Duration::from_secs(10));
        return Ok(EXIT_UNAVAILABLE);
    };
    if !matches!(st, PandoStatus::Connected { .. }) {
        eprintln!(
            "bitacora-cli semantic {verb}: Pando is not connected ({})",
            describe(&st)
        );
        session.shutdown(Duration::from_secs(10));
        return Ok(EXIT_UNAVAILABLE);
    }
    if purge {
        worker.purge();
    } else {
        worker.reconcile();
        worker.retry_now();
    }
    let end = Instant::now() + budget;
    // Give the worker a moment to take the command before judging "settled".
    std::thread::sleep(Duration::from_millis(500));
    let done = loop {
        let s = worker.status();
        if s.pending == 0 && (!purge || s.synced == 0) {
            break true;
        }
        if Instant::now() >= end {
            break false;
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    let s = worker.status();
    let note = if done {
        ""
    } else {
        " (timed out; it continues next time)"
    };
    println!("{verb}: {} synced, {} pending{note}", s.synced, s.pending);
    session.shutdown(Duration::from_secs(10));
    Ok(u8::from(!done))
}

fn search(a: &SearchArgs) -> anyhow::Result<u8> {
    let (session, _elig) = open(&a.common)?;
    let _ = wait_connected(&session, Duration::from_secs(a.common.wait.min(15)));
    let opts = HybridOptions {
        limit: a.limit,
        ..HybridOptions::default()
    };
    let res = session
        .hybrid_search(&a.query, &opts)
        .context("searching")?;
    for h in &res.hits {
        let tag = match (h.lexical_rank.is_some(), h.semantic_rank.is_some()) {
            (true, true) => "both",
            (false, true) => "semantic",
            _ => "lexical",
        };
        let stale = if h.stale { " (possibly outdated)" } else { "" };
        println!(
            "[{tag}] {}: {}{stale}",
            h.title,
            h.snippet.text.replace('\n', " ")
        );
    }
    if let SemanticState::Unavailable(why) = &res.semantic {
        eprintln!("note: lexical results only ({why:?})");
    }
    session.shutdown(Duration::from_secs(10));
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blockers_are_reported_in_order() {
        let g = Path::new("/graph");
        let mut s = PandoSettings::default();
        let e = eligibility(&s, None, g);
        assert!(!e.ready());
        assert!(e.blocker.as_deref().is_some_and(|b| b.contains("off")));
        s.enabled = true;
        let e = eligibility(&s, None, g);
        assert!(e.blocker.as_deref().is_some_and(|b| b.contains("consent")));
        s.grant_consent("/graph", 1);
        assert!(eligibility(&s, None, g).ready());
        s.features.insert(PandoFeature::SemanticSearch, false);
        let e = eligibility(&s, None, g);
        assert!(e.blocker.as_deref().is_some_and(|b| b.contains("feature")));
    }

    #[test]
    fn pando_options_follow_the_settings_file() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("pando.json");
        assert!(
            pando_options(Some(&path), dir.path())
                .expect("missing file")
                .is_none()
        );
        let s = PandoSettings {
            enabled: true,
            ..PandoSettings::default()
        };
        s.save(&path).expect("save");
        assert!(
            pando_options(Some(&path), dir.path())
                .expect("load")
                .is_some()
        );
    }
}
