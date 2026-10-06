---
id: BIT-T-0177
type: task
title: Windows Authenticode signing of binaries and installers
status: backlog
priority: high
parent: BIT-US-0090
milestone: BIT-M-0005
author: mcp
labels: [packaging, signing, windows, bitacora-app]
estimate: 2
created: 2026-10-06T14:30:54Z
updated: 2026-10-06T14:30:54Z
---

## Description
Sign `bitacora.exe`, `bitacora-cli.exe`, the MSI and the NSIS installer using Azure Trusted Signing (`azure/trusted-signing-action`) with SHA-256 and an RFC 3161 timestamp; wire it as cargo-packager's `signCommand` (or post-process in `cargo xtask bundle --sign`). Fallback documented for a PFX certificate via `signtool`. Set installer metadata (publisher "Digio", product URL, upgrade code stable across versions for MSI upgrades).

## Acceptance Criteria
- `Get-AuthenticodeSignature` shows `Valid` for every shipped executable and installer.
- Installing a newer MSI upgrades in place (same UpgradeCode) without duplicate entries in Apps & Features.

## Notes
- [[crate-stack]] §5.2 (Azure Trusted Signing); [[gpui-and-gpui-kit]] §1.8.
