---
created_at: 2026-10-08T08:09:50.567061403Z
updated_at: 2026-10-08T08:09:50.567061403Z
tags:
    - changes
    - bitacora
    - release
    - signing
    - ci
---
# Bitacora signing setup and signed tag release (2026-10-08)

Repo: github.com/digiogithub/bitacora (moved from sevir/bitacora). Same signing scheme as [[git-in-track]] and Pando release pipelines, via `digiogithub/ci-actions@v1`.

## GitHub configuration (verified with gh)
- Repository variables: AZURE_CLIENT_ID, AZURE_TENANT_ID, AZURE_SUBSCRIPTION_ID, AZURE_SIGNING_ENDPOINT (https://weu.codesigning.azure.net/), AZURE_SIGNING_ACCOUNT (digio-art-sign-acc), AZURE_SIGNING_CERT_PROFILE (digio).
- Repository secret: MACOS_SIGNING_BUNDLE (base64 tar.gz of DIGIO_Software_Signing_Keys: Developer ID .p12 files + kvagerc). No loose APPLE_* secrets.
- Environment: `release` (no protection rules, no deployment branch policy yet).
- Azure: Entra app pando-github-trusted-signing, federated credential subject repo:digiogithub/bitacora:environment:release (+ immutable variant). OIDC, no client secret.

## Workflow changes (bitacora commit 1177160, local, not pushed)
- `.github/workflows/release.yml`: jobs meta, linux, macos, windows, flatpak, release.
  - macos: macos-signing-keychain -> cargo build -> `APPLE_SIGNING_IDENTITY=$SIGNING_IDENTITY cargo xtask bundle --no-build --formats app,dmg` (cargo-packager 0.11.8 signs with hardened runtime + timestamp from the keychain search list; skips its own notarization without APPLE_ID) -> verify runtime flag -> macos-codesign CLI binaries -> macos-notarize (.dmg stapled, CLI submit-only) -> keychain cleanup.
  - windows: `environment: release`, `id-token: write`, azure/login@v2 with vars, azure/trusted-signing-action@v2.0.0 (inputs unchanged from v0; CLI credential enabled by default) on target/release exes before packaging and target/packager exe,msi after; Get-AuthenticodeSignature must be Valid.
  - release: downloads `dist-*` only, SHA256SUMS, notes, provenance attestations, draft release (docs/design/release-process.md MUST: releases are drafts).
- `.github/workflows/bundle.yml`: unsigned PR/manual dry run, no secrets, no workflow_call.
- `.github/workflows/flatpak.yml`: inputs `ref`, `artifact-name` (release passes dist-flatpak).
- `docs/design/release-process.md` §3/§4 rewritten.

## Verification
actionlint clean on the three workflows; YAML parses. Not yet exercised on GitHub: needs push + a test pre-release tag (e.g. v2.0.2-beta.1, needs `cargo xtask bump` first since meta requires tag == workspace version).

## Pending
- Push and test tag: awaiting José's confirmation.
- Recommended: restrict environment `release` deployment to tags `v*` so no branch workflow can mint an OIDC token that signs.
- trusted-signing-action v2.0.0 vs v0 (Pando uses v0): if the first run fails at auth, fall back to @v0.

Related: [[changes/2026-10-08-repo-move-digiogithub-and-dependabot.md]], [[release-process]]
