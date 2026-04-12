use meeting_notes_daemon::services::claude as svc;
use meeting_notes_daemon::services::{AppState, ServiceError};
use serde_json::Value;
use tauri::State;

#[tauri::command]
pub async fn mn_claude_status(state: State<'_, AppState>) -> Result<Value, ServiceError> {
    svc::status(&state).await
}

#[tauri::command]
pub async fn mn_claude_stop(state: State<'_, AppState>) -> Result<Value, ServiceError> {
    svc::stop(&state).await
}

#[tauri::command]
pub async fn mn_claude_approve_tools(
    state: State<'_, AppState>,
    input: svc::ApproveToolsInput,
) -> Result<Value, ServiceError> {
    svc::approve_tools(&state, input).await
}

#[tauri::command]
pub async fn mn_claude_list_sessions(
    state: State<'_, AppState>,
) -> Result<Value, ServiceError> {
    svc::list_sessions(&state).await
}

#[tauri::command]
pub async fn mn_claude_get_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<Value, ServiceError> {
    svc::get_session(&state, &id).await
}
