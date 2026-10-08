# Release process and packaging

Design for BIT-US-0090 (bundles), BIT-US-0097 (release pipeline), BIT-US-0110 (`bitacora-cli` archives and self-update) and BIT-US-0112 (Flatpak). Background: [[crate-stack]] §5.2, ADR-018 in [[architecture]].

## 1. Artifacts

| Artifact | Built by | Signing |
|---|---|---|
| macOS `.app` + `.dmg` | `cargo xtask bundle` (cargo-packager 0.11.8) | Developer ID + notarization (optional) |
| Windows NSIS `.exe` + WiX `.msi` | same | Azure Trusted Signing (optional) |
| Linux `.deb`, `.AppImage` | same | none (checksums + provenance) |
| `bitacora-cli-<version>-<target>.tar.gz\|zip` | `release.yml` `cli` job | macOS codesign + notarize, Windows Trusted Signing (optional) |
| `bitacora.flatpak` | `flatpak.yml` | Flathub signs |
| `install.sh`, `install.ps1` | `packaging/install/` | verify SHA-256 before installing |
| `SHA256SUMS` | `cargo xtask sha256sums` | GitHub build-provenance attestation on every other file |

CLI targets: `x86_64`/`aarch64-unknown-linux-gnu`, `aarch64`/`x86_64-apple-darwin`, `x86_64-pc-windows-msvc`. musl is not built: vendored OpenSSL/libgit2 plus `keyring` need a musl toolchain; revisit if requested. The Linux CLI job fails if `ldd` shows X11/Wayland/Vulkan/fontconfig/ALSA libraries.

## 2. Packager configuration

Static config: `[package.metadata.packager]` in `crates/bitacora-app/Cargo.toml` (name `Bitacora`, identifier `es.digio.bitacora`, icons from `packaging/icons/`, deb dependencies incl. the Vulkan loader). The version comes from the workspace. Icons are our own artwork (`packaging/icons/bitacora.svg`); `packaging/make-icons.sh` regenerates PNG/ICO/ICNS.

`cargo xtask bundle [--no-build] [--formats a,b] [--target T]` builds the app, converts the metadata to a packager JSON config, overlays signing settings from the environment (only when set) and runs `cargo packager`. Output: `target/packager/`. Local unsigned bundles need no secrets. Install once: `cargo install cargo-packager --locked`.

Linux bundles require a Vulkan driver (`libvulkan1` plus a Mesa/vendor ICD); the `.deb` declares it and the AppImage does not bundle GPU drivers.

## 3. Pipeline (`release.yml`)

On a `v*` tag (or `workflow_dispatch` with an existing `tag`): `meta` (tag must equal the workspace version, computes channel: `-beta.N`/`-rc.N` = pre-release/beta) → per-platform jobs, each uploading its final files as a `dist-*` artifact:

- `linux`: `bitacora-cli` for x86_64 and aarch64 (no GUI libraries linked) and the `.deb`/`.AppImage` (x86_64). Unsigned: Linux has no OS gatekeeper.
- `macos`: `bitacora-cli` for aarch64 and x86_64 and the `.app`/`.dmg`. cargo-packager signs the app (hardened runtime, secure timestamp) and the `.dmg` with the Developer ID identity; the `.dmg` is notarized and stapled, the CLI binaries are codesigned and notarized submit-only (a bare Mach-O cannot carry a ticket).
- `windows`: `bitacora.exe` and `bitacora-cli.exe` are signed before packaging, then the NSIS/MSI installers; every file must report `Get-AuthenticodeSignature` = `Valid` or the job fails.
- `flatpak` (`flatpak.yml`, artifact `dist-flatpak`).

→ `release`: downloads only `dist-*` artifacts (so nothing unsigned can be published), writes `SHA256SUMS`, generates notes from conventional commits since the previous tag (`cargo xtask release-notes`), attests provenance (`actions/attest-build-provenance`) and creates a **draft** release (`--prerelease` for beta tags). A maintainer publishes it manually.

`bundle.yml` is an unsigned dry run (pull requests touching packaging, manual runs) with install/launch/uninstall smoke tests; it never sees signing material.

Cutting a release: `cargo xtask bump X.Y.Z`, commit, `git tag vX.Y.Z`, push the tag, review the draft, publish.

## 4. Signing material

Same scheme as `digiogithub/git-in-track` and `digiogithub/pando` (composite actions from `digiogithub/ci-actions@v1`).

| Where | Name | Used for |
|---|---|---|
| repository secret | `MACOS_SIGNING_BUNDLE` | base64 `.tar.gz` of `DIGIO_Software_Signing_Keys`: Developer ID Application/Installer `.p12` files plus the `kvagerc` env file (certificate passwords, notary Apple ID / team / app password). `macos-signing-keychain` unpacks it into an ephemeral keychain and exports `SIGNING_IDENTITY` and `NOTARY_PROFILE`. |
| repository variables | `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID` | Entra app used by `azure/login` over OIDC (no client secret) |
| repository variables | `AZURE_SIGNING_ENDPOINT`, `AZURE_SIGNING_ACCOUNT`, `AZURE_SIGNING_CERT_PROFILE` | Azure Artifact Signing account (`digio-art-sign-acc`) and certificate profile (`digio`) |
| environment | `release` | the only job with `id-token: write` that runs in it is `windows`; the Entra app's federated credential is scoped to `repo:digiogithub/bitacora:environment:release` |

`xtask bundle` still understands `APPLE_SIGNING_IDENTITY`, `APPLE_CERTIFICATE`/`_PASSWORD`, `APPLE_ID`/`APPLE_PASSWORD`/`APPLE_TEAM_ID` and `WINDOWS_CERTIFICATE_THUMBPRINT` / `WINDOWS_SIGN_COMMAND` for local or self-hosted signing.

## 5. `bitacora-cli self-update`

`bitacora-cli self-update [--check] [--prerelease] [--force] [--api-url URL]` lists GitHub releases (`<api>/releases`), picks the newest non-draft release newer than the running version (pre-releases only with `--prerelease`), downloads `bitacora-cli-<version>-<target>.(tar.gz|zip)` and `SHA256SUMS`, **refuses to install unless the SHA-256 matches**, extracts the binary and replaces the running executable atomically (temp file, rename; on Windows the old binary is renamed aside). It refuses for Flatpak (`FLATPAK_ID`), the desktop bundle (`.app/Contents`), Nix, Homebrew, `/usr/bin`, snap and Windows packaged apps. Integrity beyond the checksum comes from TLS to GitHub plus the provenance attestations (`gh attestation verify`); a detached signature scheme (minisign) is deliberately left out until the project has a release key (see Open questions). Tests run against a local fake release server (`crates/bitacora-cli/src/cmd/self_update_tests.rs`).

## 6. Flatpak

`packaging/flatpak/es.digio.bitacora.yml` (runtime Freedesktop 25.08, Rust and LLVM SDK extensions), desktop file and AppStream metainfo. Cargo sources are generated from `Cargo.lock` with `flatpak-cargo-generator.py` (not committed). Permissions: Wayland/X11, DRI, network (git sync, MCP on localhost reachable from host clients), SSH agent, Secret Service, `xdg-documents`; other graph folders via the file chooser portal. No `git` is bundled (ADR-020) so the gix backend is used unless a host git is visible. Flathub submission: open a PR against `flathub/flathub` with the manifest switched to the tagged git source and `cargo-sources.json` committed beside it.

## Requirements

- MUST: signing material is only used by `release.yml` (tag builds); unsigned local and PR bundles work.
- MUST: the release job publishes only `dist-*` artifacts; macOS and Windows jobs fail rather than upload unsigned files.
- MUST: the tag equals the workspace version; releases are drafts.
- MUST: `self-update` verifies a SHA-256 from the release before replacing the binary and refuses managed installs.

## Open questions

- Add a minisign/ed25519 release key for detached signatures of `SHA256SUMS`.
- Velopack (ADR-018) for the desktop auto-update is separate work; cargo-packager bundles are the input to its spike.
- Linux CLI musl builds.
- Verify on real hardware: macOS notarization, Windows SmartScreen reputation, Flatpak portals/SSH/MCP inside the sandbox (BIT-T-0272).
