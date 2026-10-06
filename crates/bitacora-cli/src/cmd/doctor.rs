//! `bitacora-cli doctor`: index integrity checks and the diagnostics report.

use std::collections::BTreeMap;

use bitacora_index::{DoctorReport, inspect_index};
use clap::Args;

use super::GraphArgs;

/// Exit code when an integrity check fails or the index is missing.
pub const EXIT_UNHEALTHY: u8 = 2;

/// Options of `doctor`.
#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[command(flatten)]
    pub graph: GraphArgs,
}

/// Inspect the index without modifying it.
pub fn run(args: &DoctorArgs) -> anyhow::Result<DoctorReport> {
    let resolved = args.graph.resolve()?;
    Ok(inspect_index(&resolved.location.db_path()))
}

/// Print the report as text or JSON.
pub fn print(report: &DoctorReport, json: bool) -> anyhow::Result<()> {
    if json {
        let mut value = serde_json::to_value(report)?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert("healthy".into(), report.healthy().into());
        }
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    println!("Index:   {}", report.path.display());
    if !report.exists {
        println!("Status:  missing (run `bitacora-cli reindex --graph <path>`)");
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
    if report.healthy() {
        println!("Status:  healthy");
    } else {
        println!("Status:  UNHEALTHY; rebuild with `bitacora-cli reindex --graph <path>`");
    }
    Ok(())
}
