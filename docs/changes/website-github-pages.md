---
created_at: 2026-10-08T08:43:45.299098212Z
updated_at: 2026-10-08T08:43:45.299098212Z
tags:
    - changes
    - website
    - ci
---
# Static website on GitHub Pages

**What:** Added `website/` (static landing page copied from `bitacora-design-system/website`, without the `design/` canvas sources) and `.github/workflows/pages.yml` (configure-pages v6, upload-pages-artifact v5, deploy-pages v5; triggers on push to `main` touching `website/**` and on `workflow_dispatch`).

**Link changes in `website/index.html`:**
- Nav "Docs" replaced by "Downloads" (`#download`).
- Hero ghost button "Read the user guide" → "All releases on GitHub" (`/releases`).
- Download cards → `https://github.com/digiogithub/bitacora/releases/latest`; Windows format set to `.exe / .msi` (NSIS + MSI from release.yml), Linux lists Flatpak, .deb, AppImage.
- Docs/user-guide links removed; footer Resources = Releases, Changelog (`blob/main/CHANGELOG.md`), Source code, Pando.

**Why:** User request: publish the designed site on Pages; no docs or user guide yet.

**Verified:** served locally with `python3 -m http.server`, all assets 200, rendered in browser, no console errors.

**Pending:** enable Settings → Pages → Source "GitHub Actions"; `/releases/latest` 404s until a non-draft, non-prerelease release exists (v2.0.1 is still a draft).

Related: [[release-pipeline]] [[bitacora-design-system]]
