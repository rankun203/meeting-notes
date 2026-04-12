//! Diagnostics service — version, data directory, and tailing the latest
//! rotating log file. Powers the Settings › Diagnostics page in the webui
//! (via both `/api/diagnostics*` on the daemon and the `mn_*` Tauri
//! commands on the desktop app).

use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::Serialize;

use super::error::{ServiceError, ServiceResult};
use super::state::AppState;

#[derive(Debug, Serialize)]
pub struct DiagnosticsInfo {
    pub name: &'static str,
    pub name_zh: &'static str,
    pub version: &'static str,
    pub data_dir: String,
    pub logs_dir: String,
    pub current_log_path: String,
    pub available_log_files: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LogTail {
    /// Absolute path of the file the lines were read from.
    pub path: String,
    /// Total number of lines in the file at read time (for scrollbar UI).
    pub total_lines: usize,
    /// The last N lines in order (oldest → newest).
    pub lines: Vec<String>,
}

#[tracing::instrument(level = "info", skip_all)]
pub fn get_info(state: &AppState) -> ServiceResult<DiagnosticsInfo> {
    let logs_dir = state.tracing.logs_dir();
    let current = state.tracing.current_log_path();

    let mut available: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(logs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    available.push(name.to_string());
                }
            }
        }
        available.sort();
    }

    Ok(DiagnosticsInfo {
        name: "VoiceRecords",
        name_zh: "主簿",
        version: env!("CARGO_PKG_VERSION"),
        data_dir: state.data_dir.display().to_string(),
        logs_dir: logs_dir.display().to_string(),
        current_log_path: current.display().to_string(),
        available_log_files: available,
    })
}

#[tracing::instrument(level = "info", skip_all)]
/// Read the last `n` lines of the current log file (or a specific file
/// inside the logs directory if `file` is supplied). Returns them oldest
/// → newest so the UI can render them with the newest at the bottom like
/// a terminal tail.
///
/// Reads the whole file into memory — fine for our log volumes (daily
/// rotation, each day usually a few hundred KB to a few MB). A true
/// streaming tail from the end would be premature optimization.
pub fn tail_logs(state: &AppState, n: usize, file: Option<&str>) -> ServiceResult<LogTail> {
    let path = match file {
        Some(f) => {
            // Guard against path traversal — restrict to the logs dir.
            let safe = Path::new(f)
                .file_name()
                .ok_or_else(|| ServiceError::BadRequest("invalid log file name".into()))?;
            state.tracing.logs_dir().join(safe)
        }
        None => state.tracing.current_log_path().to_path_buf(),
    };

    if !path.exists() {
        return Ok(LogTail {
            path: path.display().to_string(),
            total_lines: 0,
            lines: Vec::new(),
        });
    }

    let file = std::fs::File::open(&path)
        .map_err(|e| ServiceError::Internal(format!("open log: {e}")))?;
    let reader = BufReader::new(file);
    let mut all_lines: Vec<String> = Vec::new();
    for line in reader.lines() {
        match line {
            Ok(s) => all_lines.push(s),
            Err(e) => {
                return Err(ServiceError::Internal(format!("read log: {e}")));
            }
        }
    }

    let total_lines = all_lines.len();
    let start = total_lines.saturating_sub(n);
    let tail = all_lines[start..].to_vec();

    Ok(LogTail {
        path: path.display().to_string(),
        total_lines,
        lines: tail,
    })
}
