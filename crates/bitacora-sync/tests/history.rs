//! Per-page history over real repositories (BIT-US-0048, BIT-SP-0006.R22).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use bitacora_merge::DiffKind;
use bitacora_sync::backend::{CliConfig, CommitKind, select_backend};
use bitacora_sync::history::{PageChange, diff_version, page_history, version_text};
use common::{KINDS, World, onboarding_config};

#[test]
fn history_follows_renames_and_reports_trailers_and_diffs() {
    for kind in KINDS {
        let w = World::new(kind);
        let mut a = w.first_device(&[("pages/Old.md", "- alpha block text\n- beta block text\n")]);
        a.write(
            "pages/Old.md",
            "- alpha block text\n- beta block text edited\n",
        );
        a.engine.sync_now();
        a.git(&["mv", "pages/Old.md", "pages/New.md"]);
        a.write(
            "pages/New.md",
            "- alpha block text\n- beta block text edited\n- gamma block text\n",
        );
        a.engine.sync_now();
        a.write("pages/Other.md", "- unrelated\n");
        a.engine.sync_now();

        let cfg = onboarding_config(kind, "alice");
        let backend = select_backend(&cfg.detection, &a.dir, CliConfig::default()).unwrap();
        let tip = backend.resolve_ref("HEAD").unwrap().unwrap();
        let h = page_history(backend.as_ref(), &tip, "pages/New.md", 50).unwrap();
        // newest first: the rename+edit, the edit, the creation (by the onboarding commit).
        assert_eq!(h.len(), 3, "{kind:?}: {h:#?}");
        assert!(matches!(h[0].change, PageChange::Renamed { ref from } if from == "pages/Old.md"));
        assert_eq!(h[0].path, "pages/New.md");
        assert_eq!(h[1].change, PageChange::Modified);
        assert_eq!(h[1].path, "pages/Old.md", "{kind:?}");
        assert_eq!(h[2].change, PageChange::Added);
        assert_eq!(h[0].device.as_deref(), Some("alice"));
        assert_eq!(h[0].kind, Some(CommitKind::Auto));
        assert_eq!(h[2].kind, Some(CommitKind::Migrate), "{kind:?}");

        // Versions and block-level diff against the current text.
        let current = a.read("pages/New.md");
        let v_first = version_text(backend.as_ref(), &h[2]).unwrap().unwrap();
        assert_eq!(v_first, "- alpha block text\n- beta block text\n");
        let d = diff_version(&v_first, &current);
        let kinds: Vec<DiffKind> = d.visible(false).map(|b| b.kind).collect();
        assert!(kinds.contains(&DiffKind::Changed), "{kind:?}: {d:?}");
        assert!(kinds.contains(&DiffKind::Added), "{kind:?}: {d:?}");
        let v_mid = version_text(backend.as_ref(), &h[1]).unwrap().unwrap();
        assert_eq!(diff_version(&v_mid, &v_mid).visible(true).count(), 0);

        // A page with no history, and the limit.
        assert!(
            page_history(backend.as_ref(), &tip, "pages/Nope.md", 10)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            page_history(backend.as_ref(), &tip, "pages/New.md", 1)
                .unwrap()
                .len(),
            1
        );
    }
}
