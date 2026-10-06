---
id: BIT-T-0176
type: task
title: macOS code signing, hardened runtime and notarization in CI
status: in_review
priority: high
parent: BIT-US-0090
milestone: BIT-M-0005
author: mcp
labels: [packaging, signing, macos, bitacora-app]
estimate: 3
created: 2026-10-06T14:30:54Z
updated: 2026-10-06T19:59:48Z
started: 2026-10-06T19:59:48Z
---

## Description
Sign the `.app` with a Developer ID Application certificate (imported into a temporary keychain in CI from base64 secrets), hardened runtime and an `entitlements.plist` (network server for the MCP port is allowed by default for Developer ID apps; add `com.apple.security.cs.allow-jit` only if wgpu/Metal needs it — verify), sign nested binaries, build the dmg, submit with `xcrun notarytool submit --wait` using an App Store Connect API key, then `xcrun stapler staple`. Decide universal (`lipo` of aarch64 + x86_64) vs per-arch dmgs and record it.

## Acceptance Criteria
- `spctl --assess --type execute -v Bitacora.app` reports "accepted, source=Notarized Developer ID" on a clean Mac.
- Downloaded dmg opens without Gatekeeper warnings.
- Secrets never printed in logs; keychain deleted at job end.

## Notes
- [[crate-stack]] §5.2 (macOS signing secrets); [[gpui-and-gpui-kit]] §1.8 (Zed `script/bundle-mac` reference — read for ideas only, GPL scripts not copied, ADR-014). Velopack compatibility with notarized apps is checked in the auto-update story.
