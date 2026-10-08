---
created_at: 2026-10-08T09:51:14.340496694Z
updated_at: 2026-10-08T09:51:14.340496694Z
tags:
    - changes
    - release
    - ci
---
# Release 2.0.2 and the beta MSI fix

**Problem:** release run for `v2.0.2-beta.1` failed in `windows (Authenticode)` → "Bundle (NSIS, MSI)": cargo-packager error "Optional build metadata in app version must be numeric-only and cannot be greater than 65535 for msi target". WiX/MSI needs numeric versions.

**Fix:** `.github/workflows/release.yml` windows job gets `CHANNEL` from `meta`; bundle step builds `nsis,wix` for stable and `nsis` only for beta tags. `docs/design/release-process.md` notes it.

**Also:** merged dependabot PR #7 (`azure/login` v2 → v3, same inputs). Bumped workspace to 2.0.2 (`Cargo.toml`, `Cargo.lock`), added CHANGELOG 2.0.2, Flatpak metainfo `<release version="2.0.2">`, website hero version.

**Verified:** CI, build-app (3 OS, including Windows `build.rs` icon embedding) and pages green on main `356f9f7`; tag `v2.0.2` release run 37756038734 all jobs success; draft release has dmg, msi, NSIS exe, deb, AppImage, flatpak, CLI archives, SHA256SUMS.

Related: [[app-icon-design-system]] [[website-github-pages]] [[release-process]]
