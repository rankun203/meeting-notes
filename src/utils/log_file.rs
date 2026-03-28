//! Shared log file writer for subprocess output.

use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// A thread-safe log file writer that can be shared across async tasks.
#[derive(Clone)]
pub struct LogFile {
    writer: Arc<Mutex<BufWriter<std::fs::File>>>,
}

impl LogFile {
    /// Create a new log file at the given path (truncates if exists).
    pub fn create(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::create(path)
            .map_err(|e| format!("failed to create log file at {}: {e}", path.display()))?;
        Ok(Self {
            writer: Arc::new(Mutex::new(BufWriter::new(file))),
        })
    }

    /// Write a line to the log file.
    pub fn write_line(&self, line: &str) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = writeln!(w, "{}", line);
            let _ = w.flush();
        }
    }

    /// Spawn a background task that reads lines from a tokio `Lines` reader
    /// and writes them to this log file. Returns immediately.
    pub fn spawn_reader(
        &self,
        mut lines: tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStderr>>,
    ) {
        let log = self.clone();
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                log.write_line(&line);
            }
        });
    }
}
