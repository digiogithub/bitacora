//! `bitacora-cli reindex`: delete the index and rebuild it from the graph.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Context as _;
use bitacora_index::{Index, IndexStats, Indexer, IndexerOptions, OpenOptions};
use clap::Args;
use serde::Serialize;

use super::{GraphArgs, load_config};

/// Options of `reindex`.
#[derive(Debug, Args)]
pub struct ReindexArgs {
    #[command(flatten)]
    pub graph: GraphArgs,
}

/// What a reindex did.
#[derive(Debug, Serialize)]
pub struct ReindexSummary {
    /// Index database path.
    pub index: PathBuf,
    /// Files indexed.
    pub files: i64,
    /// Pages (including placeholders and built-ins).
    pub pages: i64,
    /// Blocks.
    pub blocks: i64,
    /// Rows in `diagnostics`.
    pub diagnostics: i64,
    /// Files that could not be indexed, with the reason.
    pub errors: Vec<(String, String)>,
    /// Wall time in milliseconds.
    pub duration_ms: u128,
}

/// Delete the index files (database, `-wal`, `-shm`); missing files are fine.
fn delete_index_files(db: &std::path::Path) -> anyhow::Result<()> {
    for suffix in ["", "-wal", "-shm"] {
        let mut name = db.as_os_str().to_owned();
        name.push(suffix);
        let p = PathBuf::from(name);
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("removing {}", p.display())),
        }
    }
    Ok(())
}

/// Rebuild the index from scratch. The graph is only read.
pub fn run(args: &ReindexArgs) -> anyhow::Result<ReindexSummary> {
    let started = Instant::now();
    let resolved = args.graph.resolve()?;
    let config = load_config(&resolved.graph);
    let db = resolved.location.db_path();
    delete_index_files(&db)?;

    eprintln!("Indexing {} ...", resolved.graph.display());
    let index = Index::open(
        resolved.location,
        OpenOptions::for_config(&resolved.graph, &config),
    )
    .context("opening the index")?;
    let indexer = Indexer::start(&index, IndexerOptions::new(&resolved.graph, config))
        .context("starting the indexer")?;
    let stats = indexer.reconcile().context("building the index")?;
    indexer.shutdown();
    let counts = IndexStats::read(&*index.reader()?).context("counting rows")?;
    Ok(ReindexSummary {
        index: db,
        files: counts.files,
        pages: counts.pages,
        blocks: counts.blocks,
        diagnostics: counts.diagnostics,
        errors: stats.errors,
        duration_ms: started.elapsed().as_millis(),
    })
}

/// Print the summary as text or JSON.
pub fn print(summary: &ReindexSummary, json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(summary)?);
        return Ok(());
    }
    println!("Index:       {}", summary.index.display());
    println!("Files:       {}", summary.files);
    println!("Pages:       {}", summary.pages);
    println!("Blocks:      {}", summary.blocks);
    println!("Diagnostics: {}", summary.diagnostics);
    for (path, why) in &summary.errors {
        println!("Error:       {path}: {why}");
    }
    println!("Duration:    {} ms", summary.duration_ms);
    Ok(())
}
