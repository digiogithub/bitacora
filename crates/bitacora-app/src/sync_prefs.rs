//! Per-graph sync preferences and the validation of the sync onboarding forms
//! (BIT-US-0043, BIT-T-0282).
//!
//! Preferences live next to the per-graph UI state (`<data_dir>/graphs/<hash>/sync.json`), not
//! in the graph folder: the remote and the device name are properties of this installation.
//! Losing the file only turns background sync off; the repository keeps its own remote.

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
        }
    }
}

impl SyncPrefs {
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
    if lower.starts_with("file://") || Path::new(url).is_absolute() {
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
        GitDetection::Found { .. } => "",
        _ => " Installing git gives access to your SSH agent and credential helpers.",
    };
    let (message, advice) = match error {
        OnboardingError::Git(GitError::Auth { .. }) => (
            "The remote refused the credentials.".to_owned(),
            format!(
                "Check the user name and password or access token, or the SSH key that is loaded.{git_hint}"
            ),
        ),
        OnboardingError::Git(GitError::Network(m)) => (
            format!("The remote could not be reached: {m}"),
            "Check the URL and your network connection, then try again.".to_owned(),
        ),
        OnboardingError::Git(GitError::NotARepo) => (
            "The folder is not a git repository.".to_owned(),
            "Pick the graph folder itself.".to_owned(),
        ),
        OnboardingError::SeparateGitdir(path) => (
            format!(
                "This graph keeps its git data outside the folder ({}).",
                path.display()
            ),
            "Bitacora can copy it into the graph folder. Your original is not touched.".to_owned(),
        ),
        OnboardingError::RemoteMismatch {
            existing,
            requested,
            ..
        } => (
            format!("The graph already syncs to {existing}, not {requested}."),
            "Use the existing remote, or change it with git outside Bitacora.".to_owned(),
        ),
        OnboardingError::DestinationNotEmpty(path) => (
            format!("{} is not empty.", path.display()),
            "Choose an empty or new folder.".to_owned(),
        ),
        OnboardingError::InvalidArgument(what) => (
            format!("The value is not acceptable: {what}"),
            "Check the form fields.".to_owned(),
        ),
        other => (
            other.to_string(),
            "Check the details and try again.".to_owned(),
        ),
    };
    ActionableError { message, advice }
}

#[cfg(test)]
mod tests {
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
