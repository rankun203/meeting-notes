//! Persistent application logs, independent of how the native app was launched.
use std::path::{Path, PathBuf};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn log_directory() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("GDAY_MEETINGS_LOG_DIR") {
        if !path.is_empty() {
            return Ok(path.into());
        }
    }
    let home = dirs::home_dir().ok_or("Could not determine the home directory")?;
    Ok(home.join("Library/Logs/Gday Meetings"))
}

fn file_writer(directory: &Path) -> Result<RollingFileAppender, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(directory)
            .map_err(|error| error.to_string())?;
    }
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("client")
        .filename_suffix("log")
        .max_log_files(14)
        .build(directory)
        .map_err(|error| error.to_string())
}

pub fn init() {
    let directory = log_directory();
    let writer = directory
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|path| file_writer(path));
    let (file, failure) = match writer {
        Ok(writer) => (
            Some(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(writer),
            ),
            None,
        ),
        Err(error) => (None, Some(error)),
    };
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gday_meetings_client=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(file)
        .init();

    // Synchronous file writes also preserve panic diagnostics without needing a
    // background logging thread to drain during unwinding.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        tracing::error!(
            "Client panic: {panic}\n{}",
            std::backtrace::Backtrace::force_capture()
        );
        previous_hook(panic);
    }));
    if let Some(error) = failure {
        tracing::warn!("Persistent logging unavailable; using stderr only: {error}");
    } else if let Ok(directory) = directory {
        tracing::info!(pid = std::process::id(), directory = %directory.display(), "Persistent client logging enabled");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_append_across_restarts_without_ansi_and_preserve_unrelated_files() {
        let directory = std::env::temp_dir().join(format!("gday-logs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("keep.txt"), "unrelated").unwrap();
        for day in 1..=20 {
            std::fs::write(
                directory.join(format!("client.2020-01-{day:02}.log")),
                "old",
            )
            .unwrap();
        }
        for message in ["first launch", "second launch"] {
            let subscriber = tracing_subscriber::fmt()
                .with_ansi(false)
                .with_writer(file_writer(&directory).unwrap())
                .finish();
            tracing::subscriber::with_default(subscriber, || tracing::info!("{message}"));
        }
        let logs: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "log"))
            .collect();
        assert!(logs.len() <= 14);
        let text = logs
            .iter()
            .map(|path| std::fs::read_to_string(path).unwrap())
            .collect::<String>();
        assert!(text.contains("first launch") && text.contains("second launch"));
        assert!(!text.contains('\u{1b}'));
        assert_eq!(
            std::fs::read_to_string(directory.join("keep.txt")).unwrap(),
            "unrelated"
        );
        assert!(file_writer(&directory.join("keep.txt")).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
