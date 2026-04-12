use meeting_notes_daemon::services::diagnostics as svc;
use meeting_notes_daemon::services::{AppState, ServiceError};
use tauri::State;

#[tauri::command]
pub async fn mn_get_diagnostics(
    state: State<'_, AppState>,
) -> Result<svc::DiagnosticsInfo, ServiceError> {
    svc::get_info(&state)
}

#[tauri::command]
pub async fn mn_tail_logs(
    state: State<'_, AppState>,
    lines: Option<usize>,
    file: Option<String>,
) -> Result<svc::LogTail, ServiceError> {
    svc::tail_logs(&state, lines.unwrap_or(100), file.as_deref())
}
