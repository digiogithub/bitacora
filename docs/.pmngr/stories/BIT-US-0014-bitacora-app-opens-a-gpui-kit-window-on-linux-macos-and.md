---
id: BIT-US-0014
type: story
title: bitacora-app opens a GPUI Kit window on Linux, macOS and Windows
status: done
priority: critical
parent: BIT-EP-0002
milestone: BIT-M-0001
author: mcp
labels: [ui, bitacora-app]
estimate: 5
created: 2026-10-06T14:26:47Z
updated: 2026-10-07T08:21:31Z
started: 2026-10-06T16:46:30Z
closed: 2026-10-07T08:21:31Z
---

## Description
As a developer, I want the `bitacora` binary to start a GPUI Kit application with a Root view, logging and platform directories, and CI to publish runnable builds for the three OSes, so that every later UI story builds on a proven entry point and anyone can try the app without a local toolchain.

## Acceptance Criteria
- `cargo run -p bitacora-app -- --graph <path>` opens a window titled "Bitacora — <graph name>" using GPUI Kit 0.7's one-window entry point with `Root` as the window root.
- GPUI is reached only through `gpui_kit::gpui`; components are accessed through a thin `bitacora_app::ui` facade.
- Logs go to stderr and a rolling file in the platform cache dir (`directories::ProjectDirs::from("es", "Digio", "Bitacora")`).
- CI uploads per-OS app build artifacts; the window opens from those artifacts on Linux (X11 and Wayland), macOS and Windows (manual check recorded).

## Notes
- [[gpui-and-gpui-kit]] §1.1, §1.6, §1.7, Risks R1/R2; [[crate-stack]] §4.1 (`directories`, `tracing`).
- ADR-001. Epic acceptance: "App window opens on the 3 OSes from CI artifacts".
