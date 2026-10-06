//! Logging: stderr plus a daily rolling file in the platform cache dir.

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::MakeWriterExt as _;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// Number of daily log files kept.
pub const MAX_LOG_FILES: usize = 7;

/// Default filter directive: verbose for our own crate in debug builds.
pub fn default_directive() -> &'static str {
    if cfg!(debug_assertions) {
        "info,bitacora_app=debug"
    } else {
        "info"
    }
}

/// Builds the filter. Precedence: `RUST_LOG` > `--log-level` > built-in default.
pub fn build_filter(rust_log: Option<&str>, cli_level: Option<&str>) -> EnvFilter {
    let parse = |s: &str| EnvFilter::try_new(s).ok();
    rust_log
        .filter(|s| !s.trim().is_empty())
        .and_then(parse)
        .or_else(|| cli_level.and_then(parse))
        .unwrap_or_else(|| EnvFilter::new(default_directive()))
}

/// Keeps the non-blocking file writer alive; drop it at process exit to flush.
#[derive(Debug)]
pub struct LogGuard {
    _file: WorkerGuard,
}

/// Errors initializing logging.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    /// The rolling appender could not be created.
    #[error("cannot create the log file appender: {0}")]
    Appender(#[from] tracing_appender::rolling::InitError),
    /// A global subscriber is already installed.
    #[error("a global tracing subscriber is already installed: {0}")]
    Subscriber(#[from] tracing_subscriber::util::TryInitError),
}

/// Installs the global subscriber (stderr + rolling file in `log_dir`).
pub fn init(log_dir: &Path, cli_level: Option<&str>) -> Result<LogGuard, LogError> {
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("bitacora")
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(log_dir)?;
    let (file_writer, guard) = tracing_appender::non_blocking(appender);
    let filter = build_filter(std::env::var("RUST_LOG").ok().as_deref(), cli_level);
    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr.and(file_writer))
                .with_ansi(false),
        )
        .try_init()?;
    Ok(LogGuard { _file: guard })
}

/// Logs the startup line: version, OS and architecture.
pub fn log_startup() {
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        "bitacora starting"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_log_overrides_cli_level_and_default() {
        assert_eq!(
            build_filter(Some("trace"), Some("warn")).to_string(),
            "trace"
        );
        assert_eq!(build_filter(None, Some("warn")).to_string(), "warn");
        assert_eq!(build_filter(Some("  "), Some("warn")).to_string(), "warn");
        assert_eq!(
            build_filter(None, None).to_string(),
            EnvFilter::new(default_directive()).to_string()
        );
    }

    #[test]
    fn invalid_directives_fall_back() {
        let filter = build_filter(Some("=[bad"), Some("also bad ["));
        assert_eq!(
            filter.to_string(),
            EnvFilter::new(default_directive()).to_string()
        );
    }
}
