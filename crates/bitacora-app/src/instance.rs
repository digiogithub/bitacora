//! Single-instance glue for the desktop app (BIT-US-0086).
//!
//! The lock and IPC live in `bitacora_runtime::instance` (shared with `bitacora-cli serve`).
//! A second launch forwards `--graph` / `--page` to the running app and exits 0; a headless
//! server holding the instance is reported as an error.

use bitacora_runtime::instance::{Acquire, InstanceError, InstanceKind, Launch, Primary, forward};

use crate::cli::Args;

/// What the launcher should do.
#[derive(Debug)]
pub enum Startup {
    /// This process owns the instance; keep the guard alive until exit.
    Owner(Box<Primary>),
    /// The request was handed to the running instance; exit with success.
    Forwarded,
}

/// The launch request described by `args`: the graph is made absolute so the running instance
/// (another working directory) resolves the same folder.
pub fn launch_from_args(args: &Args) -> Launch {
    Launch {
        graph: args
            .graph
            .as_ref()
            .map(|g| g.canonicalize().unwrap_or_else(|_| g.clone())),
        page: args.page.clone(),
    }
}

/// Arguments equivalent to a forwarded launch.
pub fn args_from_launch(launch: &Launch) -> Args {
    use clap::Parser as _;
    let mut args = Args::parse_from(["bitacora"]);
    args.graph.clone_from(&launch.graph);
    args.page.clone_from(&launch.page);
    args
}

/// Takes the instance lock in `dir`, or forwards `args` to the owner.
pub fn acquire(dir: &std::path::Path, args: &Args) -> Result<Startup, InstanceError> {
    match Primary::acquire(dir, InstanceKind::App, true)? {
        Acquire::Primary(guard) => Ok(Startup::Owner(guard)),
        Acquire::Running(info) => {
            tracing::info!(
                pid = info.pid,
                "another instance runs; forwarding the launch"
            );
            forward(&info, &launch_from_args(args))?;
            Ok(Startup::Forwarded)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser as _;

    #[test]
    fn launch_roundtrips_through_args() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let args = Args::parse_from([
            "bitacora",
            "--graph",
            tmp.path().to_str().expect("utf8"),
            "--page",
            "Inbox",
        ]);
        let launch = launch_from_args(&args);
        assert_eq!(launch.page.as_deref(), Some("Inbox"));
        let back = args_from_launch(&launch);
        assert_eq!(back.page, args.page);
        assert_eq!(
            back.graph.as_deref().map(|p| p.canonicalize().ok()),
            Some(tmp.path().canonicalize().ok())
        );
    }

    #[test]
    fn second_launch_is_forwarded_to_the_owner_and_a_headless_owner_is_an_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let args = Args::parse_from(["bitacora", "--page", "Today"]);
        let Startup::Owner(mut owner) = acquire(tmp.path(), &args).expect("first") else {
            panic!("first launch must own the instance");
        };
        let rx = owner.take_launches().expect("listener");
        assert!(matches!(
            acquire(tmp.path(), &args).expect("second"),
            Startup::Forwarded
        ));
        let got = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("forwarded");
        assert_eq!(got.page.as_deref(), Some("Today"));
        drop(owner);

        let headless =
            match Primary::acquire(tmp.path(), InstanceKind::Headless, false).expect("headless") {
                Acquire::Primary(p) => p,
                Acquire::Running(_) => panic!("lock must be free after the owner dropped"),
            };
        let err = acquire(tmp.path(), &args).expect_err("headless owner");
        assert!(err.to_string().contains("headless"), "{err}");
        drop(headless);
    }
}
