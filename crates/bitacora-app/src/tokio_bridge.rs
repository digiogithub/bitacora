//! Bridge between GPUI's executors and a dedicated tokio runtime (ADR-010, ADR-012).
//!
//! GPUI runs futures on its own foreground/background executors, which are not
//! tokio: anything that needs a tokio reactor (axum, `rmcp`, tokio timers) panics
//! with "no reactor running" if polled there. [`init`] creates a small multi-thread
//! runtime and stores it as a GPUI global; [`spawn`] runs a future on it and hands
//! back a GPUI [`Task`] that aborts the tokio task when dropped.
//!
//! This module is an original implementation of the pattern described in the
//! Apache-2.0 `gpui_tokio` crate of Zed (see `NOTICE`); no code was copied.
//!
//! Rules enforced by `clippy.toml` (`disallowed-methods`): no `tokio::spawn` from
//! GPUI tasks (use [`spawn`]) and no `block_on` on the main thread.

use std::future::Future;
use std::time::Duration;

use tokio::runtime::{Builder, Handle, Runtime};
use tokio::task::{AbortHandle, JoinError};

use crate::ui::{App, AppContext as _, Global, Task};

/// How long the runtime may take to wind down when the app quits.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// The runtime kept as a GPUI global.
struct TokioGlobal {
    /// Taken (and shut down) on app quit.
    runtime: Option<Runtime>,
    handle: Handle,
}

impl Global for TokioGlobal {}

/// Number of tokio worker threads: 2 to 4, depending on the machine.
pub fn default_worker_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(2, 4)
}

/// Creates the runtime and stores it as a global; shuts it down when the app quits.
pub fn init(cx: &mut App) -> std::io::Result<()> {
    init_with_workers(cx, default_worker_threads())
}

/// Like [`init`] with an explicit worker count.
pub fn init_with_workers(cx: &mut App, workers: usize) -> std::io::Result<()> {
    let runtime = Builder::new_multi_thread()
        .worker_threads(workers)
        .thread_name("bitacora-tokio")
        .enable_all()
        .build()?;
    let handle = runtime.handle().clone();
    cx.set_global(TokioGlobal {
        runtime: Some(runtime),
        handle,
    });
    cx.on_app_quit(|cx| {
        let runtime = cx
            .try_global::<TokioGlobal>()
            .is_some()
            .then(|| cx.global_mut::<TokioGlobal>().runtime.take())
            .flatten();
        // Shutting down blocks until workers stop (bounded by the timeout), so do
        // it off the main thread.
        cx.background_spawn(async move {
            if let Some(runtime) = runtime {
                runtime.shutdown_timeout(SHUTDOWN_TIMEOUT);
                tracing::debug!("tokio runtime shut down");
            }
        })
    })
    .detach();
    Ok(())
}

/// A handle to the runtime, for crates that spawn their own long-lived servers
/// (for example the MCP server).
///
/// # Panics
///
/// Panics if [`init`] was not called, which is a programming error at app start.
pub fn handle(cx: &App) -> Handle {
    cx.global::<TokioGlobal>().handle.clone()
}

/// Aborts the tokio task when dropped.
struct AbortOnDrop(AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Runs `fut` on the tokio runtime.
///
/// The returned GPUI task resolves with the future's output (or a [`JoinError`] if
/// it panicked or was cancelled). Dropping the GPUI task aborts the tokio task.
pub fn spawn<Fut, R>(cx: &App, fut: Fut) -> Task<Result<R, JoinError>>
where
    Fut: Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    let join = cx.global::<TokioGlobal>().handle.spawn(fut);
    let guard = AbortOnDrop(join.abort_handle());
    cx.background_spawn(async move {
        let result = join.await;
        drop(guard);
        result
    })
}

/// Like [`spawn`] for fallible futures: flattens the join error into `anyhow`.
pub fn spawn_result<Fut, R>(cx: &App, fut: Fut) -> Task<anyhow::Result<R>>
where
    Fut: Future<Output = anyhow::Result<R>> + Send + 'static,
    R: Send + 'static,
{
    let task = spawn(cx, fut);
    cx.background_spawn(async move { task.await? })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::ui::testing::{TestAppContext, gpui_test};

    struct SetOnDrop(Arc<AtomicBool>);

    impl Drop for SetOnDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[gpui_test]
    async fn spawned_tokio_future_returns_its_value(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(|cx| init_with_workers(cx, 2)).expect("runtime");
        let task = cx.update(|cx| {
            spawn(cx, async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                42_u32
            })
        });
        assert_eq!(task.await.expect("join"), 42);
    }

    #[gpui_test]
    async fn spawn_result_propagates_errors(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(|cx| init_with_workers(cx, 2)).expect("runtime");
        let task = cx.update(|cx| {
            spawn_result(cx, async {
                tokio::task::yield_now().await;
                Err::<(), _>(anyhow::anyhow!("boom"))
            })
        });
        assert_eq!(task.await.expect_err("error").to_string(), "boom");
    }

    #[gpui_test]
    fn dropping_the_gpui_task_cancels_the_tokio_task(cx: &mut TestAppContext) {
        cx.update(|cx| init_with_workers(cx, 2)).expect("runtime");
        let started = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicBool::new(false));
        let task = cx.update(|cx| {
            let (started, dropped) = (started.clone(), dropped.clone());
            spawn(cx, async move {
                let _guard = SetOnDrop(dropped);
                started.store(true, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_secs(3600)).await;
            })
        });
        wait_for(&started, "started");
        assert!(!dropped.load(Ordering::SeqCst));
        drop(task);
        // The test scheduler only releases a cancelled task's future when it runs it.
        cx.run_until_parked();
        wait_for(&dropped, "dropped");
    }

    /// Polls a flag set by another thread, failing after 5 seconds.
    fn wait_for(flag: &AtomicBool, what: &str) {
        for _ in 0..500 {
            if flag.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("`{what}` was not set within 5s");
    }
}
