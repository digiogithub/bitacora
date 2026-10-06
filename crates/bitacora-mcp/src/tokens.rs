//! Bearer token generation, storage and verification (BIT-SP-0007.R3/R5).
//!
//! Tokens are 256-bit random values (`bit_` + 64 hex chars). The token file (platform config dir,
//! never inside the graph, mode `0600`) holds the metadata of every token. The secrets live in the
//! OS keychain when a [`SecretBackend`] is attached and works (service `bitacora`, account
//! `mcp/<name>`); otherwise, or when the keychain refuses a secret, they stay in the file. A
//! version 1 file (secrets inline) is migrated to the keychain on load. Verification compares
//! every stored token in constant time and does not short-circuit.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq as _;

use crate::Error;

/// Permission level of a token (design `mcp-server.md` section 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Search and read tools.
    Read,
    /// Create, append, update, move.
    Write,
    /// Remove blocks/pages, rename pages.
    Delete,
}

/// Public view of a token (never includes the secret).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenInfo {
    /// Unique client name, recorded in the audit log.
    pub name: String,
    /// Granted scopes.
    pub scopes: Vec<Scope>,
}

/// Where a token secret lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenStorage {
    /// In the OS keychain; the file keeps metadata only.
    Keychain,
    /// In the `0600` token file (no keychain, or the keychain refused it).
    File,
}

/// A token as the settings page lists it (never includes the secret).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSummary {
    /// Unique client name.
    pub name: String,
    /// Granted scopes.
    pub scopes: Vec<Scope>,
    /// Creation time, unix seconds (`None` for tokens made before the field existed).
    pub created_at: Option<i64>,
    /// Where the secret is kept.
    pub storage: TokenStorage,
    /// `false` when the keychain no longer returns the secret: the token cannot authenticate and
    /// should be rotated.
    pub available: bool,
}

/// Secret storage behind the token file (the OS keychain in the app, a map in tests). Errors are
/// plain messages: callers fall back to the file and log them.
pub trait SecretBackend: Send + Sync + std::fmt::Debug {
    /// The secret stored for token `name`, if any.
    ///
    /// # Errors
    /// A message when the store cannot be read.
    fn get(&self, name: &str) -> Result<Option<String>, String>;
    /// Stores (or replaces) the secret of token `name`.
    ///
    /// # Errors
    /// A message when the store refuses the write.
    fn set(&self, name: &str, secret: &str) -> Result<(), String>;
    /// Removes the secret of token `name` (absent is not an error).
    ///
    /// # Errors
    /// A message when the store cannot be written.
    fn delete(&self, name: &str) -> Result<(), String>;
}

/// Keychain service name of the MCP token secrets.
pub const KEYCHAIN_SERVICE: &str = "bitacora";

/// Keychain account of the token `name`.
#[must_use]
pub fn keychain_account(name: &str) -> String {
    format!("mcp/{name}")
}

/// [`SecretBackend`] over the OS keychain (macOS Keychain, Windows Credential Manager, Secret
/// Service).
#[cfg(feature = "keyring-store")]
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyringBackend;

#[cfg(feature = "keyring-store")]
impl KeyringBackend {
    fn entry(name: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(KEYCHAIN_SERVICE, &keychain_account(name)).map_err(|e| e.to_string())
    }
}

#[cfg(feature = "keyring-store")]
impl SecretBackend for KeyringBackend {
    fn get(&self, name: &str) -> Result<Option<String>, String> {
        match Self::entry(name)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set(&self, name: &str, secret: &str) -> Result<(), String> {
        Self::entry(name)?
            .set_password(secret)
            .map_err(|e| e.to_string())
    }

    fn delete(&self, name: &str) -> Result<(), String> {
        match Self::entry(name)?.delete_credential() {
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

/// In-memory [`SecretBackend`] (tests); [`set_failing`](Self::set_failing) makes every call fail
/// like an unavailable keychain.
#[derive(Debug, Default)]
pub struct MemoryBackend {
    map: parking_lot::Mutex<std::collections::HashMap<String, String>>,
    failing: std::sync::atomic::AtomicBool,
}

impl MemoryBackend {
    /// An empty, working backend.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes every later call fail (`true`) or work again (`false`).
    pub fn set_failing(&self, failing: bool) {
        self.failing
            .store(failing, std::sync::atomic::Ordering::SeqCst);
    }

    /// Whether a secret is stored for `name`.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.map.lock().contains_key(name)
    }

    /// Forgets the secret of `name` without going through the trait (simulates a wiped keychain).
    pub fn wipe(&self, name: &str) {
        self.map.lock().remove(name);
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
    fn get(&self, name: &str) -> Result<Option<String>, String> {
        self.check()?;
        Ok(self.map.lock().get(name).cloned())
    }

    fn set(&self, name: &str, secret: &str) -> Result<(), String> {
        self.check()?;
        self.map.lock().insert(name.to_owned(), secret.to_owned());
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<(), String> {
        self.check()?;
        self.map.lock().remove(name);
        Ok(())
    }
}

/// In memory. `secret` is `None` only when the keychain lost it.
#[derive(Clone)]
struct Entry {
    name: String,
    secret: Option<String>,
    scopes: Vec<Scope>,
    created_at: Option<i64>,
    storage: TokenStorage,
}

// Manual Debug so secrets never reach logs.
impl std::fmt::Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Entry")
            .field("name", &self.name)
            .field("scopes", &self.scopes)
            .field("storage", &self.storage)
            .finish_non_exhaustive()
    }
}

/// One token in `mcp-tokens.json`. Version 1 files always carry `secret`; version 2 omits it for
/// tokens kept in the keychain (`keychain: true`).
#[derive(Clone, Serialize, Deserialize)]
struct FileEntry {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secret: Option<String>,
    scopes: Vec<Scope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    created_at: Option<i64>,
    #[serde(default, skip_serializing_if = "is_false")]
    keychain: bool,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

impl std::fmt::Debug for FileEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileEntry")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct FileFormat {
    version: u32,
    tokens: Vec<FileEntry>,
}

/// Name of the token created on first run.
pub const DEFAULT_TOKEN_NAME: &str = "default";

/// Thread-safe token registry, optionally persisted. Share it through an `Arc`; changes (create,
/// revoke, rotate) take effect for the running server immediately.
#[derive(Debug)]
pub struct TokenStore {
    path: Option<PathBuf>,
    backend: Option<Arc<dyn SecretBackend>>,
    entries: RwLock<Vec<Entry>>,
}

/// `<platform config dir>/mcp-tokens.json`, or `None` when no home directory can be determined.
pub fn default_token_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("es", "digio", "Bitacora")
        .map(|d| d.config_dir().join("mcp-tokens.json"))
}

/// `<platform data dir>/mcp-audit`, or `None` when no home directory can be determined.
pub fn default_audit_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("es", "digio", "Bitacora")
        .map(|d| d.data_dir().join("mcp-audit"))
}

fn generate_secret() -> Result<String, Error> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| Error::Random(e.to_string()))?;
    let mut s = String::with_capacity(4 + 64);
    s.push_str("bit_");
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    Ok(s)
}

fn now_secs() -> i64 {
    jiff::Timestamp::now().as_second()
}

impl TokenStore {
    /// A store that is never persisted (tests, embedding).
    pub fn in_memory() -> Self {
        Self {
            path: None,
            backend: None,
            entries: RwLock::new(Vec::new()),
        }
    }

    /// Load the token file with secrets kept in the file itself, creating it with one read-only
    /// `default` token on first run.
    ///
    /// A corrupt file is an error and is never overwritten.
    pub fn load_or_init(path: impl Into<PathBuf>) -> Result<Self, Error> {
        Self::load_or_init_with(path, None)
    }

    /// Like [`load_or_init`](Self::load_or_init), keeping secrets in `backend` (the OS keychain)
    /// when it accepts them. Secrets found inline in the file are moved to the backend; a secret
    /// the backend refuses stays in the file (with a warning), so no token is ever lost.
    pub fn load_or_init_with(
        path: impl Into<PathBuf>,
        backend: Option<Arc<dyn SecretBackend>>,
    ) -> Result<Self, Error> {
        let path = path.into();
        let (file_entries, existed) = match read_file(&path)? {
            Some(text) => {
                let parsed: FileFormat =
                    serde_json::from_str(&text).map_err(|e| Error::TokenFile {
                        path: path.clone(),
                        message: e.to_string(),
                    })?;
                restrict_permissions(&path)?;
                (parsed.tokens, true)
            }
            None => (Vec::new(), false),
        };
        let mut migrated = false;
        let mut entries = Vec::with_capacity(file_entries.len());
        for fe in file_entries {
            let (entry, moved) = hydrate(fe, backend.as_deref());
            migrated |= moved;
            entries.push(entry);
        }
        let store = Self {
            path: Some(path),
            backend,
            entries: RwLock::new(entries),
        };
        if migrated {
            let snapshot = store.entries.read().clone();
            store.persist(&snapshot)?;
        }
        if !existed {
            store.create(DEFAULT_TOKEN_NAME, &[Scope::Read])?;
        }
        Ok(store)
    }

    /// Whether this store keeps secrets in a keychain backend (when it accepts them).
    #[must_use]
    pub fn has_keychain(&self) -> bool {
        self.backend.is_some()
    }

    /// Stores `secret` for `name`: keychain first, the file as the fallback.
    fn place(&self, name: &str, secret: &str) -> TokenStorage {
        if let Some(backend) = &self.backend {
            match backend.set(name, secret) {
                Ok(()) => return TokenStorage::Keychain,
                Err(e) => tracing::warn!(
                    token = name,
                    "keychain refused the token secret, keeping it in the 0600 file: {e}"
                ),
            }
        }
        TokenStorage::File
    }

    /// Create a named token and return its secret.
    pub fn create(&self, name: &str, scopes: &[Scope]) -> Result<String, Error> {
        let secret = generate_secret()?;
        let mut entries = self.entries.write();
        if entries.iter().any(|e| e.name == name) {
            return Err(Error::TokenExists(name.to_owned()));
        }
        let storage = self.place(name, &secret);
        let mut next = entries.clone();
        next.push(Entry {
            name: name.to_owned(),
            secret: Some(secret.clone()),
            scopes: scopes.to_vec(),
            created_at: Some(now_secs()),
            storage,
        });
        if let Err(e) = self.persist(&next) {
            self.forget_secret(name, storage);
            return Err(e);
        }
        *entries = next;
        Ok(secret)
    }

    fn forget_secret(&self, name: &str, storage: TokenStorage) {
        if storage == TokenStorage::Keychain
            && let Some(backend) = &self.backend
            && let Err(e) = backend.delete(name)
        {
            tracing::warn!(
                token = name,
                "cannot remove the token secret from the keychain: {e}"
            );
        }
    }

    /// Revoke a token; returns `false` when it did not exist.
    pub fn revoke(&self, name: &str) -> Result<bool, Error> {
        let mut entries = self.entries.write();
        let Some(removed) = entries.iter().find(|e| e.name == name).cloned() else {
            return Ok(false);
        };
        let next: Vec<Entry> = entries.iter().filter(|e| e.name != name).cloned().collect();
        self.persist(&next)?;
        *entries = next;
        self.forget_secret(name, removed.storage);
        Ok(true)
    }

    /// Replace a token's secret, keeping name and scopes. Returns the new secret.
    pub fn rotate(&self, name: &str) -> Result<String, Error> {
        let secret = generate_secret()?;
        let mut entries = self.entries.write();
        let mut next = entries.clone();
        let entry = next
            .iter_mut()
            .find(|e| e.name == name)
            .ok_or_else(|| Error::TokenNotFound(name.to_owned()))?;
        let previous = entry.storage;
        entry.storage = self.place(name, &secret);
        entry.secret = Some(secret.clone());
        let storage = entry.storage;
        self.persist(&next)?;
        *entries = next;
        if previous == TokenStorage::Keychain && storage == TokenStorage::File {
            // The file now holds the secret; do not leave a stale one in the keychain.
            self.forget_secret(name, previous);
        }
        Ok(secret)
    }

    /// Replace the scopes of a token.
    pub fn set_scopes(&self, name: &str, scopes: &[Scope]) -> Result<(), Error> {
        let mut entries = self.entries.write();
        let mut next = entries.clone();
        next.iter_mut()
            .find(|e| e.name == name)
            .ok_or_else(|| Error::TokenNotFound(name.to_owned()))?
            .scopes = scopes.to_vec();
        self.persist(&next)?;
        *entries = next;
        Ok(())
    }

    /// Names and scopes of all tokens.
    pub fn list(&self) -> Vec<TokenInfo> {
        self.entries
            .read()
            .iter()
            .map(|e| TokenInfo {
                name: e.name.clone(),
                scopes: e.scopes.clone(),
            })
            .collect()
    }

    /// Everything the settings page shows about each token (no secrets).
    pub fn summaries(&self) -> Vec<TokenSummary> {
        self.entries
            .read()
            .iter()
            .map(|e| TokenSummary {
                name: e.name.clone(),
                scopes: e.scopes.clone(),
                created_at: e.created_at,
                storage: e.storage,
                available: e.secret.is_some(),
            })
            .collect()
    }

    /// Secret of a token, for the "copy config snippet" action.
    pub fn secret_of(&self, name: &str) -> Option<String> {
        self.entries
            .read()
            .iter()
            .find(|e| e.name == name)
            .and_then(|e| e.secret.clone())
    }

    /// Constant-time lookup of a presented secret. An empty store matches nothing.
    pub fn verify(&self, candidate: &str) -> Option<TokenInfo> {
        let entries = self.entries.read();
        let mut found: Option<&Entry> = None;
        for e in entries.iter() {
            // Visit every entry; no early exit on a match.
            if let Some(secret) = &e.secret
                && bool::from(secret.as_bytes().ct_eq(candidate.as_bytes()))
            {
                found = Some(e);
            }
        }
        found.map(|e| TokenInfo {
            name: e.name.clone(),
            scopes: e.scopes.clone(),
        })
    }

    fn persist(&self, entries: &[Entry]) -> Result<(), Error> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let tokens = entries
            .iter()
            .map(|e| FileEntry {
                name: e.name.clone(),
                secret: match e.storage {
                    TokenStorage::File => e.secret.clone(),
                    TokenStorage::Keychain => None,
                },
                scopes: e.scopes.clone(),
                created_at: e.created_at,
                keychain: e.storage == TokenStorage::Keychain,
            })
            .collect();
        let body = serde_json::to_string_pretty(&FileFormat { version: 2, tokens })
            .map_err(|e| Error::Io(std::io::Error::other(e)))?;
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(dir)?;
        // Temp file in the same directory (created 0600 on unix), fsync, atomic rename.
        let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
        tmp.write_all(body.as_bytes())?;
        tmp.write_all(b"\n")?;
        tmp.as_file().sync_all()?;
        tmp.persist(path).map_err(|e| Error::Io(e.error))?;
        Ok(())
    }
}

/// Turns a file entry into an in-memory one; the flag says an inline secret moved to the backend
/// (the file must be rewritten without it).
fn hydrate(fe: FileEntry, backend: Option<&dyn SecretBackend>) -> (Entry, bool) {
    let FileEntry {
        name,
        secret,
        scopes,
        created_at,
        keychain,
    } = fe;
    if keychain {
        let secret = match backend.map(|b| b.get(&name)) {
            Some(Ok(found)) => found,
            Some(Err(e)) => {
                tracing::warn!(token = %name, "cannot read the token secret from the keychain: {e}");
                None
            }
            None => None,
        };
        return (
            Entry {
                name,
                secret,
                scopes,
                created_at,
                storage: TokenStorage::Keychain,
            },
            false,
        );
    }
    let mut storage = TokenStorage::File;
    let mut moved = false;
    if let (Some(backend), Some(secret)) = (backend, &secret) {
        match backend.set(&name, secret) {
            Ok(()) => {
                storage = TokenStorage::Keychain;
                moved = true;
            }
            Err(e) => tracing::warn!(
                token = %name,
                "cannot move the token secret to the keychain, keeping it in the file: {e}"
            ),
        }
    }
    (
        Entry {
            name,
            secret,
            scopes,
            created_at,
            storage,
        },
        moved,
    )
}

fn read_file(path: &Path) -> Result<Option<String>, Error> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt as _;
    let meta = std::fs::metadata(path)?;
    if meta.permissions().mode() & 0o077 != 0 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<(), Error> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_generates_one_256_bit_token_and_persists_0600() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cfg").join("mcp-tokens.json");
        let store = TokenStore::load_or_init(&path).expect("init");
        let list = store.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, DEFAULT_TOKEN_NAME);
        let secret = store.secret_of(DEFAULT_TOKEN_NAME).expect("secret");
        assert_eq!(secret.len(), 4 + 64);
        assert!(secret.starts_with("bit_"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&path).expect("meta").permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        // Reload keeps the same token and does not mint another.
        let again = TokenStore::load_or_init(&path).expect("reload");
        assert_eq!(again.secret_of(DEFAULT_TOKEN_NAME), Some(secret));
        assert_eq!(again.list().len(), 1);
    }

    #[test]
    fn verify_create_rotate_revoke() {
        let store = TokenStore::in_memory();
        assert!(
            store.verify("anything").is_none(),
            "empty store refuses everything"
        );
        assert!(store.verify("").is_none());
        let a = store.create("a", &[Scope::Read]).expect("a");
        let b = store.create("b", &[Scope::Read, Scope::Write]).expect("b");
        assert_ne!(a, b);
        assert!(matches!(store.create("a", &[]), Err(Error::TokenExists(_))));
        assert_eq!(store.verify(&b).expect("b").name, "b");
        assert!(store.verify(&a[..a.len() - 1]).is_none());
        let a2 = store.rotate("a").expect("rotate");
        assert!(
            store.verify(&a).is_none(),
            "old secret is dead after rotation"
        );
        assert_eq!(store.verify(&a2).expect("a2").name, "a");
        assert!(store.revoke("a").expect("revoke"));
        assert!(!store.revoke("a").expect("revoke again"));
        assert!(store.verify(&a2).is_none());
        assert!(matches!(store.rotate("zzz"), Err(Error::TokenNotFound(_))));
    }

    #[test]
    fn corrupt_file_is_an_error_and_untouched() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mcp-tokens.json");
        std::fs::write(&path, "{ not json").expect("write");
        assert!(matches!(
            TokenStore::load_or_init(&path),
            Err(Error::TokenFile { .. })
        ));
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "{ not json");
    }

    fn with_backend(dir: &Path, backend: &Arc<MemoryBackend>) -> (PathBuf, TokenStore) {
        let path = dir.join("mcp-tokens.json");
        let dynamic: Arc<dyn SecretBackend> = backend.clone();
        let store = TokenStore::load_or_init_with(&path, Some(dynamic)).expect("init");
        (path, store)
    }

    #[test]
    fn keychain_holds_the_secrets_and_the_file_only_metadata() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = Arc::new(MemoryBackend::new());
        let (path, store) = with_backend(dir.path(), &backend);
        let secret = store.secret_of(DEFAULT_TOKEN_NAME).expect("secret");
        assert!(backend.contains(DEFAULT_TOKEN_NAME));
        let text = std::fs::read_to_string(&path).expect("file");
        assert!(
            !text.contains(&secret),
            "the secret must not be in the file"
        );
        assert!(text.contains("\"keychain\": true"));
        let summary = &store.summaries()[0];
        assert_eq!(summary.storage, TokenStorage::Keychain);
        assert!(summary.available);
        assert!(summary.created_at.is_some());

        // A reload reads the secret back from the keychain and still authenticates.
        let dynamic: Arc<dyn SecretBackend> = backend.clone();
        let again = TokenStore::load_or_init_with(&path, Some(dynamic)).expect("reload");
        assert_eq!(again.secret_of(DEFAULT_TOKEN_NAME), Some(secret.clone()));
        assert!(again.verify(&secret).is_some());

        // Rotate replaces the keychain secret; revoke deletes it.
        let rotated = again.rotate(DEFAULT_TOKEN_NAME).expect("rotate");
        assert_ne!(rotated, secret);
        assert!(again.verify(&secret).is_none());
        assert!(
            !std::fs::read_to_string(&path)
                .expect("file")
                .contains(&rotated)
        );
        assert!(again.revoke(DEFAULT_TOKEN_NAME).expect("revoke"));
        assert!(!backend.contains(DEFAULT_TOKEN_NAME));
    }

    #[test]
    fn inline_secrets_migrate_to_the_keychain() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mcp-tokens.json");
        // Version 1: secrets inline, no creation time.
        let plain = TokenStore::load_or_init(&path).expect("v1");
        let secret = plain
            .create("claude", &[Scope::Read, Scope::Write])
            .expect("create");
        let default_secret = plain.secret_of(DEFAULT_TOKEN_NAME).expect("default");
        drop(plain);
        let backend = Arc::new(MemoryBackend::new());
        let dynamic: Arc<dyn SecretBackend> = backend.clone();
        let store = TokenStore::load_or_init_with(&path, Some(dynamic)).expect("migrate");
        assert_eq!(store.verify(&secret).expect("claude").name, "claude");
        assert_eq!(
            store.verify(&default_secret).expect("default").name,
            DEFAULT_TOKEN_NAME
        );
        assert!(backend.contains("claude") && backend.contains(DEFAULT_TOKEN_NAME));
        let text = std::fs::read_to_string(&path).expect("file");
        assert!(!text.contains(&secret) && !text.contains(&default_secret));
        assert!(
            store
                .summaries()
                .iter()
                .all(|s| s.storage == TokenStorage::Keychain)
        );
    }

    #[test]
    fn unavailable_keychain_falls_back_to_the_file_and_never_loses_a_token() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = Arc::new(MemoryBackend::new());
        backend.set_failing(true);
        let (path, store) = with_backend(dir.path(), &backend);
        let secret = store.secret_of(DEFAULT_TOKEN_NAME).expect("secret");
        assert_eq!(store.summaries()[0].storage, TokenStorage::File);
        assert!(
            std::fs::read_to_string(&path)
                .expect("file")
                .contains(&secret)
        );
        // The keychain comes back: the next load moves the inline secret into it.
        backend.set_failing(false);
        let dynamic: Arc<dyn SecretBackend> = backend.clone();
        let again = TokenStore::load_or_init_with(&path, Some(dynamic)).expect("reload");
        assert_eq!(again.summaries()[0].storage, TokenStorage::Keychain);
        assert!(again.verify(&secret).is_some());
        assert!(
            !std::fs::read_to_string(&path)
                .expect("file")
                .contains(&secret)
        );
    }

    #[test]
    fn a_secret_lost_by_the_keychain_is_flagged_and_cannot_authenticate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = Arc::new(MemoryBackend::new());
        let (path, store) = with_backend(dir.path(), &backend);
        let secret = store.secret_of(DEFAULT_TOKEN_NAME).expect("secret");
        backend.wipe(DEFAULT_TOKEN_NAME);
        let dynamic: Arc<dyn SecretBackend> = backend.clone();
        let again = TokenStore::load_or_init_with(&path, Some(dynamic)).expect("reload");
        assert!(!again.summaries()[0].available);
        assert!(again.secret_of(DEFAULT_TOKEN_NAME).is_none());
        assert!(again.verify(&secret).is_none());
        // Rotating repairs it.
        let fresh = again.rotate(DEFAULT_TOKEN_NAME).expect("rotate");
        assert!(again.verify(&fresh).is_some());
        assert!(again.summaries()[0].available);
    }

    #[test]
    fn scopes_can_be_changed_and_survive_a_reload() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mcp-tokens.json");
        let store = TokenStore::load_or_init(&path).expect("init");
        store
            .set_scopes(DEFAULT_TOKEN_NAME, &[Scope::Read, Scope::Delete])
            .expect("scopes");
        let again = TokenStore::load_or_init(&path).expect("reload");
        assert_eq!(again.list()[0].scopes, vec![Scope::Read, Scope::Delete]);
        assert!(matches!(
            again.set_scopes("nope", &[]),
            Err(Error::TokenNotFound(_))
        ));
    }

    /// Manual cross-OS check of the real keychain (macOS Keychain, Windows Credential Manager,
    /// Secret Service): `cargo test -p bitacora-mcp -- --ignored os_keychain`. Without a keychain
    /// (headless Linux) the calls fail and the token store falls back to the file.
    #[cfg(feature = "keyring-store")]
    #[test]
    #[ignore = "touches the real OS keychain"]
    fn os_keychain_round_trip_or_clean_failure() {
        let backend = KeyringBackend;
        let name = "__bitacora_test__";
        match backend.set(name, "secret-value") {
            Ok(()) => {
                assert_eq!(backend.get(name), Ok(Some("secret-value".to_owned())));
                assert_eq!(backend.delete(name), Ok(()));
                assert_eq!(backend.get(name), Ok(None));
            }
            Err(error) => eprintln!("no usable keychain here ({error}); the file fallback applies"),
        }
    }

    #[test]
    fn debug_output_never_contains_secrets() {
        let store = TokenStore::in_memory();
        let secret = store.create("a", &[Scope::Read]).expect("a");
        assert!(!format!("{store:?}").contains(&secret));
    }
}
