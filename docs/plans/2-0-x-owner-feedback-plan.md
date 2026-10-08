# 2.0.x owner feedback plan

Owner feedback after installing 2.0.2 (2026-10-08). Milestone BIT-M-0010, epic BIT-EP-0026. Continues [[bitacora-v2-plan]] and [[owner-manual-validation-checklist]].

## Findings (code map, 2026-10-08)

| Story | Finding |
|---|---|
| BIT-US-0172 empty today in feed | Virtual today has no rows; feed empty-state text (`views/journals.rs` `render_entry`) has no click handler and `ensure_editor` only builds an editor when Loaded; `on_new_block`/`on_indent` no-op without an editing row. Page view `Item::Empty` same. |
| BIT-US-0173 tasks vanish | No confirmed root cause. Candidates: `act` reloads before flush (stale read); index events reach Tasks only while it is the current route (`main_view.rs` `on_index_event`); selection pill persists across `show`; `TaskRow::from_item` silently drops; `reload` error keeps stale/empty model. |
| BIT-US-0174 version in footer | `views/status_bar.rs` has no version; `CARGO_PKG_VERSION` only in logging/update. |
| BIT-US-0175 gear icon | Settings reachable from app menu, palette, Pando popover; no gear in `views/workspace/top_bar.rs`. |
| BIT-US-0176 HTTPS creds | Askpass + keyring chain already exists (`bitacora-sync` `askpass.rs`, `credentials.rs`; app `credentials.rs`). **Root cause**: `askpass_helper_path()` looks for a `bitacora-askpass` binary next to the exe, but release/packaging only builds and ships `bitacora` (cargo-packager `binaries = [bitacora]`), so `GIT_ASKPASS` is empty and `GIT_TERMINAL_PROMPT=0` makes git fail silently. Fix: make the `bitacora` executable itself act as askpass when launched with a marker env var (multi-call), so nothing extra must be packaged. |
| BIT-US-0177 SSH key | No key setting anywhere; add per-graph `ssh_key` to `SyncPrefs`, `GIT_SSH_COMMAND` for CLI, `Cred::ssh_key` for git2. Passphrase through existing askpass `ssh-passphrase` store. |
| BIT-US-0178 today's tasks | Index has `agenda(today, days_ahead)` (no caller) and `task_groups` with `TaskFilter.markers` for DOING. Render under today's blocks in feed and page view. |
| BIT-US-0179 fold tool calls | `views/chat/render.rs`: tool cards individually collapsible, approval cards always expanded; no turn-level grouping. |
| BIT-US-0180 model selector | Pando already exposes generic `GET /api/v1/models` and AG-UI profiles with a `Model` override (`internal/agui/deps.go` Profile). Bitacora (managed mode) can generate one chat profile per enabled model in its managed `.pando.toml` and route the run to that profile. No Pando change ([[pando-generic-only]]). External mode: selector lists only profiles the server exposes via `/info`. |

## Phases (parallel worktree agents, sonnet)

1. Journals: US-0172 + US-0178.
2. Tasks + footer: US-0173 + US-0174.
3. Git auth: US-0176 + US-0177.
4. Shell: US-0175.
5. Agent: US-0179 + US-0180.

Coordinator merges each branch into `main`, runs fmt/clippy/tests, removes worktrees ([[worktree-cleanup]]), pushes.

## Progress
- [ ] Phase 1
- [ ] Phase 2
- [ ] Phase 3
- [ ] Phase 4
- [ ] Phase 5
