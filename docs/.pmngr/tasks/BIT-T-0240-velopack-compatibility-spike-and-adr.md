---
id: BIT-T-0240
type: task
title: Velopack compatibility spike and ADR
status: done
priority: high
parent: BIT-US-0100
milestone: BIT-M-0002
author: mcp
labels: [auto-update, spike, bitacora-app, adr]
estimate: 3
created: 2026-10-06T14:31:45Z
updated: 2026-10-06T20:18:19Z
closed: 2026-10-06T20:18:19Z
---

## Description
On a branch, integrate `velopack = "1.2"` minimally (`VelopackApp::build().run()` at the very top of `main`) and package with `vpk pack` for macOS (signed + notarized `.app`), Windows and Linux (AppImage), starting from the cargo-packager bundles. Test: install v0.0.1 -> publish v0.0.2 to a test GitHub release -> update -> relaunch, on each OS. Check: Gatekeeper after update (signature/notarization preserved), coexistence or conflict with cargo-packager MSI/dmg (does Velopack require its own installer?), delta size, per-user vs per-machine installs, deb users (likely notice-only). Write findings into `docs/design/auto-update.md` and record the per-OS outcome against ADR-018 in [[architecture]] §5 (amend the ADR row if an OS must fall back to notice-only); update the open question in [[crate-stack]].

Scheduled early (M1) per ADR-018 so packaging choices are validated before the release pipeline is built.

## Acceptance Criteria
- Decision table per OS/format (Velopack / notice-only) with evidence.
- ADR-018 confirmed or amended; docs cross-linked.

## Notes
- ADR-018 (Velopack, validated by an early spike against cargo-packager bundles; fallback = update notice + GitHub Releases link).
- [[crate-stack]] §4.1 (velopack 1.2.161), Risk R7, Open questions; [[gpui-and-gpui-kit]] §1.8.
