//! Credential provider (BIT-T-0374, ADR-023).
//!
//! The provider chain is: a [`SecretStore`] (OS keyring, or in-memory for tests) first, then an
//! in-app [`PromptHandler`] (the UI modal; the seam is a trait so `bitacora-sync` stays UI-free).
//! The same chain serves the libgit2 push credential callback (gix-only backend) and the askpass
//! bridge (system git backend, see [`crate::askpass`]).
//!
//! Secrets never appear in `Debug` output or logs: [`Secret`] redacts itself.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

/// A secret string (password, token or passphrase) that redacts itself in `Debug`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wraps a secret.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The raw value; call only when handing it to git.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// What kind of credential is being asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialKind {
    /// HTTPS username and password / token.
    UserPassword,
    /// Passphrase of an SSH private key.
    SshPassphrase,
}

/// A credential request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialRequest {
    /// Remote URL (or key path for passphrases) the credential is for.
    pub url: String,
    /// Username already known (from the URL), if any.
    pub username: Option<String>,
    /// Kind of credential.
    pub kind: CredentialKind,
}

/// A resolved credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    /// Username (empty for passphrases).
    pub username: String,
    /// Password, token or passphrase.
    pub secret: Secret,
}

/// The user's answer to an in-app prompt.
#[derive(Debug, Clone)]
pub struct PromptAnswer {
    /// The credential entered.
    pub credential: Credential,
    /// Persist it in the secret store ("remember").
    pub remember: bool,
}

/// UI seam: asks the user for a credential. Returns `None` when the user cancels.
pub trait PromptHandler: Send + Sync {
    /// Shows a (blocking) prompt for `request`.
    fn prompt(&self, request: &CredentialRequest) -> Option<PromptAnswer>;
}

/// Persistent secret storage.
pub trait SecretStore: Send + Sync {
    /// Looks up a credential.
    fn get(&self, request: &CredentialRequest) -> Option<Credential>;
    /// Saves a credential.
    fn save(&self, request: &CredentialRequest, credential: &Credential);
    /// Forgets a credential (called when the server rejected it).
    fn forget(&self, request: &CredentialRequest);
}

/// Source of credentials for the git backends.
pub trait CredentialProvider: Send + Sync {
    /// Returns a credential for `request`, or `None` when none is available / the user cancelled.
    fn credential(&self, request: &CredentialRequest) -> Option<Credential>;
    /// Reports that `credential` was rejected, so it is not served again from storage.
    fn rejected(&self, request: &CredentialRequest);
}

fn store_key(request: &CredentialRequest) -> String {
    let kind = match request.kind {
        CredentialKind::UserPassword => "https",
        CredentialKind::SshPassphrase => "ssh-passphrase",
    };
    format!("{kind}:{}", request.url)
}

/// In-memory [`SecretStore`] (tests, and a fallback when no OS keyring is available).
#[derive(Default, Debug)]
pub struct MemoryStore {
    map: Mutex<HashMap<String, Credential>>,
}

impl MemoryStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, request: &CredentialRequest) -> Option<Credential> {
        let map = self.map.lock().unwrap_or_else(PoisonError::into_inner);
        map.get(&store_key(request)).cloned()
    }

    fn save(&self, request: &CredentialRequest, credential: &Credential) {
        let mut map = self.map.lock().unwrap_or_else(PoisonError::into_inner);
        map.insert(store_key(request), credential.clone());
    }

    fn forget(&self, request: &CredentialRequest) {
        let mut map = self.map.lock().unwrap_or_else(PoisonError::into_inner);
        map.remove(&store_key(request));
    }
}

/// [`SecretStore`] backed by the OS keyring (macOS Keychain, Windows Credential Manager, Secret
/// Service). The username and secret are stored together as `username\nsecret` under service
/// `bitacora-git` and the request key. Every failure degrades to "no stored credential".
#[cfg(feature = "keyring-store")]
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyringStore;

#[cfg(feature = "keyring-store")]
const KEYRING_SERVICE: &str = "bitacora-git";

#[cfg(feature = "keyring-store")]
impl KeyringStore {
    fn entry(request: &CredentialRequest) -> Option<keyring::Entry> {
        keyring::Entry::new(KEYRING_SERVICE, &store_key(request)).ok()
    }
}

#[cfg(feature = "keyring-store")]
impl SecretStore for KeyringStore {
    fn get(&self, request: &CredentialRequest) -> Option<Credential> {
        let raw = Self::entry(request)?.get_password().ok()?;
        let (username, secret) = raw.split_once('\n')?;
        Some(Credential {
            username: username.to_string(),
            secret: Secret::new(secret),
        })
    }

    fn save(&self, request: &CredentialRequest, credential: &Credential) {
        if let Some(entry) = Self::entry(request) {
            let _ = entry.set_password(&format!(
                "{}\n{}",
                credential.username,
                credential.secret.expose()
            ));
        }
    }

    fn forget(&self, request: &CredentialRequest) {
        if let Some(entry) = Self::entry(request) {
            let _ = entry.delete_credential();
        }
    }
}

/// Store first, prompt second; a cancelled prompt is remembered so a failing sync does not
/// re-open the modal on every retry ("no retry storm") until [`ChainProvider::reset_cancel`].
pub struct ChainProvider {
    store: Arc<dyn SecretStore>,
    prompt: Option<Arc<dyn PromptHandler>>,
    cancelled: Mutex<Vec<String>>,
}

impl fmt::Debug for ChainProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChainProvider").finish_non_exhaustive()
    }
}

impl ChainProvider {
    /// Builds a chain; `prompt = None` means headless (store only).
    pub fn new(store: Arc<dyn SecretStore>, prompt: Option<Arc<dyn PromptHandler>>) -> Self {
        Self {
            store,
            prompt,
            cancelled: Mutex::new(Vec::new()),
        }
    }

    /// Clears remembered cancellations (call when the user explicitly retries a sync).
    pub fn reset_cancel(&self) {
        self.cancelled
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}

impl CredentialProvider for ChainProvider {
    fn credential(&self, request: &CredentialRequest) -> Option<Credential> {
        if let Some(c) = self.store.get(request) {
            return Some(c);
        }
        let key = store_key(request);
        if self
            .cancelled
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&key)
        {
            return None;
        }
        let Some(handler) = &self.prompt else {
            return None;
        };
        match handler.prompt(request) {
            Some(answer) => {
                if answer.remember {
                    self.store.save(request, &answer.credential);
                }
                Some(answer.credential)
            }
            None => {
                self.cancelled
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(key);
                None
            }
        }
    }

    fn rejected(&self, request: &CredentialRequest) {
        self.store.forget(request);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Scripted {
        calls: AtomicUsize,
        answer: Option<PromptAnswer>,
    }

    impl PromptHandler for Scripted {
        fn prompt(&self, _: &CredentialRequest) -> Option<PromptAnswer> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.answer.clone()
        }
    }

    fn req() -> CredentialRequest {
        CredentialRequest {
            url: "https://example.com/r.git".into(),
            username: None,
            kind: CredentialKind::UserPassword,
        }
    }

    fn cred() -> Credential {
        Credential {
            username: "u".into(),
            secret: Secret::new("hunter2"),
        }
    }

    #[test]
    fn secret_is_redacted() {
        let dbg = format!("{:?}", cred());
        assert!(!dbg.contains("hunter2"), "{dbg}");
    }

    #[test]
    fn prompt_remember_then_served_from_store() {
        let prompt = Arc::new(Scripted {
            calls: AtomicUsize::new(0),
            answer: Some(PromptAnswer {
                credential: cred(),
                remember: true,
            }),
        });
        let chain = ChainProvider::new(Arc::new(MemoryStore::new()), Some(prompt.clone()));
        assert_eq!(chain.credential(&req()), Some(cred()));
        assert_eq!(chain.credential(&req()), Some(cred()));
        assert_eq!(prompt.calls.load(Ordering::SeqCst), 1);
        chain.rejected(&req());
        assert_eq!(chain.credential(&req()), Some(cred()));
        assert_eq!(prompt.calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn cancel_is_sticky_until_reset() {
        let prompt = Arc::new(Scripted {
            calls: AtomicUsize::new(0),
            answer: None,
        });
        let chain = ChainProvider::new(Arc::new(MemoryStore::new()), Some(prompt.clone()));
        assert_eq!(chain.credential(&req()), None);
        assert_eq!(chain.credential(&req()), None);
        assert_eq!(prompt.calls.load(Ordering::SeqCst), 1);
        chain.reset_cancel();
        assert_eq!(chain.credential(&req()), None);
        assert_eq!(prompt.calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn headless_without_store_entry_yields_none() {
        let chain = ChainProvider::new(Arc::new(MemoryStore::new()), None);
        assert_eq!(chain.credential(&req()), None);
    }
}
