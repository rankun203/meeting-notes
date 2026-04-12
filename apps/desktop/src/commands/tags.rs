use meeting_notes_daemon::services::tags as svc;
use meeting_notes_daemon::services::{AppState, ServiceError};
use serde_json::Value;
use tauri::State;

#[tauri::command]
pub async fn mn_list_tags(state: State<'_, AppState>) -> Result<Value, ServiceError> {
    svc::list_tags(&state).await
}

#[tauri::command]
pub async fn mn_create_tag(
    state: State<'_, AppState>,
    input: svc::CreateTagInput,
) -> Result<Value, ServiceError> {
    svc::create_tag(&state, input).await
}

#[tauri::command]
pub async fn mn_update_tag(
    state: State<'_, AppState>,
    name: String,
    input: svc::UpdateTagInput,
) -> Result<Value, ServiceError> {
    svc::update_tag(&state, &name, input).await
}

#[tauri::command]
pub async fn mn_delete_tag(
    state: State<'_, AppState>,
    name: String,
) -> Result<(), ServiceError> {
    svc::delete_tag(&state, &name).await
}

#[tauri::command]
pub async fn mn_get_tag_sessions(
    state: State<'_, AppState>,
    name: String,
) -> Result<Value, ServiceError> {
    svc::get_tag_sessions(&state, &name).await
}

#[tauri::command]
pub async fn mn_set_session_tags(
    state: State<'_, AppState>,
    id: String,
    input: svc::SetSessionTagsInput,
) -> Result<Value, ServiceError> {
    svc::set_session_tags(&state, &id, input).await
}
