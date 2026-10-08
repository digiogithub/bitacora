# BIT-US-0174: App version in the footer

Part of [[2-0-x-owner-feedback-plan]].

- `crates/bitacora-app/src/views/status_bar.rs`: `version_label()` (`v` + `CARGO_PKG_VERSION`) rendered on the
  right of `AppStatusBar::render` in the theme `muted` colour; the demo heartbeat counter is no longer
  displayed (the `StatusEvent::Heartbeat` plumbing stays, tests use it).
- Test `version_tests::the_footer_shows_the_cargo_package_version`.
- Verified: `cargo test -p bitacora-app --locked --lib status_bar`. Visual check not possible on this host.
