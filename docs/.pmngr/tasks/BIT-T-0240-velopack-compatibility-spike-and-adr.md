---
id: BIT-T-0240
type: task
title: Velopack compatibility spike and ADR
status: backlog
priority: high
parent: BIT-US-0100
milestone: BIT-M-0005
author: mcp
labels: [auto-update, spike, bitacora-app, adr]
estimate: 3
created: 2026-10-06T14:31:45Z
updated: 2026-10-06T14:31:45Z
---

## Description
On a branch, integrate `velopack = "1.2"` minimally (`VelopackApp::build().run()` at the very top of `main`) and package with `vpk pack` for macOS (signed + notarized `.app`), Windows and Linux (AppImage). Test: install v0.0.1 -> publish v0.0.2 to a test GitHub release -> update -> relaunch, on each OS. Check: Gatekeeper after update (signature/notarization preserved), coexistence or conflict with cargo-packager MSI/dmg (does Velopack require its own installer?), delta size, per-user vs per-machine installs, deb users (likely notice-only). Write findings into `docs/design/auto-update.md` and add an ADR row in [[architecture]] §5 with the decision per OS; update the open decision in [[architecture]] §7 and the open question in [[crate-stack]].

## Acceptance Criteria
- Decision table per OS/format (Velopack / notice-only) with evidence.
- ADR row added; docs cross-linked.

## Notes
- [[crate-stack]] §4.1 (velopack 1.2.161), Risk R7, Open questions; [[gpui-and-gpui-kit]] §1.8.
