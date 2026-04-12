use meeting_notes_daemon::services::settings as svc;
use meeting_notes_daemon::services::{AppState, ServiceError};
use serde_json::Value;
use tauri::State;

#[tauri::command]
pub async fn mn_get_settings(state: State<'_, AppState>) -> Result<Value, ServiceError> {
    svc::get_settings(&state).await
}

#[tauri::command]
pub async fn mn_update_settings(
    state: State<'_, AppState>,
    body: Value,
) -> Result<Value, ServiceError> {
    svc::update_settings(&state, body).await
}
