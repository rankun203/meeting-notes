use meeting_notes_daemon::services::chat as svc;
use meeting_notes_daemon::services::{AppState, ServiceError};
use serde_json::Value;
use tauri::State;

#[tauri::command]
pub async fn mn_list_conversations(state: State<'_, AppState>) -> Result<Value, ServiceError> {
    svc::list_conversations(&state).await
}

#[tauri::command]
pub async fn mn_create_conversation(
    state: State<'_, AppState>,
    input: svc::CreateConversationInput,
) -> Result<Value, ServiceError> {
    svc::create_conversation(&state, input).await
}

#[tauri::command]
pub async fn mn_get_conversation(
    state: State<'_, AppState>,
    id: String,
) -> Result<Value, ServiceError> {
    svc::get_conversation(&state, &id).await
}

#[tauri::command]
pub async fn mn_delete_conversation(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), ServiceError> {
    svc::delete_conversation(&state, &id).await
}

#[tauri::command]
pub async fn mn_delete_message(
    state: State<'_, AppState>,
    id: String,
    msg_id: String,
) -> Result<(), ServiceError> {
    svc::delete_message(&state, &id, &msg_id).await
}

#[tauri::command]
pub async fn mn_sync_claude_messages(
    state: State<'_, AppState>,
    id: String,
    body: Value,
) -> Result<Value, ServiceError> {
    svc::sync_claude_messages(&state, &id, body).await
}

#[tauri::command]
pub async fn mn_export_prompt(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, ServiceError> {
    svc::export_prompt(&state, &id).await
}

#[tauri::command]
pub async fn mn_list_models(state: State<'_, AppState>) -> Result<Value, ServiceError> {
    svc::list_models(&state).await
}
