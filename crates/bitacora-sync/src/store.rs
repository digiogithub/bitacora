//! Persisted merge state: `.git/bitacora/merge-state.json` (design 4.5, BIT-SP-0006.R15/R21).
//!
//! The file holds the commits of the pending merge, every conflict record with the user's
//! resolution so far, and the resolution memo. It lives inside `.git`, so it is never part of the
//! graph, and is written atomically (temp file, fsync, rename) so a crash cannot corrupt it.

use std::io::Write;
use std::path::{Path, PathBuf};

use bitacora_merge::Side;
use serde::{Deserialize, Serialize};

use crate::backend::Oid;
use crate::merge::{ConflictRecord, ConflictType, MemoEntry, Resolution};
use crate::state::{MergeStateStore, PendingMerge};

const FILE_VERSION: u32 = 1;
const MEMO_CAP: usize = 2000;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ResolutionDto {
    Ours,
    Theirs,
    Both,
    Edit(String),
}

impl From<&Resolution> for ResolutionDto {
    fn from(r: &Resolution) -> Self {
        match r {
            Resolution::Ours => Self::Ours,
            Resolution::Theirs => Self::Theirs,
            Resolution::Both => Self::Both,
            Resolution::Edit(s) => Self::Edit(s.clone()),
        }
    }
}

impl From<ResolutionDto> for Resolution {
    fn from(r: ResolutionDto) -> Self {
        match r {
            ResolutionDto::Ours => Self::Ours,
            ResolutionDto::Theirs => Self::Theirs,
            ResolutionDto::Both => Self::Both,
            ResolutionDto::Edit(s) => Self::Edit(s),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ConflictDto {
    id: String,
    path: String,
    kind: String,
    field: Option<String>,
    block_key: Option<String>,
    breadcrumb: Vec<String>,
    base: Option<String>,
    ours: Option<String>,
    theirs: Option<String>,
    deleted_by: Option<String>,
    suggestion: Option<String>,
    ours_commit: Option<String>,
    theirs_commit: Option<String>,
    theirs_author: Option<String>,
    theirs_time: Option<i64>,
    resolution: Option<ResolutionDto>,
}

#[derive(Serialize, Deserialize)]
struct MemoDto {
    path: String,
    kind: String,
    block_key: Option<String>,
    field: Option<String>,
    base_hash: String,
    ours_hash: String,
    theirs_hash: String,
    result_hash: Option<String>,
    resolution: ResolutionDto,
}

#[derive(Serialize, Deserialize)]
struct StateFile {
    version: u32,
    base: Option<String>,
    ours: String,
    theirs: String,
    pending_commit: String,
    external: bool,
    conflicts: Vec<ConflictDto>,
    memo: Vec<MemoDto>,
}

fn side_str(s: Side) -> &'static str {
    match s {
        Side::Ours => "ours",
        Side::Theirs => "theirs",
    }
}

fn to_file(p: &PendingMerge) -> StateFile {
    StateFile {
        version: FILE_VERSION,
        base: p.base.as_ref().map(|o| o.as_hex().to_owned()),
        ours: p.ours.as_hex().to_owned(),
        theirs: p.theirs.as_hex().to_owned(),
        pending_commit: p.pending_commit.as_hex().to_owned(),
        external: p.external,
        conflicts: p
            .conflicts
            .iter()
            .map(|c| ConflictDto {
                id: c.id.clone(),
                path: c.path.clone(),
                kind: c.kind.as_str().to_owned(),
                field: c.field.clone(),
                block_key: c.block_key.clone(),
                breadcrumb: c.breadcrumb.clone(),
                base: c.base.clone(),
                ours: c.ours.clone(),
                theirs: c.theirs.clone(),
                deleted_by: c.deleted_by.map(|s| side_str(s).to_owned()),
                suggestion: c.suggestion.clone(),
                ours_commit: c.ours_commit.clone(),
                theirs_commit: c.theirs_commit.clone(),
                theirs_author: c.theirs_author.clone(),
                theirs_time: c.theirs_time,
                resolution: c.resolution.as_ref().map(ResolutionDto::from),
            })
            .collect(),
        memo: p
            .memo
            .iter()
            .map(|m| MemoDto {
                path: m.path.clone(),
                kind: m.kind.as_str().to_owned(),
                block_key: m.block_key.clone(),
                field: m.field.clone(),
                base_hash: m.base_hash.clone(),
                ours_hash: m.ours_hash.clone(),
                theirs_hash: m.theirs_hash.clone(),
                result_hash: m.result_hash.clone(),
                resolution: ResolutionDto::from(&m.resolution),
            })
            .collect(),
    }
}

fn from_file(f: StateFile) -> Option<PendingMerge> {
    if f.version != FILE_VERSION {
        return None;
    }
    let oid = |s: &str| Oid::from_hex(s).ok();
    let mut conflicts = Vec::with_capacity(f.conflicts.len());
    for c in f.conflicts {
        conflicts.push(ConflictRecord {
            id: c.id,
            path: c.path,
            kind: ConflictType::parse(&c.kind)?,
            field: c.field,
            block_key: c.block_key,
            breadcrumb: c.breadcrumb,
            base: c.base,
            ours: c.ours,
            theirs: c.theirs,
            deleted_by: match c.deleted_by.as_deref() {
                Some("ours") => Some(Side::Ours),
                Some("theirs") => Some(Side::Theirs),
                _ => None,
            },
            suggestion: c.suggestion,
            ours_commit: c.ours_commit,
            theirs_commit: c.theirs_commit,
            theirs_author: c.theirs_author,
            theirs_time: c.theirs_time,
            resolution: c.resolution.map(Resolution::from),
        });
    }
    let mut memo = Vec::with_capacity(f.memo.len());
    for m in f.memo {
        memo.push(MemoEntry {
            path: m.path,
            kind: ConflictType::parse(&m.kind)?,
            block_key: m.block_key,
            field: m.field,
            base_hash: m.base_hash,
            ours_hash: m.ours_hash,
            theirs_hash: m.theirs_hash,
            result_hash: m.result_hash,
            resolution: m.resolution.into(),
        });
    }
    if memo.len() > MEMO_CAP {
        memo.drain(..memo.len() - MEMO_CAP);
    }
    Some(PendingMerge {
        base: match f.base.as_deref() {
            Some(b) => Some(oid(b)?),
            None => None,
        },
        ours: oid(&f.ours)?,
        theirs: oid(&f.theirs)?,
        pending_commit: oid(&f.pending_commit)?,
        conflicts,
        memo,
        external: f.external,
    })
}

/// [`MergeStateStore`] backed by `<git dir>/bitacora/merge-state.json`.
#[derive(Debug, Clone)]
pub struct JsonMergeStore {
    path: PathBuf,
}

impl JsonMergeStore {
    /// Store for the repository whose git directory is `git_dir`.
    pub fn new(git_dir: &Path) -> Self {
        Self {
            path: git_dir.join("bitacora").join("merge-state.json"),
        }
    }

    /// Store for the graph at `graph` (`None` when it has no git directory).
    pub fn for_graph(graph: &Path) -> Option<Self> {
        crate::repo_setup::git_dir_of(graph).map(|g| Self::new(&g))
    }

    /// The state file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, pending: &PendingMerge) -> std::io::Result<()> {
        let dir = self.path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_vec_pretty(&to_file(pending)).map_err(std::io::Error::other)?;
        let tmp = dir.join("merge-state.json.tmp");
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&json)?;
        f.sync_all()?;
        std::fs::rename(&tmp, &self.path)
    }
}

impl MergeStateStore for JsonMergeStore {
    fn load(&self) -> Option<PendingMerge> {
        let bytes = std::fs::read(&self.path).ok()?;
        match serde_json::from_slice::<StateFile>(&bytes)
            .ok()
            .and_then(from_file)
        {
            Some(p) => Some(p),
            None => {
                // Keep the unreadable file for inspection instead of silently dropping conflicts.
                let _ = std::fs::rename(&self.path, self.path.with_extension("json.corrupt"));
                None
            }
        }
    }

    fn save(&mut self, pending: &PendingMerge) {
        // A failed write leaves the previous state in place; the engine re-saves on the next
        // change, and the pending-merge ref still anchors the commits.
        let _ = self.write(pending);
    }

    fn clear(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn sample() -> PendingMerge {
        let oid = |n: u8| Oid::from_hex(&format!("{:040x}", u128::from(n))).unwrap();
        let rec = |id: &str, kind, res| ConflictRecord {
            id: id.to_owned(),
            path: "pages/P.md".into(),
            kind,
            field: Some("content".into()),
            block_key: Some("u1".into()),
            breadcrumb: vec!["P".into(), "Beta".into()],
            base: Some("b".into()),
            ours: Some("o".into()),
            theirs: None,
            deleted_by: Some(Side::Theirs),
            suggestion: Some("alias:: [[X]]".into()),
            ours_commit: Some("a".into()),
            theirs_commit: Some("b".into()),
            theirs_author: Some("phone".into()),
            theirs_time: Some(1_700_000_000),
            resolution: res,
        };
        let c1 = rec("c-0001", ConflictType::Content, None);
        let c2 = rec(
            "c-0002",
            ConflictType::FileDeleteVsModify,
            Some(Resolution::Edit("a \"quoted\"\nline".into())),
        );
        PendingMerge {
            base: Some(oid(1)),
            ours: oid(2),
            theirs: oid(3),
            pending_commit: oid(4),
            memo: vec![MemoEntry::of(&c2, c2.resolution.as_ref().unwrap())],
            conflicts: vec![c1, c2],
            external: false,
        }
    }

    #[test]
    fn roundtrip_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = JsonMergeStore::new(dir.path());
        assert!(store.load().is_none());
        let p = sample();
        store.save(&p);
        assert!(store.path().ends_with("bitacora/merge-state.json"));
        assert_eq!(store.load(), Some(p));
        store.clear();
        assert!(store.load().is_none());
        assert!(!store.path().exists());
    }

    #[test]
    fn unreadable_state_is_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonMergeStore::new(dir.path());
        std::fs::create_dir_all(dir.path().join("bitacora")).unwrap();
        std::fs::write(store.path(), "not json").unwrap();
        assert!(store.load().is_none());
        assert!(
            dir.path()
                .join("bitacora/merge-state.json.corrupt")
                .exists()
        );
    }

    #[test]
    fn resolution_serialises_like_the_design() {
        let json = serde_json::to_string(&ResolutionDto::Ours).unwrap();
        assert_eq!(json, "\"ours\"");
        let json = serde_json::to_string(&ResolutionDto::Edit("x".into())).unwrap();
        assert_eq!(json, "{\"edit\":\"x\"}");
    }
}
