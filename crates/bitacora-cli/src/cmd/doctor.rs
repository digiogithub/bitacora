//! `bitacora-cli doctor`: index integrity checks and the diagnostics report.

use std::collections::BTreeMap;

use bitacora_index::{DoctorReport, inspect_index};
use clap::Args;

use super::GraphArgs;
use super::semantic::{Eligibility, eligibility, load_settings};

/// Exit code when an integrity check fails or the index is missing.
pub const EXIT_UNHEALTHY: u8 = 2;

/// Options of `doctor`.
#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[command(flatten)]
    pub graph: GraphArgs,
    /// Pando settings file (default: `pando.json` in the platform config dir).
    #[arg(long)]
    pub pando_settings: Option<std::path::PathBuf>,
}

/// Index report plus the Pando / semantic search status of the graph.
pub struct Full {
    /// Index integrity.
    pub index: DoctorReport,
    /// Pando settings for this graph (`None` when the settings file cannot be read).
    pub pando: Result<Eligibility, String>,
}

/// Inspect the index without modifying it; reads (never writes) the Pando settings. An unhealthy
/// Pando setup never makes the doctor fail: the integration is optional.
pub fn run(args: &DoctorArgs) -> anyhow::Result<Full> {
    let resolved = args.graph.resolve()?;
    let pando = load_settings(args.pando_settings.as_deref())
        .map(|(s, file)| eligibility(&s, file, &resolved.graph))
        .map_err(|e| format!("{e:#}"));
    Ok(Full {
        index: inspect_index(&resolved.location.db_path()),
        pando,
    })
}

fn print_pando(p: &Result<Eligibility, String>) {
    match p {
        Err(e) => println!("Pando:   settings unreadable: {e}"),
        Ok(e) if !e.active => println!("Pando:   off (semantic search disabled)"),
        Ok(e) => {
            println!(
                "Pando:   mode {}, consent {}, semantic_search {}, {} exclusion(s)",
                e.mode,
                if e.consent { "yes" } else { "no" },
                if e.feature { "on" } else { "off" },
                e.exclusions
            );
            match &e.blocker {
                None => println!("Semantic: enabled (see `bitacora-cli semantic status`)"),
                Some(b) => println!("Semantic: not running: {b}"),
            }
        }
    }
}

/// Print the report as text or JSON.
pub fn print(full: &Full, json: bool) -> anyhow::Result<()> {
    let report = &full.index;
    if json {
        let mut value = serde_json::to_value(report)?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert("healthy".into(), report.healthy().into());
            obj.insert(
                "pando".into(),
                match &full.pando {
                    Ok(e) => serde_json::to_value(e)?,
                    Err(msg) => serde_json::json!({ "error": msg }),
                },
            );
        }
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    println!("Index:   {}", report.path.display());
    if !report.exists {
        println!("Status:  missing (run `bitacora-cli reindex --graph <path>`)");
        print_pando(&full.pando);
        return Ok(());
    }
    println!("Size:    {} bytes", report.size_bytes);
    println!("SQLite:  {}", report.sqlite_version);
    for key in [
        "schema_version",
        "parser_version",
        "normalizer_version",
        "config_hash",
    ] {
        if let Some((_, v)) = report.meta.iter().find(|(k, _)| k == key) {
            println!("{key}: {v}");
        }
    }
    if let Some(s) = report.stats {
        println!(
            "Rows:    {} files, {} pages, {} blocks",
            s.files, s.pages, s.blocks
        );
    }
    println!("Checks:");
    for c in &report.checks {
        println!("  {:<22} {}", c.name, if c.ok { "ok" } else { &c.detail });
    }
    let mut by_kind: BTreeMap<&str, Vec<&bitacora_index::DiagnosticRow>> = BTreeMap::new();
    for d in &report.diagnostics {
        by_kind.entry(d.kind.as_str()).or_default().push(d);
    }
    if by_kind.is_empty() {
        println!("Diagnostics: none");
    } else {
        println!("Diagnostics:");
        for (kind, rows) in by_kind {
            println!("  {kind} ({})", rows.len());
            for d in rows {
                let place = match (&d.file, d.line) {
                    (Some(f), Some(l)) => format!("{f}:{l}"),
                    (Some(f), None) => f.clone(),
                    _ => "-".to_owned(),
                };
                println!("    {place}: {}", d.message);
            }
        }
    }
    print_pando(&full.pando);
    if report.healthy() {
        println!("Status:  healthy");
    } else {
        println!("Status:  UNHEALTHY; rebuild with `bitacora-cli reindex --graph <path>`");
    }
    Ok(())
}
