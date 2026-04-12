//! Shared tracing setup — stderr (ANSI) + rotating file appender under
//! `<data_dir>/logs/<basename>.log.YYYY-MM-DD`.
//!
//! Called by both the headless daemon (`src/main.rs`) and the VoiceRecords
//! desktop crate (`apps/desktop/src/main.rs`) so both binaries log with the
//! same format, same filter precedence, and the same rotation policy.

use std::path::{Path, PathBuf};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

/// Handle returned by `init()`. Keep it alive for the program's lifetime
/// — dropping it flushes any buffered log lines and stops the background
/// writer. Leaking it (via `Box::leak` or storing at top-level) is fine
/// in a CLI that runs until Ctrl-C.
pub struct TracingHandle {
    pub logs_dir: PathBuf,
    pub current_log_path: PathBuf,
    // Worker guards for the non-blocking file writers. Keep them alive
    // or the background thread drops pending log lines on exit.
    _file_guard: WorkerGuard,
}

impl TracingHandle {
    /// Path of the current (most recent) rotated log file on disk.
    /// Diagnostics endpoints read this to tail the latest log.
    pub fn current_log_path(&self) -> &Path {
        &self.current_log_path
    }

    pub fn logs_dir(&self) -> &Path {
        &self.logs_dir
    }
}

/// Initialize tracing for the whole process.
///
///   - `data_dir`: root directory of the app's persistent state. Log files
///     go under `<data_dir>/logs/`.
///   - `file_basename`: prefix for the rotated file names — e.g.
///     `"voicerecords"` or `"meeting-notes-daemon"`. The actual file is
///     `<basename>.log.YYYY-MM-DD`.
///   - `default_filter`: the `RUST_LOG`-style filter string used when the
///     environment variable isn't set.
///
/// Panics if called twice in the same process (tracing_subscriber refuses
/// to install a second global dispatcher). Both the daemon and the desktop
/// app call this exactly once from `main()`.
pub fn init(
    data_dir: &Path,
    file_basename: &str,
    default_filter: &str,
) -> TracingHandle {
    let logs_dir = data_dir.join("logs");
    std::fs::create_dir_all(&logs_dir).ok();

    // Rotate daily — one file per UTC day, named
    // `<basename>.log.YYYY-MM-DD`. tracing-appender picks the suffix for
    // us and keeps all previous files on disk (no auto-pruning; the user
    // can clean old days manually for now).
    let file_appender = RollingFileAppender::new(
        Rotation::DAILY,
        &logs_dir,
        format!("{file_basename}.log"),
    );
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_filter));

    tracing_subscriber::registry()
        .with(filter)
        // stderr layer — colored, for terminal / `Console.app` readers.
        .with(fmt::layer().with_writer(std::io::stderr))
        // File layer — plain, rotates daily, stripped of ANSI escapes.
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();

    // Today's log file path — what diagnostics endpoints tail.
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let current_log_path = logs_dir.join(format!("{file_basename}.log.{today}"));

    TracingHandle {
        logs_dir,
        current_log_path,
        _file_guard: guard,
    }
}
