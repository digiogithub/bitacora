---
created_at: 2026-10-06T16:37:44.034184183Z
updated_at: 2026-10-06T16:37:44.034184183Z
tags:
    - plan
    - orchestration
    - backlog
---
# Plan: Full Bitacora development (BIT backlog M0 → M4)

Orchestrated by the main Claude Code session; implementation by Claude Code subagents (model sonnet). Owner decisions (2026-10-06):
- Commit on `main`, push to `origin` at each milestone close.
- Work not verifiable on this Linux host (macOS/Windows IME, signing, GitHub Releases, 3-OS CI) is implemented and left `in_review` with a comment listing the manual validation missing.
- Minimum system git = 2.38 ([[architecture]] ADR-022).
- Run M0 → M4 end to end; stop only for owner decisions.

## Conventions for agents
- Follow `AGENTS.md`. One story (or a small group of related stories in one crate) per agent.
- Gintrack: story + tasks → `in_progress` at start, `done` (or `in_review`) with comment at end; requirement traces via `update_requirement`.
- KB change record per story under `changes/<story-id>-<slug>.md`, linking [[bitacora-full-development-plan]].
- Conventional commits, `Refs: BIT-US-xxxx`. Agents do not commit `docs/.pmngr/**` or KB records (orchestrator commits them in batches) and do not push.

## Phases (waves)
### M0 — Foundations & spikes
1. US-0001 workspace skeleton (solo).
2. US-0002 quality guards, US-0011 fixtures, US-0014 app window.
3. US-0003 CI, US-0012 test toolkit, US-0013 upgrade watch, US-0024 tokio bridge.
4. US-0025 shell layout, US-0040 editor spike, US-0060 1,000-block spike, US-0072 IME report (in_review).
### M1 — Read-only viewer
- Markdown (EP-0003): 0026 → 0058 → 0059/0083/0084 → 0092 → 0093/0094 → 0095.
- Config/graph (EP-0004): 0056, 0071; 0027, 0081, 0085, 0091, 0088, 0098.
- Index (EP-0005): 0004 → 0005 → 0006 → 0007 → 0008/0009 → 0010.
- MCP (EP-0010 M1): 0015 → 0016 → 0017/0018 → 0019.
- UI (EP-0006): 0073 → 0074/0075 → 0076/0077/0078/0079/0080.
### M2 — Editor MVP
- Merge core (EP-0012 M2): 0049 → 0050 → 0051.
- Write path (EP-0008): 0062 → 0063/0064/0065 → 0066/0067 → 0068 → 0069 → 0070.
- Editor (EP-0007): 0029 → 0030/0031 → 0032..0039.
- Page lifecycle (EP-0009): 0028, 0057, 0061, 0082, 0087, 0089, 0096, 0099.
- MCP writes: 0020 → 0021/0022 → 0023.
### M3 — Git sync & merge
- Sync (EP-0011): 0041/0042 → 0043 → 0044 → 0045 → 0046/0047 → 0048.
- Merge (EP-0012 M3): 0052 → 0053 → 0054 → 0055.
### M4 — Release 1.0
- EP-0013: 0101 → 0102/0103; 0104..0109.
- EP-0014: 0086, 0090, 0097, 0100, 0110, 0111, 0112.

## Progress
- 2026-10-06: AGENTS.md committed (95ecf43). ADR-022 recorded. Starting M0 wave 1.
- Merged so far: US-0001/0002 (done); US-0011/0012/0003/0013 (in_review: remote CI/Windows/Logseq app); US-0056/0071 (done; rewrite-edn oracle pending); US-0081/0085/0091/0027 (in_review: Logseq black-box, missing journals/ignore-rules fixtures); US-0026/0058 (done); US-0015/0016 (in_review).

## Follow-up backlog (to close later)
- Core: committed fixtures for journals and ignore-rules; move US-0081/0085/0091/0027 to done once fixtures exist.
- MCP: start server from app/tray, OS keychain via keyring, Settings > Agents UI (BIT-T-0116), per-tool scope enforcement (with US-0021).
- Config: rewrite-edn oracle outputs (needs Babashka).
- Merge: BIT-SP-0006.R14 second scenario (write `id::` for newly referenced blocks in merge commit) — do in sync orchestration (US-0053). Metadata-only change vs delete keeps delete (documented risk).
- Requirements verification: needs JUnit via nextest + `gintrack spec ingest` (being prototyped in US-0095).
- Index: canonical_dump queries blocks_fts_tri_docsize (fails when search.substring off); search.substring config key + persistence (wire in settings US-0107); fuzzy title list cache for huge graphs.
- INTEGRATION (next after core write path): runtime wiring in a shared place used by app and cli — core queue observers → index writer + watch EchoFilter (register before rename), watch FileEvent → core ExternalFileChanged / index FsChange, sync GraphWriter adapter over core QueueLock, MCP GraphReader over index. BIT-T-0341 belongs here.
- Flaky tests to stabilise: bitacora-index tests/property.rs incremental_equals_rebuild_on_a_small_graph (failed once under parallel load); bitacora-mcp page_changes_notify_subscribed_resources_within_two_seconds (timing).
- Spec text: BIT-SP-0007.R11 and BIT-US-0018 criterion still say raw Datalog → INVALID_QUERY; MCP now executes the advanced Datalog subset. Update texts.
- App must adopt bitacora-runtime (next app story with editing); sync resolve_conflict UI (US-0054); config.edn hot reload; bak not indexed (fine).

## App UI backlog (pending app tasks collected from core/sync/mcp stories)
T-0084 today journal startup/rollover; T-0174 delete confirm; T-0157 rename merge dialog; T-0211 Agent activity view; T-0344/T-0349 external-change conflict notice + editing-block protection; T-0291 askpass modal; T-0282 sync onboarding UI; US-0047 status bar sync indicator/Sync now; US-0048 history view; US-0054 visual conflict resolver; T-0116 Settings > Agents; US-0030/0031 editor + keymap/IME; US-0032..0039 key bindings over core semantics; US-0096 assets paste/drop.
