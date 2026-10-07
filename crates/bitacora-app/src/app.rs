//! Application startup: logging, directories, runtime, theme, keymap and the window.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Instant;

use anyhow::Context as _;
use bitacora_runtime::instance::Launch;

use crate::actions::Quit;
use crate::cli::Args;
use crate::instance::{self, Startup};
use crate::paths::AppDirs;
use crate::settings::AppSettings;
use crate::ui::{
    self, AnyWindowHandle, App, AppContext as _, Bounds, Entity, Focusable as _, Global, px, size,
};
use crate::views::panels::register_panels;
use crate::views::workspace::{Workspace, WorkspaceConfig};
use crate::{crash, keymap, logging, theme, tokio_bridge, update};

/// Default window size.
const DEFAULT_SIZE: (f32, f32) = (1200.0, 800.0);
/// How long `--smoke-test` waits for the first frame before failing.
const SMOKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// Smallest allowed window size.
const MIN_SIZE: (f32, f32) = (640.0, 400.0);

/// The workspace of the (possibly closed) main window. Kept as a global so the session, and with
/// it the MCP server, stays alive when the last window closes in background mode (BIT-T-0155).
struct CurrentWorkspace(Entity<Workspace>);

impl Global for CurrentWorkspace {}

/// Services handed to [`start`].
struct Services {
    launches: Option<mpsc::Receiver<Launch>>,
    crash: bitacora_runtime::crash::CrashConfig,
}

/// Runs the application until the last window closes. Returns an error if startup
/// or window creation fails.
pub fn run(args: Args) -> anyhow::Result<()> {
    crate::perf::init();
    if args.perf_bench {
        crate::perf::enable();
    }
    if let Some(graph) = &args.graph
        && !graph.is_dir()
    {
        anyhow::bail!("graph folder does not exist: {}", graph.display());
    }
    let dirs = AppDirs::from_project_dirs().context("resolving platform directories")?;
    dirs.ensure().context("creating app directories")?;
    let _log_guard =
        logging::init(&dirs.log_dir, args.log_level.as_deref()).context("initializing logging")?;
    logging::log_startup();
    tracing::info!(log_dir = %dirs.log_dir.display(), "app directories ready");

    // Smoke tests and the editor spike must not fight a real instance for the lock.
    let single_instance =
        !(args.smoke_test || args.spike_editor || args.spike_bench || args.perf_bench);
    let mut instance_guard = None;
    let mut launches = None;
    let mut previous_crash_pid = None;
    if single_instance {
        match instance::acquire(&dirs.data_dir, &args).context("single-instance check")? {
            Startup::Forwarded => {
                tracing::info!("launch forwarded to the running instance");
                return Ok(());
            }
            Startup::Owner(mut guard) => {
                launches = guard.take_launches();
                previous_crash_pid = guard.previous_abnormal.as_ref().map(|i| i.pid);
                instance_guard = Some(guard);
            }
        }
    }
    let crash_cfg = crash::config(&dirs);
    crash::init(&crash_cfg, previous_crash_pid);
    let services = Services {
        launches,
        crash: crash_cfg,
    };

    let failure: Rc<RefCell<Option<anyhow::Error>>> = Rc::new(RefCell::new(None));
    let failure_in_app = failure.clone();
    ui::application().run(move |cx| {
        if let Err(err) = start(cx, &args, &dirs, services) {
            tracing::error!("startup failed: {err:#}");
            *failure_in_app.borrow_mut() = Some(err);
            cx.quit();
        }
    });
    // Dropping the guard releases the lock and removes the sidecar: a clean exit.
    drop(instance_guard);
    match failure.borrow_mut().take() {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

fn keep_running_in_background(dirs: &AppDirs) -> bool {
    AppSettings::load(&dirs.settings_file()).keep_running_in_background
}

fn start(cx: &mut App, args: &Args, dirs: &AppDirs, services: Services) -> anyhow::Result<()> {
    let started = Instant::now();
    ui::init(cx);
    crate::fonts::register(cx);
    tokio_bridge::init(cx).context("starting the tokio runtime")?;
    theme::install(
        cx,
        AppSettings::load(&dirs.settings_file()),
        Some(dirs.settings_file()),
    );
    let user_keymap = std::fs::read_to_string(dirs.keymap_file()).ok();
    let keymap_report = keymap::load_with_user_report(cx, user_keymap.as_deref())?;
    tracing::debug!(bindings = keymap_report.bound, "keymap loaded");
    cx.on_action(|_: &Quit, cx| cx.quit());
    crate::menus::install(
        cx,
        crate::recent::RecentGraphs::load(&dirs.recent_graphs_file()).graphs(),
    );
    register_panels(cx);
    // Closing the last window ends the app unless "keep running in background" is on; Quit
    // always stops everything (BIT-T-0155).
    let dirs_for_close = dirs.clone();
    cx.on_window_closed(move |cx, _| {
        if !cx.windows().is_empty() {
            return;
        }
        if keep_running_in_background(&dirs_for_close) {
            if let Some(current) = cx.try_global::<CurrentWorkspace>() {
                current.0.read(cx).save_now(cx);
            }
            tracing::info!("last window closed; staying alive in the background");
        } else {
            cx.quit();
        }
    })
    .detach();

    if args.spike_editor || args.spike_bench {
        let options = crate::spike::block_editor::SpikeOptions {
            blocks: args.spike_blocks,
            page: args.spike_page.clone(),
            bench: args.spike_bench,
        };
        crate::spike::block_editor::open(cx, &options, started)?;
        return Ok(());
    }

    let (handle, workspace) = open_workspace(cx, args, dirs)?;
    crate::perf::mark("window_opened");

    cx.on_app_quit(move |cx| {
        let session = cx
            .try_global::<CurrentWorkspace>()
            .map(|current| current.0.clone())
            .and_then(|ws| {
                ws.read(cx).save_now(cx);
                // Window-close quits end up here: stop the session in order (final flush)
                // unless the `Quit` action already did.
                ws.update(cx, |ws, _| ws.take_session())
            });
        let shutdown = session.map(|s| {
            cx.background_executor()
                .spawn(async move { s.shutdown_with_report(crate::session::SHUTDOWN_BUDGET) })
        });
        async move {
            if let Some(task) = shutdown
                && let Some(report) = task.await
                && let Some(problem) = crate::views::workspace::shutdown_problem(&report)
            {
                tracing::error!("{problem}");
            }
        }
    })
    .detach();

    if let Some(rx) = services.launches {
        spawn_launch_listener(cx, rx, dirs.clone());
    }
    update::start(handle, dirs, cx);
    crash::show_pending(handle, &services.crash, cx);

    let smoke = args.smoke_test;
    let perf_bench = args.perf_bench;
    handle.update(cx, |_, window, cx| {
        if perf_bench {
            crate::perf::bench::start(workspace.clone(), window, cx);
        }
        if !keymap_report.problems.is_empty() {
            ui::notify(
                window,
                cx,
                ui::Level::Warning,
                format!(
                    "keymap.json: {} entries ignored ({})",
                    keymap_report.problems.len(),
                    keymap_report.problems[0]
                ),
            );
        }
        if let Some(specs) = window.gpu_specs() {
            tracing::info!(
                device = %specs.device_name,
                driver = %specs.driver_name,
                software = specs.is_software_emulated,
                "gpu"
            );
        }
        if smoke {
            // Watchdog: a compositor that never delivers frames (no output, no GPU)
            // must fail the smoke test instead of hanging CI.
            cx.spawn(async move |cx| {
                cx.background_executor().timer(SMOKE_TIMEOUT).await;
                tracing::error!("smoke test: no frame rendered within {SMOKE_TIMEOUT:?}");
                std::process::exit(2);
            })
            .detach();
            // Wait for two frames so the first real paint has happened.
            window.on_next_frame(move |window, _| {
                window.on_next_frame(move |_, cx| {
                    tracing::info!(
                        "first frame rendered in {} ms",
                        started.elapsed().as_millis()
                    );
                    cx.quit();
                });
            });
        }
    })?;
    Ok(())
}

/// Opens the main window with a fresh workspace, makes it the current one and opens the graph
/// named by `args`, if any.
fn open_workspace(
    cx: &mut App,
    args: &Args,
    dirs: &AppDirs,
) -> anyhow::Result<(AnyWindowHandle, Entity<Workspace>)> {
    let config = WorkspaceConfig {
        graph_name: args.graph_name(),
        layout_file: Some(dirs.workspace_file()),
        recent_file: Some(dirs.recent_graphs_file()),
        index_data_dir: None,
        initial_page: args.page.clone(),
        mcp_token_path: bitacora_mcp::default_token_path(),
        global_config: None,
        state_dir: Some(dirs.data_dir.clone()),
        system_credentials: true,
        keymap_file: Some(dirs.keymap_file()),
        mcp_secrets: bitacora_mcp::os_keychain(),
        pando_settings_path: bitacora_runtime::default_pando_settings_path(),
    };
    let options = crate::views::title_bar::main_window_options(
        args.window_title(),
        Bounds::centered(None, size(px(DEFAULT_SIZE.0), px(DEFAULT_SIZE.1)), cx),
        size(px(MIN_SIZE.0), px(MIN_SIZE.1)),
    );
    let (handle, workspace) = ui::open_main_window(options, cx, |window, cx| {
        let workspace = cx.new(|cx| Workspace::new(config, window, cx));
        window
            .observe_window_appearance(|window, cx| {
                if theme::settings(cx).mode == crate::settings::ThemePreference::System {
                    theme::apply(cx, Some(window));
                }
            })
            .detach();
        workspace
    })
    .context("opening the main window")?;
    tracing::info!(title = %args.window_title(), "window opened");
    cx.set_global(CurrentWorkspace(workspace.clone()));
    handle.update(cx, |_, window, cx| {
        if let Some(graph) = args.graph.clone() {
            workspace.update(cx, |ws, cx| ws.open_graph(graph, window, cx));
        } else {
            // No `--graph`: reopen the last graph, or fall back to the picker (BIT-US-0165).
            workspace.update(cx, |ws, cx| {
                ws.open_startup_graph(window, cx);
            });
        }
        workspace.focus_handle(cx).focus(window, cx);
    })?;
    Ok((handle, workspace))
}

/// Applies launches forwarded by second invocations (BIT-T-0153).
fn spawn_launch_listener(cx: &mut App, rx: mpsc::Receiver<Launch>, dirs: AppDirs) {
    let (tx, async_rx) = async_channel::unbounded();
    let spawned = std::thread::Builder::new()
        .name("bitacora-launch-bridge".into())
        .spawn(move || {
            while let Ok(launch) = rx.recv() {
                if tx.send_blocking(launch).is_err() {
                    break;
                }
            }
        });
    if let Err(err) = spawned {
        tracing::warn!("cannot forward launches to the UI: {err}");
        return;
    }
    cx.spawn(async move |cx| {
        while let Ok(launch) = async_rx.recv().await {
            let dirs = dirs.clone();
            cx.update(|cx| handle_launch(cx, &dirs, &launch));
        }
    })
    .detach();
}

/// Focuses the running window and opens the requested graph in it; with no window left
/// (background mode) the old session is shut down and a new window opens.
fn handle_launch(cx: &mut App, dirs: &AppDirs, launch: &Launch) {
    tracing::info!(graph = ?launch.graph, "handling a forwarded launch");
    let args = instance::args_from_launch(launch);
    let Some(current) = cx.try_global::<CurrentWorkspace>().map(|c| c.0.clone()) else {
        return;
    };
    if let Some(window) = cx.windows().first().copied() {
        let _ = window.update(cx, |_, window, cx| {
            window.activate_window();
            if let Some(graph) = args.graph.clone() {
                current.update(cx, |ws, cx| ws.open_graph(graph, window, cx));
            }
        });
        return;
    }
    let session = current.update(cx, |ws, _| ws.take_session());
    let dirs = dirs.clone();
    cx.spawn(async move |cx| {
        if let Some(session) = session {
            cx.background_executor()
                .spawn(async move { session.shutdown_with_report(crate::session::SHUTDOWN_BUDGET) })
                .await;
        }
        cx.update(|cx| {
            if let Err(err) = open_workspace(cx, &args, &dirs) {
                tracing::error!("cannot reopen the window: {err:#}");
            }
        });
    })
    .detach();
}
