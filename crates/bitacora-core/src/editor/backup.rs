//! `logseq/bak` backups (BIT-US-0066, BIT-SP-0005.R9; `docs/analysis/logseq/01-file-graph-layout.md`
//! §11).
//!
//! Layout: `logseq/bak/<rel dir>/<file stem>/<ISO ts with ':' -> '_'>.Desktop.<ext>`, newest 6
//! files kept per directory. The folder is never indexed or watched (the scanner ignores it).
//! Backups go through the [`FileStore`], so they are atomic like every other write.

use std::collections::HashMap;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use super::flush::FileStore;
use crate::graph_path::GraphPath;

/// Number of backups kept per file directory.
pub const KEEP: usize = 6;

/// Client label in the file name (`Desktop`, like Logseq's desktop app).
const CLIENT: &str = "Desktop";

/// ISO-8601 UTC with milliseconds and `:` replaced by `_`: `2025-11-14T09_30_12.345Z`.
#[must_use]
pub fn timestamp(at: SystemTime) -> String {
    let d = at.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = i64::try_from(d.as_secs()).unwrap_or(0);
    let ms = d.subsec_millis();
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil-from-days (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}_{:02}_{:02}.{ms:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Directory holding the backups of `file`: `logseq/bak/<rel dir>/<stem>`.
#[must_use]
pub fn backup_dir(file: &GraphPath) -> String {
    let rel = file.as_str();
    let (dir, name) = rel.rsplit_once('/').map_or(("", rel), |(d, n)| (d, n));
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    if dir.is_empty() {
        format!("logseq/bak/{stem}")
    } else {
        format!("logseq/bak/{dir}/{stem}")
    }
}

/// Full backup path of `file` at time `at`.
#[must_use]
pub fn backup_path(file: &GraphPath, at: SystemTime) -> String {
    let ext = file.extension().map_or(String::new(), |e| format!(".{e}"));
    format!("{}/{}.{CLIENT}{ext}", backup_dir(file), timestamp(at))
}

/// True when writing `new` over `old` drops text: some non-blank line occurs fewer times in
/// `new` than in `old` (the cheap equivalent of Logseq's "the diff deletes something").
#[must_use]
pub fn removes_text(old: &[u8], new: &[u8]) -> bool {
    fn counts(b: &[u8]) -> HashMap<&[u8], usize> {
        let mut m = HashMap::new();
        for l in b.split(|c| *c == b'\n') {
            let t = l.trim_ascii();
            if !t.is_empty() {
                *m.entry(t).or_insert(0) += 1;
            }
        }
        m
    }
    let (o, n) = (counts(old), counts(new));
    o.iter().any(|(l, c)| n.get(l).copied().unwrap_or(0) < *c)
}

/// Writes `content` as a backup of `file` and prunes old versions. Returns the backup path.
///
/// # Errors
/// I/O errors from the store. Pruning problems are ignored (the backup itself is safe).
pub fn write_backup(
    store: &mut dyn FileStore,
    file: &GraphPath,
    content: &[u8],
    at: SystemTime,
) -> io::Result<GraphPath> {
    let rel = backup_path(file, at);
    let path = GraphPath::new(&rel).map_err(io::Error::other)?;
    store.write(&path, content)?;
    prune(store, file);
    Ok(path)
}

fn prune(store: &mut dyn FileStore, file: &GraphPath) {
    let Ok(dir) = GraphPath::new(&backup_dir(file)) else {
        return;
    };
    let Ok(mut names) = store.list(&dir) else {
        return;
    };
    names.sort();
    let excess = names.len().saturating_sub(KEEP);
    for n in names.into_iter().take(excess) {
        if let Ok(p) = GraphPath::new(&format!("{}/{n}", dir.as_str())) {
            let _ = store.remove(&p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn timestamp_format() {
        let t = UNIX_EPOCH + Duration::from_millis(1_763_112_612_345);
        assert_eq!(timestamp(t), "2025-11-14T09_30_12.345Z");
        assert_eq!(timestamp(UNIX_EPOCH), "1970-01-01T00_00_00.000Z");
        let leap = UNIX_EPOCH + Duration::from_secs(1_709_164_800); // 2024-02-29
        assert!(timestamp(leap).starts_with("2024-02-29T00_00_00"));
    }

    #[test]
    fn path_layout() {
        let f = GraphPath::new("pages/foo.md").expect("p");
        let t = UNIX_EPOCH + Duration::from_millis(1_763_112_612_345);
        assert_eq!(
            backup_path(&f, t),
            "logseq/bak/pages/foo/2025-11-14T09_30_12.345Z.Desktop.md"
        );
        let nested = GraphPath::new("pages/a/b.md").expect("p");
        assert_eq!(backup_dir(&nested), "logseq/bak/pages/a/b");
        let j = GraphPath::new("journals/2025_11_14.md").expect("p");
        assert_eq!(backup_dir(&j), "logseq/bak/journals/2025_11_14");
    }

    #[test]
    fn removal_detection() {
        assert!(removes_text(b"- a\n- b\n", b"- a\n"));
        assert!(!removes_text(b"- a\n", b"- a\n- b\n"));
        assert!(!removes_text(b"- a\n\n", b"- a\n"));
        assert!(removes_text(b"- a\n- a\n", b"- a\n"));
    }
}
