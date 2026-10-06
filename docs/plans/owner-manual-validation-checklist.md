---
tags: [plan, validation, owner]
---
# Owner manual validation checklist

Items left `in_review` because they cannot be validated from the Linux build host. Linked from [[bitacora-full-development-plan]].

## A. Workflows to trigger (agent token lacks admin rights for workflow_dispatch)
- Coverage → BIT-T-0017 / BIT-US-0012
- GPUI Kit canary → BIT-T-0028 / BIT-US-0013
- bundle → BIT-T-0178 / BIT-US-0090 (AppImage untested)
- flatpak → BIT-T-0271 / BIT-US-0112
- release with throwaway tag (e.g. `v0.1.1-beta.0` via `cargo xtask bump`), then delete draft+tag → BIT-T-0218/US-0097, BIT-T-0256/US-0110 (CLI archives, ldd, self-update), BIT-T-0243/T-0242/US-0100 (vpk pack/upload, real update v0.1.0→v0.1.1)
- Second main CI run for warm vs cold cache numbers → BIT-T-0011 / BIT-US-0003

## B. Signing secrets (see BIT-US-0090 comment, docs/design/release-process.md)
- macOS notarization (BIT-T-0176), Windows Authenticode via Azure Trusted Signing (BIT-T-0177)

## C. Manual desktop checks
- Window on macOS, Windows, Linux Wayland from build-app artifacts → BIT-US-0014
- Live OS appearance switch → BIT-T-0050 / BIT-US-0025
- `bitacora --spike-bench` on macOS, Windows, real-GPU Linux → BIT-US-0060
- IME checklist docs/design/ime-test-checklist.md on 3 OS (+ ADR-002 go/no-go) → BIT-US-0072, T-0105, T-0106
- Git auth matrix (macOS keychain, Windows GCM, ssh) → BIT-T-0292 / BIT-US-0046
- Flatpak sandbox (portals, ssh-agent, Secret Service, MCP) GNOME/KDE → BIT-T-0272

## D. Logseq 0.10.15 desktop / Clojure oracle
- triple-lowbar codec outputs → BIT-US-0081
- date parser leniency vs cljs-time → BIT-US-0091
- rename cascade golden outputs → BIT-T-0131 / BIT-US-0082
- query DSL + advanced query results → BIT-T-0301 / US-0101, BIT-T-0308 / US-0103
- open edge-case fixtures in Logseq → BIT-T-0014 / BIT-US-0011
- rewrite-edn oracle for config editor goldens (needs Babashka)
