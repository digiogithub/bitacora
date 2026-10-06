//! Bearer token generation, storage and verification (BIT-SP-0007.R3/R5).
//!
//! Tokens are 256-bit random values (`bit_` + 64 hex chars). They live in a `0600` JSON file in the
//! platform config dir, never inside the graph. Verification compares every stored token in constant
//! time and does not short-circuit.

use std::io::Write as _;
use std::path::{Path, PathBuf};

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

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    name: String,
    secret: String,
    scopes: Vec<Scope>,
}

// Manual Debug so secrets never reach logs.
impl std::fmt::Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Entry")
            .field("name", &self.name)
            .field("scopes", &self.scopes)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct FileFormat {
    version: u32,
    tokens: Vec<Entry>,
}

/// Name of the token created on first run.
pub const DEFAULT_TOKEN_NAME: &str = "default";

/// Thread-safe token registry, optionally persisted. Share it through an `Arc`; changes (create,
/// revoke, rotate) take effect for the running server immediately.
#[derive(Debug)]
pub struct TokenStore {
    path: Option<PathBuf>,
    entries: RwLock<Vec<Entry>>,
}

/// `<platform config dir>/mcp-tokens.json`, or `None` when no home directory can be determined.
pub fn default_token_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("es", "digio", "Bitacora")
        .map(|d| d.config_dir().join("mcp-tokens.json"))
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

impl TokenStore {
    /// A store that is never persisted (tests, embedding).
    pub fn in_memory() -> Self {
        Self {
            path: None,
            entries: RwLock::new(Vec::new()),
        }
    }

    /// Load the token file, creating it with one read-only `default` token on first run.
    ///
    /// A corrupt file is an error and is never overwritten.
    pub fn load_or_init(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        let (entries, existed) = match read_file(&path)? {
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
        let store = Self {
            path: Some(path),
            entries: RwLock::new(entries),
        };
        if !existed {
            store.create(DEFAULT_TOKEN_NAME, &[Scope::Read])?;
        }
        Ok(store)
    }

    /// Create a named token and return its secret.
    pub fn create(&self, name: &str, scopes: &[Scope]) -> Result<String, Error> {
        let secret = generate_secret()?;
        let mut entries = self.entries.write();
        if entries.iter().any(|e| e.name == name) {
            return Err(Error::TokenExists(name.to_owned()));
        }
        let mut next = entries.clone();
        next.push(Entry {
            name: name.to_owned(),
            secret: secret.clone(),
            scopes: scopes.to_vec(),
        });
        self.persist(&next)?;
        *entries = next;
        Ok(secret)
    }

    /// Revoke a token; returns `false` when it did not exist.
    pub fn revoke(&self, name: &str) -> Result<bool, Error> {
        let mut entries = self.entries.write();
        let next: Vec<Entry> = entries.iter().filter(|e| e.name != name).cloned().collect();
        if next.len() == entries.len() {
            return Ok(false);
        }
        self.persist(&next)?;
        *entries = next;
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
        entry.secret = secret.clone();
        self.persist(&next)?;
        *entries = next;
        Ok(secret)
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

    /// Secret of a token, for the "copy config snippet" action.
    pub fn secret_of(&self, name: &str) -> Option<String> {
        self.entries
            .read()
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.secret.clone())
    }

    /// Constant-time lookup of a presented secret. An empty store matches nothing.
    pub fn verify(&self, candidate: &str) -> Option<TokenInfo> {
        let entries = self.entries.read();
        let mut found: Option<&Entry> = None;
        for e in entries.iter() {
            // Visit every entry; no early exit on a match.
            if bool::from(e.secret.as_bytes().ct_eq(candidate.as_bytes())) {
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
        let body = serde_json::to_string_pretty(&FileFormat {
            version: 1,
            tokens: entries.to_vec(),
        })
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
}
