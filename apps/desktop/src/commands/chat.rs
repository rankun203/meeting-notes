use futures::StreamExt;
use meeting_notes_daemon::services::chat as svc;
use meeting_notes_daemon::services::{AppState, ServiceError};
use serde_json::Value;
use tauri::ipc::Channel;
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

/// Streaming chat endpoint. The frontend passes a `tauri::ipc::Channel`
/// (created via `new Channel()` on the JS side); every `ChatEvent` the
/// service pipeline produces is pushed through it. The command itself
/// resolves to `()` once the stream completes — the events are delivered
/// out-of-band via the channel.
#[tauri::command]
pub async fn mn_send_message(
    state: State<'_, AppState>,
    id: String,
    input: svc::SendMessageInput,
    on_event: Channel<svc::ChatEvent>,
) -> Result<(), ServiceError> {
    let mut stream = svc::send_message_stream(&state, &id, input).await?;
    while let Some(ev) = stream.next().await {
        if let Err(e) = on_event.send(ev) {
            // Channel closed — frontend stopped listening. Stop streaming.
            tracing::debug!("mn_send_message channel closed: {}", e);
            break;
        }
    }
    Ok(())
}
