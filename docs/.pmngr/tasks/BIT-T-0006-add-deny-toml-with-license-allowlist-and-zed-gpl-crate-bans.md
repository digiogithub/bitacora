---
id: BIT-T-0006
type: task
title: Add deny.toml with license allowlist and Zed GPL crate bans
status: done
priority: critical
parent: BIT-US-0002
milestone: BIT-M-0001
author: mcp
labels: [infra, licensing, dependencies]
estimate: 2
created: 2026-10-06T14:24:49Z
updated: 2026-10-06T16:45:00Z
started: 2026-10-06T16:44:55Z
closed: 2026-10-06T16:45:00Z
---

## Description
Create `deny.toml` for `cargo-deny` (latest 0.18+ config format):
- `[licenses]`: allow `MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Zlib`, `Unicode-3.0`, `MPL-2.0` (file-level copyleft, acceptable for deps), `CC0-1.0`, `BSL-1.0`; everything else (notably `GPL-*`, `AGPL-*`, `LGPL-*`) is denied. Add `clarify` entries if a dependency (e.g. `ring`) needs them.
- `[bans]`: deny well-known GPL Zed crates by name if they ever appear on crates.io under the same names (`editor`, `ui`, `theme`, `workspace`, `language`, `project` are too generic — instead deny by license and add a comment), and `multiple-versions = "warn"`; add a `deny` entry for crates.io `gpui` (only `gpui-pre` via `gpui-kit` is allowed, ADR-001).
- `[advisories]`: RustSec DB, `yanked = "deny"`.
- `[sources]`: only crates.io; unknown git sources denied.

## Acceptance Criteria
- `cargo deny check` passes on the skeleton (including `gpui-kit` 0.7.1's tree).
- Temporarily adding a GPL-licensed crate in a scratch branch makes `cargo deny check licenses` fail (record the crate used in the PR description).
- Adding `gpui = "0.2"` directly fails `cargo deny check bans`.

## Notes
- ADR-001, ADR-014. [[gpui-and-gpui-kit]] §1.3 and Risk R5; [[crate-stack]] §5.2 `checks` job.
