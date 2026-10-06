//! Push through libgit2 for the gix-only backend (ADR-023).
//!
//! gix 0.88 has no push client, so when no system git is usable this module pushes `HEAD` to
//! `refs/heads/<branch>` with the `git2` crate. Everything else stays on gix. All `git2` API usage
//! is isolated here (`git2-push` cargo feature).
//!
//! Authentication:
//! * HTTPS: the [`CredentialProvider`] (OS keyring, then the in-app prompt) supplies username and
//!   password/token; a rejected credential is forgotten and the attempt count is bounded.
//! * SSH: ssh-agent first, then the default keys in `~/.ssh` (`id_ed25519`, `id_ecdsa`, `id_rsa`),
//!   asking the provider for a passphrase when needed. libssh2 does not read `~/.ssh/config`;
//!   users who rely on host aliases / ProxyJump must install git (the UI suggests it).
//!
//! The push is never forced.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use git2::{Cred, CredentialType, ErrorClass, ErrorCode, PushOptions, RemoteCallbacks, Repository};

use super::{GitError, PushOutcome, Result};
use crate::credentials::{CredentialKind, CredentialProvider, CredentialRequest};

/// Maximum credential callback invocations per kind before giving up (libgit2 retries forever).
const MAX_ATTEMPTS: u32 = 3;

const AUTH_HINT: &str = "install git for full credential support";

fn map_err(e: &git2::Error) -> GitError {
    let msg = e.message().to_string();
    match (e.code(), e.class()) {
        (ErrorCode::Auth, _) | (_, ErrorClass::Ssh)
            if !msg.to_ascii_lowercase().contains("connect") =>
        {
            GitError::Auth {
                hint: Some(AUTH_HINT.into()),
            }
        }
        (ErrorCode::NotFastForward, _) => GitError::NonFastForward,
        (ErrorCode::Certificate, _) => GitError::Network(msg),
        _ => match super::classify_failure(&msg) {
            GitError::Auth { .. } => GitError::Auth {
                hint: Some(AUTH_HINT.into()),
            },
            GitError::Other { .. } if matches!(e.class(), ErrorClass::Net | ErrorClass::Http) => {
                GitError::Network(msg)
            }
            other => other,
        },
    }
}

fn default_ssh_keys() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) else {
        return Vec::new();
    };
    let dir = Path::new(&home).join(".ssh");
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .iter()
        .map(|n| dir.join(n))
        .filter(|p| p.is_file())
        .collect()
}

/// Pushes `HEAD` of the repository at `repo_path` to `refs/heads/<branch>` on `remote_name`.
pub(crate) fn push(
    repo_path: &Path,
    remote_name: &str,
    branch: &str,
    creds: Option<&Arc<dyn CredentialProvider>>,
) -> Result<PushOutcome> {
    let repo = Repository::open(repo_path).map_err(|_| GitError::NotARepo)?;
    let head = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .ok_or_else(|| GitError::other("nothing to push: HEAD has no commits"))?;
    let tracking = format!("refs/remotes/{remote_name}/{branch}");
    let before = repo.refname_to_id(&tracking).ok();
    let mut remote = repo
        .find_remote(remote_name)
        .map_err(|e| GitError::other(e.message()))?;

    let rejected: Cell<Option<String>> = Cell::new(None);
    let https_attempts = Cell::new(0u32);
    let agent_tried = Cell::new(false);
    let key_attempts = Cell::new(0u32);
    let keys = default_ssh_keys();

    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(|cb_url, user_from_url, allowed| {
        if allowed.contains(CredentialType::SSH_KEY) {
            let user = user_from_url.unwrap_or("git");
            if !agent_tried.replace(true)
                && let Ok(c) = Cred::ssh_key_from_agent(user)
            {
                return Ok(c);
            }
            let idx = key_attempts.get() as usize;
            if let Some(key) = keys.get(idx) {
                key_attempts.set(key_attempts.get() + 1);
                // Try without a passphrase first; an encrypted key then fails and we retry
                // with the provider's passphrase.
                if let Ok(c) = Cred::ssh_key(user, None, key, None)
                    && !is_encrypted(key)
                {
                    return Ok(c);
                }
                let req = CredentialRequest {
                    url: key.to_string_lossy().into_owned(),
                    username: Some(user.to_string()),
                    kind: CredentialKind::SshPassphrase,
                };
                if let Some(p) = creds.and_then(|c| c.credential(&req)) {
                    return Cred::ssh_key(user, None, key, Some(p.secret.expose()));
                }
                return Cred::ssh_key(user, None, key, None);
            }
            return Err(git2::Error::from_str("no usable ssh credentials"));
        }
        if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
            let attempt = https_attempts.get();
            let req = CredentialRequest {
                url: cb_url.to_string(),
                username: user_from_url.map(str::to_string),
                kind: CredentialKind::UserPassword,
            };
            if let Some(provider) = creds {
                if attempt > 0 {
                    // The previous answer was rejected by the server.
                    provider.rejected(&req);
                }
                if attempt >= MAX_ATTEMPTS {
                    return Err(git2::Error::from_str("authentication failed"));
                }
                https_attempts.set(attempt + 1);
                if let Some(c) = provider.credential(&req) {
                    return Cred::userpass_plaintext(&c.username, c.secret.expose());
                }
            }
            return Err(git2::Error::from_str(
                "authentication failed: no credentials",
            ));
        }
        if allowed.contains(CredentialType::DEFAULT) {
            return Cred::default();
        }
        Err(git2::Error::from_str("unsupported credential type"))
    });
    callbacks.push_update_reference(|refname, status| {
        if let Some(msg) = status {
            rejected.set(Some(format!("{refname}: {msg}")));
        }
        Ok(())
    });

    let mut opts = PushOptions::new();
    opts.remote_callbacks(callbacks);
    // Deliberately no leading '+': never forced.
    let spec = format!("HEAD:refs/heads/{branch}");
    let result = remote.push(&[spec.as_str()], Some(&mut opts));
    drop(opts);
    if let Some(msg) = rejected.take() {
        let lower = msg.to_ascii_lowercase();
        return Err(
            if lower.contains("non-fast-forward")
                || lower.contains("fetch first")
                || lower.contains("not fast")
                || lower.contains("stale")
            {
                GitError::NonFastForward
            } else {
                GitError::other(format!("push rejected: {msg}"))
            },
        );
    }
    result.map_err(|e| map_err(&e))?;

    repo.reference(&tracking, head, true, "push: update tracking ref")
        .map_err(|e| GitError::other(e.message()))?;
    Ok(PushOutcome {
        pushed: before != Some(head),
    })
}

/// Cheap check for a passphrase-protected key (PEM `ENCRYPTED` marker or new OpenSSH format
/// with a cipher other than `none`).
fn is_encrypted(key: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(key) else {
        return false;
    };
    if text.contains("ENCRYPTED") {
        return true;
    }
    if text.contains("BEGIN OPENSSH PRIVATE KEY") {
        let body: String = text.lines().filter(|l| !l.starts_with("-----")).collect();
        // base64 of "openssh-key-v1\0" + length-prefixed cipher name; "none" encodes to this.
        return !body.contains("AAAABG5vbmU");
    }
    false
}
