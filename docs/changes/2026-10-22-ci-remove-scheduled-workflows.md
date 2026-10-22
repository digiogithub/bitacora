---
created_at: 2026-10-22T07:56:39.226543177Z
updated_at: 2026-10-22T07:56:39.226543177Z
tags:
    - changes
    - ci
---
# CI: remove scheduled (cron) workflows

Date: 2026-10-22

## What changed
- `.github/workflows/audit.yml`: removed daily `schedule` (cron `23 5 * * *`). Now runs on `workflow_dispatch`, `pull_request` and `push` to `main`, both limited to `Cargo.lock`, `deny.toml` and the workflow file.
- `.github/workflows/coverage.yml`: removed weekly `schedule` (cron `17 4 * * 1`); manual only.
- `.github/workflows/gpui-kit-canary.yml`: removed weekly `schedule` (cron `41 6 * * 1`); manual only (`workflow_dispatch` with optional `version`).

## Why
Owner received near-daily GitHub notifications from scheduled runs. Policy: CI runs only on pushes/PRs (`ci.yml`, `build-app.yml`, `pages.yml`, `bundle.yml`, `audit.yml`) and on release tags `v*` (`release.yml`). Everything else is manual.

## Verification
All workflow files parse as YAML; `grep cron .github/workflows` returns nothing.

Related: [[release-process]], [[ci]]
