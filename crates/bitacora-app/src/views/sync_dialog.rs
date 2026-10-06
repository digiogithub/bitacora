//! "Enable sync" and "Open graph from a git remote" dialogs (BIT-US-0043, BIT-T-0282).
//!
//! One overlay with two modes. The form is validated by [`crate::sync_prefs::SyncForm`]; the
//! git work (`init`/`remote`/first push, or `clone`) runs on a background thread through
//! `bitacora_sync::onboarding`, so the window never blocks. Failures come back as
//! [`ActionableError`]s ("check the URL and your network"), and a graph that keeps its git data
//! in Logseq's separate gitdir is offered the migration instead of an error.

use std::path::{Path, PathBuf};

use bitacora_sync::GitDetection;
use bitacora_sync::backend::detect_git;
use bitacora_sync::onboarding::{
    EnableOutcome, OnboardingConfig, OnboardingError, RemoteState, clone_graph, enable_sync,
    migrate_separate_gitdir,
};
use rust_i18n::t;

use crate::credentials::CredentialHub;
use crate::sync_prefs::{
    ActionableError, DEFAULT_BRANCH, FieldError, FormErrors, SyncForm, SyncPrefs, actionable,
};
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::input::{Input, InputEvent, InputState};
use crate::ui::{
    ActiveTheme as _, AppContext as _, Context, Disableable as _, Entity, EventEmitter,
    FluentBuilder as _, FocusHandle, Focusable, IconName, InteractiveElement as _, IntoElement,
    ParentElement as _, PathPromptOptions, Render, Sizable as _, Styled as _, Subscription, Task,
    Window, div, h_flex, px, v_flex,
};
use crate::views::modal::{labelled, modal, title_bar};

/// What the dialog is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogMode {
    /// Connect the open graph to a remote.
    Enable,
    /// Clone a graph from a remote into a new folder.
    Clone,
}

/// Where the dialog stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Waiting for input.
    Editing,
    /// Git is working.
    Running,
    /// The operation failed; `migrate` offers the separate-gitdir migration.
    Failed {
        /// What happened and what to try.
        error: ActionableError,
        /// The failure is the separate gitdir layout.
        migrate: bool,
    },
}

/// What the dialog tells the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncDialogEvent {
    /// The graph is connected; save the preferences and restart the session with sync on.
    Enabled {
        /// The graph folder.
        root: PathBuf,
        /// Preferences to store (enabled).
        prefs: SyncPrefs,
        /// The remote already had history: the first sync merges it.
        needs_merge: bool,
    },
    /// The graph was cloned; open it with these preferences.
    Cloned {
        /// The new graph folder.
        root: PathBuf,
        /// Preferences to store (enabled).
        prefs: SyncPrefs,
    },
    /// The dialog closed.
    Closed,
}

/// Result of the background git work.
#[derive(Debug)]
enum OpResult {
    Enabled(Box<EnableOutcome>),
    Cloned,
    Failed {
        error: ActionableError,
        migrate: bool,
    },
}

struct Op {
    mode: DialogMode,
    graph: Option<PathBuf>,
    form: SyncForm,
    cli: bitacora_sync::CliConfig,
    migrate: bool,
}

fn run_op(op: &Op) -> OpResult {
    let detection = detect_git(None);
    let mut config = OnboardingConfig::new(detection.clone());
    config.cli = op.cli.clone();
    config.identity = op.form.identity();
    let url = op.form.remote_url.trim();
    let branch = op.form.branch.trim();
    let fail = |e: &OnboardingError, detection: &GitDetection| OpResult::Failed {
        migrate: matches!(e, OnboardingError::SeparateGitdir(_)),
        error: actionable(e, detection),
    };
    match op.mode {
        DialogMode::Enable => {
            let Some(graph) = &op.graph else {
                return OpResult::Failed {
                    error: ActionableError {
                        message: t!("sync.dialog.no_graph").to_string(),
                        advice: String::new(),
                    },
                    migrate: false,
                };
            };
            if op.migrate
                && let Err(e) = migrate_separate_gitdir(graph)
            {
                return fail(&e, &detection);
            }
            match enable_sync(graph, url, branch, &config) {
                Ok(outcome) => OpResult::Enabled(Box::new(outcome)),
                Err(e) => fail(&e, &detection),
            }
        }
        DialogMode::Clone => {
            let dest = Path::new(op.form.destination.trim());
            match clone_graph(url, dest, &config) {
                Ok(_) => OpResult::Cloned,
                Err(e) => fail(&e, &detection),
            }
        }
    }
}

/// `https://host/me/notes.git` -> `notes`; the folder name offered for a clone.
pub fn repo_name_from_url(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':']).next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    (!name.is_empty()).then(|| name.to_owned())
}

/// The overlay.
pub struct SyncDialog {
    mode: Option<DialogMode>,
    graph: Option<PathBuf>,
    parent_dir: PathBuf,
    url: Entity<InputState>,
    branch: Entity<InputState>,
    name: Entity<InputState>,
    email: Entity<InputState>,
    device: Entity<InputState>,
    dest: Entity<InputState>,
    auto_dest: Option<String>,
    show_errors: bool,
    phase: Phase,
    hub: Option<std::sync::Arc<CredentialHub>>,
    task: Option<Task<()>>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl std::fmt::Debug for SyncDialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncDialog")
            .field("mode", &self.mode)
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

impl EventEmitter<SyncDialogEvent> for SyncDialog {}

impl Focusable for SyncDialog {
    fn focus_handle(&self, _: &crate::ui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl SyncDialog {
    /// A closed dialog.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = |cx: &mut Context<Self>, window: &mut Window, placeholder: String| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        let url = field(cx, window, t!("sync.dialog.url_placeholder").to_string());
        let branch = field(cx, window, DEFAULT_BRANCH.to_owned());
        let name = field(cx, window, t!("sync.dialog.name_placeholder").to_string());
        let email = field(cx, window, t!("sync.dialog.email_placeholder").to_string());
        let device = field(cx, window, t!("sync.dialog.device_placeholder").to_string());
        let dest = field(cx, window, t!("sync.dialog.dest_placeholder").to_string());
        let subscriptions = vec![
            cx.subscribe_in(&url, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.suggest_destination(window, cx);
                    cx.notify();
                }
            }),
            cx.subscribe(&dest, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        Self {
            mode: None,
            graph: None,
            parent_dir: std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default(),
            url,
            branch,
            name,
            email,
            device,
            dest,
            auto_dest: None,
            show_errors: false,
            phase: Phase::Editing,
            hub: None,
            task: None,
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// The mode shown, `None` while closed.
    pub fn mode(&self) -> Option<DialogMode> {
        self.mode
    }

    /// Where the dialog stands.
    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    /// Whether validation messages are visible (after a submit attempt).
    pub fn shows_errors(&self) -> bool {
        self.show_errors
    }

    /// Opens the "Enable sync" form for the graph at `graph`.
    pub fn open_enable(
        &mut self,
        graph: PathBuf,
        prefs: &SyncPrefs,
        hub: Option<std::sync::Arc<CredentialHub>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mode = Some(DialogMode::Enable);
        self.graph = Some(graph);
        self.hub = hub;
        self.fill(prefs, window, cx);
        self.dest.update(cx, |i, cx| i.set_value("", window, cx));
        self.auto_dest = None;
        self.begin(window, cx);
    }

    /// Opens the "Open graph from git remote" form; clones land in a folder inside
    /// `parent_dir` unless the user picks another.
    pub fn open_clone(
        &mut self,
        parent_dir: PathBuf,
        hub: Option<std::sync::Arc<CredentialHub>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mode = Some(DialogMode::Clone);
        self.graph = None;
        self.parent_dir = parent_dir;
        self.hub = hub;
        self.fill(&SyncPrefs::default(), window, cx);
        self.dest.update(cx, |i, cx| i.set_value("", window, cx));
        self.auto_dest = None;
        self.begin(window, cx);
    }

    fn fill(&mut self, prefs: &SyncPrefs, window: &mut Window, cx: &mut Context<Self>) {
        let set = |input: &Entity<InputState>,
                   value: &str,
                   window: &mut Window,
                   cx: &mut Context<Self>| {
            input.update(cx, |i, cx| i.set_value(value.to_owned(), window, cx));
        };
        set(&self.url, &prefs.remote_url, window, cx);
        set(&self.branch, &prefs.branch, window, cx);
        set(
            &self.name,
            prefs.author_name.as_deref().unwrap_or(""),
            window,
            cx,
        );
        set(
            &self.email,
            prefs.author_email.as_deref().unwrap_or(""),
            window,
            cx,
        );
        set(&self.device, &prefs.device, window, cx);
    }

    fn begin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_errors = false;
        self.phase = Phase::Editing;
        self.task = None;
        let focus = self.url.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Closes the dialog (a running operation keeps going but its result is dropped).
    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode.take().is_some() {
            self.task = None;
            self.phase = Phase::Editing;
            cx.emit(SyncDialogEvent::Closed);
            let _ = window;
            cx.notify();
        }
    }

    /// The form as typed.
    pub fn form(&self, cx: &crate::ui::App) -> SyncForm {
        let get = |input: &Entity<InputState>| input.read(cx).value().to_string();
        SyncForm {
            remote_url: get(&self.url),
            branch: get(&self.branch),
            name: get(&self.name),
            email: get(&self.email),
            device: get(&self.device),
            destination: get(&self.dest),
        }
    }

    /// Fills every field (scripting and tests).
    pub fn set_form(&mut self, form: &SyncForm, window: &mut Window, cx: &mut Context<Self>) {
        let set = |input: &Entity<InputState>,
                   value: &str,
                   window: &mut Window,
                   cx: &mut Context<Self>| {
            input.update(cx, |i, cx| i.set_value(value.to_owned(), window, cx));
        };
        set(&self.url, &form.remote_url, window, cx);
        set(&self.branch, &form.branch, window, cx);
        set(&self.name, &form.name, window, cx);
        set(&self.email, &form.email, window, cx);
        set(&self.device, &form.device, window, cx);
        set(&self.dest, &form.destination, window, cx);
    }

    /// Validation of the current form.
    pub fn errors(&self, cx: &crate::ui::App) -> FormErrors {
        self.form(cx).validate(self.mode == Some(DialogMode::Clone))
    }

    fn suggest_destination(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Some(DialogMode::Clone) {
            return;
        }
        let current = self.dest.read(cx).value().to_string();
        let untouched = current.is_empty() || self.auto_dest.as_deref() == Some(current.as_str());
        if !untouched {
            return;
        }
        let url = self.url.read(cx).value().to_string();
        let suggestion = repo_name_from_url(&url)
            .map(|name| self.parent_dir.join(name).display().to_string())
            .unwrap_or_default();
        self.auto_dest = (!suggestion.is_empty()).then(|| suggestion.clone());
        self.dest
            .update(cx, |i, cx| i.set_value(suggestion, window, cx));
    }

    /// Validates the form and, when it is fine, starts the git work.
    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start(false, window, cx);
    }

    /// Copies the separate gitdir into the graph folder, then connects (after the
    /// "migration required" failure).
    pub fn migrate_and_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start(true, window, cx);
    }

    fn start(&mut self, migrate: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mode) = self.mode else { return };
        if matches!(self.phase, Phase::Running) {
            return;
        }
        self.show_errors = true;
        let form = self.form(cx);
        if !form.validate(mode == DialogMode::Clone).is_empty() {
            cx.notify();
            return;
        }
        let op = Op {
            mode,
            graph: self.graph.clone(),
            form: form.clone(),
            cli: self
                .hub
                .as_ref()
                .map(|h| {
                    h.reset_cancel();
                    h.cli_config()
                })
                .unwrap_or_default(),
            migrate,
        };
        self.phase = Phase::Running;
        let background = cx.background_spawn(async move { run_op(&op) });
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = background.await;
            let _ = this.update_in(cx, |dialog, window, cx| {
                dialog.finish(&form, result, window, cx);
            });
        }));
        cx.notify();
    }

    fn finish(
        &mut self,
        form: &SyncForm,
        result: OpResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.task = None;
        let prefs = SyncPrefs {
            enabled: true,
            remote_url: form.remote_url.trim().to_owned(),
            branch: form.branch.trim().to_owned(),
            device: form.device.trim().to_owned(),
            author_name: form.identity().map(|i| i.name),
            author_email: form.identity().map(|i| i.email),
        };
        match result {
            OpResult::Enabled(outcome) => {
                let root = self.graph.clone().unwrap_or_default();
                let needs_merge = matches!(outcome.remote, RemoteState::NeedsMerge { .. });
                self.mode = None;
                self.phase = Phase::Editing;
                cx.emit(SyncDialogEvent::Enabled {
                    root,
                    prefs,
                    needs_merge,
                });
                cx.emit(SyncDialogEvent::Closed);
            }
            OpResult::Cloned => {
                self.mode = None;
                self.phase = Phase::Editing;
                cx.emit(SyncDialogEvent::Cloned {
                    root: PathBuf::from(form.destination.trim()),
                    prefs,
                });
                cx.emit(SyncDialogEvent::Closed);
            }
            OpResult::Failed { error, migrate } => {
                self.phase = Phase::Failed { error, migrate };
                let _ = window;
            }
        }
        cx.notify();
    }

    fn browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(t!("sync.dialog.dest_prompt").to_string().into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            // A cancelled dialog, a closed channel or a portal error all mean "nothing chosen".
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update_in(cx, |dialog, window, cx| {
                    let chosen = repo_name_from_url(&dialog.url.read(cx).value())
                        .map_or(path.clone(), |name| path.join(name));
                    dialog.parent_dir = path;
                    dialog.dest.update(cx, |i, cx| {
                        i.set_value(chosen.display().to_string(), window, cx)
                    });
                });
            }
        })
        .detach();
    }

    fn field(
        &self,
        theme: &crate::ui::theme::Theme,
        label: String,
        input: &Entity<InputState>,
        error: Option<FieldError>,
    ) -> crate::ui::AnyElement {
        let show = self.show_errors;
        v_flex()
            .gap_1()
            .child(labelled(theme, label, Input::new(input)))
            .when_some(error.filter(|_| show), |col, error| {
                col.child(
                    div()
                        .text_xs()
                        .text_color(theme.danger)
                        .child(rust_i18n::t!(error.key()).to_string()),
                )
            })
            .into_any_element()
    }
}

impl Render for SyncDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(mode) = self.mode else {
            return div().into_any_element();
        };
        let theme = cx.theme().clone();
        let errors = self.errors(cx);
        let running = matches!(self.phase, Phase::Running);
        let this = cx.entity();
        let dismiss = this.clone();

        let title = match mode {
            DialogMode::Enable => t!("sync.dialog.enable_title"),
            DialogMode::Clone => t!("sync.dialog.clone_title"),
        }
        .to_string();
        let intro = match mode {
            DialogMode::Enable => t!("sync.dialog.enable_intro"),
            DialogMode::Clone => t!("sync.dialog.clone_intro"),
        }
        .to_string();

        let mut body = v_flex().gap_3().p_4().child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(intro),
        );
        body = body.child(self.field(
            &theme,
            t!("sync.dialog.url").to_string(),
            &self.url,
            errors.remote_url,
        ));
        if mode == DialogMode::Clone {
            let browse = this.clone();
            body = body.child(
                h_flex()
                    .gap_2()
                    .items_end()
                    .child(div().flex_1().child(self.field(
                        &theme,
                        t!("sync.dialog.dest").to_string(),
                        &self.dest,
                        errors.destination,
                    )))
                    .child(
                        Button::new("sync-dest-browse")
                            .icon(IconName::FolderOpen)
                            .label(t!("sync.dialog.browse").to_string())
                            .on_click(move |_, window, cx| {
                                browse.update(cx, |d, cx| d.browse(window, cx));
                            }),
                    ),
            );
        }
        body = body
            .child(self.field(
                &theme,
                t!("sync.dialog.branch").to_string(),
                &self.branch,
                errors.branch,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().child(self.field(
                        &theme,
                        t!("sync.dialog.name").to_string(),
                        &self.name,
                        errors.identity,
                    )))
                    .child(div().flex_1().child(self.field(
                        &theme,
                        t!("sync.dialog.email").to_string(),
                        &self.email,
                        None,
                    ))),
            )
            .child(self.field(
                &theme,
                t!("sync.dialog.device").to_string(),
                &self.device,
                errors.device,
            ))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t!("sync.dialog.identity_note").to_string()),
            );

        match &self.phase {
            Phase::Running => {
                body = body.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(crate::ui::icon(IconName::LoaderCircle).small())
                        .child(match mode {
                            DialogMode::Enable => t!("sync.dialog.running_enable").to_string(),
                            DialogMode::Clone => t!("sync.dialog.running_clone").to_string(),
                        }),
                );
            }
            Phase::Failed { error, migrate } => {
                let migrate_this = this.clone();
                body = body.child(
                    v_flex()
                        .id("sync-dialog-error")
                        .gap_1()
                        .p_2()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(theme.danger)
                        .child(div().text_sm().child(error.message.clone()))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(error.advice.clone()),
                        )
                        .when(*migrate, |col| {
                            col.child(
                                Button::new("sync-migrate")
                                    .small()
                                    .label(t!("sync.dialog.migrate").to_string())
                                    .on_click(move |_, window, cx| {
                                        migrate_this
                                            .update(cx, |d, cx| d.migrate_and_submit(window, cx));
                                    }),
                            )
                        }),
                );
            }
            Phase::Editing => {}
        }

        let submit = this.clone();
        let cancel = this.clone();
        let footer = h_flex()
            .px_4()
            .py_3()
            .gap_2()
            .justify_end()
            .border_t_1()
            .border_color(theme.border)
            .child(
                Button::new("sync-dialog-cancel")
                    .label(t!("sync.dialog.cancel").to_string())
                    .on_click(move |_, window, cx| {
                        cancel.update(cx, |d, cx| d.close(window, cx));
                    }),
            )
            .child(
                Button::new("sync-dialog-submit")
                    .primary()
                    .disabled(running)
                    .label(match mode {
                        DialogMode::Enable => t!("sync.dialog.enable").to_string(),
                        DialogMode::Clone => t!("sync.dialog.clone").to_string(),
                    })
                    .on_click(move |_, window, cx| {
                        submit.update(cx, |d, cx| d.submit(window, cx));
                    }),
            );

        let card = v_flex()
            .child(title_bar(&theme, title, div()))
            .child(body)
            .child(footer);
        modal(
            "sync-dialog",
            &theme,
            560.,
            move |window, cx| dismiss.update(cx, |d, cx| d.close(window, cx)),
            card,
        )
    }
}
