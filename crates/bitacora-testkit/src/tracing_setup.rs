//! Tracing setup for tests.

use std::sync::Once;

use tracing_subscriber::EnvFilter;

static INIT: Once = Once::new();

/// Install a test-friendly tracing subscriber (idempotent).
///
/// Output goes through the libtest writer, so it is captured per test unless run with
/// `--nocapture`. Honours `RUST_LOG` (default: `warn`). Safe to call from every test in a
/// binary, and safe when another subscriber was already installed.
pub fn init_tracing() {
    INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
        // `try_init` fails only if a global subscriber exists already; that is fine.
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_test_writer()
            .try_init();
    });
}

#[cfg(test)]
mod tests {
    use super::init_tracing;

    #[test]
    fn first_call() {
        init_tracing();
        tracing_subscriber::fmt::try_init().ok();
    }

    #[test]
    fn second_call_does_not_panic() {
        init_tracing();
        init_tracing();
    }
}
