//! Diagnostics service — version, data directory, and tailing the latest
//! rotating log file. Powers the Settings › Diagnostics page in the webui
//! (via both `/api/diagnostics*` on the daemon and the `mn_*` Tauri
//! commands on the desktop app).

use std::io::{Read, Seek, SeekFrom};
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
    /// Total size of the file in bytes at read time. Used by the UI
    /// to show a human-readable size and by the server as the cursor
    /// for the next follow-up `after` request.
    pub file_size: u64,
    /// Whether there are earlier lines above the returned tail that
    /// were not included. True on the first window-based read; false
    /// on follow-up cursor reads that only return new bytes.
    pub truncated: bool,
    /// Byte offset one past the last returned line. The client stores
    /// this and passes it as `after` on the next poll to get just
    /// what's been written since (classic `tail -f` cursor semantics).
    /// If the file was rotated (smaller than the previous cursor), the
    /// server notices and returns a fresh window instead.
    pub cursor: u64,
    /// Whether the server detected that the file rotated since the
    /// client's `after` cursor (file size shrunk below `after`). The
    /// frontend uses this to clear its rolling buffer before appending.
    pub rotated: bool,
    /// The lines themselves in order (oldest → newest). On a window
    /// read this is the last N lines; on a cursor read this is every
    /// line written since `after`.
    pub lines: Vec<String>,
}

// Intentionally NOT instrumented. Diagnostics are meta-operations
// that the user polls while looking at the log itself — logging each
// call would flood the very file the UI is tailing.
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

// Intentionally NOT instrumented — see `get_info`.
/// Read from the log file in one of two modes:
///
///   - **Window mode** (`after` is `None`): return the last `n` lines
///     from the end of the file. Used for the initial page load so
///     the UI has immediate context.
///   - **Cursor mode** (`after` is `Some(offset)`): seek to `offset`
///     and return every line written since. Used by the "Tail" poll
///     to cheaply fetch only the delta. If the file has rotated
///     (size smaller than the cursor), fall back to window mode and
///     set `rotated = true` so the client can reset its buffer.
///
/// Both modes return a `cursor` field — the new byte offset just
/// past the last returned line. The client stores it and passes it
/// back on the next poll. Work done per poll is O(bytes written
/// since last poll), independent of total file size.
pub fn tail_logs(
    state: &AppState,
    n: usize,
    file: Option<&str>,
    after: Option<u64>,
) -> ServiceResult<LogTail> {
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
            file_size: 0,
            truncated: false,
            cursor: 0,
            rotated: false,
            lines: Vec::new(),
        });
    }

    let file_size = std::fs::metadata(&path)
        .map_err(|e| ServiceError::Internal(format!("stat log: {e}")))?
        .len();

    // Cursor mode — read only bytes since `after`.
    if let Some(cursor) = after {
        if cursor > file_size {
            // File rotated (shrunk). Fall through to window mode and
            // tell the client its buffer is stale.
            let (lines, _, truncated) = read_tail_from_end(&path, n, file_size)?;
            return Ok(LogTail {
                path: path.display().to_string(),
                file_size,
                truncated,
                cursor: file_size,
                rotated: true,
                lines,
            });
        }
        if cursor == file_size {
            // No new data. Cheap happy path — no read at all.
            return Ok(LogTail {
                path: path.display().to_string(),
                file_size,
                truncated: false,
                cursor: file_size,
                rotated: false,
                lines: Vec::new(),
            });
        }
        // cursor < file_size — read the delta.
        let new_lines = read_lines_from_offset(&path, cursor, file_size)?;
        return Ok(LogTail {
            path: path.display().to_string(),
            file_size,
            truncated: false,
            cursor: file_size,
            rotated: false,
            lines: new_lines,
        });
    }

    // Window mode — first page load.
    let (lines, file_size, truncated) = read_tail_from_end(&path, n, file_size)?;
    Ok(LogTail {
        path: path.display().to_string(),
        file_size,
        truncated,
        cursor: file_size,
        rotated: false,
        lines,
    })
}

/// Read from `offset` to EOF, split on newlines, return the complete
/// lines. Drops a trailing partial line (one without a final `\n`)
/// because the next poll will pick it up once it's complete.
fn read_lines_from_offset(path: &Path, offset: u64, file_size: u64) -> ServiceResult<Vec<String>> {
    if offset >= file_size {
        return Ok(Vec::new());
    }
    let mut file = std::fs::File::open(path)
        .map_err(|e| ServiceError::Internal(format!("open log: {e}")))?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| ServiceError::Internal(format!("seek log: {e}")))?;

    let bytes_remaining = (file_size - offset) as usize;
    let mut buffer = Vec::with_capacity(bytes_remaining);
    file.read_to_end(&mut buffer)
        .map_err(|e| ServiceError::Internal(format!("read log: {e}")))?;

    let text = String::from_utf8_lossy(&buffer);
    // `split('\n')` keeps the empty string after a trailing '\n', which we
    // want to drop; also drops any non-newline-terminated suffix (it'll
    // come through on the next poll once the line is complete).
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last().map(|s| s.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    Ok(lines.into_iter().map(|s| s.to_string()).collect())
}

/// Seek from the end of `path` and return up to `n` lines from the tail.
///
/// Strategy:
///   1. Open the file, stat its size.
///   2. Read the last `INITIAL_CHUNK` bytes (or the whole file if smaller).
///   3. Count newlines. If we already have > n + 1 newlines — done.
///   4. Otherwise double the window and re-read from earlier, up to a
///      `MAX_CHUNK` ceiling. Past that, fall back to reading the whole
///      file (rare in practice — would mean an average line >512 bytes
///      or very long stack traces).
///   5. Drop the first (possibly partial) line if we didn't start at
///      byte 0, because the window almost certainly cut it in half.
///   6. Return the last `n` from what remains, plus a `truncated` flag
///      indicating whether there were earlier lines we didn't return.
fn read_tail_from_end(
    path: &Path,
    n: usize,
    file_size: u64,
) -> ServiceResult<(Vec<String>, u64, bool)> {
    const INITIAL_CHUNK: u64 = 16 * 1024;
    const MAX_CHUNK: u64 = 2 * 1024 * 1024;

    if file_size == 0 {
        return Ok((Vec::new(), 0, false));
    }

    let mut file = std::fs::File::open(path)
        .map_err(|e| ServiceError::Internal(format!("open log: {e}")))?;

    let mut chunk = INITIAL_CHUNK;
    loop {
        let read_from = file_size.saturating_sub(chunk);
        file.seek(SeekFrom::Start(read_from))
            .map_err(|e| ServiceError::Internal(format!("seek log: {e}")))?;
        let mut buffer = Vec::with_capacity((file_size - read_from) as usize);
        file.read_to_end(&mut buffer)
            .map_err(|e| ServiceError::Internal(format!("read log: {e}")))?;

        // Count newlines. We need at least n+1 to be sure the first
        // line boundary inside the buffer is a real boundary (not a
        // cut-off prefix of a line that started further back).
        let newline_count = buffer.iter().filter(|&&b| b == b'\n').count();
        let covers_whole_file = read_from == 0;

        if newline_count >= n + 1 || covers_whole_file || chunk >= MAX_CHUNK {
            return Ok(extract_tail(&buffer, n, covers_whole_file, file_size));
        }

        chunk = chunk.saturating_mul(2).min(MAX_CHUNK);
        // If we've already hit the ceiling, one more pass at MAX_CHUNK then we stop.
        if chunk >= MAX_CHUNK {
            continue;
        }
    }
}

/// Turn a raw byte buffer from `read_tail_from_end` into (lines, size, truncated).
/// Called in both the "we have enough newlines" and "fell through to whole file" paths.
fn extract_tail(
    buffer: &[u8],
    n: usize,
    covers_whole_file: bool,
    file_size: u64,
) -> (Vec<String>, u64, bool) {
    let text = String::from_utf8_lossy(buffer);
    let mut all_lines: Vec<&str> = text.lines().collect();

    // If the buffer doesn't start at byte 0, drop the first line — it's
    // almost certainly the tail of a line that started before our
    // seek point.
    if !covers_whole_file && !all_lines.is_empty() {
        all_lines.remove(0);
    }

    let start = all_lines.len().saturating_sub(n);
    let truncated = !covers_whole_file || start > 0;
    let tail: Vec<String> = all_lines[start..].iter().map(|&s| s.to_string()).collect();
    (tail, file_size, truncated)
}

