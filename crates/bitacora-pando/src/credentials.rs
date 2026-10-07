//! Pando tokens: OS keychain storage with environment-variable override (BIT-SP-0009.R3).
//!
//! Secrets never reach the settings file, a graph or git. Resolution order per token:
//! environment variable, then keychain. The environment value is never written back to the
//! keychain. [`Token`] is redacted in `Debug`; nothing in this module logs a secret.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use pando::Token;
use parking_lot::Mutex;

/// Keychain service shared with the MCP token secrets.
pub const KEYCHAIN_SERVICE: &str = "bitacora";

/// Which Pando credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// REST knowledge-base token (`X-Pando-Token`).
    Rest,
    /// AG-UI bearer token.
    Agui,
}

impl TokenKind {
    /// Environment variable that overrides the stored token.
    #[must_use]
    pub fn env_var(self) -> &'static str {
        match self {
            Self::Rest => "BITACORA_PANDO_REST_TOKEN",
            Self::Agui => "BITACORA_PANDO_AGUI_TOKEN",
        }
    }

    /// Keychain account name.
    #[must_use]
    pub fn account(self) -> &'static str {
        match self {
            Self::Rest => "pando/rest",
            Self::Agui => "pando/agui",
        }
    }
}

/// Where a resolved token came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
    /// An environment variable (not persisted).
    Env,
    /// The OS keychain.
    Keychain,
}

/// Credential failures. Messages never contain a secret.
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    /// This build or host has no keychain.
    #[error("no OS keychain is available; set {0} in the environment instead")]
    NoKeychain(&'static str),
    /// The keychain refused the operation.
    #[error("keychain: {0}")]
    Keychain(String),
    /// An empty token was given.
    #[error("the token is empty")]
    Empty,
}

/// Secret storage (the OS keychain in the app, a map in tests). Errors are plain messages.
pub trait SecretBackend: Send + Sync + fmt::Debug {
    /// The stored secret of `kind`, if any.
    ///
    /// # Errors
    /// A message when the store cannot be read.
    fn get(&self, kind: TokenKind) -> Result<Option<String>, String>;
    /// Stores (or replaces) the secret of `kind`.
    ///
    /// # Errors
    /// A message when the store refuses the write.
    fn set(&self, kind: TokenKind, secret: &str) -> Result<(), String>;
    /// Removes the secret of `kind` (absent is not an error).
    ///
    /// # Errors
    /// A message when the store cannot be written.
    fn delete(&self, kind: TokenKind) -> Result<(), String>;
}

/// [`SecretBackend`] over the OS keychain.
#[cfg(feature = "keyring-store")]
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyringBackend;

#[cfg(feature = "keyring-store")]
impl KeyringBackend {
    fn entry(kind: TokenKind) -> Result<keyring::Entry, String> {
        keyring::Entry::new(KEYCHAIN_SERVICE, kind.account()).map_err(|e| e.to_string())
    }
}

#[cfg(feature = "keyring-store")]
impl SecretBackend for KeyringBackend {
    fn get(&self, kind: TokenKind) -> Result<Option<String>, String> {
        match Self::entry(kind)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set(&self, kind: TokenKind, secret: &str) -> Result<(), String> {
        Self::entry(kind)?
            .set_password(secret)
            .map_err(|e| e.to_string())
    }

    fn delete(&self, kind: TokenKind) -> Result<(), String> {
        match Self::entry(kind)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// The OS keychain backend when this build has one.
#[must_use]
pub fn os_keychain() -> Option<Arc<dyn SecretBackend>> {
    #[cfg(feature = "keyring-store")]
    {
        Some(Arc::new(KeyringBackend))
    }
    #[cfg(not(feature = "keyring-store"))]
    {
        None
    }
}

/// In-memory [`SecretBackend`] (tests); [`set_failing`](Self::set_failing) simulates an
/// unavailable keychain.
#[derive(Default)]
pub struct MemoryBackend {
    map: Mutex<HashMap<&'static str, String>>,
    failing: std::sync::atomic::AtomicBool,
}

impl fmt::Debug for MemoryBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MemoryBackend(<redacted>)")
    }
}

impl MemoryBackend {
    /// Makes every later call fail (or succeed again).
    pub fn set_failing(&self, failing: bool) {
        self.failing
            .store(failing, std::sync::atomic::Ordering::SeqCst);
    }

    fn check(&self) -> Result<(), String> {
        if self.failing.load(std::sync::atomic::Ordering::SeqCst) {
            Err("keychain unavailable".to_owned())
        } else {
            Ok(())
        }
    }
}

impl SecretBackend for MemoryBackend {
    fn get(&self, kind: TokenKind) -> Result<Option<String>, String> {
        self.check()?;
        Ok(self.map.lock().get(kind.account()).cloned())
    }

    fn set(&self, kind: TokenKind, secret: &str) -> Result<(), String> {
        self.check()?;
        self.map.lock().insert(kind.account(), secret.to_owned());
        Ok(())
    }

    fn delete(&self, kind: TokenKind) -> Result<(), String> {
        self.check()?;
        self.map.lock().remove(kind.account());
        Ok(())
    }
}

type EnvLookup = dyn Fn(&str) -> Option<String> + Send + Sync;

/// Resolves, stores and clears the Pando tokens.
#[derive(Clone)]
pub struct PandoCredentials {
    backend: Option<Arc<dyn SecretBackend>>,
    env: Arc<EnvLookup>,
}

impl fmt::Debug for PandoCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PandoCredentials")
            .field("keychain", &self.backend.is_some())
            .finish_non_exhaustive()
    }
}

impl PandoCredentials {
    /// OS keychain (when the build has one) and the process environment.
    #[must_use]
    pub fn system() -> Self {
        Self::new(os_keychain(), |name| std::env::var(name).ok())
    }

    /// Custom backend and environment (tests, embedding).
    pub fn new(
        backend: Option<Arc<dyn SecretBackend>>,
        env: impl Fn(&str) -> Option<String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            backend,
            env: Arc::new(env),
        }
    }

    /// The token of `kind` and where it came from: environment first, then keychain. A keychain
    /// read failure is logged (without secrets) and reads as "no token".
    #[must_use]
    pub fn resolve(&self, kind: TokenKind) -> Option<(Token, TokenSource)> {
        if let Some(v) = (self.env)(kind.env_var()) {
            let v = v.trim();
            if !v.is_empty() {
                tracing::debug!(?kind, "pando token taken from the environment");
                return Some((Token::new(v), TokenSource::Env));
            }
        }
        match self.backend.as_ref()?.get(kind) {
            Ok(Some(s)) if !s.trim().is_empty() => {
                tracing::debug!(?kind, "pando token taken from the keychain");
                Some((Token::new(s.trim()), TokenSource::Keychain))
            }
            Ok(_) => None,
            Err(e) => {
                tracing::warn!(?kind, error = %e, "cannot read the pando token from the keychain");
                None
            }
        }
    }

    /// Stores `token` in the keychain.
    ///
    /// # Errors
    /// [`CredentialError`] when empty, when there is no keychain, or when it refuses.
    pub fn store(&self, kind: TokenKind, token: &str) -> Result<(), CredentialError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(CredentialError::Empty);
        }
        let backend = self
            .backend
            .as_ref()
            .ok_or(CredentialError::NoKeychain(kind.env_var()))?;
        backend
            .set(kind, token)
            .map_err(CredentialError::Keychain)?;
        tracing::debug!(?kind, "pando token stored in the keychain");
        Ok(())
    }

    /// Removes the stored token of `kind` (an environment override is untouched).
    ///
    /// # Errors
    /// [`CredentialError`] when there is no keychain or it refuses.
    pub fn clear(&self, kind: TokenKind) -> Result<(), CredentialError> {
        let backend = self
            .backend
            .as_ref()
            .ok_or(CredentialError::NoKeychain(kind.env_var()))?;
        backend.delete(kind).map_err(CredentialError::Keychain)
    }

    /// Whether a token exists for `kind` (never exposes it); for the settings page.
    #[must_use]
    pub fn is_set(&self, kind: TokenKind) -> Option<TokenSource> {
        self.resolve(kind).map(|(_, s)| s)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use std::io::Write as _;

    const SECRET: &str = "s3cr3t-pando-token-value";

    fn creds(env: Option<(&'static str, &'static str)>) -> (PandoCredentials, Arc<MemoryBackend>) {
        let mem = Arc::new(MemoryBackend::default());
        let backend: Arc<dyn SecretBackend> = mem.clone();
        let c = PandoCredentials::new(Some(backend), move |name| {
            env.filter(|(k, _)| *k == name).map(|(_, v)| v.to_owned())
        });
        (c, mem)
    }

    #[test]
    fn keychain_roundtrip_and_clear() {
        let (c, _) = creds(None);
        assert!(c.resolve(TokenKind::Rest).is_none());
        c.store(TokenKind::Rest, &format!("  {SECRET}\n")).unwrap();
        let (t, src) = c.resolve(TokenKind::Rest).unwrap();
        assert_eq!((t.expose(), src), (SECRET, TokenSource::Keychain));
        assert!(c.resolve(TokenKind::Agui).is_none(), "kinds are separate");
        c.clear(TokenKind::Rest).unwrap();
        assert!(c.resolve(TokenKind::Rest).is_none());
    }

    #[test]
    fn env_overrides_keychain_and_is_not_persisted() {
        let (c, mem) = creds(Some(("BITACORA_PANDO_REST_TOKEN", "from-env")));
        c.store(TokenKind::Rest, "from-keychain").unwrap();
        let (t, src) = c.resolve(TokenKind::Rest).unwrap();
        assert_eq!((t.expose(), src), ("from-env", TokenSource::Env));
        assert_eq!(
            mem.get(TokenKind::Rest).unwrap().as_deref(),
            Some("from-keychain")
        );
        // The AG-UI token has its own variable.
        assert!(c.resolve(TokenKind::Agui).is_none());
        // A blank variable is ignored.
        let (blank, _) = creds(Some(("BITACORA_PANDO_REST_TOKEN", "  ")));
        assert!(blank.resolve(TokenKind::Rest).is_none());
    }

    #[test]
    fn failing_or_missing_keychain_degrades_without_panicking() {
        let (c, mem) = creds(None);
        c.store(TokenKind::Agui, SECRET).unwrap();
        mem.set_failing(true);
        assert!(c.resolve(TokenKind::Agui).is_none());
        assert!(matches!(
            c.store(TokenKind::Agui, SECRET),
            Err(CredentialError::Keychain(_))
        ));
        let none = PandoCredentials::new(None, |_| None);
        assert!(matches!(
            none.store(TokenKind::Rest, SECRET),
            Err(CredentialError::NoKeychain(_))
        ));
        assert!(matches!(
            none.store(TokenKind::Rest, " "),
            Err(CredentialError::Empty)
        ));
    }

    #[test]
    fn debug_output_never_contains_the_secret() {
        let (c, mem) = creds(None);
        c.store(TokenKind::Rest, SECRET).unwrap();
        let (t, _) = c.resolve(TokenKind::Rest).unwrap();
        for text in [format!("{c:?}"), format!("{mem:?}"), format!("{t:?}")] {
            assert!(!text.contains(SECRET), "{text}");
        }
    }

    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Buf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Buf {
        type Writer = Buf;
        fn make_writer(&'a self) -> Buf {
            self.clone()
        }
    }

    #[test]
    fn trace_logging_never_leaks_the_secret() {
        let buf = Buf::default();
        let sub = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(buf.clone())
            .finish();
        tracing::subscriber::with_default(sub, || {
            let (c, mem) = creds(None);
            c.store(TokenKind::Rest, SECRET).unwrap();
            let _ = c.resolve(TokenKind::Rest);
            mem.set_failing(true);
            let _ = c.resolve(TokenKind::Rest);
            let _ = c.store(TokenKind::Rest, SECRET);
            let env = PandoCredentials::new(None, |_| Some(SECRET.to_owned()));
            let _ = env.resolve(TokenKind::Agui);
            let _ = format!("{env:?}");
        });
        let mut out = buf.0.lock();
        out.flush().unwrap();
        let text = String::from_utf8_lossy(&out).into_owned();
        assert!(text.contains("pando token"), "logging ran: {text}");
        assert!(!text.contains(SECRET), "{text}");
    }
}
