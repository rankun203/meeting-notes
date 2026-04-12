use meeting_notes_daemon::services::people as svc;
use meeting_notes_daemon::services::{AppState, ServiceError};
use serde_json::Value;
use tauri::State;

#[tauri::command]
pub async fn mn_list_people(state: State<'_, AppState>) -> Result<Value, ServiceError> {
    svc::list_people(&state).await
}

#[tauri::command]
pub async fn mn_create_person(
    state: State<'_, AppState>,
    input: svc::CreatePersonInput,
) -> Result<Value, ServiceError> {
    svc::create_person(&state, input).await
}

#[tauri::command]
pub async fn mn_get_person(
    state: State<'_, AppState>,
    id: String,
) -> Result<Value, ServiceError> {
    svc::get_person(&state, &id).await
}

#[tauri::command]
pub async fn mn_update_person(
    state: State<'_, AppState>,
    id: String,
    input: svc::UpdatePersonInput,
) -> Result<Value, ServiceError> {
    svc::update_person(&state, &id, input).await
}

#[tauri::command]
pub async fn mn_delete_person(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), ServiceError> {
    svc::delete_person(&state, &id).await
}

#[tauri::command]
pub async fn mn_get_person_sessions(
    state: State<'_, AppState>,
    person_id: String,
) -> Result<Value, ServiceError> {
    svc::get_person_sessions(&state, &person_id).await
}
