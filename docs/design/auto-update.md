# Auto-update, single instance and crash reports

Implements BIT-US-0100, BIT-US-0086 and BIT-US-0111 (ADR-018, ADR-025). Code: `crates/bitacora-app/src/update/`, `crates/bitacora-runtime/src/{instance,crash}.rs`, `crates/bitacora-app/src/{instance,crash}.rs`.

## Update flow

* `main` calls `velopack::VelopackApp::build().run()` first (install/update hooks; no-op otherwise).
* At startup and from the palette command "Check for updates..." a background check runs, automatically at most once per day (`update-state.json` in the data dir) and only when `updates.enabled` is true in `settings.json` (`updates.channel`: `stable` | `beta`, beta also follows pre-releases).
* Velopack install (detected by `UpdateManager::new` succeeding): feed = `GithubSource` on the project repository; the update downloads in the background (delta when available); a sticky notice offers "Restart now", which hands the package to the Velopack updater (`wait_exit_then_apply_updates`) and dispatches `Quit`. The updater waits for the process to exit, so pending writes are flushed and sync stopped by the normal ordered shutdown before anything is replaced. A downloaded update is also applied at the next start.
* Any other install (AppImage, deb, MSI, dev build): `GET api.github.com/repos/<owner>/<repo>/releases`, newest semver newer than the running version honouring the channel, notice with a Download button opening the release page; one notice per version.
* Only traffic: the GitHub API request (no identifiers).

## Release workflow requirements (BIT-T-0243, not yet done)

The release job must, per OS, run `vpk pack` over the cargo-packager output (`--packId Bitacora --packVersion <tag> --channel <os-default>`; pre-releases published as GitHub pre-releases) and `vpk upload github` so the release carries `releases.<channel>.json`, the full and delta `.nupkg`s and `RELEASES`. Until then every install uses the notice-only path.

## Single instance

`<data_dir>/instance.lock` (exclusive `File::try_lock`, freed by the kernel on crash) and `instance.json` `{pid, kind, version, ipc_port, token}`. Desktop launch: lock free -> owner (listens on `127.0.0.1:<ephemeral>`); lock held by an app -> forward `{graph, page}` with the token and exit 0; held by `bitacora-cli serve` -> error. `serve` refuses to start when anyone holds the lock. Clean exit removes `instance.json`; finding one with a free lock means the previous owner crashed. The `keep_running_in_background` setting keeps the process (and MCP) alive after the last window closes; a forwarded launch then reopens a window. Smoke-test and spike runs skip the lock.

## Crash reports

Panic hook -> `<data_dir>/crashes/<timestamp>-<pid>.json` (version, OS, thread, message, location, backtrace, last 80 log lines; graph folders -> `<graph:NAME>`, home -> `~`). Abnormal exits produce a report from the log tail. Next start: sticky notice -> dialog with the report preview, "Open GitHub issue" (prefilled `issues/new` URL, the user reviews and submits) or "Copy report"; Esc dismisses. Shown reports move to `crashes/dismissed/`. `bitacora-cli` writes the same format and prints the path.

## Requirements and open questions

* MUST NOT upload anything automatically.
* Panic messages and log lines are free text: only paths are redacted, the user reviews before sharing.
* Open: Velopack with notarized `.app` / MSIX; verify the real update on a published release (see BIT-US-0100 validation steps).
* Open: settings UI toggles (BIT-EP-0013); today edit `settings.json`.
