//! Resolution API for the visual resolver (BIT-US-0054) and bulk actions (BIT-SP-0006.R15/R16).
//!
//! [`plan_resolution`] turns "this conflict, that choice" into work-tree changes that the engine
//! applies through the graph writer. It never writes markers: an `Edit` containing marker lines
//! is refused, and a resolution never leaves a marker line that was not already in the file.

use bitacora_merge::{Choice, ConflictKind, PageConflict, Side, add_page_alias, resolve_conflict};

use crate::merge::edn::set_config_value;
use crate::merge::markers::has_marker_line;
use crate::merge::{ConflictRecord, ConflictType, Resolution};
use crate::state::SyncState;
use crate::writer::FileChange;

/// Why a resolution could not be applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    /// No pending merge.
    #[error("there is no pending merge")]
    NoPendingMerge,
    /// Unknown conflict id.
    #[error("no conflict `{0}`")]
    UnknownConflict(String),
    /// Already decided.
    #[error("conflict `{0}` is already resolved")]
    AlreadyResolved(String),
    /// The block or key is gone from the file (the user edited it meanwhile): open the editor.
    #[error("`{0}` can no longer be located in the file; resolve it with an edit")]
    NotLocatable(String),
    /// The choice does not fit this kind of conflict.
    #[error("{0}")]
    Unsupported(String),
    /// The supplied text would put conflict markers into a file, or is not valid for the file.
    #[error("{0}")]
    InvalidEdit(String),
    /// A file changed on disk since it was read, or the writer is busy: try again.
    #[error("the graph changed while resolving ({0}); try again")]
    Busy(String),
    /// The writer or git failed.
    #[error("{0}")]
    Failed(String),
}

/// Result of resolving one conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveOutcome {
    /// Conflicts still waiting for the user.
    pub remaining: usize,
    /// Engine state afterwards (`Idle` when the last conflict was resolved and the merge pushed).
    pub state: SyncState,
}

fn read_text(
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
    path: &str,
) -> Result<Option<(Vec<u8>, String)>, ResolveError> {
    match read(path) {
        None => Ok(None),
        Some(b) => match String::from_utf8(b.clone()) {
            Ok(s) => Ok(Some((b, s))),
            Err(_) => Err(ResolveError::Unsupported(format!(
                "`{path}` is not UTF-8 text"
            ))),
        },
    }
}

fn write_if_changed(
    path: &str,
    current: Option<(Vec<u8>, String)>,
    new: String,
) -> Result<Vec<FileChange>, ResolveError> {
    let was = current.as_ref().map(|(_, s)| s.as_str());
    if was == Some(new.as_str()) {
        return Ok(Vec::new());
    }
    if has_marker_line(&new) && !was.is_some_and(has_marker_line) {
        return Err(ResolveError::InvalidEdit(
            "conflict markers are never written into the graph".to_owned(),
        ));
    }
    Ok(vec![FileChange::Write {
        path: path.to_owned(),
        content: new.into_bytes(),
        expected: current.map(|(b, _)| b),
    }])
}

/// Work-tree changes that apply `resolution` to `rec`. `read` returns the current bytes of a
/// graph-relative path. An empty result means the work tree already holds the chosen state.
pub fn plan_resolution(
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
    rec: &ConflictRecord,
    resolution: &Resolution,
) -> Result<Vec<FileChange>, ResolveError> {
    if let Resolution::Edit(s) = resolution
        && has_marker_line(s)
    {
        return Err(ResolveError::InvalidEdit(
            "the edit contains conflict marker lines".to_owned(),
        ));
    }
    let current = || read_text(read, &rec.path);
    match rec.kind {
        ConflictType::Content | ConflictType::Property | ConflictType::DeleteVsModify => {
            let Some((bytes, text)) = current()? else {
                return Err(ResolveError::NotLocatable(rec.path.clone()));
            };
            let kind = match rec.kind {
                ConflictType::Property => ConflictKind::Property,
                ConflictType::DeleteVsModify => ConflictKind::DeleteVsModify,
                _ => ConflictKind::Content,
            };
            let pc = PageConflict {
                block_key: rec.block_key.clone(),
                breadcrumb: rec.breadcrumb.clone(),
                conflict: bitacora_merge::Conflict {
                    kind,
                    field: rec.field.clone().unwrap_or_else(|| "content".to_owned()),
                    base: rec.base.clone(),
                    ours: rec.ours.clone(),
                    theirs: rec.theirs.clone(),
                },
            };
            let new = resolve_conflict(&text, &pc, resolution)
                .ok_or_else(|| ResolveError::NotLocatable(rec.path.clone()))?;
            write_if_changed(&rec.path, Some((bytes, text)), new)
        }
        ConflictType::Config => {
            let Some((bytes, text)) = current()? else {
                return Err(ResolveError::NotLocatable(rec.path.clone()));
            };
            let whole = rec.field.as_deref() == Some("file");
            let new = match (resolution, whole) {
                (Resolution::Ours | Resolution::Both, _) => return Ok(Vec::new()),
                (Resolution::Theirs, true) => rec.theirs.clone().unwrap_or_default(),
                (Resolution::Edit(s), true) => {
                    if bitacora_config::read_str(s).is_err() {
                        return Err(ResolveError::InvalidEdit(
                            "the text is not valid EDN".to_owned(),
                        ));
                    }
                    s.clone()
                }
                (Resolution::Theirs, false) => {
                    set_config_value(&text, &rec.breadcrumb, rec.theirs.as_deref())
                        .ok_or_else(|| ResolveError::NotLocatable(rec.breadcrumb.join(" ")))?
                }
                (Resolution::Edit(s), false) => set_config_value(&text, &rec.breadcrumb, Some(s))
                    .ok_or_else(|| {
                    ResolveError::InvalidEdit("the value is not valid EDN".to_owned())
                })?,
            };
            write_if_changed(&rec.path, Some((bytes, text)), new)
        }
        ConflictType::Text => {
            let current = current()?;
            let new = match resolution {
                Resolution::Ours | Resolution::Both => return Ok(Vec::new()),
                Resolution::Theirs => rec.theirs.clone().unwrap_or_default(),
                Resolution::Edit(s) => s.clone(),
            };
            write_if_changed(&rec.path, current, new)
        }
        ConflictType::ExternalMarkers => match resolution {
            Resolution::Edit(s) => write_if_changed(&rec.path, current()?, s.clone()),
            _ => Err(ResolveError::Unsupported(
                "a file with unbalanced conflict markers must be fixed with an edit".to_owned(),
            )),
        },
        ConflictType::FileDeleteVsModify => {
            let on_disk = read(&rec.path);
            if let Resolution::Edit(s) = resolution {
                return write_if_changed(&rec.path, current()?, s.clone());
            }
            let delete = matches!(
                (resolution, rec.deleted_by),
                (Resolution::Ours, Some(Side::Ours)) | (Resolution::Theirs, Some(Side::Theirs))
            );
            if !delete || on_disk.is_none() {
                return Ok(Vec::new());
            }
            Ok(vec![FileChange::Delete {
                path: rec.path.clone(),
                expected: on_disk,
            }])
        }
        ConflictType::RenameRename => {
            let (Some(ours_title), Some(theirs_title)) =
                (rec.breadcrumb.first(), rec.breadcrumb.get(1))
            else {
                return Err(ResolveError::NotLocatable(rec.path.clone()));
            };
            let Some((bytes, text)) = current()? else {
                return Err(ResolveError::NotLocatable(rec.path.clone()));
            };
            match resolution {
                Resolution::Ours => Ok(Vec::new()),
                Resolution::Both => match add_page_alias(&text, theirs_title) {
                    Some(new) => write_if_changed(&rec.path, Some((bytes, text)), new),
                    None => Ok(Vec::new()),
                },
                Resolution::Theirs => {
                    let Some(target) = rec.theirs.as_deref() else {
                        return Err(ResolveError::NotLocatable(rec.path.clone()));
                    };
                    let content = add_page_alias(&text, ours_title).unwrap_or_else(|| text.clone());
                    Ok(vec![
                        FileChange::Delete {
                            path: rec.path.clone(),
                            expected: Some(bytes),
                        },
                        FileChange::Write {
                            path: target.to_owned(),
                            content: content.into_bytes(),
                            expected: read(target),
                        },
                    ])
                }
                Resolution::Edit(_) => Err(ResolveError::Unsupported(
                    "a rename conflict is resolved by choosing a title".to_owned(),
                )),
            }
        }
    }
}

/// Convenience for the UI: the choice that keeps the work tree as it is.
pub fn keep_mine() -> Choice {
    Choice::Ours
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::collections::HashMap;

    fn rec(kind: ConflictType, path: &str) -> ConflictRecord {
        ConflictRecord {
            id: "c-0001".into(),
            path: path.into(),
            kind,
            field: None,
            block_key: None,
            breadcrumb: Vec::new(),
            base: None,
            ours: None,
            theirs: None,
            deleted_by: None,
            suggestion: None,
            ours_commit: None,
            theirs_commit: None,
            theirs_author: None,
            theirs_time: None,
            resolution: None,
        }
    }

    fn fs(files: &[(&str, &str)]) -> impl Fn(&str) -> Option<Vec<u8>> {
        let m: HashMap<String, Vec<u8>> = files
            .iter()
            .map(|(p, c)| ((*p).to_owned(), c.as_bytes().to_vec()))
            .collect();
        move |p| m.get(p).cloned()
    }

    fn written(changes: &[FileChange]) -> String {
        match &changes[0] {
            FileChange::Write { content, .. } => String::from_utf8(content.clone()).unwrap(),
            other => panic!("not a write: {other:?}"),
        }
    }

    #[test]
    fn block_content_theirs() {
        let mut r = rec(ConflictType::Content, "pages/P.md");
        r.field = Some("content".into());
        r.breadcrumb = vec!["Beta ours".into()];
        r.ours = Some("Beta ours".into());
        r.theirs = Some("Beta theirs".into());
        let read = fs(&[("pages/P.md", "- Alpha\n- Beta ours\n")]);
        let c = plan_resolution(&read, &r, &Resolution::Theirs).unwrap();
        assert_eq!(written(&c), "- Alpha\n- Beta theirs\n");
        assert!(
            plan_resolution(&read, &r, &Resolution::Ours)
                .unwrap()
                .is_empty()
        );
        let err = plan_resolution(&read, &r, &Resolution::Edit("<<<<<<< x\n".into())).unwrap_err();
        assert!(matches!(err, ResolveError::InvalidEdit(_)));
        let gone = fs(&[("pages/P.md", "- other\n")]);
        assert!(matches!(
            plan_resolution(&gone, &r, &Resolution::Theirs),
            Err(ResolveError::NotLocatable(_))
        ));
    }

    #[test]
    fn config_key_and_whole_file() {
        let mut r = rec(ConflictType::Config, "logseq/config.edn");
        r.field = Some("config".into());
        r.breadcrumb = vec!["a".into()];
        r.theirs = Some("3".into());
        let read = fs(&[("logseq/config.edn", ";; c\n{:a 2 :b 1}\n")]);
        let c = plan_resolution(&read, &r, &Resolution::Theirs).unwrap();
        assert_eq!(written(&c), ";; c\n{:a 3 :b 1}\n");
        r.field = Some("file".into());
        r.theirs = Some("{:z 1}".into());
        let c = plan_resolution(&read, &r, &Resolution::Theirs).unwrap();
        assert_eq!(written(&c), "{:z 1}");
        assert!(plan_resolution(&read, &r, &Resolution::Edit("{".into())).is_err());
    }

    #[test]
    fn file_delete_vs_modify_both_directions() {
        let mut r = rec(ConflictType::FileDeleteVsModify, "pages/Q.md");
        let read = fs(&[("pages/Q.md", "- q2\n")]);
        // We deleted, they modified (restored): "Keep deleted" is Ours.
        r.deleted_by = Some(Side::Ours);
        let c = plan_resolution(&read, &r, &Resolution::Ours).unwrap();
        assert!(matches!(&c[0], FileChange::Delete { path, .. } if path == "pages/Q.md"));
        assert!(
            plan_resolution(&read, &r, &Resolution::Theirs)
                .unwrap()
                .is_empty()
        );
        // They deleted, we modified: accepting their deletion is Theirs.
        r.deleted_by = Some(Side::Theirs);
        assert!(
            plan_resolution(&read, &r, &Resolution::Ours)
                .unwrap()
                .is_empty()
        );
        let c = plan_resolution(&read, &r, &Resolution::Theirs).unwrap();
        assert!(matches!(&c[0], FileChange::Delete { .. }));
    }

    #[test]
    fn rename_rename_picks_a_title_with_alias() {
        let mut r = rec(ConflictType::RenameRename, "pages/A.md");
        r.breadcrumb = vec!["A".into(), "B".into()];
        r.theirs = Some("pages/B.md".into());
        let read = fs(&[("pages/A.md", "- body\n")]);
        let c = plan_resolution(&read, &r, &Resolution::Both).unwrap();
        assert_eq!(written(&c), "alias:: [[B]]\n\n- body\n");
        let c = plan_resolution(&read, &r, &Resolution::Theirs).unwrap();
        assert!(matches!(&c[0], FileChange::Delete { path, .. } if path == "pages/A.md"));
        assert!(matches!(&c[1], FileChange::Write { path, content, .. }
            if path == "pages/B.md" && content.starts_with(b"alias:: [[A]]")));
    }

    #[test]
    fn external_markers_need_an_edit() {
        let r = rec(ConflictType::ExternalMarkers, "pages/P.md");
        let read = fs(&[("pages/P.md", "<<<<<<< x\n- a\n")]);
        assert!(matches!(
            plan_resolution(&read, &r, &Resolution::Theirs),
            Err(ResolveError::Unsupported(_))
        ));
        let c = plan_resolution(&read, &r, &Resolution::Edit("- a\n".into())).unwrap();
        assert_eq!(written(&c), "- a\n");
    }
}
