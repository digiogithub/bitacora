//! Per-graph sync preferences and the validation of the sync onboarding forms
//! (BIT-US-0043, BIT-T-0282).
//!
//! Preferences live next to the per-graph UI state (`<data_dir>/graphs/<hash>/sync.json`), not
//! in the graph folder: the remote and the device name are properties of this installation.
//! Losing the file only turns background sync off; the repository keeps its own remote.

use rust_i18n::t;
use std::path::{Path, PathBuf};

use bitacora_sync::GitDetection;
use bitacora_sync::backend::GitError;
use bitacora_sync::onboarding::OnboardingError;
use bitacora_sync::repo_setup::{self, Identity};
use serde::{Deserialize, Serialize};

use crate::paths::graph_hash;
use crate::settings::write_atomic;

/// Default branch name offered by the forms.
pub const DEFAULT_BRANCH: &str = "main";

/// `sync.commit_idle_secs` is clamped to this range (seconds).
pub const IDLE_RANGE: (u64, u64) = (5, 600);
/// `sync.commit_max_secs` is clamped to this range (seconds).
pub const MAX_RANGE: (u64, u64) = (30, 3600);
/// `sync.fetch_interval_secs` (foreground) is clamped to this range (seconds).
pub const FETCH_RANGE: (u64, u64) = (30, 3600);

/// What sync does for one graph on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncPrefs {
    /// Run the background sync engine when the graph opens.
    pub enabled: bool,
    /// Remote URL the graph was connected to (informational; git holds the real one).
    pub remote_url: String,
    /// Branch to sync.
    pub branch: String,
    /// `Bitacora-Device` trailer value.
    pub device: String,
    /// Commit author name (`None` keeps the repository or fallback identity).
    pub author_name: Option<String>,
    /// Commit author email.
    pub author_email: Option<String>,
    /// Commit after this many idle seconds (`sync.commit_idle_secs`, 5-600).
    pub commit_idle_secs: u64,
    /// Hard cap of continuous editing before a commit (`sync.commit_max_secs`).
    pub commit_max_secs: u64,
    /// Fetch interval while the app is focused (`sync.fetch_interval_secs`); the background
    /// interval is five times as long.
    pub fetch_interval_secs: u64,
    /// Amend recent auto-commits instead of piling them up (`sync.squash_auto_commits`).
    pub squash_auto_commits: bool,
    /// Private key used for SSH remotes (BIT-US-0177); `None` uses the agent and default keys.
    /// Only the path is stored here, passphrases live in the OS keychain.
    pub ssh_key: Option<PathBuf>,
}

impl Default for SyncPrefs {
    fn default() -> Self {
        Self {
            enabled: false,
            remote_url: String::new(),
            branch: DEFAULT_BRANCH.to_owned(),
            device: repo_setup::hostname(),
            author_name: None,
            author_email: None,
            commit_idle_secs: 20,
            commit_max_secs: 300,
            fetch_interval_secs: 120,
            squash_auto_commits: true,
            ssh_key: None,
        }
    }
}

fn clamp_secs(value: u64, (lo, hi): (u64, u64)) -> u64 {
    value.clamp(lo, hi)
}

impl SyncPrefs {
    /// The preferences with every timing inside its documented range (the cap is never
    /// shorter than the idle window).
    #[must_use]
    pub fn clamped(mut self) -> Self {
        self.commit_idle_secs = clamp_secs(self.commit_idle_secs, IDLE_RANGE);
        self.commit_max_secs =
            clamp_secs(self.commit_max_secs, MAX_RANGE).max(self.commit_idle_secs);
        self.fetch_interval_secs = clamp_secs(self.fetch_interval_secs, FETCH_RANGE);
        self
    }

    /// `<data_dir>/graphs/<hash>/sync.json` for the graph at `root`.
    pub fn file_for(data_dir: &Path, root: &Path) -> PathBuf {
        data_dir
            .join("graphs")
            .join(graph_hash(root))
            .join("sync.json")
    }

    /// Loads the preferences; a missing or invalid file yields the defaults (sync off).
    pub fn load(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|err| {
                tracing::warn!(path = %path.display(), "invalid sync preferences, ignoring: {err}");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Writes the preferences atomically.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        write_atomic(path, &json)
    }

    /// The SSH key to use, ignoring an empty path.
    pub fn ssh_key_path(&self) -> Option<&Path> {
        self.ssh_key
            .as_deref()
            .filter(|p| !p.as_os_str().is_empty())
    }

    /// The identity to configure in the repository, when the user gave one.
    pub fn identity(&self) -> Option<Identity> {
        identity_from(self.author_name.as_deref(), self.author_email.as_deref())
    }
}

fn identity_from(name: Option<&str>, email: Option<&str>) -> Option<Identity> {
    let name = name.map(str::trim).filter(|n| !n.is_empty())?;
    let email = email.map(str::trim).filter(|e| !e.is_empty())?;
    Some(Identity {
        name: name.to_owned(),
        email: email.to_owned(),
    })
}

/// Kind of remote a URL names; the forms accept the three kinds git does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteKind {
    /// `https://` or `http://`.
    Https,
    /// `ssh://...` or the scp-like `user@host:path`.
    Ssh,
    /// A local path or `file://` URL (a bare repository on disk, a shared folder).
    Local,
}

/// Why a form field is not acceptable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldError {
    /// Empty.
    Required,
    /// Contains whitespace or control characters.
    Whitespace,
    /// Starts with `-` (would be read as an option).
    LeadingDash,
    /// Not a URL or path git understands.
    UnsupportedUrl,
    /// A branch name git rejects.
    BadBranch,
    /// Not an email address.
    BadEmail,
    /// A name without an email or the other way round.
    IdentityIncomplete,
    /// The destination folder exists and is not empty.
    DestinationNotEmpty,
    /// A timing field holds something that is not a whole number of seconds.
    NotANumber,
}

impl FieldError {
    /// Stable key into the locale file (`sync.error.<key>`).
    pub fn key(self) -> &'static str {
        match self {
            Self::Required => "sync.error.required",
            Self::Whitespace => "sync.error.whitespace",
            Self::LeadingDash => "sync.error.leading_dash",
            Self::UnsupportedUrl => "sync.error.unsupported_url",
            Self::BadBranch => "sync.error.bad_branch",
            Self::BadEmail => "sync.error.bad_email",
            Self::IdentityIncomplete => "sync.error.identity_incomplete",
            Self::DestinationNotEmpty => "sync.error.destination_not_empty",
            Self::NotANumber => "sync.error.not_number",
        }
    }
}

/// Validates a remote URL (syntax only: reachability is reported by the operation itself).
pub fn validate_remote_url(url: &str) -> Result<RemoteKind, FieldError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(FieldError::Required);
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(FieldError::Whitespace);
    }
    if url.starts_with('-') {
        return Err(FieldError::LeadingDash);
    }
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return host_after(url, "://").map(|()| RemoteKind::Https);
    }
    if lower.starts_with("ssh://") {
        return host_after(url, "://").map(|()| RemoteKind::Ssh);
    }
    if lower.starts_with("file://") || Path::new(url).is_absolute() || is_rooted_local_path(url) {
        return Ok(RemoteKind::Local);
    }
    // scp-like: `[user@]host:path`, where the part before the colon has no slash.
    if let Some((host, path)) = url.split_once(':')
        && !host.is_empty()
        && !path.is_empty()
        && !host.contains('/')
        && !host.contains("://")
    {
        return Ok(RemoteKind::Ssh);
    }
    Err(FieldError::UnsupportedUrl)
}

/// Rooted paths that `Path::is_absolute` rejects on Windows (`/srv/git`) are still local remotes
/// there; drive paths and UNC shares are local on Windows only.
fn is_rooted_local_path(url: &str) -> bool {
    let b = url.as_bytes();
    let drive =
        b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'/' | b'\\');
    url.starts_with('/') || (cfg!(windows) && (drive || url.starts_with("\\\\")))
}

fn host_after(url: &str, marker: &str) -> Result<(), FieldError> {
    let rest = url.split_once(marker).map_or("", |(_, r)| r);
    let authority = rest.split('/').next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    if host.is_empty() {
        Err(FieldError::UnsupportedUrl)
    } else {
        Ok(())
    }
}

/// Validates a branch name with the rules of `git check-ref-format` that matter here.
pub fn validate_branch(branch: &str) -> Result<(), FieldError> {
    let b = branch.trim();
    if b.is_empty() {
        return Err(FieldError::Required);
    }
    if b.starts_with('-')
        || b.starts_with('/')
        || b.ends_with('/')
        || b.ends_with('.')
        || b.ends_with(".lock")
        || b.contains("..")
        || b.contains("//")
        || b.contains("@{")
        || b == "@"
        || b.chars()
            .any(|c| c.is_whitespace() || c.is_control() || "~^:?*[\\".contains(c))
    {
        return Err(FieldError::BadBranch);
    }
    Ok(())
}

/// Validates the optional identity: both fields or none.
pub fn validate_identity(name: &str, email: &str) -> Result<(), FieldError> {
    let (name, email) = (name.trim(), email.trim());
    match (name.is_empty(), email.is_empty()) {
        (true, true) => Ok(()),
        (false, false) => {
            let ok = email
                .split_once('@')
                .is_some_and(|(local, host)| !local.is_empty() && host.contains('.'))
                && !email.chars().any(char::is_whitespace);
            if ok {
                Ok(())
            } else {
                Err(FieldError::BadEmail)
            }
        }
        _ => Err(FieldError::IdentityIncomplete),
    }
}

/// The fields of the "Enable sync" and "Open graph from git remote" forms.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncForm {
    /// Remote URL.
    pub remote_url: String,
    /// Branch.
    pub branch: String,
    /// Author name.
    pub name: String,
    /// Author email.
    pub email: String,
    /// Device name.
    pub device: String,
    /// Clone destination (clone form only).
    pub destination: String,
    /// Seconds of idle before a commit (empty: the default).
    pub commit_idle: String,
    /// Seconds before an editing session is committed anyway (empty: the default).
    pub commit_max: String,
    /// Seconds between fetches while focused (empty: the default).
    pub fetch: String,
    /// Squash recent auto-commits.
    pub squash: bool,
}

/// Parses a timing field: empty means `default`, anything else must be a whole number, which
/// is then clamped into `range`.
pub fn parse_secs(text: &str, range: (u64, u64), default: u64) -> Result<u64, FieldError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(default);
    }
    text.parse::<u64>()
        .map(|v| clamp_secs(v, range))
        .map_err(|_| FieldError::NotANumber)
}

/// Validation result per field; empty means the form can be submitted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FormErrors {
    /// Remote URL problem.
    pub remote_url: Option<FieldError>,
    /// Branch problem.
    pub branch: Option<FieldError>,
    /// Identity problem.
    pub identity: Option<FieldError>,
    /// Device name problem.
    pub device: Option<FieldError>,
    /// Destination problem (clone form).
    pub destination: Option<FieldError>,
    /// A timing field is not a number.
    pub timing: Option<FieldError>,
}

impl FormErrors {
    /// No field has a problem.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl SyncForm {
    /// Validates the enable form (`clone` adds the destination check).
    pub fn validate(&self, clone: bool) -> FormErrors {
        let mut errors = FormErrors {
            remote_url: validate_remote_url(&self.remote_url).err(),
            branch: validate_branch(&self.branch).err(),
            identity: validate_identity(&self.name, &self.email).err(),
            ..FormErrors::default()
        };
        errors.timing = [
            parse_secs(&self.commit_idle, IDLE_RANGE, 20),
            parse_secs(&self.commit_max, MAX_RANGE, 300),
            parse_secs(&self.fetch, FETCH_RANGE, 120),
        ]
        .into_iter()
        .find_map(Result::err);
        let device = self.device.trim();
        if device.is_empty() {
            errors.device = Some(FieldError::Required);
        } else if device.chars().any(char::is_control) {
            errors.device = Some(FieldError::Whitespace);
        }
        if clone {
            let dest = self.destination.trim();
            errors.destination = if dest.is_empty() {
                Some(FieldError::Required)
            } else if Path::new(dest)
                .read_dir()
                .is_ok_and(|mut entries| entries.next().is_some())
            {
                Some(FieldError::DestinationNotEmpty)
            } else {
                None
            };
        }
        errors
    }

    /// The identity to configure, when both fields are filled in.
    pub fn identity(&self) -> Option<Identity> {
        identity_from(Some(&self.name), Some(&self.email))
    }

    /// The preferences this (valid) form describes, enabled, timings clamped.
    pub fn to_prefs(&self) -> SyncPrefs {
        let defaults = SyncPrefs::default();
        SyncPrefs {
            enabled: true,
            remote_url: self.remote_url.trim().to_owned(),
            branch: self.branch.trim().to_owned(),
            device: self.device.trim().to_owned(),
            author_name: self.identity().map(|i| i.name),
            author_email: self.identity().map(|i| i.email),
            commit_idle_secs: parse_secs(&self.commit_idle, IDLE_RANGE, defaults.commit_idle_secs)
                .unwrap_or(defaults.commit_idle_secs),
            commit_max_secs: parse_secs(&self.commit_max, MAX_RANGE, defaults.commit_max_secs)
                .unwrap_or(defaults.commit_max_secs),
            fetch_interval_secs: parse_secs(&self.fetch, FETCH_RANGE, defaults.fetch_interval_secs)
                .unwrap_or(defaults.fetch_interval_secs),
            squash_auto_commits: self.squash,
            ssh_key: None,
        }
        .clamped()
    }
}

/// A user-presentable failure with the next step to take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionableError {
    /// What went wrong, in plain words.
    pub message: String,
    /// What to try next.
    pub advice: String,
}

/// Turns an onboarding failure into text a non-technical user can act on (BIT-T-0282).
pub fn actionable(error: &OnboardingError, detection: &GitDetection) -> ActionableError {
    let git_hint = match detection {
        GitDetection::Found { .. } => String::new(),
        _ => format!(" {}", t!("sync.onboarding.git_hint")),
    };
    let (message, advice) = match error {
        OnboardingError::Git(GitError::Auth { .. }) => (
            t!("sync.onboarding.auth").to_string(),
            t!("sync.onboarding.auth_advice", hint = git_hint).to_string(),
        ),
        OnboardingError::Git(GitError::Network(m)) => (
            t!("sync.onboarding.network", message = m).to_string(),
            t!("sync.onboarding.network_advice").to_string(),
        ),
        OnboardingError::Git(GitError::NotARepo) => (
            t!("sync.onboarding.not_repo").to_string(),
            t!("sync.onboarding.not_repo_advice").to_string(),
        ),
        OnboardingError::SeparateGitdir(path) => (
            t!(
                "sync.onboarding.separate_gitdir",
                path = path.display().to_string()
            )
            .to_string(),
            t!("sync.onboarding.separate_gitdir_advice").to_string(),
        ),
        OnboardingError::RemoteMismatch {
            existing,
            requested,
            ..
        } => (
            t!(
                "sync.onboarding.remote_mismatch",
                existing = existing,
                requested = requested
            )
            .to_string(),
            t!("sync.onboarding.remote_mismatch_advice").to_string(),
        ),
        OnboardingError::DestinationNotEmpty(path) => (
            t!(
                "sync.onboarding.dest_not_empty",
                path = path.display().to_string()
            )
            .to_string(),
            t!("sync.onboarding.dest_not_empty_advice").to_string(),
        ),
        OnboardingError::InvalidArgument(what) => (
            t!("sync.onboarding.invalid_argument", what = what).to_string(),
            t!("sync.onboarding.invalid_argument_advice").to_string(),
        ),
        other => (
            other.to_string(),
            t!("sync.onboarding.other_advice").to_string(),
        ),
    };
    ActionableError { message, advice }
}

#[cfg(test)]
mod tests {
    #[test]
    fn prefs_without_ssh_key_still_load_and_the_key_roundtrips() {
        // A sync.json written before BIT-US-0177 has no `ssh_key` field.
        let old = r#"{"enabled":true,"remote_url":"git@h:o/r.git","branch":"main"}"#;
        let prefs: SyncPrefs = serde_json::from_str(old).unwrap_or_default();
        assert!(prefs.enabled);
        assert_eq!(prefs.ssh_key, None);
        assert_eq!(prefs.ssh_key_path(), None);
        let with_key = SyncPrefs {
            ssh_key: Some(PathBuf::from("/k/my id")),
            ..prefs
        };
        let json = serde_json::to_string(&with_key).unwrap_or_default();
        let back: SyncPrefs = serde_json::from_str(&json).unwrap_or_default();
        assert_eq!(back.ssh_key_path(), Some(Path::new("/k/my id")));
        let empty = SyncPrefs {
            ssh_key: Some(PathBuf::new()),
            ..back
        };
        assert_eq!(empty.ssh_key_path(), None);
    }

    use super::*;

    #[test]
    fn urls_are_classified_and_bad_ones_rejected() {
        assert_eq!(
            validate_remote_url("https://github.com/me/notes.git"),
            Ok(RemoteKind::Https)
        );
        assert_eq!(
            validate_remote_url("git@github.com:me/notes.git"),
            Ok(RemoteKind::Ssh)
        );
        assert_eq!(
            validate_remote_url("ssh://git@host:2222/srv/notes"),
            Ok(RemoteKind::Ssh)
        );
        assert_eq!(
            validate_remote_url("/srv/git/notes.git"),
            Ok(RemoteKind::Local)
        );
        #[cfg(windows)]
        for p in [
            r"C:\repos\notes.git",
            "C:/repos/notes.git",
            r"\\srv\share\n",
        ] {
            assert_eq!(validate_remote_url(p), Ok(RemoteKind::Local), "{p}");
        }
        assert_eq!(validate_remote_url(""), Err(FieldError::Required));
        assert_eq!(validate_remote_url("  "), Err(FieldError::Required));
        assert_eq!(
            validate_remote_url("https://host/a b"),
            Err(FieldError::Whitespace)
        );
        assert_eq!(
            validate_remote_url("--upload-pack=x"),
            Err(FieldError::LeadingDash)
        );
        assert_eq!(
            validate_remote_url("notes"),
            Err(FieldError::UnsupportedUrl)
        );
        assert_eq!(
            validate_remote_url("https:///x"),
            Err(FieldError::UnsupportedUrl)
        );
    }

    #[test]
    fn branches_follow_ref_format_rules() {
        assert!(validate_branch("main").is_ok());
        assert!(validate_branch("feature/x-1").is_ok());
        for bad in ["", "-x", "a..b", "a b", "a~1", "x.lock", "/x", "x/", "a@{b"] {
            assert!(validate_branch(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn identity_needs_both_fields_or_none() {
        assert!(validate_identity("", "").is_ok());
        assert!(validate_identity("Ana", "ana@example.org").is_ok());
        assert_eq!(
            validate_identity("Ana", ""),
            Err(FieldError::IdentityIncomplete)
        );
        assert_eq!(validate_identity("Ana", "ana"), Err(FieldError::BadEmail));
    }

    #[test]
    fn form_validation_states() {
        let tmp = tempfile::tempdir().expect("tmp");
        let mut form = SyncForm {
            remote_url: "https://example.org/n.git".into(),
            branch: "main".into(),
            device: "laptop".into(),
            destination: tmp.path().join("new").display().to_string(),
            ..SyncForm::default()
        };
        assert!(form.validate(true).is_empty());
        form.remote_url.clear();
        assert_eq!(form.validate(false).remote_url, Some(FieldError::Required));
        form.remote_url = "https://example.org/n.git".into();
        // A non-empty destination is refused for clones, ignored for the enable form.
        std::fs::write(tmp.path().join("x"), "x").expect("file");
        form.destination = tmp.path().display().to_string();
        assert_eq!(
            form.validate(true).destination,
            Some(FieldError::DestinationNotEmpty)
        );
        assert!(form.validate(false).is_empty());
        form.destination.clear();
        assert_eq!(form.validate(true).destination, Some(FieldError::Required));
        form.device = " ".into();
        assert_eq!(form.validate(false).device, Some(FieldError::Required));
    }

    #[test]
    fn timings_are_clamped_into_their_ranges_and_must_be_numbers() {
        assert_eq!(parse_secs("", IDLE_RANGE, 20), Ok(20));
        assert_eq!(parse_secs("1", IDLE_RANGE, 20), Ok(5));
        assert_eq!(parse_secs("9999", IDLE_RANGE, 20), Ok(600));
        assert_eq!(parse_secs(" 45 ", IDLE_RANGE, 20), Ok(45));
        assert_eq!(
            parse_secs("soon", IDLE_RANGE, 20),
            Err(FieldError::NotANumber)
        );
        let prefs = SyncPrefs {
            commit_idle_secs: 500,
            commit_max_secs: 40,
            fetch_interval_secs: 1,
            ..SyncPrefs::default()
        }
        .clamped();
        assert_eq!(prefs.commit_idle_secs, 500);
        assert_eq!(
            prefs.commit_max_secs, 500,
            "the cap never undercuts the idle window"
        );
        assert_eq!(prefs.fetch_interval_secs, 30);
        let form = SyncForm {
            remote_url: "https://example.org/n.git".into(),
            branch: "main".into(),
            device: "d".into(),
            commit_idle: "2".into(),
            fetch: "x".into(),
            ..SyncForm::default()
        };
        assert_eq!(form.validate(false).timing, Some(FieldError::NotANumber));
        let ok = SyncForm {
            fetch: String::new(),
            ..form
        };
        assert!(ok.validate(false).is_empty());
        assert_eq!(ok.to_prefs().commit_idle_secs, 5);
    }

    #[test]
    fn prefs_roundtrip_and_corrupt_files_turn_sync_off() {
        let tmp = tempfile::tempdir().expect("tmp");
        let file = SyncPrefs::file_for(tmp.path(), Path::new("/graphs/a"));
        assert!(!SyncPrefs::load(&file).enabled);
        let prefs = SyncPrefs {
            enabled: true,
            remote_url: "https://example.org/n.git".into(),
            author_name: Some("Ana".into()),
            author_email: Some("ana@example.org".into()),
            ..SyncPrefs::default()
        };
        prefs.save(&file).expect("save");
        assert_eq!(SyncPrefs::load(&file), prefs);
        assert_eq!(prefs.identity().map(|i| i.name), Some("Ana".to_owned()));
        std::fs::write(&file, b"{nope").expect("corrupt");
        assert!(!SyncPrefs::load(&file).enabled);
    }

    #[test]
    fn auth_failures_suggest_installing_git_without_it() {
        let err = OnboardingError::Git(GitError::Auth { hint: None });
        let without = actionable(&err, &GitDetection::Missing);
        assert!(without.advice.contains("Installing git"));
        let with = actionable(
            &err,
            &GitDetection::Found {
                path: "/usr/bin/git".into(),
                version: bitacora_sync::GitVersion {
                    major: 2,
                    minor: 43,
                    patch: 0,
                },
            },
        );
        assert!(!with.advice.contains("Installing git"));
    }
}
