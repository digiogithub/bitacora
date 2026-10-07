---
id: BIT-US-0090
type: story
title: Signed installable bundles with cargo-packager for macOS, Windows and Linux
status: done
priority: high
parent: BIT-EP-0014
milestone: BIT-M-0005
author: mcp
labels: [release, packaging, bitacora-app]
estimate: 8
created: 2026-10-06T14:30:31Z
updated: 2026-10-07T08:21:31Z
started: 2026-10-06T19:59:49Z
closed: 2026-10-07T08:21:31Z
---

## Description
As a user, I want to install Bitacora with a normal installer for my OS — without Gatekeeper or SmartScreen warnings — so that I can use it without a Rust toolchain.

## Acceptance Criteria
- `cargo xtask bundle` produces: macOS `.app` + `.dmg` (Developer ID signed, notarized, stapled; universal or per-arch), Windows MSI and NSIS `.exe` (Authenticode / Azure Trusted Signing), Linux `.deb` and AppImage, all with app icon, name "Bitacora", identifier `es.digio.bitacora` and version from the workspace.
- Installed app launches, registers a `.desktop` entry / Start menu shortcut, and uninstalls cleanly (user graphs and data dirs untouched).
- Signing secrets are only used in CI on tagged builds; unsigned local bundles work for development.
- Linux bundles document the Vulkan requirement.

## Notes
- [[crate-stack]] §5.2 (`bundle` job: cargo packager, dmg signed+notarized, msi/NSIS signed, deb + AppImage), §5.1 (`xtask`: bundle, sign).
- [[gpui-and-gpui-kit]] §1.8 (cargo-packager 0.11.8; Zed's bundle scripts as reference for codesigning, notarization and `.desktop`), §1.5 (Vulkan on Linux).
