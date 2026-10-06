//! Application startup: logging, directories, runtime, theme, keymap and the window.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use anyhow::Context as _;

use crate::actions::Quit;
use crate::cli::Args;
use crate::paths::AppDirs;
use crate::settings::AppSettings;
use crate::ui::{
    self, App, AppContext as _, Bounds, Focusable as _, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};
use crate::views::panels::register_panels;
use crate::views::workspace::{Workspace, WorkspaceConfig};
use crate::{keymap, logging, theme, tokio_bridge};

/// Default window size.
const DEFAULT_SIZE: (f32, f32) = (1200.0, 800.0);
/// How long `--smoke-test` waits for the first frame before failing.
const SMOKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// Smallest allowed window size.
const MIN_SIZE: (f32, f32) = (640.0, 400.0);

/// Runs the application until the last window closes. Returns an error if startup
/// or window creation fails.
pub fn run(args: Args) -> anyhow::Result<()> {
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

    let failure: Rc<RefCell<Option<anyhow::Error>>> = Rc::new(RefCell::new(None));
    let failure_in_app = failure.clone();
    ui::application().run(move |cx| {
        if let Err(err) = start(cx, &args, &dirs) {
            tracing::error!("startup failed: {err:#}");
            *failure_in_app.borrow_mut() = Some(err);
            cx.quit();
        }
    });
    match failure.borrow_mut().take() {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

fn start(cx: &mut App, args: &Args, dirs: &AppDirs) -> anyhow::Result<()> {
    let started = Instant::now();
    ui::init(cx);
    tokio_bridge::init(cx).context("starting the tokio runtime")?;
    rust_i18n::set_locale("en");
    theme::install(
        cx,
        AppSettings::load(&dirs.settings_file()),
        Some(dirs.settings_file()),
    );
    let bound = keymap::load_with_user(cx, None)?;
    tracing::debug!(bindings = bound, "keymap loaded");
    cx.on_action(|_: &Quit, cx| cx.quit());
    register_panels(cx);
    // Closing the last window ends the app (also on macOS: there is no document
    // lifecycle to keep alive yet).
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
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
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        return Ok(());
    }

    let config = WorkspaceConfig {
        graph_name: args.graph_name(),
        layout_file: Some(dirs.workspace_file()),
        recent_file: Some(dirs.recent_graphs_file()),
        index_data_dir: None,
        initial_page: args.page.clone(),
        mcp_token_path: bitacora_mcp::default_token_path(),
        global_config: None,
        state_dir: Some(dirs.data_dir.clone()),
    };
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(args.window_title().into()),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(DEFAULT_SIZE.0), px(DEFAULT_SIZE.1)),
            cx,
        ))),
        window_min_size: Some(size(px(MIN_SIZE.0), px(MIN_SIZE.1))),
        ..Default::default()
    };
    let smoke = args.smoke_test;
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

    let save_target = workspace.clone();
    cx.on_app_quit(move |cx| {
        save_target.read(cx).save_now(cx);
        // Window-close quits end up here: stop the session in order (final flush) unless the
        // `Quit` action already did.
        let session = save_target.update(cx, |ws, _| ws.take_session());
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
    if let Some(graph) = args.graph.clone() {
        handle.update(cx, |_, window, cx| {
            workspace.update(cx, |ws, cx| ws.open_graph(graph, window, cx));
        })?;
    }

    handle.update(cx, |_, window, cx| {
        if let Some(specs) = window.gpu_specs() {
            tracing::info!(
                device = %specs.device_name,
                driver = %specs.driver_name,
                software = specs.is_software_emulated,
                "gpu"
            );
        }
        workspace.focus_handle(cx).focus(window, cx);
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
