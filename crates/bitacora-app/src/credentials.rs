//! Credentials for the sync backends (BIT-US-0046, BIT-T-0291, ADR-023).
//!
//! One [`CredentialHub`] per app: it owns the credential provider chain (OS keyring first, then
//! the in-app prompt), the loopback [`AskpassServer`] that the `bitacora-askpass` helper talks
//! to, and the [`CliConfig`] that makes the system git use that helper. The prompt itself is a
//! [`PromptHandler`] that hands a [`PromptRequest`] to the UI over a channel and blocks the
//! asking thread (the sync engine, an onboarding task) until the modal answers or is dropped.
//! Secrets are never logged: [`Secret`] redacts itself and nothing here formats a credential.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_channel::Sender;
use bitacora_sync::CliConfig;
use bitacora_sync::askpass::{AskpassServer, ProviderAskpass};
use bitacora_sync::credentials::{
    ChainProvider, CredentialKind, CredentialProvider, CredentialRequest, MemoryStore,
    PromptAnswer, PromptHandler, SecretStore,
};

/// How long a prompt waits for the user before it counts as cancelled.
const PROMPT_TIMEOUT: Duration = Duration::from_secs(600);

/// A credential question for the UI. Answer it with [`PromptRequest::answer`] or drop it to
/// cancel (the asking thread then sees "no credential").
#[derive(Debug)]
pub struct PromptRequest {
    /// What is being asked for.
    pub request: CredentialRequest,
    reply: mpsc::Sender<Option<PromptAnswer>>,
}

impl PromptRequest {
    /// Sends the user's answer (`None` = cancelled).
    pub fn answer(self, answer: Option<PromptAnswer>) {
        // The asking thread may have given up (timeout); nothing to report then.
        let _ = self.reply.send(answer);
    }

    /// A request wired to a receiver, for tests.
    pub fn for_test(request: CredentialRequest) -> (Self, mpsc::Receiver<Option<PromptAnswer>>) {
        let (reply, rx) = mpsc::channel();
        (Self { request, reply }, rx)
    }
}

/// [`PromptHandler`] that forwards requests to the UI.
struct UiPrompt {
    tx: Sender<PromptRequest>,
}

impl PromptHandler for UiPrompt {
    fn prompt(&self, request: &CredentialRequest) -> Option<PromptAnswer> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send_blocking(PromptRequest {
                request: request.clone(),
                reply,
            })
            .ok()?;
        rx.recv_timeout(PROMPT_TIMEOUT).ok().flatten()
    }
}

/// Everything credential-related the app shares between its sync features.
pub struct CredentialHub {
    provider: Arc<ChainProvider>,
    /// Keeps the askpass server alive; `None` when it could not start.
    server: Mutex<Option<AskpassServer>>,
    cli: CliConfig,
}

impl std::fmt::Debug for CredentialHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialHub").finish_non_exhaustive()
    }
}

/// The OS keyring where the build has it, memory otherwise.
fn default_store() -> Arc<dyn SecretStore> {
    Arc::new(bitacora_sync::credentials::KeyringStore)
}

/// The askpass helper: a standalone `bitacora-askpass` next to the running executable when it
/// exists, otherwise the running executable itself, which acts as the helper when git starts it
/// with `BITACORA_ASKPASS_MODE=1` (multi-call, BIT-US-0176; releases ship only `bitacora`).
pub fn askpass_helper_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) {
        "bitacora-askpass.exe"
    } else {
        "bitacora-askpass"
    };
    if let Some(candidate) = exe.parent().map(|d| d.join(name))
        && candidate.is_file()
    {
        return Some(candidate);
    }
    Some(exe)
}

impl CredentialHub {
    /// Starts the hub: prompts go to `prompts`, secrets to the OS keyring.
    pub fn start(prompts: Sender<PromptRequest>) -> Self {
        Self::with_store(prompts, default_store(), askpass_helper_path())
    }

    /// Starts the hub with an in-memory store (tests).
    pub fn start_in_memory(prompts: Sender<PromptRequest>, helper: Option<PathBuf>) -> Self {
        Self::with_store(prompts, Arc::new(MemoryStore::new()), helper)
    }

    fn with_store(
        prompts: Sender<PromptRequest>,
        store: Arc<dyn SecretStore>,
        helper: Option<PathBuf>,
    ) -> Self {
        let provider = Arc::new(ChainProvider::new(
            store,
            Some(Arc::new(UiPrompt { tx: prompts })),
        ));
        let mut cli = CliConfig::default();
        let mut server = None;
        if let Some(helper) = helper {
            let handler = Arc::new(ProviderAskpass::new(provider.clone()));
            match AskpassServer::start(handler) {
                Ok(s) => {
                    cli.askpass = Some(helper);
                    cli.askpass_env = s.env();
                    server = Some(s);
                }
                Err(err) => tracing::warn!("askpass server not started: {err}"),
            }
        }
        Self {
            provider,
            server: Mutex::new(server),
            cli,
        }
    }

    /// The chain (store, then prompt), for the built-in backend.
    pub fn provider(&self) -> Arc<dyn CredentialProvider> {
        self.provider.clone()
    }

    /// System-backend tunables with the askpass helper wired in.
    pub fn cli_config(&self) -> CliConfig {
        self.cli.clone()
    }

    /// Whether the system git can prompt through the app.
    pub fn askpass_active(&self) -> bool {
        self.cli.askpass.is_some()
            && self
                .server
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_some()
    }

    /// Forgets the credentials saved for `remote_url` (and the passphrase of `ssh_key`).
    pub fn forget_remote(&self, remote_url: &str, ssh_key: Option<&str>) {
        self.provider.forget_remote(remote_url, ssh_key);
    }

    /// Forgets remembered cancellations: call when the user explicitly retries.
    pub fn reset_cancel(&self) {
        self.provider.reset_cancel();
    }
}

/// Title and field labels for a request (keys in the locale file).
pub fn prompt_kind_key(kind: CredentialKind) -> &'static str {
    match kind {
        CredentialKind::UserPassword => "credentials.kind_password",
        CredentialKind::SshPassphrase => "credentials.kind_passphrase",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitacora_sync::credentials::{Credential, Secret};

    fn request() -> CredentialRequest {
        CredentialRequest {
            url: "https://example.org/n.git".into(),
            username: None,
            kind: CredentialKind::UserPassword,
        }
    }

    #[test]
    fn prompt_blocks_until_the_ui_answers_and_remembers() {
        let (tx, rx) = async_channel::unbounded();
        let hub = CredentialHub::start_in_memory(tx, None);
        let provider = hub.provider();
        let asker = std::thread::spawn({
            let provider = provider.clone();
            move || provider.credential(&request())
        });
        let asked = rx.recv_blocking().expect("prompt");
        assert_eq!(asked.request.url, "https://example.org/n.git");
        asked.answer(Some(PromptAnswer {
            credential: Credential {
                username: "ana".into(),
                secret: Secret::new("token"),
            },
            remember: true,
        }));
        let got = asker.join().expect("join").expect("credential");
        assert_eq!(got.username, "ana");
        // Remembered: the second ask is served from the store without a prompt.
        let again = provider.credential(&request()).expect("stored");
        assert_eq!(again.secret.expose(), "token");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn dropping_the_request_cancels_without_a_retry_storm() {
        let (tx, rx) = async_channel::unbounded();
        let hub = CredentialHub::start_in_memory(tx, None);
        let provider = hub.provider();
        let asker = std::thread::spawn({
            let provider = provider.clone();
            move || provider.credential(&request())
        });
        drop(rx.recv_blocking().expect("prompt"));
        assert!(asker.join().expect("join").is_none());
        // The cancellation is remembered: no second modal until the user retries.
        assert!(provider.credential(&request()).is_none());
        assert!(rx.try_recv().is_err());
        hub.reset_cancel();
        let again = std::thread::spawn({
            let provider = provider.clone();
            move || provider.credential(&request())
        });
        let asked = rx.recv_blocking().expect("prompt after reset");
        asked.answer(None);
        assert!(again.join().expect("join").is_none());
    }

    #[test]
    fn helper_and_server_configure_the_system_git() {
        let (tx, _rx) = async_channel::unbounded();
        let helper = std::env::temp_dir().join("bitacora-askpass-test-helper");
        let hub = CredentialHub::start_in_memory(tx, Some(helper.clone()));
        assert!(hub.askpass_active());
        let cli = hub.cli_config();
        assert_eq!(cli.askpass, Some(helper));
        assert!(
            cli.askpass_env
                .iter()
                .any(|(k, _)| k == bitacora_sync::askpass::ENV_ADDR)
        );
        assert!(
            cli.askpass_env
                .iter()
                .any(|(k, _)| k == bitacora_sync::askpass::ENV_TOKEN)
        );
    }
}
