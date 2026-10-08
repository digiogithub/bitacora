# BIT-US-0177: per-graph SSH private key

Part of [[2-0-x-owner-feedback-plan]]. Related: [[BIT-US-0176-askpass-multicall]], [[git-sync-merge]].

## What changed
- `SyncPrefs.ssh_key: Option<PathBuf>` (`bitacora-app/src/sync_prefs.rs`), serde default, so old
  `sync.json` files load unchanged. Only the path is stored; passphrases stay in the keychain.
- `CliConfig.ssh_key` + `ssh_command()` (`bitacora-sync/src/backend/cli.rs`): git runs with
  `GIT_SSH_COMMAND=ssh -i '<path>' -o IdentitiesOnly=yes` (single-quoted, embedded quotes escaped,
  Windows backslashes turned into `/`). Without a key the user's own `GIT_SSH_COMMAND` is untouched.
- libgit2 push (`git2_push::push`, `GixBackend::with_ssh_key`, wired in
  `bitacora-runtime/src/live.rs`): an explicit key replaces agent/default keys; the passphrase comes
  from the existing `ssh-passphrase` provider path.
- `SSH_AUTH_HINT` in `classify_failure` (publickey denied, host key refused, no usable ssh
  credentials) and in the libgit2 error mapping: suggests setting the key in Settings > Sync.
- Settings > Sync "SSH key" row (shown only for SSH remotes) with Choose... (native file dialog,
  `SettingsEvent::SetSshKey`) and Clear; changing it restarts the session when sync is on. The
  enable form keeps the key set in Settings. "Forget saved credentials" also drops the key passphrase.
- en/es strings in `assets/locales/settings.*.yml`.

## Verification
- Tests: `ssh_command` quoting, fake-git env check for `GIT_SSH_COMMAND` (unix), hint classification,
  prefs back-compat and roundtrip.
- `cargo clippy -p bitacora-sync -p bitacora-runtime -p bitacora-app --all-targets -D warnings`;
  `cargo test` for the same crates all pass (app lib 627).
- Not verified: real SSH server, Windows quoting, visual rendering of the new row.
- Gap: the onboarding/clone dialog still uses the default ssh config (the key is set afterwards in Settings).
