# Release checklist: Bitacora 2.0.0

Outward-facing steps: they await the owner. Pipeline background: [[release-process]]. Owner validation items: [[owner-manual-validation-checklist]]. Story BIT-US-0163, task BIT-T-0487.

## 1. Before tagging
- [ ] All v2 stories done or consciously deferred (note BIT-US-0151/0152 journal review and recommendation views, and BIT-US-0164 colour schemes; update `docs/user/ai-features.md` and `CHANGELOG.md` to match what ships).
- [ ] `main` green: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, `cargo deny check`, `cargo xtask fixtures verify`, `cargo xtask tokens --check`, `cargo xtask check-deps`.
- [ ] Manual checks that agents could not do (see [[owner-manual-validation-checklist]], [[frameless-checklist]], [[ime-test-checklist]] C21-C23): frameless window on macOS and Windows, macOS menu bar, real Pando run (managed and external), look in Light and Dark, graph export dialogs.
- [ ] Upgrade test with a real 1.x profile (settings, layout, index) on each OS.
- [ ] Review `docs/user/*` against the shipped UI (screenshots optional).

## 2. Version and changelog
- [ ] The workspace version in `Cargo.toml` is still `0.1.0`: confirm the intended 2.0.0 numbering with the owner, then `cargo xtask bump 2.0.0` (also refreshes `Cargo.lock`).
- [ ] Replace "Unreleased" in `CHANGELOG.md` with the date; commit `chore(release): 2.0.0`.

## 3. Tag and pipeline (owner only)
- [ ] `git tag v2.0.0` and push the tag (`release.yml` checks tag == workspace version).
- [ ] CI builds bundles for 3 OSes (smoke tests install/launch/uninstall), Flatpak and CLI archives, writes `SHA256SUMS`, attests provenance, creates a **draft** release.
- [ ] Signing secrets present in the repo (Apple Developer ID and notarization, Azure Trusted Signing); otherwise artifacts are unsigned: decide whether that is acceptable.
- [ ] Review the generated notes (`cargo xtask release-notes 2.0.0`), merge with `CHANGELOG.md`, publish the draft by hand.

## 4. After publishing
- [ ] Auto-update feed and `bitacora-cli self-update` see 2.0.0 (`self-update --check`).
- [ ] Install scripts (`packaging/install`) work against the release.
- [ ] Flathub PR switched to the tagged source (BIT-T-0272).
- [ ] Update README status and announce.
- [ ] Move BIT-T-0487 and BIT-US-0163 to done.

## Open questions
- Version numbering (workspace is 0.1.0 while docs speak of 1.x).
- Whether 2.0 ships with the journal review and recommendation views.
